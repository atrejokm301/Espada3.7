//! Section titles inside an entry: where a combined module moves on to another work or author
//! (Multiléxico: «Diccionario Strong», «Léxico Griego-Español Tuggy»…; Catena Aurea: «San
//! Agustín, de sermone Domini, 1,1»; LBLA: «Notas al pie… (1997)» / «Notas… de Estudio (2000)»),
//! and the headings of a single work («REFERENCIAS CRUZADAS», «NOTAS»).
//!
//! The page shows module text without its formatting, so these titles read like any other
//! line. In the markup they are whole lines in bold (or a heading tag) — in the Catena, an
//! author in bold followed by the work in italics — and they come back entry after entry. So
//! each module's titles are learned once per session from a sample of its entries: bold lines
//! that recur in at least 3 % of them. The markup those titles wear (centred teal bold, `<h1>`,
//! bold + underline, purple bold…) then marks the rarer ones too — the Fathers the Catena
//! quotes once in a while, Barclay in the Multiléxico. Plain bold is also worn by headwords and
//! labels, so it only reaches titles of the same shape: ALL CAPS (Mundo Hispano's
//! «ARQUEOLÓGICAS») or an outline letter («B. Verbo» in Vine).
//!
//! On the display paths (Léxico, Diccionario, Índice, Comentario) a title line is wrapped in
//! TITLE_OPEN…TITLE_CLOSE (an author line: TITLE_OPEN author TITLE_SPLIT work TITLE_CLOSE)
//! and the page draws a section heading. Probes, searches, the Strong's glosses and the word
//! table keep reading plain `clean_text`.

use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::types::Value;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use super::reader::{column_set_cached, table_exists_cached};
use super::text::{
    cell_to_text, clean_text, clean_text_emphasis, tidy_whitespace, EM_BOLD_CLOSE, EM_BOLD_OPEN,
    EM_ITALIC_CLOSE, EM_ITALIC_OPEN, EM_SIG_BASE, TITLE_CLOSE, TITLE_OPEN, TITLE_SPLIT,
};

/// Entries read to learn a module's titles: spread evenly over the main table, plus a few from
/// the other commentary tables (chapter and book notes).
const SAMPLE_MAIN: i64 = 300;
const SAMPLE_OTHER: i64 = 50;
/// A title recurs in at least this share of the sampled entries, and in at least 3 of them.
const RECUR_SHARE: f64 = 0.03;
const RECUR_MIN: usize = 3;
const MAX_TITLE_CHARS: usize = 90;
const MAX_TITLE_WORDS: usize = 12;

const TEXT_TABLES: [(&str, &str); 5] = [
    ("Lexicon", "Definition"),
    ("Dictionary", "Definition"),
    ("VerseCommentary", "Comments"),
    ("ChapterCommentary", "Comments"),
    ("BookCommentary", "Comments"),
];

/// Markup that sets a run apart from plain bold: a colour, centring, underline, a heading tag,
/// a size, a background, small caps, a class.
const DISTINCT_MARKUP: [&str; 15] = [
    "color", "center", "underline", "<u>", "<u ", "<h1", "<h2", "<h3", "<h4", "<h5", "<h6",
    "size", "background", "small-caps", "class=",
];

static RE_OUTLINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?:[A-Z]|[IVXLC]{1,4}|\d{1,2})[.)]\s+\S").unwrap());
static RE_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)https?:|www\.|\.(?:com|org|net)\b").unwrap());
/// «NO HAY DEFINICION EN ESTE LEXICO…» is the whole entry, not a section.
static RE_NOTICE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^no (?:hay|tiene|existe)\b").unwrap());
/// «Número: Singular», «Part of Speech: verb» — a field with its value, not a title.
static RE_FIELD: Lazy<Regex> = Lazy::new(|| Regex::new(r":\s+\S").unwrap());

