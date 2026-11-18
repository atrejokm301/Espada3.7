//! Topic index (Índice) for the Diccionario and Léxico tabs, e-Sword style.
//!
//! Beside the article the panel lists every topic of the module in Spanish alphabetical
//! order, like e-Sword and like Espada 3.7 for macOS (user choice 2026-11-11, extended here to
//! the Léxico): a search jumps to its exact topic or marks where the word would go, and any
//! topic opens with a click. The installed modules carry up to 115 837 topics (webster-1913),
//! so the UI asks for windows of the sorted list. Each list is built once per file — read-only,
//! nothing is ever written to the module — rebuilt when the file changes, and kept in a small
//! LRU cache.
//!
//! Strong's-keyed rows (`G26`, `H1254`) also carry the Greek/Hebrew headword of their entry so
//! the Léxico index reads `G26 ἀγάπη` instead of a bare number. The headword is read lazily,
//! only for the rows of the window being shown, and only when the entry prints it right at the
//! start (see [`extract_lemma`]); otherwise the row shows the code alone.

use once_cell::sync::Lazy;
use rusqlite::types::{Value, ValueRef};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use super::sections::Titles;

use super::reader::{
    column_set_cached, module_labels_cached, table_exists_cached, truncate,
    with_conn, DictionaryResult, MAX_TEXT,
};
use super::text::cell_to_text;

/// One row of a module's alphabetical topic index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRow {
    /// Position in the sorted index (0-based).
    pub pos: u32,
    /// The topic's row in the module: opens exactly this article, even when two rows share a
    /// topic (StrongsPlus.lexi lists every code twice).
    pub rowid: i64,
    pub topic: String,
    /// Greek/Hebrew headword of a Strong's-keyed entry; empty when unknown.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lemma: String,
}

/// A window of the topic index, around a query or an anchor position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexSlice {
    pub total: u32,
    /// Position of `rows[0]`.
    pub start: u32,
    /// Where the query is in the index: its topic, or the position it would take.
    pub focus: Option<u32>,
    /// The topic at `focus` is the query itself (case, accents and leading zeros aside).
    pub exact: bool,
    /// Most topics are Strong's codes (a Strong lexicon, or Vine with numbers).
    pub strong_keyed: bool,
    /// First `G…` / `H…` code, for the Léxico's «Griego» / «Hebreo» jumps.
    pub greek_start: Option<u32>,
    pub hebrew_start: Option<u32>,
    pub rows: Vec<IndexRow>,
}

/// Upper bound for one window (the UI asks for ~120 rows).
const INDEX_MAX_WINDOW: u32 = 1000;
/// Sorted indexes kept in memory (webster-1913, the largest, is ~10 MB).
const INDEX_CACHE_MAX: usize = 6;
/// Characters of a topic shown in the index (01LBLA.dcti has a 14 664-character "topic").
const TOPIC_MAX_CHARS: usize = 120;
/// Raw characters of a definition read to find its headword.
const LEMMA_SOURCE_CHARS: u32 = 2000;
/// A headword printed later than this (in cleaned characters) is not the entry's own word:
/// 04Vine.lexi's G26 is a 274-character preface before any Greek.
const LEMMA_WINDOW: usize = 40;
/// Longest headword accepted, in characters (pointed Hebrew counts its vowel points).
const LEMMA_MAX_CHARS: usize = 40;

/// Strong's prefix of a row's topic: `b'G'`, `b'H'`, or 0 for an ordinary topic.
type StrongTag = u8;

struct TopicIndex {
    table: &'static str,
    keys: Vec<String>,
    /// `(rowid, display topic, strong tag)` in index order.
    rows: Vec<(i64, String, StrongTag)>,
    greek_start: Option<u32>,
    hebrew_start: Option<u32>,
    strong_rows: u32,
    /// rowid → headword ("" when the entry does not print one where it counts).
    lemmas: Mutex<HashMap<i64, String>>,
}

struct CacheSlot {
    ident: Option<(u64, u64)>,
    stamp: u64,
    index: Arc<TopicIndex>,
}

static INDEX_CACHE: Lazy<Mutex<HashMap<String, CacheSlot>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static INDEX_TICK: AtomicU64 = AtomicU64::new(0);

/// A window of `module_path`'s alphabetical topic index: `before` rows above and `after` rows
/// from the query's position (or from `anchor`, for «Anteriores…» / «Siguientes…» and the
/// Griego / Hebreo jumps).
pub fn topic_index(
    module_path: &str,
    query: Option<&str>,
    anchor: Option<u32>,
    before: u32,
    after: u32,
) -> Result<IndexSlice, String> {
    let path = PathBuf::from(module_path);
    let ident = file_ident(&path);
    with_conn(&path, |conn, path_key| {
        let idx = cached_or_build(conn, &path, path_key, ident)?;
        let total = idx.rows.len() as u32;
        let (focus, exact) = match query.map(str::trim).filter(|q| !q.is_empty()) {
            Some(q) => {
                let k = query_key(q);
                let pos = idx.keys.partition_point(|x| x.as_str() < k.as_str());
                (Some(pos as u32), idx.keys.get(pos).is_some_and(|x| *x == k))
            }
            None => (None, false),
        };
        let center = anchor.or(focus).unwrap_or(0).min(total);
        let start = center.saturating_sub(before.min(INDEX_MAX_WINDOW));
        let end = center
            .saturating_add(after.min(INDEX_MAX_WINDOW))
            .min(total);
        let lemmas = lemmas_for(conn, &idx, start, end);
        let rows = (start..end)
            .map(|p| {
                let (rowid, topic, _) = &idx.rows[p as usize];
                IndexRow {
                    pos: p,
                    rowid: *rowid,
                    topic: topic.clone(),
                    lemma: lemmas.get(rowid).cloned().unwrap_or_default(),
                }
            })
            .collect();
        Ok(IndexSlice {
            total,
            start,
            focus,
            exact,
            strong_keyed: total > 0 && idx.strong_rows.saturating_mul(2) >= total,
            greek_start: idx.greek_start,
            hebrew_start: idx.hebrew_start,
            rows,
        })
    })
}