static RULES: Lazy<Mutex<HashMap<String, Arc<TitleRules>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
enum Shape {
    Caps,
    CapsLabel,
    Outline,
    OutlineLabel,
}

/// What marks a section title in one module.
#[derive(Default, Debug)]
pub(crate) struct TitleRules {
    /// Titles seen recurring, by their text.
    texts: HashSet<String>,
    /// Distinctive markup those titles wear: any whole line in it is a title.
    markup: HashSet<String>,
    /// Plain bold that titles wear, with their shape: whole lines of that shape are titles.
    shaped: HashSet<(String, Shape)>,
}

impl TitleRules {
    pub(crate) fn is_empty(&self) -> bool {
        self.texts.is_empty() && self.markup.is_empty() && self.shaped.is_empty()
    }

    fn matches(&self, c: &Candidate, sigs: &[String]) -> bool {
        if !titlelike(&c.key) {
            return false;
        }
        if self.texts.contains(&c.key) {
            return true;
        }
        let Some(sig) = c.sig.and_then(|n| sigs.get(n)) else {
            return false;
        };
        if self.markup.contains(sig) {
            return true;
        }
        shape(&c.key).is_some_and(|sh| self.shaped.contains(&(sig.clone(), sh)))
    }
}

/// A line that is bold all through, or bold then italic («San Agustín, de sermone Domini»).
#[derive(Debug)]
struct Candidate {
    /// The whole line, as the page shows it.
    text: String,
    /// Bold-then-italic lines: the bold part and the rest.
    lead: Option<(String, String)>,
    /// What recurs: the whole line, or the bold part without its trailing comma.
    key: String,
    /// The markup that opened the first bold run (index into the entry's signature list).
    sig: Option<usize>,
}

fn is_sig_char(c: char) -> bool {
    (EM_SIG_BASE..=0xFFFFD).contains(&(c as u32))
}

fn is_marker(c: char) -> bool {
    matches!(c, EM_BOLD_OPEN | EM_BOLD_CLOSE | EM_ITALIC_OPEN | EM_ITALIC_CLOSE) || is_sig_char(c)
}

fn strip_emphasis(line: &str) -> String {
    line.chars().filter(|&c| !is_marker(c)).collect()
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn candidate(line: &str) -> Option<Candidate> {
    let line = line.trim_start();
    let mut it = line.chars();
    if it.next() != Some(EM_BOLD_OPEN) {
        return None;
    }
    let sig = it
        .next()
        .filter(|&c| is_sig_char(c))
        .map(|c| (c as u32 - EM_SIG_BASE) as usize);
    let (mut bold, mut italic) = (0usize, 0usize);
    let mut first_run_done = false;
    let (mut text, mut lead, mut rest) = (String::new(), String::new(), String::new());
    let mut rest_has_italic = false;
    for ch in line.chars() {
        match ch {
            EM_BOLD_OPEN => {
                bold += 1;
                continue;
            }
            EM_BOLD_CLOSE => {
                bold = bold.saturating_sub(1);
                if bold == 0 {
                    first_run_done = true;
                }
                continue;
            }
            EM_ITALIC_OPEN => {
                italic += 1;
                continue;
            }
            EM_ITALIC_CLOSE => {
                italic = italic.saturating_sub(1);
                continue;
            }
            c if is_sig_char(c) => continue,
            _ => {}
        }
        text.push(ch);
        if first_run_done {
            rest.push(ch);
        } else {
            lead.push(ch);
        }
        if bold > 0 || ch.is_whitespace() || ",.;:-–—()[]".contains(ch) {
            continue;
        }
        if italic > 0 {
            rest_has_italic = true;
            continue;
        }
        return None; // plain text on the line: a paragraph that opens in bold
    }
    let text = squash(&text);
    if text.is_empty() {
        return None;
    }
    let lead = squash(&lead);
    if !rest_has_italic || lead.is_empty() || lead == text {
        return Some(Candidate { key: text.clone(), text, lead: None, sig });
    }
    if lead.ends_with(':') {
        return None; // «Etiqueta:» followed by its value in italics
    }
    let key = lead.trim_end_matches([',', ';', ' ']).to_string();
    Some(Candidate { text, lead: Some((lead, squash(&rest))), key, sig })
}

fn has_word_of_three(t: &str) -> bool {
    let mut run = 0;
    for c in t.chars() {
        if c.is_alphabetic() {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn is_greek_or_hebrew(c: char) -> bool {
    matches!(c as u32, 0x0370..=0x03FF | 0x1F00..=0x1FFF | 0x0590..=0x05FF)
}

/// Could be a section title: short, starts in capitals or a number, has a real word, no
/// Greek/Hebrew (a headword), no web address, not «Campo: valor», not a «no hay definición».
fn titlelike(t: &str) -> bool {
    let chars = t.chars().count();
    if !(2..=MAX_TITLE_CHARS).contains(&chars) || t.split_whitespace().count() > MAX_TITLE_WORDS {
        return false;
    }
    let start = t.trim_start_matches(['¿', '¡', '«', '“', '"', '\'', '(']);
    match start.chars().next() {
        Some(c) if c.is_uppercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    has_word_of_three(t)
        && !t.chars().any(is_greek_or_hebrew)
        && !RE_URL.is_match(t)
        && !RE_NOTICE.is_match(t)
        && !RE_FIELD.is_match(t)
}

fn shape(t: &str) -> Option<Shape> {
    let label = t.trim_end().ends_with(':');
    let (mut letters, mut upper) = (0usize, 0usize);
    for c in t.chars().filter(|c| c.is_alphabetic()) {
        letters += 1;
        if c.is_uppercase() {
            upper += 1;
        }
    }
    if letters >= 3 && upper == letters {
        return Some(if label { Shape::CapsLabel } else { Shape::Caps });
    }
    if RE_OUTLINE.is_match(t) {
        return Some(if label { Shape::OutlineLabel } else { Shape::Outline });
    }
    None
}

fn distinctive(sig: &str) -> bool {
    DISTINCT_MARKUP.iter().any(|m| sig.contains(m))
}

/// Indexes of the lines of a marked entry that have something to read.
fn content_lines(lines: &[&str]) -> Vec<usize> {
    (0..lines.len())
        .filter(|&i| !strip_emphasis(lines[i]).trim().is_empty())
        .collect()
}

/// Raw text of entries spread evenly over the module (rowid list: no table scan).
fn sample_entries(conn: &Connection, path_key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut tables = 0;
    for (table, column) in TEXT_TABLES {
        if !table_exists_cached(conn, path_key, table) {
            continue;
        }
        let cols = column_set_cached(conn, path_key, table);
        let Some(col) = cols.iter().find(|c| c.eq_ignore_ascii_case(column)).cloned() else {
            continue;
        };
        let want = if tables == 0 { SAMPLE_MAIN } else { SAMPLE_OTHER };
        tables += 1;
        let bounds = conn.query_row(
            &format!("SELECT min(rowid), max(rowid) FROM [{table}]"),
            [],
            |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?)),
        );
        let Ok((Some(lo), Some(hi))) = bounds else {
            continue;
        };
        let span = i128::from(hi) - i128::from(lo);
        let mut ids: Vec<i64> = (0..want)
            .map(|k| (i128::from(lo) + span * i128::from(k) / i128::from((want - 1).max(1))) as i64)
            .collect();
        ids.dedup();
        let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT [{col}] FROM [{table}] WHERE rowid IN ({list})");
        let Ok(mut stmt) = conn.prepare(&sql) else {
            continue;
        };
        let Ok(rows) = stmt.query_map([], |r| r.get::<_, Value>(0)) else {
            continue;
        };
        for v in rows.flatten() {
            if let Value::Text(s) = v {
                if !s.trim().is_empty() {
                    out.push(s);
                }
            }
        }
    }
    out
}

fn learn(entries: &[String]) -> TitleRules {
    struct Seen {
        entries: usize,
        sigs: HashMap<String, usize>,
    }
    let mut seen: HashMap<String, Seen> = HashMap::new();
    for raw in entries {
        let mut sigs = Vec::new();
        let marked = clean_text_emphasis(raw, &mut sigs);
        let lines: Vec<&str> = marked.split('\n').collect();
        let content = content_lines(&lines);
        let mut here = HashSet::new();
        // Never the last line: footers («Diccionario de filosofía en CD-ROM.»), signatures
        // («d.f.w.»), an entry that is only «NO HAY DEFINICION…».
        for &i in content.iter().take(content.len().saturating_sub(1)) {
            let Some(c) = candidate(lines[i]) else {
                continue;
            };
            if !titlelike(&c.key) || !here.insert(c.key.clone()) {
                continue;
            }
            let s = seen
                .entry(c.key)
                .or_insert_with(|| Seen { entries: 0, sigs: HashMap::new() });
            s.entries += 1;
            if let Some(sig) = c.sig.and_then(|n| sigs.get(n)) {
                *s.sigs.entry(sig.clone()).or_default() += 1;
            }
        }
    }
    let need = ((entries.len() as f64 * RECUR_SHARE).ceil() as usize).max(RECUR_MIN);
    let mut rules = TitleRules::default();
    for (key, s) in seen {
        if s.entries < need {
            continue;
        }
        // Every markup that carries this title in a fair share of its appearances.
        let bar = (s.entries / 5).max(2);
        for (sig, n) in &s.sigs {
            if *n < bar && s.sigs.len() > 1 {
                continue;
            }
            if distinctive(sig) {
                rules.markup.insert(sig.clone());
            } else if let Some(sh) = shape(&key) {
                rules.shaped.insert((sig.clone(), sh));
            }
        }
        rules.texts.insert(key);
    }
    rules
}

// ----- learned rules on disk -----
// Learning reads ~300 entries: 50–100 ms for the Multiléxico or the Catena, up to ~350 ms for
// the biggest English commentaries on a cold disk. Kept in
// %LOCALAPPDATA%\asignacion-del-cielo-bible\section-titles.json (beside the module catalog)
// so each module pays it once, not once per session; a module whose file changes is learned
// again.

/// Bump when the learning changes: rules saved by an older version are learned again.
const RULES_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Default)]
struct SavedRules {
    version: u32,
    modules: HashMap<String, SavedModule>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SavedModule {
    mtime_ms: u64,
    size: u64,
    texts: Vec<String>,
    markup: Vec<String>,
    shaped: Vec<(String, Shape)>,
}

static SAVED: Lazy<Mutex<Option<SavedRules>>> = Lazy::new(|| Mutex::new(None));

fn saved_path() -> Option<PathBuf> {
    if cfg!(test) {
        return None; // tests never touch the user's cache
    }
    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA"))?;
    Some(PathBuf::from(base).join("asignacion-del-cielo-bible").join("section-titles.json"))
}

fn fingerprint(path: &Path) -> Option<(u64, u64)> {
    let meta = path.metadata().ok()?;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64);
    Some((mtime_ms, meta.len()))
}

fn with_saved<T>(f: impl FnOnce(&mut SavedRules) -> T) -> Option<T> {
    let mut slot = SAVED.lock().ok()?;
    if slot.is_none() {
        let loaded = saved_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|raw| serde_json::from_str::<SavedRules>(&raw).ok())
            .filter(|s| s.version == RULES_VERSION);
        *slot = Some(loaded.unwrap_or(SavedRules { version: RULES_VERSION, ..Default::default() }));
    }
    slot.as_mut().map(f)
}