/// The article of one index row (exactly that topic, by its rowid).
pub fn topic_entry(module_path: &str, rowid: i64) -> Result<Option<DictionaryResult>, String> {
    let path = PathBuf::from(module_path);
    with_conn(&path, |conn, path_key| {
        let table = index_table(conn, &path, path_key)?;
        let cols = column_set_cached(conn, path_key, table);
        if !cols.iter().any(|c| c.eq_ignore_ascii_case("Definition")) {
            return Err("Formato de módulo no compatible: la tabla no tiene columna Definition."
                .into());
        }
        let sql = format!("SELECT Topic, Definition FROM [{table}] WHERE rowid = ?1");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("No se pudo leer la entrada: {e}"))?;
        let mut rows = stmt
            .query([rowid])
            .map_err(|e| format!("No se pudo leer la entrada: {e}"))?;
        let Some(row) = rows
            .next()
            .map_err(|e| format!("No se pudo leer la entrada: {e}"))?
        else {
            return Ok(None);
        };
        let topic = row.get_ref(0).map(lossy_text).unwrap_or_default();
        let def = row.get_ref(1).map(lossy_value).unwrap_or(Value::Null);
        let (title, abbr) = module_labels_cached(conn, &path);
        let definition = Titles::new(conn, path_key)
            .display(&def, 2)
            .map(|d| truncate(&d, MAX_TEXT))
            .unwrap_or_default();
        Ok(Some(DictionaryResult {
            term: display_topic(&topic),
            source: abbr,
            title,
            definition,
            module: path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            query_term: String::new(),
            fallback: false,
        }))
    })
}

fn file_ident(path: &Path) -> Option<(u64, u64)> {
    let meta = path.metadata().ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Some((mtime, meta.len()))
}

/// The cached index of this file, or a fresh one when it is new or changed on disk.
///
/// Runs inside `with_conn`, which holds this module's connection lock, so two windows asked
/// for at once never build the same index twice.
fn cached_or_build(
    conn: &Connection,
    path: &Path,
    path_key: &str,
    ident: Option<(u64, u64)>,
) -> Result<Arc<TopicIndex>, String> {
    let stamp = INDEX_TICK.fetch_add(1, Ordering::Relaxed);
    if ident.is_some() {
        if let Ok(mut cache) = INDEX_CACHE.lock() {
            if let Some(slot) = cache.get_mut(path_key) {
                if slot.ident == ident {
                    slot.stamp = stamp;
                    return Ok(slot.index.clone());
                }
            }
        }
    }
    let idx = Arc::new(build_index(conn, path, path_key)?);
    if let Ok(mut cache) = INDEX_CACHE.lock() {
        cache.insert(
            path_key.to_string(),
            CacheSlot {
                ident,
                stamp,
                index: idx.clone(),
            },
        );
        while cache.len() > INDEX_CACHE_MAX {
            let oldest = cache
                .iter()
                .filter(|(k, _)| k.as_str() != path_key)
                .min_by_key(|(_, s)| s.stamp)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    cache.remove(&k);
                }
                None => break,
            }
        }
    }
    Ok(idx)
}

/// Which table holds the topics. Same preference as the searches: a lexicon reads `Lexicon`
/// first (`lookup_lexicon`), a dictionary `Dictionary` first (`lookup_dictionary`), so an index
/// row and a search hit always point at the same table.
fn index_table(conn: &Connection, path: &Path, path_key: &str) -> Result<&'static str, String> {
    let is_lexicon = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.to_ascii_lowercase().starts_with("lex"));
    let order: [&'static str; 2] = if is_lexicon {
        ["Lexicon", "Dictionary"]
    } else {
        ["Dictionary", "Lexicon"]
    };
    let Some(table) = order
        .into_iter()
        .find(|t| table_exists_cached(conn, path_key, t))
    else {
        return Err("Este módulo no tiene tabla Dictionary/Lexicon.".into());
    };
    let cols = column_set_cached(conn, path_key, table);
    if !cols.iter().any(|c| c.eq_ignore_ascii_case("Topic")) {
        return Err("Formato de módulo no compatible: la tabla no tiene columna Topic.".into());
    }
    Ok(table)
}

fn build_index(conn: &Connection, path: &Path, path_key: &str) -> Result<TopicIndex, String> {
    let table = index_table(conn, path, path_key)?;
    let read_err = |e: rusqlite::Error| format!("No se pudo leer el índice del módulo: {e}");
    let mut stmt = conn
        .prepare(&format!("SELECT rowid, Topic FROM [{table}]"))
        .map_err(read_err)?;
    let mut rows = stmt.query([]).map_err(read_err)?;
    let mut all: Vec<(String, i64, String, StrongTag)> = Vec::new();
    while let Some(row) = rows.next().map_err(read_err)? {
        let Ok(rowid) = row.get::<_, i64>(0) else {
            continue;
        };
        let Ok(raw) = row.get_ref(1) else {
            continue;
        };
        if matches!(raw, ValueRef::Null) {
            continue;
        }
        let shown = display_topic(&lossy_text(raw));
        let key = index_key(&shown);
        // Separators and blanks ("-----", "\u{a0}\u{a0}", "??") have no letters to sort by.
        if key.is_empty() {
            continue;
        }
        let tag = strong_prefix(&shown).map(|c| c as u8).unwrap_or(0);
        all.push((key, rowid, shown, tag));
    }
    all.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut idx = TopicIndex {
        table,
        keys: Vec::with_capacity(all.len()),
        rows: Vec::with_capacity(all.len()),
        greek_start: None,
        hebrew_start: None,
        strong_rows: 0,
        lemmas: Mutex::new(HashMap::new()),
    };
    for (pos, (key, rowid, shown, tag)) in all.into_iter().enumerate() {
        match tag {
            b'G' => {
                idx.strong_rows += 1;
                idx.greek_start.get_or_insert(pos as u32);
            }
            b'H' => {
                idx.strong_rows += 1;
                idx.hebrew_start.get_or_insert(pos as u32);
            }
            _ => {}
        }
        idx.keys.push(key);
        idx.rows.push((rowid, shown, tag));
    }
    Ok(idx)
}

/// Headwords for the Strong's rows in `start..end`, reading only rows not seen before.
fn lemmas_for(conn: &Connection, idx: &TopicIndex, start: u32, end: u32) -> HashMap<i64, String> {
    let wanted: Vec<(i64, StrongTag, &str)> = (start..end)
        .filter_map(|p| {
            let (rowid, topic, tag) = &idx.rows[p as usize];
            (*tag != 0).then_some((*rowid, *tag, topic.as_str()))
        })
        .collect();
    let mut out = HashMap::with_capacity(wanted.len());
    if wanted.is_empty() {
        return out;
    }
    let Ok(mut known) = idx.lemmas.lock() else {
        return out;
    };
    let sql = format!(
        "SELECT CASE WHEN typeof(Definition) = 'text' \
                THEN substr(Definition, 1, {LEMMA_SOURCE_CHARS}) ELSE Definition END \
         FROM [{}] WHERE rowid = ?1",
        idx.table
    );
    let mut stmt = conn.prepare_cached(&sql).ok();
    for (rowid, tag, code) in wanted {
        if let Some(l) = known.get(&rowid) {
            if !l.is_empty() {
                out.insert(rowid, l.clone());
            }
            continue;
        }
        let lemma = stmt
            .as_mut()
            .and_then(|s| {
                s.query_row([rowid], |r| Ok(r.get_ref(0).map(lossy_value).unwrap_or(Value::Null)))
                    .ok()
            })
            .and_then(|v| cell_to_text(&v))
            .and_then(|text| extract_lemma(&text, tag as char, code))
            .unwrap_or_default();
        known.insert(rowid, lemma.clone());
        if !lemma.is_empty() {
            out.insert(rowid, lemma);
        }
    }
    out
}