fn load_saved(path_key: &str) -> Option<TitleRules> {
    let (mtime_ms, size) = fingerprint(Path::new(path_key))?;
    let m = with_saved(|s| s.modules.get(path_key).cloned())??;
    if m.mtime_ms != mtime_ms || m.size != size {
        return None;
    }
    Some(TitleRules {
        texts: m.texts.into_iter().collect(),
        markup: m.markup.into_iter().collect(),
        shaped: m.shaped.into_iter().collect(),
    })
}

fn save(path_key: &str, rules: &TitleRules) {
    let (Some(file), Some((mtime_ms, size))) = (saved_path(), fingerprint(Path::new(path_key))) else {
        return;
    };
    let sorted = |v: &HashSet<String>| {
        let mut v: Vec<String> = v.iter().cloned().collect();
        v.sort();
        v
    };
    let mut shaped: Vec<(String, Shape)> = rules.shaped.iter().cloned().collect();
    shaped.sort_by(|a, b| a.0.cmp(&b.0).then(format!("{:?}", a.1).cmp(&format!("{:?}", b.1))));
    let module = SavedModule { mtime_ms, size, texts: sorted(&rules.texts), markup: sorted(&rules.markup), shaped };
    let json = with_saved(|s| {
        s.modules.insert(path_key.to_string(), module);
        serde_json::to_string(s).ok()
    })
    .flatten();
    if let Some(json) = json {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Write then rename: a crash mid-write never leaves half a file behind.
        let tmp = file.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &file);
        }
    }
}

/// The module's title rules: from memory, else from disk (same file size and date), else
/// learned now from a sample of its entries and saved.
pub(crate) fn title_rules(conn: &Connection, path_key: &str) -> Arc<TitleRules> {
    if let Some(r) = RULES.lock().ok().and_then(|m| m.get(path_key).cloned()) {
        return r;
    }
    let rules = Arc::new(match load_saved(path_key) {
        Some(saved) => saved,
        None => {
            let learned = learn(&sample_entries(conn, path_key));
            save(path_key, &learned);
            learned
        }
    });
    if let Ok(mut m) = RULES.lock() {
        m.insert(path_key.to_string(), rules.clone());
    }
    rules
}

/// `clean_text` for the study panels: the same text, with title lines wrapped in TITLE markers.
pub(crate) fn display_text(raw: &str, rules: &TitleRules) -> String {
    if rules.is_empty() {
        return clean_text(raw);
    }
    let mut sigs = Vec::new();
    let marked = clean_text_emphasis(raw, &mut sigs);
    let lines: Vec<&str> = marked.split('\n').collect();
    let last = content_lines(&lines).last().copied();
    let mut out = String::with_capacity(marked.len() + 32);
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let title = if Some(i) == last {
            None
        } else {
            candidate(line).filter(|c| rules.matches(c, &sigs))
        };
        match title {
            Some(c) => {
                out.push_str(TITLE_OPEN);
                match &c.lead {
                    Some((lead, rest)) => {
                        out.push_str(lead);
                        out.push_str(TITLE_SPLIT);
                        out.push_str(rest);
                    }
                    None => out.push_str(&c.text),
                }
                out.push_str(TITLE_CLOSE);
            }
            None => out.push_str(&strip_emphasis(line)),
        }
    }
    tidy_whitespace(&out)
}

/// A module's titles for one lookup, learned only when a row is actually shown: a lookup that
/// finds nothing never samples the module.
pub(crate) struct Titles<'a> {
    conn: &'a Connection,
    path_key: &'a str,
    rules: std::cell::OnceCell<Arc<TitleRules>>,
}

impl<'a> Titles<'a> {
    pub(crate) fn new(conn: &'a Connection, path_key: &'a str) -> Self {
        Titles { conn, path_key, rules: std::cell::OnceCell::new() }
    }

    /// The cell as the study panel shows it (see `display_value`).
    pub(crate) fn display(&self, val: &Value, min_len: usize) -> Option<String> {
        let rules = self.rules.get_or_init(|| title_rules(self.conn, self.path_key));
        display_value(val, rules, min_len)
    }
}