/// A cell as text, never failing on bad UTF-8 (one stray byte must not hide a topic).
fn lossy_text(v: ValueRef<'_>) -> String {
    match v {
        ValueRef::Text(t) | ValueRef::Blob(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(r) => r.to_string(),
        ValueRef::Null => String::new(),
    }
}

fn lossy_value(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::Blob(b.to_vec()),
        ValueRef::Integer(i) => Value::Integer(i),
        ValueRef::Real(r) => Value::Real(r),
        ValueRef::Null => Value::Null,
    }
}

/// What the index shows for a raw Topic cell: markup and entities decoded, whitespace (NBSP
/// included) collapsed to single spaces, and at most [`TOPIC_MAX_CHARS`] characters.
pub(crate) fn display_topic(raw: &str) -> String {
    let cleaned;
    let text = if raw.contains(['<', '&', '\\']) {
        cleaned = super::text::clean_text(raw);
        cleaned.as_str()
    } else {
        raw
    };
    let mut out = String::with_capacity(text.len().min(TOPIC_MAX_CHARS * 4));
    let mut gap = false;
    let mut count = 0usize;
    for c in text.chars() {
        if c.is_whitespace() || c.is_control() {
            gap = !out.is_empty();
            continue;
        }
        // Private-use markers (red letters) and zero-width characters never show.
        if matches!(c as u32, 0xE000..=0xF8FF | 0x200B..=0x200D | 0xFEFF) {
            continue;
        }
        // A pending space needs a slot of its own, so the cut always lands on a word.
        if count + usize::from(gap) + 1 > TOPIC_MAX_CHARS {
            out.push('…');
            break;
        }
        if gap {
            out.push(' ');
            count += 1;
            gap = false;
        }
        out.push(c);
        count += 1;
    }
    out
}

/// Spanish dictionary order: case and accents ignored, ñ right after n, leading spaces and
/// punctuation («¡», «"», «(», «-----») ignored, inner spaces collapsed, and numbers in natural
/// order (G2 before G10, and G00026 equal to G26). Same rule as Espada 3.7 for macOS.
pub(crate) fn index_key(topic: &str) -> String {
    let t = topic.trim_start_matches(|c: char| !c.is_alphanumeric());
    let mut out = String::with_capacity(t.len() + 16);
    let mut digits = String::new();
    let mut gap = false;
    for c in t.chars() {
        if c.is_whitespace() {
            flush_digits(&mut digits, &mut out);
            gap = !out.is_empty();
            continue;
        }
        if gap {
            out.push(' ');
            gap = false;
        }
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        flush_digits(&mut digits, &mut out);
        let u = c as u32;
        if (0x0300..=0x036F).contains(&u) {
            // Decomposed accents fold like composed ones; n + U+0303 is ñ.
            if u == 0x0303 && out.ends_with('n') {
                out.push('~');
            }
            continue;
        }
        for lc in c.to_lowercase() {
            match lc {
                // '~' sorts after every ASCII letter: n < ñ < o.
                'ñ' => out.push_str("n~"),
                'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => out.push('a'),
                'é' | 'è' | 'ê' | 'ë' => out.push('e'),
                'í' | 'ì' | 'î' | 'ï' => out.push('i'),
                'ó' | 'ò' | 'ô' | 'ö' | 'õ' => out.push('o'),
                'ú' | 'ù' | 'û' | 'ü' => out.push('u'),
                'ç' => out.push('c'),
                'ý' | 'ÿ' => out.push('y'),
                other => out.push(other),
            }
        }
    }
    flush_digits(&mut digits, &mut out);
    out
}

/// Left-pad a digit run so runs compare by value (runs longer than the width keep their order).
fn flush_digits(digits: &mut String, out: &mut String) {
    const NUM_WIDTH: usize = 12;
    if digits.is_empty() {
        return;
    }
    let significant = digits.trim_start_matches('0');
    let significant = if significant.is_empty() { "0" } else { significant };
    for _ in significant.len()..NUM_WIDTH {
        out.push('0');
    }
    out.push_str(significant);
    digits.clear();
}

/// The key a typed query is looked up by. "H 01254" is the code H1254.
fn query_key(q: &str) -> String {
    if strong_prefix(q).is_some() {
        index_key(&q.split_whitespace().collect::<String>())
    } else {
        index_key(q)
    }
}

/// `Some('G' | 'H')` when the topic is a Strong's code: G26, h1254, "G 26", G00001, H1254a.
/// Bare numbers (nsbdt.lexi's "1", "2") and other prefixes (Peshito's A200001) are not.
fn strong_prefix(topic: &str) -> Option<char> {
    let t = topic.trim();
    let mut chars = t.chars();
    let p = chars.next()?.to_ascii_uppercase();
    if p != 'G' && p != 'H' {
        return None;
    }
    let rest = chars.as_str().trim_start();
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 5 {
        return None;
    }
    let tail = &rest[digits..];
    let mut tail_chars = tail.chars();
    match (tail_chars.next(), tail_chars.next()) {
        (None, _) => Some(p),
        (Some(c), None) if c.is_ascii_alphabetic() => Some(p),
        _ => None,
    }
}

fn is_hebrew_letter(c: char) -> bool {
    matches!(c as u32, 0x05D0..=0x05EA | 0x05EF..=0x05F2 | 0xFB1D..=0xFB4F)
}

/// Vowel points and cantillation inside a Hebrew word (not maqaf, paseq or sof pasuq).
fn is_hebrew_mark(c: char) -> bool {
    matches!(c as u32, 0x0591..=0x05BD | 0x05BF | 0x05C1..=0x05C2 | 0x05C4..=0x05C5 | 0x05C7)
}

fn is_greek_letter(c: char) -> bool {
    matches!(c as u32, 0x0370..=0x03FF | 0x1F00..=0x1FFF) && c.is_alphabetic()
}

fn is_combining_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F)
}

/// `&#1488` with no closing `;` (Diccionario Moises spells final letters that way): decoded
/// here for Hebrew and Greek code points only, so the headword is read whole.
fn decode_bare_script_entities(text: &str) -> String {
    static RE_BARE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"&#(?:[xX]([0-9A-Fa-f]{2,6})|([0-9]{2,7}));?").unwrap());
    if !text.contains("&#") {
        return text.to_string();
    }
    RE_BARE
        .replace_all(text, |caps: &regex::Captures| {
            let cp = match (caps.get(1), caps.get(2)) {
                (Some(h), _) => u32::from_str_radix(h.as_str(), 16).ok(),
                (_, Some(d)) => d.as_str().parse::<u32>().ok(),
                _ => None,
            };
            match cp.and_then(char::from_u32) {
                Some(c) if is_hebrew_letter(c) || is_hebrew_mark(c) || is_greek_letter(c) => {
                    c.to_string()
                }
                _ => caps[0].to_string(),
            }
        })
        .into_owned()
}

/// The Strong's code that sits right after a word, as in «ἀγάπη G26» or «λαλέω[G2980]».
fn adjacent_code(rest: &[char]) -> Option<String> {
    let mut i = 0;
    while i < rest.len() && (rest[i] == ' ' || rest[i] == '[' || rest[i] == '(') {
        i += 1;
    }
    let p = rest.get(i)?.to_ascii_uppercase();
    if p != 'G' && p != 'H' {
        return None;
    }
    let digits: String = rest[i + 1..].iter().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || rest.get(i + 1 + digits.len()).is_some_and(|c| c.is_alphanumeric()) {
        return None;
    }
    Some(format!("{p}{}", digits.trim_start_matches('0')))
}