/// A study-panel cell as the page shows it: `display_text` for module text, the usual
/// `cell_to_text` otherwise (blobs, «[Módulo cifrado…]» notices, kept even when short).
/// Shorter than `min_len` bytes: nothing to show.
pub(crate) fn display_value(val: &Value, rules: &TitleRules, min_len: usize) -> Option<String> {
    if let (Value::Text(raw), false) = (val, rules.is_empty()) {
        let text = display_text(raw, rules);
        return (text.len() >= min_len).then_some(text);
    }
    let text = cell_to_text(val)?;
    if text.starts_with("[Módulo cifrado") || text.starts_with("[Contenido binario") {
        return Some(text);
    }
    (text.len() >= min_len).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules_from(entries: &[&str]) -> TitleRules {
        learn(&entries.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    fn titles(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(a) = rest.find(TITLE_OPEN) {
            let after = &rest[a + TITLE_OPEN.len()..];
            let b = after.find(TITLE_CLOSE).expect("closed title");
            out.push(after[..b].replace(TITLE_SPLIT, " | "));
            rest = &after[b + TITLE_CLOSE.len()..];
        }
        out
    }

    fn plain(text: &str) -> String {
        text.replace([TITLE_OPEN, TITLE_CLOSE].concat().as_str(), "")
            .chars()
            .filter(|c| !matches!(*c as u32, 0xE002..=0xE004))
            .collect()
    }

    #[test]
    fn candidates_are_whole_bold_lines_or_bold_then_italic() {
        let mut sigs = Vec::new();
        let m = clean_text_emphasis(
            "<p><b>Glosa</b></p><p><b>San Agustín, </b><i>de sermone Domini, 1,1</i></p>\
             <p><b>Ojo:</b> texto normal</p><p><b>Etiqueta:</b> <i>valor</i></p>",
            &mut sigs,
        );
        let lines: Vec<&str> = m.split('\n').collect();
        let c0 = candidate(lines[0]).expect("Glosa");
        assert_eq!((c0.text.as_str(), c0.lead.is_none()), ("Glosa", true));
        let c1 = candidate(lines[1]).expect("author line");
        assert_eq!(c1.key, "San Agustín");
        assert_eq!(
            c1.lead,
            Some(("San Agustín,".to_string(), "de sermone Domini, 1,1".to_string()))
        );
        assert!(candidate(lines[2]).is_none(), "bold label + plain text");
        assert!(candidate(lines[3]).is_none(), "label: value");
    }

    #[test]
    fn titlelike_keeps_titles_and_drops_headwords_fields_and_notices() {
        for t in ["Diccionario Strong", "BDB-THAYER", "San Agustín", "NOTAS", "Thayer Definición:",
                  "A. Nombre", "¿Quién, dónde y cuándo?", "CAPITULO 1", "Notas al pie De La Biblia de las Americas (1997)"] {
            assert!(titlelike(t), "{t}");
        }
        for t in ["ab", "about, 1", "d.f.w.", "I.", "(a)", "Ἀγρίππας", "otros usos", "Número: Singular",
                  "NO HAY DEFINICION EN ESTE LEXICO DE W. BARCLAY", "http://biblia.com/diccionario/"] {
            assert!(!titlelike(t), "{t}");
        }
    }

    #[test]
    fn markup_of_recurring_titles_marks_the_rare_ones_too() {
        let title = |t: &str| format!(r#"<p style="text-align:center;"><span style="color:#008080;font-weight:bold;">{t}</span></p>"#);
        let word = |w: &str| format!(r#"<p><span style="color:#800000;font-weight:bold;">{w}</span></p>"#);
        let mut entries = Vec::new();
        for i in 0..40 {
            entries.push(format!(
                "{}{}<p>definición {i}</p>{}<p>uso {i}</p>",
                title("Diccionario Strong"),
                word(&format!("Palabra{i}")),
                title("Concordancia de Hebreo y Griego")
            ));
        }
        let rules = learn(&entries);
        assert!(rules.texts.contains("Diccionario Strong"));
        assert!(!rules.texts.iter().any(|t| t.starts_with("Palabra")), "headwords never recur");
        // An entry with a section the sample never saw.
        let raw = format!(
            "{}{}<p>definición</p>{}<p>Barclay dice…</p>",
            title("Diccionario Strong"),
            word("Ἀγρίππας"),
            title("Palabras Griegas del Nuevo Testamento (Barclay)")
        );
        let shown = display_text(&raw, &rules);
        assert_eq!(
            titles(&shown),
            ["Diccionario Strong", "Palabras Griegas del Nuevo Testamento (Barclay)"]
        );
        assert_eq!(plain(&shown), clean_text(&raw), "same text as before, plus the markers");
    }

    #[test]
    fn plain_bold_only_reaches_titles_of_the_same_shape() {
        let mut entries = Vec::new();
        for i in 0..40 {
            entries.push(format!(
                "<span style=\"font-weight:bold;\">HERMENÉUTICAS</span><br/>texto {i}\
                 <br/><span style=\"font-weight:bold;\">Swanson {i}</span><br/>más {i}"
            ));
        }
        let rules = learn(&entries);
        let raw = "<span style=\"font-weight:bold;\">ARQUEOLÓGICAS</span><br/>La evidencia…\
                   <br/><span style=\"font-weight:bold;\">Swanson 99</span><br/>fin";
        assert_eq!(titles(&display_text(raw, &rules)), ["ARQUEOLÓGICAS"]);
    }

    #[test]
    fn the_last_line_is_never_a_title() {
        let mut entries = Vec::new();
        for i in 0..40 {
            entries.push(format!("<p>texto {i}</p><p><b>Diccionario de filosofía en CD-ROM.</b></p>"));
        }
        assert!(rules_from(&entries.iter().map(String::as_str).collect::<Vec<_>>()).is_empty());
    }

    #[test]
    fn headings_and_rules_break_the_line() {
        let mut sigs = Vec::new();
        let m = clean_text_emphasis("<h1>BDB-THAYER</h1><p><grk>Ἀγρίππας</grk> Agrippas</p><hr><h1>SWANSON</h1>x", &mut sigs);
        let lines: Vec<String> = m.split('\n').map(strip_emphasis).filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines, ["BDB-THAYER", "Ἀγρίππας Agrippas", "SWANSON", "x"]);
    }

    // ----- real modules (skipped when e-Sword isn't installed) -----

    const ESWORD: &str = r"C:\Program Files (x86)\e-Sword";

    fn shown(file: &str, sql: &str) -> Option<String> {
        let path = std::path::Path::new(ESWORD).join(file);
        if !path.exists() {
            eprintln!("skip: {file} not installed");
            return None;
        }
        let path_key = path.to_string_lossy().to_string();
        let conn = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        let rules = title_rules(&conn, &path_key);
        let raw: String = conn.query_row(sql, [], |r| r.get(0)).ok()?;
        Some(display_text(&raw, &rules))
    }

    #[test]
    fn multilexico_g67_sections() {
        let Some(t) = shown("01 Multilexico de idiomas biblicos en espanol.lexi",
            "SELECT Definition FROM Lexicon WHERE Topic='G67'") else { return };
        assert_eq!(
            titles(&t),
            ["Diccionario Strong", "Diccionario de Griego Bíblico Swanson", "Léxico Griego-Español Tuggy",
             "Concordancia de Hebreo y Griego"]
        );
        assert!(t.contains("Swanson 68") && !titles(&t).iter().any(|x| x.contains("Swanson 68")));
    }

    #[test]
    fn multilexico_pro_sections_and_labels() {
        let Some(t) = shown("Multilexico_Pro.lexi", "SELECT Definition FROM Lexicon WHERE Topic='G67'") else { return };
        let got = titles(&t);
        for want in ["BDB-THAYER", "Thayer Definición:", "StrongPLUS", "Definición Strong:", "SWANSON", "TUGGY"] {
            assert!(got.iter().any(|g| g == want), "{want} in {got:?}");
        }
        assert!(!t.contains("BDB-THAYERἈ"), "the heading has its own line");
    }

    #[test]
    fn catena_marks_every_father() {
        let path = std::path::Path::new(ESWORD).join("catena_aurea.cmti");
        if !path.exists() {
            return;
        }
        let conn = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let rules = title_rules(&conn, &path.to_string_lossy());
        let raw: String = conn
            .query_row("SELECT Comments FROM VerseCommentary WHERE Book=40 AND ChapterBegin=5 AND VerseBegin=1", [], |r| r.get(0))
            .unwrap();
        // Every author paragraph (purple or red bold lead) in Mt 5:1.
        let authors = Regex::new(r#"(?is)<span style="color:#(?:804DB3|B34D4D);font-weight:bold;">(.*?)</span>"#).unwrap();
        let want = authors.captures_iter(&raw).count();
        let t = display_text(&raw, &rules);
        let got = titles(&t);
        assert!(want >= 10, "sample entry has {want} author lines");
        assert_eq!(got.len(), want, "{got:#?}");
        assert!(got.iter().any(|g| g.starts_with("San Agustín, | de ")), "{got:#?}");
    }

    #[test]
    fn lbla_notes_say_which_edition() {
        let Some(t) = shown("01LBLA.cmti",
            "SELECT Comments FROM VerseCommentary WHERE Book=43 AND ChapterBegin=3 AND VerseBegin=1 LIMIT 1") else { return };
        let got = titles(&t);
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(got[0].starts_with("Notas"), "{got:?}");
    }

    /// What learning costs on the biggest modules: `cargo test --release --lib -- --ignored
    /// learn_cost --nocapture`.
    #[test]
    #[ignore]
    fn learn_cost() {
        let dir = std::path::Path::new(ESWORD);
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut files: Vec<(u64, std::path::PathBuf)> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| matches!(p.extension().and_then(|x| x.to_str()), Some("cmti" | "dcti" | "lexi")))
            .map(|p| (std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0), p))
            .collect();
        files.sort_by(|a, b| b.0.cmp(&a.0));
        let mut pick: Vec<std::path::PathBuf> = files.iter().take(6).map(|f| f.1.clone()).collect();
        for name in ["catena_aurea.cmti", "01 Multilexico de idiomas biblicos en espanol.lexi", "Multilexico_Pro.lexi"] {
            pick.push(dir.join(name));
        }
        let mut total = std::time::Duration::ZERO;
        for p in pick {
            let Ok(conn) = Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { continue };
            let key = p.to_string_lossy().to_string();
            let t = std::time::Instant::now();
            let entries = sample_entries(&conn, &key);
            let read = t.elapsed();
            let rules = learn(&entries);
            let all = t.elapsed();
            total += all;
            eprintln!(
                "{:>7.1} MB  {:<55} read {:>6.1} ms  learn {:>6.1} ms  ({} entries; {} titles, {} markups, {} shapes)",
                std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) as f64 / 1e6,
                p.file_name().unwrap().to_string_lossy(),
                read.as_secs_f64() * 1e3,
                (all - read).as_secs_f64() * 1e3,
                entries.len(), rules.texts.len(), rules.markup.len(), rules.shaped.len()
            );
        }
        eprintln!("total {:.0} ms", total.as_secs_f64() * 1e3);
    }

    #[test]
    fn strong_dictionary_has_no_titles() {
        let path = std::path::Path::new(ESWORD).join("01 Diccionario strong.lexi");
        if !path.exists() {
            return;
        }
        let conn = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert!(title_rules(&conn, &path.to_string_lossy()).is_empty());
    }
}