/// The headword of a Strong's entry: the first Hebrew word (H codes) or Greek word (G codes)
/// of its cleaned text, accepted only when it starts within the first [`LEMMA_WINDOW`]
/// characters and stands as a word of its own. Lexicons print the headword first
/// («ἀγάπη agápe; de G25…», «Diccionario Strong בָּרָא bara…», «Número Strong: G26 Lexema:
/// ἀγάπη»). Rejected, because each would show a wrong word (no headword beats a wrong one):
/// a word glued to letters of another script (Swanson's «ἀγάπē», mojibake like «á̓γάπη»),
/// a form from an occurrence list («θεε (16)» in the LXX/HOT concordances), and a word
/// labelled with another code («λαλέω[G2980]» heading Trench's synonyms for G3056).
/// `code` is the row's own code (G26), used for that last check.
pub(crate) fn extract_lemma(text: &str, prefix: char, code: &str) -> Option<String> {
    let hebrew = prefix.eq_ignore_ascii_case(&'H');
    let is_letter = |c: char| {
        if hebrew {
            is_hebrew_letter(c)
        } else {
            is_greek_letter(c)
        }
    };
    let is_mark = |c: char| {
        if hebrew {
            is_hebrew_mark(c)
        } else {
            is_combining_mark(c)
        }
    };
    let head: String = text.chars().take(LEMMA_WINDOW * 8).collect();
    let chars: Vec<char> = decode_bare_script_entities(&head)
        .chars()
        .take(LEMMA_WINDOW + LEMMA_MAX_CHARS + 16)
        .collect();
    let start = chars.iter().take(LEMMA_WINDOW).position(|&c| is_letter(c))?;
    // Whatever touches the word on the left, combining marks skipped, must not be a letter.
    let mut before = start;
    while before > 0 && (is_combining_mark(chars[before - 1]) || is_hebrew_mark(chars[before - 1])) {
        before -= 1;
    }
    if before > 0 && chars[before - 1].is_alphabetic() {
        return None;
    }
    let is_hyphen = |c: char| matches!(c, '-' | '\u{2010}');
    let mut word = String::new();
    let mut end = start;
    loop {
        while end < chars.len() && (is_letter(chars[end]) || is_mark(chars[end])) {
            word.push(chars[end]);
            end += 1;
        }
        // LSJ marks the stem with a hyphen: «ἀγάπ-η» is the headword ἀγάπη.
        if end + 1 < chars.len() && is_hyphen(chars[end]) && is_letter(chars[end + 1]) {
            end += 1;
            continue;
        }
        break;
    }
    if end - start > LEMMA_MAX_CHARS {
        return None;
    }
    // Glued to letters of another script, directly or across a hyphen: not a whole word.
    let after = chars.get(end).copied();
    let after_hyphen = chars.get(end + 1).copied();
    if after.is_some_and(char::is_alphabetic)
        || (after.is_some_and(is_hyphen) && after_hyphen.is_some_and(char::is_alphabetic))
    {
        return None;
    }
    let rest = &chars[end..];
    // «θεε (16)»: a form counted in an occurrence list, not the headword.
    let spaced = rest.iter().skip_while(|c| **c == ' ').copied().collect::<Vec<char>>();
    if spaced.first() == Some(&'(') {
        let n = spaced[1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if n > 0 && spaced.get(1 + n) == Some(&')') {
            return None;
        }
    }
    // «λαλέω[G2980]» under G3056: that word belongs to another number.
    if let Some(next) = adjacent_code(rest) {
        let own: String = code.split_whitespace().collect::<String>().to_ascii_uppercase();
        let own_num = own
            .get(1..)
            .map(|d| d.trim_end_matches(|c: char| c.is_ascii_alphabetic()).trim_start_matches('0'))
            .unwrap_or("");
        if next != format!("{}{own_num}", own.chars().next().unwrap_or(' ')) {
            return None;
        }
    }
    Some(word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    const ESWORD: &str = r"C:\Program Files (x86)\e-Sword";

    fn sorted(topics: &[&str]) -> Vec<String> {
        let mut v: Vec<(String, String)> = topics
            .iter()
            .map(|t| (index_key(&display_topic(t)), t.to_string()))
            .collect();
        v.sort();
        v.into_iter().map(|(_, t)| t).collect()
    }

    #[test]
    fn spanish_order_ignores_case_and_accents_and_puts_n_tilde_after_n() {
        let expected = [
            "ABOGADO",
            "ABRAM",
            "AMÓN",
            "AMOR",
            "ANZUELO",
            "AÑO",
            "NUBE",
            "ÑU",
            "OVEJA",
            "PIEDRA",
            "PIEDRAS PRECIOSAS",
            "PILATO, PONCIO",
            "PINO",
            "ZARZA",
        ];
        let mut shuffled = expected.to_vec();
        shuffled.reverse();
        shuffled.swap(2, 9);
        assert_eq!(sorted(&shuffled), expected);
        assert_eq!(index_key("Amor"), index_key("AMOR"));
        assert_eq!(index_key("Jehová"), index_key("JEHOVA"));
    }

    #[test]
    fn numbers_sort_by_value_and_leading_zeros_do_not_count() {
        assert_eq!(
            sorted(&["H1", "G100", "G10", "G2", "G1", "G26"]),
            ["G1", "G2", "G10", "G26", "G100", "H1"]
        );
        // na26-.lexi spells its codes G00026.
        assert_eq!(index_key("G00026"), index_key("G26"));
        assert_eq!(query_key("g 026"), index_key("G26"));
        assert_eq!(sorted(&["TEMA 10", "TEMA 9", "TEMA 100"]), ["TEMA 9", "TEMA 10", "TEMA 100"]);
    }

    #[test]
    fn blanks_separators_and_leading_punctuation_are_not_letters() {
        assert_eq!(index_key(&display_topic("\u{a0}\u{a0}Zorobabel\u{a0}\u{a0}")), "zorobabel");
        assert_eq!(display_topic("zorra\u{a0}\u{a0}\u{a0}"), "zorra");
        assert_eq!(index_key("-----------------"), "");
        assert_eq!(index_key(&display_topic("\u{a0}\u{a0}\u{a0}")), "");
        assert_eq!(index_key("¡Ay de mí!"), index_key("ay de mi!"));
        assert_eq!(index_key("  (palabra)"), "palabra)");
        assert_eq!(index_key("piedra   preciosa"), "piedra preciosa");
    }

    #[test]
    fn decomposed_accents_fold_like_composed_ones() {
        assert_eq!(index_key("an\u{303}o"), index_key("año"));
        assert_eq!(index_key("Ame\u{301}rica"), index_key("América"));
        assert_eq!(index_key("año"), "an~o");
    }

    #[test]
    fn display_topic_decodes_markup_and_caps_length() {
        assert_eq!(display_topic("<b>Amor</b> &amp; gracia"), "Amor & gracia");
        assert_eq!(display_topic("Fe\r\n  y\tobras"), "Fe y obras");
        let long = "x".repeat(500);
        let shown = display_topic(&long);
        assert_eq!(shown.chars().count(), TOPIC_MAX_CHARS + 1);
        assert!(shown.ends_with('…'));
        assert_eq!(display_topic(&"y".repeat(TOPIC_MAX_CHARS)), "y".repeat(TOPIC_MAX_CHARS));
        // A space right at the limit is not kept in front of the ellipsis.
        let spaced = format!("{} {}", "z".repeat(TOPIC_MAX_CHARS - 1), "zz");
        let shown = display_topic(&spaced);
        assert!(shown.chars().count() <= TOPIC_MAX_CHARS + 1, "{} chars", shown.chars().count());
        assert!(shown.ends_with("z…"));
    }

    #[test]
    fn strong_codes_are_recognised_by_shape() {
        for (t, want) in [
            ("G26", Some('G')),
            ("h1254", Some('H')),
            ("G 26", Some('G')),
            ("G00001", Some('G')),
            ("H1254a", Some('H')),
            ("  H8674 ", Some('H')),
            ("Gozo", None),
            ("666", None),
            ("1", None),
            ("G123456", None),
            ("A200001", None),
            ("G26 amor", None),
            ("", None),
        ] {
            assert_eq!(strong_prefix(t), want, "{t:?}");
        }
    }

    #[test]
    fn headword_is_taken_only_from_the_start_of_the_entry() {
        // Each case is the start of a real entry in an installed module.
        let ok = |t: &str, code: &str| extract_lemma(t, code.chars().next().unwrap(), code);
        assert_eq!(ok("ἀγάπη agápe de G25 ; amor", "G26").as_deref(), Some("ἀγάπη"));
        assert_eq!(ok("Diccionario Strong ἀγάπη agápe de G25", "G26").as_deref(), Some("ἀγάπη"));
        assert_eq!(ok("Número Strong: G26 Lexema: ἀγάπη Transliteración", "G26").as_deref(), Some("ἀγάπη"));
        assert_eq!(ok("— בָּרָא bara ; raíz prim.", "H1254").as_deref(), Some("בָּרָא"));
        assert_eq!(ok("CREAR bara˒ ( בָּרָא , H1254 )", "H1254").as_deref(), Some("בָּרָא"));
        assert_eq!(ok("בָּרָא׃ crear", "H1254").as_deref(), Some("בָּרָא"));
        // G3588 ὁ: one-letter headwords exist.
        assert_eq!(ok("ὁ ho; el artículo definido", "G3588").as_deref(), Some("ὁ"));
        // Kittel names the entry's own code right after the word; zero padding aside.
        assert_eq!(ok("ἀγάπη G26 [amor] , ἀγαπάω G25 [amar]", "G26").as_deref(), Some("ἀγάπη"));
        assert_eq!(ok("ἀγάπη G26 [amor]", "G00026").as_deref(), Some("ἀγάπη"));
        // LSJ (lsj+str-key.lexi) hyphenates the stem: «ἀγάπ-η, ἡ,» is ἀγάπη, not ἀγάπ.
        assert_eq!(ok("ἀγάπ-η, ἡ,\nI\n1. love, LXX Jer 2:2", "G26").as_deref(), Some("ἀγάπη"));
        assert_eq!(ok("λόγ-ος, ὁ, verbal noun", "G3056").as_deref(), Some("λόγος"));
        assert_eq!(ok("ἀγάπ-e", "G26"), None, "a Latin ending across the hyphen");
        // Diccionario Moises leaves the semicolon off its final-letter entities.
        assert_eq!(ok("(I) בר&#1488 QAL:\nCrear (Gen 1:1", "H1254").as_deref(), Some("ברא"));
        assert_eq!(
            ok("א\u{5b1}ל\u{5b9}ה\u{5b4}י&#1501 Ver", "H430").as_deref(),
            Some("א\u{5b1}ל\u{5b9}ה\u{5b4}ים")
        );
        // Swanson writes ἀγάπē with a Latin ē: the Greek run is not a whole word.
        assert_eq!(ok("Swanson 27 ἀγάπē (agapē), ēς (ēs)", "G26"), None);
        // Multilexico_Pro's mojibake «á̓γάπη» and Vine NT+'s «ἀדÜנח»: glued to other scripts.
        assert_eq!(ok("BDB-THAYERá\u{313}γα\u{301}πη agape\u{304}", "G26"), None);
        assert_eq!(ok("agape ( ἀדÜנח, G26), amor.", "G26"), None);
        // LXX / HOT concordances count forms; the first form is not the headword.
        assert_eq!(ok("Total LXX Occurrences: 15\nαγαπη (4)\nEcc 9:6", "G26"), None);
        assert_eq!(ok("Total HOT Occurrences: 55\nבורא (10)\nIsa 40:28", "H1254"), None);
        // Trench heads a synonym group with another number's word…
        let trench = "§ lxxvi. λαλέω[G2980], λέγω[G3004] (λαλιά[G2981], λόγος[G3056]).";
        assert_eq!(ok(trench, "G3056"), None);
        // …which is the headword of that other number.
        assert_eq!(ok(trench, "G2980").as_deref(), Some("λαλέω"));
        // Too far in: a citation inside a preface, not the headword.
        let late = format!("{} ἀγάπη", "Para una comprensión más detallada y profunda".repeat(2));
        assert_eq!(ok(&late, "G26"), None);
        // Wrong script for the code, or no original script at all.
        assert_eq!(ok("ἀγάπη agápe", "H26"), None);
        assert_eq!(ok("בָּרָא bara", "G1254"), None);
        assert_eq!(ok("Rom 5:5 amor , ágape ; Strong G26 : Una palabra", "G26"), None);
        assert_eq!(ok("", "G26"), None);
    }

    /// A throwaway module file. Module connections stay cached for the life of the test
    /// process, so the file is left in the temp folder rather than deleted.
    fn temp_module(name: &str, table: &str, rows: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join("espada-topic-index-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!(
            "{name}-{}-{}.{}",
            std::process::id(),
            INDEX_TICK.fetch_add(1, Ordering::Relaxed),
            if table == "Lexicon" { "lexi" } else { "dcti" }
        ));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE Details (Title TEXT, Abbreviation TEXT);\
             INSERT INTO Details VALUES ('Prueba', 'PRB');\
             CREATE TABLE [{table}] (Topic TEXT, Definition TEXT);\
             CREATE INDEX TopicIndex ON [{table}] (Topic);"
        ))
        .unwrap();
        for t in rows {
            conn.execute(
                &format!("INSERT INTO [{table}] (Topic, Definition) VALUES (?1, ?2)"),
                rusqlite::params![t, format!("Artículo de {t}")],
            )
            .unwrap();
        }
        path
    }

    fn topic_at(slice: &IndexSlice, pos: u32) -> &str {
        &slice.rows[(pos - slice.start) as usize].topic
    }

    #[test]
    fn windows_focus_and_exact_matches() {
        let mut topics: Vec<String> = [
            "ZARZA", "PINO", "PILATO, PONCIO", "PIEDRAS PRECIOSAS", "PIEDRA", "OVEJA", "ÑU",
            "NUBE", "AÑO", "ANZUELO", "AMOR", "AMÓN", "ABRAM", "ABOGADO",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        topics.extend((0..300).map(|i| format!("TEMA {i:03}")));
        // Junk e-Sword modules really contain: separators, blanks, markup, a duplicate.
        topics.extend(
            ["-----------", "\u{a0}\u{a0}", "<b>NEGRITA</b>", "AMOR"]
                .iter()
                .map(|s| s.to_string()),
        );
        let refs: Vec<&str> = topics.iter().map(|s| s.as_str()).collect();
        let path = temp_module("dict", "Dictionary", &refs);
        let p = path.to_str().unwrap();

        let all = topic_index(p, None, None, 0, 1000).unwrap();
        // 14 + 300 + NEGRITA + the second AMOR; separator and blank dropped.
        assert_eq!(all.total, 316);
        assert!(all.rows.iter().all(|r| !r.topic.is_empty()));
        assert!(all.rows.iter().any(|r| r.topic == "NEGRITA"));
        let amors: Vec<&IndexRow> = all.rows.iter().filter(|r| r.topic == "AMOR").collect();
        assert_eq!(amors.len(), 2, "both AMOR rows are listed");
        assert_ne!(amors[0].rowid, amors[1].rowid);
        assert!(!all.strong_keyed);
        assert_eq!((all.greek_start, all.hebrew_start), (None, None));

        // «pierda» is not a topic: focus is where it would go, between PIEDRAS and PILATO.
        let s = topic_index(p, Some("pierda"), None, 40, 80).unwrap();
        assert!(!s.exact);
        let f = s.focus.unwrap();
        assert_eq!(topic_at(&s, f), "PILATO, PONCIO");
        assert_eq!(topic_at(&s, f - 1), "PIEDRAS PRECIOSAS");

        // «amor» is: exact, any case, any accent.
        let s = topic_index(p, Some("Amor"), None, 40, 80).unwrap();
        assert!(s.exact);
        assert_eq!(topic_at(&s, s.focus.unwrap()), "AMOR");

        // Windows: before/after around the focus, clipped at both ends.
        let s = topic_index(p, Some("tema 200"), None, 40, 80).unwrap();
        let f = s.focus.unwrap();
        assert_eq!(s.start, f - 40);
        assert_eq!(s.rows.len(), 120);
        let s = topic_index(p, Some("abogado"), None, 40, 80).unwrap();
        assert_eq!((s.start, s.focus), (0, Some(0)));
        let s = topic_index(p, None, Some(all.total - 5), 0, 120).unwrap();
        assert_eq!((s.start, s.rows.len()), (all.total - 5, 5));
        let s = topic_index(p, Some("zzzz"), None, 40, 80).unwrap();
        assert_eq!((s.focus, s.exact), (Some(all.total), false));
        assert!(!s.rows.is_empty(), "the last rows show above the marker");

        // A click opens exactly that row's article.
        let piedra = all.rows.iter().find(|r| r.topic == "PIEDRA").unwrap();
        let e = topic_entry(p, piedra.rowid).unwrap().unwrap();
        assert_eq!(e.term, "PIEDRA");
        assert_eq!(e.definition, "Artículo de PIEDRA");
        assert_eq!((e.source.as_str(), e.title.as_str()), ("PRB", "Prueba"));
        assert!(topic_entry(p, 999_999).unwrap().is_none());

        // Changing the file rebuilds its index.
        {
            let conn = Connection::open(&path).unwrap();
            for i in 0..400 {
                conn.execute(
                    "INSERT INTO Dictionary (Topic, Definition) VALUES (?1, ?2)",
                    rusqlite::params![format!("NUEVO {i}"), "x".repeat(200)],
                )
                .unwrap();
            }
        }
        let after = topic_index(p, None, None, 0, 10).unwrap();
        assert_eq!(after.total, 716);
    }

    #[test]
    fn strong_lexicon_rows_carry_their_headword_and_sections() {
        let path = temp_module("lex", "Lexicon", &[]);
        {
            let conn = Connection::open(&path).unwrap();
            for (t, d) in [
                ("H1", "אָב ab; raíz; padre"),
                ("G2", "Ἀαρών Aarón; de origen hebreo"),
                ("G1", "<font color=\"#800000\">&#x0391;</font> alfa; primera letra"),
                ("H1254", "Diccionario Strong בָּרָא bara raíz primaria"),
                ("G26", "Swanson 27 ἀγάπē (agapē)"),
                ("Abreviaturas", "ἀγάπη en la tabla de abreviaturas"),
            ] {
                conn.execute(
                    "INSERT INTO Lexicon (Topic, Definition) VALUES (?1, ?2)",
                    rusqlite::params![t, d],
                )
                .unwrap();
            }
        }
        let p = path.to_str().unwrap();
        let s = topic_index(p, Some("h 01254"), None, 40, 80).unwrap();
        assert!(s.exact, "a spaced, zero-padded code is still the code");
        let topics: Vec<&str> = s.rows.iter().map(|r| r.topic.as_str()).collect();
        assert_eq!(topics, ["Abreviaturas", "G1", "G2", "G26", "H1", "H1254"]);
        assert!(s.strong_keyed);
        assert_eq!((s.greek_start, s.hebrew_start), (Some(1), Some(4)));
        let lemma = |t: &str| s.rows.iter().find(|r| r.topic == t).unwrap().lemma.clone();
        assert_eq!(lemma("G1"), "Α", "entities decode before the headword is read");
        assert_eq!(lemma("G2"), "Ἀαρών");
        assert_eq!(lemma("H1"), "אָב");
        assert_eq!(lemma("H1254"), "בָּרָא");
        assert_eq!(lemma("G26"), "", "Swanson's mixed-script word is not trusted");
        assert_eq!(lemma("Abreviaturas"), "", "ordinary topics have no headword");
    }

    #[test]
    fn unusable_modules_give_a_short_spanish_reason() {
        let dir = std::env::temp_dir().join("espada-topic-index-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let no_table = dir.join(format!("notable-{}.dcti", std::process::id()));
        Connection::open(&no_table)
            .unwrap()
            .execute_batch("CREATE TABLE IF NOT EXISTS Details (Title TEXT);")
            .unwrap();
        let err = topic_index(no_table.to_str().unwrap(), None, None, 40, 80).unwrap_err();
        assert!(err.contains("no tiene tabla"), "{err}");

        let no_topic = dir.join(format!("notopic-{}.lexi", std::process::id()));
        Connection::open(&no_topic)
            .unwrap()
            .execute_batch("CREATE TABLE IF NOT EXISTS Lexicon (Word TEXT, Definition TEXT);")
            .unwrap();
        let err = topic_index(no_topic.to_str().unwrap(), None, None, 40, 80).unwrap_err();
        assert!(err.contains("columna Topic"), "{err}");
    }

    fn installed(name: &str) -> Option<String> {
        let p = Path::new(ESWORD).join(name);
        if p.is_file() {
            Some(p.to_string_lossy().into_owned())
        } else {
            eprintln!("skip: {name} not installed");
            None
        }
    }

    #[test]
    fn real_strong_lexicon_index() {
        let Some(p) = installed("01 Diccionario strong.lexi") else {
            return;
        };
        let t0 = Instant::now();
        let s = topic_index(&p, Some("G26"), None, 40, 80).unwrap();
        eprintln!("01 Diccionario strong: {} entries, first window {:?}", s.total, t0.elapsed());
        assert!(s.exact);
        let row = &s.rows[(s.focus.unwrap() - s.start) as usize];
        assert_eq!((row.topic.as_str(), row.lemma.as_str()), ("G26", "ἀγάπη"));
        assert!(s.strong_keyed);
        assert!(s.greek_start.is_some() && s.hebrew_start.is_some());

        let s = topic_index(&p, Some("H1254"), None, 40, 80).unwrap();
        let row = &s.rows[(s.focus.unwrap() - s.start) as usize];
        assert_eq!(row.topic, "H1254");
        // Pointed bara: the letters ברא with their vowel points (modules differ in the order
        // they store dagesh and qamats, so compare letters and count the points).
        let letters: String = row.lemma.chars().filter(|c| is_hebrew_letter(*c)).collect();
        assert_eq!(letters, "ברא", "{:?}", row.lemma);
        assert_eq!(row.lemma.chars().filter(|c| is_hebrew_mark(*c)).count(), 3);
        let with_lemma = s.rows.iter().filter(|r| !r.lemma.is_empty()).count();
        assert!(with_lemma * 10 >= s.rows.len() * 9, "{with_lemma}/{} rows", s.rows.len());

        let e = topic_entry(&p, row.rowid).unwrap().unwrap();
        assert_eq!(e.term, "H1254");
        assert!(e.definition.contains("crear"), "{}", e.definition);
    }

    #[test]
    fn real_modules_with_odd_topics() {
        // Zero-padded codes: a G26 query lands on G00026.
        if let Some(p) = installed("na26-.lexi") {
            let s = topic_index(&p, Some("G26"), None, 0, 1).unwrap();
            assert!(s.exact);
            assert_eq!(s.rows[0].topic, "G00026");
        }
        // LSJ hyphenates its stems («ἀγάπ-η»): the row still reads ἀγάπη.
        if let Some(p) = installed("lsj+str-key.lexi") {
            let s = topic_index(&p, Some("G26"), None, 0, 1).unwrap();
            assert_eq!((s.rows[0].topic.as_str(), s.rows[0].lemma.as_str()), ("G26", "ἀγάπη"));
        }
        // Every code twice: both rows listed, each opens its own article.
        if let Some(p) = installed("StrongsPlus.lexi") {
            let s = topic_index(&p, Some("G1"), None, 0, 2).unwrap();
            assert_eq!(s.rows.len(), 2);
            assert_eq!((s.rows[0].topic.as_str(), s.rows[1].topic.as_str()), ("G1", "G1"));
            assert_ne!(s.rows[0].rowid, s.rows[1].rowid);
        }
        // A 14 664-character topic is cut for the list.
        if let Some(p) = installed("01LBLA.dcti") {
            let total = topic_index(&p, None, None, 0, 0).unwrap().total;
            let longest = (0..total)
                .step_by(1000)
                .flat_map(|a| topic_index(&p, None, Some(a), 0, 1000).unwrap().rows)
                .map(|r| r.topic.chars().count())
                .max()
                .unwrap_or(0);
            assert!(longest <= TOPIC_MAX_CHARS + 1, "longest topic shown: {longest} chars");
        }
        // Bare numbers are not Strong's codes.
        if let Some(p) = installed("nsbdt.lexi") {
            let s = topic_index(&p, Some("1"), None, 0, 3).unwrap();
            assert!(s.rows.iter().all(|r| r.lemma.is_empty()));
        }
        if let Some(p) = installed("strong-PLUS.lexi") {
            let err = topic_index(&p, None, None, 40, 80).unwrap_err();
            assert!(err.contains("Topic"), "{err}");
        }
        if let Some(p) = installed("CONSTRONGES.dcti") {
            let err = topic_index(&p, None, None, 40, 80).unwrap_err();
            assert!(err.contains("tabla"), "{err}");
        }
    }

    #[test]
    fn real_largest_dictionary_builds_quickly() {
        let Some(p) = installed("webster-1913.dcti") else {
            return;
        };
        let t0 = Instant::now();
        let s = topic_index(&p, Some("love"), None, 40, 80).unwrap();
        let cold = t0.elapsed();
        let t1 = Instant::now();
        let again = topic_index(&p, Some("lovely"), None, 40, 80).unwrap();
        let warm = t1.elapsed();
        eprintln!("webster-1913: {} topics, cold {cold:?}, warm {warm:?}", s.total);
        assert!(s.total > 100_000);
        assert!(s.exact && again.exact);
        // Debug builds are several times slower than the release app; this only catches a
        // pathological regression (a quadratic sort, a per-row query).
        assert!(cold.as_secs() < 20, "cold build took {cold:?}");
        assert!(warm.as_millis() < 500, "cached window took {warm:?}");
    }

    /// What headword each installed Strong's module gives for a few well-known codes, to spot
    /// module conventions that would show a wrong word. Report only:
    /// `cargo test --lib -- --ignored headword_survey --nocapture`.
    #[test]
    #[ignore]
    fn headword_survey() {
        let Ok(dir) = std::fs::read_dir(ESWORD) else {
            return;
        };
        let codes = ["G1", "G26", "G2316", "G3056", "G3588", "H1", "H430", "H1254", "H3068"];
        let mut seen: Vec<HashMap<String, Vec<String>>> = vec![HashMap::new(); codes.len()];
        let mut coverage: Vec<(String, usize, usize)> = Vec::new();
        for entry in dir.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if !matches!(ext.as_str(), "dcti" | "lexi") {
                continue;
            }
            let p = path.to_string_lossy().into_owned();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let Ok(first) = topic_index(&p, Some("G1"), None, 0, 120) else {
                continue;
            };
            if !first.strong_keyed {
                continue;
            }
            let strong_rows = first.rows.iter().filter(|r| strong_prefix(&r.topic).is_some()).count();
            let with = first.rows.iter().filter(|r| !r.lemma.is_empty()).count();
            coverage.push((name.clone(), with, strong_rows));
            for (i, code) in codes.iter().enumerate() {
                if let Ok(s) = topic_index(&p, Some(code), None, 0, 1) {
                    if s.exact {
                        let l = s.rows[0].lemma.clone();
                        seen[i].entry(if l.is_empty() { "—".into() } else { l }).or_default().push(name.clone());
                    }
                }
            }
        }
        for (i, code) in codes.iter().enumerate() {
            let mut v: Vec<(&String, &Vec<String>)> = seen[i].iter().collect();
            v.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
            eprintln!("{code}:");
            for (lemma, mods) in v {
                let sample: Vec<&str> = mods.iter().take(3).map(|s| s.as_str()).collect();
                eprintln!("    {lemma:<14} ×{:<3} {}", mods.len(), sample.join(" · "));
            }
        }
        coverage.sort_by(|a, b| (a.1 * 1000 / a.2.max(1)).cmp(&(b.1 * 1000 / b.2.max(1))));
        eprintln!("lowest headword coverage in the G1 window:");
        for (name, with, total) in coverage.iter().take(14) {
            eprintln!("    {with:>3}/{total:<3} {name}");
        }
    }

    /// Every installed dictionary and lexicon either indexes or explains why not — never a
    /// panic. Slow in debug builds: `cargo test --lib -- --ignored every_installed`.
    #[test]
    #[ignore]
    fn every_installed_dictionary_and_lexicon_indexes() {
        let Ok(dir) = std::fs::read_dir(ESWORD) else {
            eprintln!("skip: no e-Sword folder");
            return;
        };
        let (mut ok, mut failed, mut topics) = (0, 0, 0u64);
        let mut slowest = (std::time::Duration::ZERO, String::new());
        for entry in dir.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(ext.as_str(), "dcti" | "dctx" | "lexi" | "lexx") {
                continue;
            }
            let p = path.to_string_lossy().into_owned();
            let t0 = Instant::now();
            match topic_index(&p, Some("amor"), None, 40, 80) {
                Ok(s) => {
                    ok += 1;
                    topics += s.total as u64;
                    if let Some(r) = s.rows.first() {
                        topic_entry(&p, r.rowid).expect("entry of a listed row");
                    }
                }
                Err(e) => {
                    failed += 1;
                    eprintln!("  {}: {e}", path.file_name().unwrap().to_string_lossy());
                }
            }
            let dt = t0.elapsed();
            if dt > slowest.0 {
                slowest = (dt, p);
            }
        }
        eprintln!("indexed {ok} modules ({topics} topics), {failed} explained; slowest {slowest:?}");
        assert!(ok > 0);
    }

    /// The ● probe tries the same spellings of a Strong's code as the lookup, so a module that
    /// stores G00026 or H0430 is marked for G26 / H430.
    #[test]
    fn lexicon_probe_matches_zero_padded_codes_like_the_lookup() {
        use crate::esword::reader::{lookup_lexicon, probe_lexicon};
        let path = temp_module("padded", "Lexicon", &["G00026", "H0430", "G27"]);
        let p = path.to_string_lossy().into_owned();
        for (term, expect) in [("G26", true), ("g0026", true), ("H430", true), ("G27", true), ("G28", false)] {
            let found = !lookup_lexicon(&p, term, 20).unwrap().is_empty();
            assert_eq!(found, expect, "lookup {term}");
            assert_eq!(probe_lexicon(&p, term).has_content, expect, "probe {term}");
        }
    }
}
