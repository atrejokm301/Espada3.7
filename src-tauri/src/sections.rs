//! Section titles inside an entry, ported from the Windows build (module-pipeline 2f0840f,
//! `sections.rs`): where a combined module moves on to another work or author (Multiléxico:
//! «Diccionario Strong», «Léxico Griego-Español Tuggy»…; Catena Aurea: «San Agustín, de sermone
//! Domini, 1,1»; LBLA: «Notas al pie… (1997)»), and the headings of a single work.
//!
//! In the markup these titles are whole lines in bold (or a heading tag) — in the Catena, an
//! author in bold followed by the work in italics — and they come back entry after entry. So
//! each module's titles are learned once from a sample of its entries: bold lines that recur in
//! at least 3 % of them. The markup those titles wear (centred teal bold, `<h1>`, bold +
//! underline, purple bold…) then marks the rarer ones too — the Fathers the Catena quotes once
//! in a while, Barclay in the Multiléxico. Plain bold is also worn by headwords and labels, so
//! it only reaches titles of the same shape: ALL CAPS («ARQUEOLÓGICAS») or an outline letter
//! («B. Verbo»).
//!
//! The Mac shows module text as HTML (bold and italics kept), so instead of marking text the
//! title lines of the display HTML are wrapped in `<span class="sec-title">` (the page draws a
//! section heading). Only the main panel asks for it (`modules::with_section_titles`): probes,
//! searches, the side pane and the Strong's lists read the plain cleaner.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use crate::text::{clean_esword_text, to_plain};

/// Entries read to learn a module's titles: spread evenly over the main table, plus a few from
/// the other commentary tables (chapter and book notes).
const SAMPLE_MAIN: i64 = 300;
const SAMPLE_OTHER: i64 = 50;
/// A title recurs in at least this share of the sampled entries, and in at least 3 of them.
const RECUR_SHARE: f64 = 0.03;
const RECUR_MIN: usize = 3;
const MAX_TITLE_CHARS: usize = 90;
const MAX_TITLE_WORDS: usize = 12;

/// Markup that sets a run apart from plain bold: a colour, centring, underline, a heading tag,
/// a size, a background, small caps, a class.
const DISTINCT_MARKUP: [&str; 15] = [
    "color", "center", "underline", "<u>", "<u ", "<h1", "<h2", "<h3", "<h4", "<h5", "<h6", "size", "background",
    "small-caps", "class=",
];

/// Bold and italic runs kept through the analysis (never shown): each bold-open is followed by
/// one character U+F0000 + n, the markup that opened the run being `sigs[n]`.
const EM_BOLD_OPEN: char = '\u{E010}';
const EM_BOLD_CLOSE: char = '\u{E011}';
const EM_ITALIC_OPEN: char = '\u{E012}';
const EM_ITALIC_CLOSE: char = '\u{E013}';
const EM_SIG_BASE: u32 = 0xF0000;
const EM_SIG_LAST: u32 = 0xFFFFD;

/// Bold runs: a heading tag, <b>, <strong>, or a span set in bold. Inner groups 1–4.
static RE_EM_BOLD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?is)<h[1-6]\b[^>]*>(.*?)</h[1-6]\s*>|<b\b[^>]*>(.*?)</b\s*>|<strong\b[^>]*>(.*?)</strong\s*>|<span\b[^>]*font-weight\s*:\s*(?:bold|[6-9]00)[^>]*>(.*?)</span\s*>",
    )
    .unwrap()
});
/// Italic runs: <i>, <em>, or a span set in italics. Inner groups 1–3.
static RE_EM_ITALIC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?is)<i\b[^>]*>(.*?)</i\s*>|<em\b[^>]*>(.*?)</em\s*>|<span\b[^>]*font-style\s*:\s*italic[^>]*>(.*?)</span\s*>")
        .unwrap()
});
static RE_EM_RULE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<hr\b[^>]*>").unwrap());
static RE_OUTLINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:[A-Z]|[IVXLC]{1,4}|\d{1,2})[.)]\s+\S").unwrap());
static RE_URL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)https?:|www\.|\.(?:com|org|net)\b").unwrap());
/// «NO HAY DEFINICION EN ESTE LEXICO…» is the whole entry, not a section.
static RE_NOTICE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^no (?:hay|tiene|existe)\b").unwrap());
/// «Número: Singular», «Part of Speech: verb» — a field with its value, not a title.
static RE_FIELD: Lazy<Regex> = Lazy::new(|| Regex::new(r":\s+\S").unwrap());

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
    /// Bold-then-italic lines: the bold part and the rest (the HTML keeps both as <strong> and
    /// <em>, so only the tests read it).
    #[cfg_attr(not(test), allow(dead_code))]
    lead: Option<(String, String)>,
    /// What recurs: the whole line, or the bold part without its trailing comma.
    key: String,
    /// The markup that opened the first bold run (index into the entry's signature list).
    sig: Option<usize>,
}

fn is_sig_char(c: char) -> bool {
    (EM_SIG_BASE..=EM_SIG_LAST).contains(&(c as u32))
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

/// The markup that opened a bold run: the opening tags right before it (nothing visible in
/// between) plus its own, lowercased with whitespace folded. «Diccionario Strong» in the
/// Multiléxico: `<p style="text-align:center;"><span style="color:#008080;font-weight:bold;">`.
fn emphasis_signature(before: &str, own: &str) -> String {
    let mut rest = before.trim_end();
    let mut tags: Vec<&str> = Vec::new();
    while tags.len() < 4 && rest.ends_with('>') {
        let Some(lt) = rest.rfind('<') else { break };
        let tag = &rest[lt..];
        let name = tag[1..].trim_start().to_ascii_lowercase();
        let closes_or_breaks = name.starts_with('/') || name.starts_with('!') || name.starts_with("br") || name.starts_with("hr");
        if closes_or_breaks {
            break;
        }
        tags.push(tag);
        rest = rest[..lt].trim_end();
    }
    let mut sig = String::new();
    for tag in tags.iter().rev().copied().chain(std::iter::once(own)) {
        let mut space = false;
        for c in tag.chars() {
            if c.is_whitespace() {
                space = true;
                continue;
            }
            if space && !sig.ends_with('<') {
                sig.push(' ');
            }
            space = false;
            sig.push(if c == '\'' { '"' } else { c.to_ascii_lowercase() });
        }
    }
    sig
}

/// Bold runs → EM_BOLD_OPEN + signature char … EM_BOLD_CLOSE (a heading on a line of its own),
/// italic runs → EM_ITALIC_*, `<hr>` → a line break.
fn mark_emphasis(text: &str, sigs: &mut Vec<String>) -> String {
    let bold = RE_EM_BOLD.replace_all(text, |caps: &regex::Captures| {
        let whole = caps.get(0).expect("match");
        let open_len = whole.as_str().find('>').map_or(0, |i| i + 1);
        let own = &whole.as_str()[..open_len];
        let sig = emphasis_signature(&text[..whole.start()], own);
        let n = match sigs.iter().position(|s| *s == sig) {
            Some(n) => n,
            None => {
                sigs.push(sig);
                sigs.len() - 1
            }
        };
        let sig_char = char::from_u32((EM_SIG_BASE + n as u32).min(EM_SIG_LAST)).unwrap_or(EM_BOLD_OPEN);
        let inner = (1..=4).find_map(|g| caps.get(g)).map_or("", |m| m.as_str());
        let heading = own.len() > 2 && own.as_bytes()[2].is_ascii_digit();
        let mut s = String::with_capacity(inner.len() + 12);
        if heading {
            s.push_str("<br>");
        }
        s.push(EM_BOLD_OPEN);
        s.push(sig_char);
        s.push_str(inner);
        s.push(EM_BOLD_CLOSE);
        if heading {
            s.push_str("<br>");
        }
        s
    });
    let italic = RE_EM_ITALIC.replace_all(&bold, |caps: &regex::Captures| {
        let inner = (1..=3).find_map(|g| caps.get(g)).map_or("", |m| m.as_str());
        format!("{EM_ITALIC_OPEN}{inner}{EM_ITALIC_CLOSE}")
    });
    RE_EM_RULE.replace_all(&italic, "<br>").into_owned()
}

/// The entry's lines as plain text with its bold/italic runs marked: the Mac cleaner on the
/// marked markup, one line per `<br>`.
fn emphasis_lines(raw: &str, sigs: &mut Vec<String>) -> Vec<String> {
    let marked = mark_emphasis(raw, sigs);
    clean_esword_text(&marked).split("<br>").map(to_plain).collect()
}

fn candidate(line: &str) -> Option<Candidate> {
    let line = line.trim_start();
    let mut it = line.chars();
    if it.next() != Some(EM_BOLD_OPEN) {
        return None;
    }
    let sig = it.next().filter(|&c| is_sig_char(c)).map(|c| (c as u32 - EM_SIG_BASE) as usize);
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

/// Indexes of the lines that have something to read.
fn content_lines(lines: &[String]) -> Vec<usize> {
    (0..lines.len()).filter(|&i| !strip_emphasis(&lines[i]).trim().is_empty()).collect()
}

/// Raw text of entries spread evenly over the module (rowid list: no table scan). `tables`:
/// the module's text tables and their text column, the main one first.
pub(crate) fn sample_entries(conn: &Connection, tables: &[(&str, &str)]) -> Vec<String> {
    let mut out = Vec::new();
    for (k, (table, column)) in tables.iter().enumerate() {
        let want = if k == 0 { SAMPLE_MAIN } else { SAMPLE_OTHER };
        let bounds = conn.query_row(&format!("SELECT min(rowid), max(rowid) FROM \"{table}\""), [], |r| {
            Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?))
        });
        let Ok((Some(lo), Some(hi))) = bounds else {
            continue;
        };
        let span = i128::from(hi) - i128::from(lo);
        let mut ids: Vec<i64> = (0..want)
            .map(|k| (i128::from(lo) + span * i128::from(k) / i128::from((want - 1).max(1))) as i64)
            .collect();
        ids.dedup();
        let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT \"{column}\" FROM \"{table}\" WHERE rowid IN ({list})");
        let Ok(mut stmt) = conn.prepare(&sql) else {
            continue;
        };
        let Ok(rows) = stmt.query_map([], |r| r.get::<_, Option<String>>(0)) else {
            continue;
        };
        out.extend(rows.flatten().flatten().filter(|s| !s.trim().is_empty()));
    }
    out
}

pub(crate) fn learn(entries: &[String]) -> TitleRules {
    struct Seen {
        entries: usize,
        sigs: HashMap<String, usize>,
    }
    let mut seen: HashMap<String, Seen> = HashMap::new();
    for raw in entries {
        let mut sigs = Vec::new();
        let lines = emphasis_lines(raw, &mut sigs);
        let content = content_lines(&lines);
        let mut here = HashSet::new();
        // Never the last line: footers («Diccionario de filosofía en CD-ROM.»), signatures
        // («d.f.w.»), an entry that is only «NO HAY DEFINICION…».
        for &i in content.iter().take(content.len().saturating_sub(1)) {
            let Some(c) = candidate(&lines[i]) else {
                continue;
            };
            if !titlelike(&c.key) || !here.insert(c.key.clone()) {
                continue;
            }
            let s = seen.entry(c.key).or_insert_with(|| Seen { entries: 0, sigs: HashMap::new() });
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

// ─── learned rules: memory, and a small file so each module learns once ───
// Learning reads ~300 entries (50–100 ms for the Multiléxico or the Catena on Windows). Kept in
// ~/Library/Caches/com.asignaciondelcielo.espada37/section-titles.json; a module whose file
// changes (size or date) is learned again.

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

static RULES: Lazy<Mutex<HashMap<String, ((u64, u64), Arc<TitleRules>)>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static SAVED: Lazy<Mutex<Option<SavedRules>>> = Lazy::new(|| Mutex::new(None));

fn saved_path() -> Option<PathBuf> {
    if cfg!(test) {
        return None; // tests never touch the user's cache
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Caches/com.asignaciondelcielo.espada37/section-titles.json"))
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

fn with_saved<T>(f: impl FnOnce(&mut SavedRules) -> T) -> T {
    let mut slot = SAVED.lock();
    if slot.is_none() {
        let loaded = saved_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|raw| serde_json::from_str::<SavedRules>(&raw).ok())
            .filter(|s| s.version == RULES_VERSION);
        *slot = Some(loaded.unwrap_or(SavedRules { version: RULES_VERSION, ..Default::default() }));
    }
    f(slot.as_mut().expect("loaded"))
}

fn load_saved(path_key: &str, fp: (u64, u64)) -> Option<TitleRules> {
    let m = with_saved(|s| s.modules.get(path_key).cloned())?;
    if (m.mtime_ms, m.size) != fp {
        return None;
    }
    Some(TitleRules {
        texts: m.texts.into_iter().collect(),
        markup: m.markup.into_iter().collect(),
        shaped: m.shaped.into_iter().collect(),
    })
}

fn save(path_key: &str, fp: (u64, u64), rules: &TitleRules) {
    let Some(file) = saved_path() else { return };
    let sorted = |v: &HashSet<String>| {
        let mut v: Vec<String> = v.iter().cloned().collect();
        v.sort();
        v
    };
    let mut shaped: Vec<(String, Shape)> = rules.shaped.iter().cloned().collect();
    shaped.sort_by(|a, b| a.0.cmp(&b.0).then(format!("{:?}", a.1).cmp(&format!("{:?}", b.1))));
    let module = SavedModule { mtime_ms: fp.0, size: fp.1, texts: sorted(&rules.texts), markup: sorted(&rules.markup), shaped };
    let json = with_saved(|s| {
        s.modules.insert(path_key.to_string(), module);
        serde_json::to_string(s).ok()
    });
    if let Some(json) = json {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Write then rename: a crash mid-write never leaves half a file behind, and two modules
        // learned at once never share a temporary file.
        static SAVE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SAVE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = file.with_extension(format!("json.{}.{n}.tmp", std::process::id()));
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &file);
        }
    }
}

/// The rules already known for `path` (memory, else the saved file), without learning.
pub(crate) fn known_rules(path: &str) -> Option<Arc<TitleRules>> {
    let fp = fingerprint(Path::new(path))?;
    if let Some((f, r)) = RULES.lock().get(path) {
        if *f == fp {
            return Some(r.clone());
        }
    }
    let saved = Arc::new(load_saved(path, fp)?);
    RULES.lock().insert(path.to_string(), (fp, saved.clone()));
    Some(saved)
}

/// Learn `path`'s rules from `entries` (its sample) and keep them.
pub(crate) fn remember_rules(path: &str, entries: &[String]) -> Arc<TitleRules> {
    let rules = Arc::new(learn(entries));
    if let Some(fp) = fingerprint(Path::new(path)) {
        save(path, fp, &rules);
        RULES.lock().insert(path.to_string(), (fp, rules.clone()));
    }
    rules
}

// ─── wrapping the title lines of the display HTML ───

/// The title lines of one entry, in order: the line's text, and for an author line its bold
/// part and the work after it.
fn title_lines(raw: &str, rules: &TitleRules) -> Vec<String> {
    let mut sigs = Vec::new();
    let lines = emphasis_lines(raw, &mut sigs);
    let last = content_lines(&lines).last().copied();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if Some(i) == last {
            continue;
        }
        if let Some(c) = candidate(line).filter(|c| rules.matches(c, &sigs)) {
            out.push(c.text);
        }
    }
    out
}

/// Text of a display line, as the analysis sees it (private-use characters never count).
fn line_key(html_line: &str) -> String {
    squash(&to_plain(html_line).chars().filter(|c| !matches!(*c as u32, 0xE000..=0xF8FF)).collect::<String>())
}

/// Every tag the line opens it also closes (so a wrapper around it is well formed).
fn balanced(html_line: &str) -> bool {
    let mut stack: Vec<String> = Vec::new();
    let mut rest = html_line;
    while let Some(lt) = rest.find('<') {
        let Some(gt) = rest[lt..].find('>') else { return false };
        let tag = &rest[lt + 1..lt + gt];
        rest = &rest[lt + gt + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        if name.is_empty() || tag.ends_with('/') || matches!(name.as_str(), "br" | "img" | "hr" | "wbr") {
            continue;
        }
        if closing {
            if stack.pop().as_deref() != Some(name.as_str()) {
                return false;
            }
        } else {
            stack.push(name);
        }
    }
    stack.is_empty()
}

/// `html` (the cleaner's display HTML of `raw`) with its section-title lines wrapped in
/// `<span class="sec-title">`. Only whole, well-formed lines whose text is the next title are
/// wrapped, so the visible text is exactly the same; a title the HTML does not have as a line of
/// its own is left as it is.
pub(crate) fn wrap_titles(html: &str, raw: &str, rules: &TitleRules) -> String {
    if rules.is_empty() || html.is_empty() {
        return html.to_string();
    }
    let titles = title_lines(raw, rules);
    if titles.is_empty() {
        return html.to_string();
    }
    // Titles in order; a title the HTML has not as a line of its own (glued after an <hr>) is
    // skipped, never blocking the ones after it.
    let mut next = 0;
    let mut out = String::with_capacity(html.len() + titles.len() * 32);
    for (i, line) in html.split("<br>").enumerate() {
        if i > 0 {
            out.push_str("<br>");
        }
        let found = if next < titles.len() {
            let key = line_key(line);
            titles[next..].iter().position(|t| *t == key).filter(|_| balanced(line))
        } else {
            None
        };
        if let Some(k) = found {
            next += k + 1;
            out.push_str(r#"<span class="sec-title">"#);
            out.push_str(line);
            out.push_str("</span>");
        } else {
            out.push_str(line);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(html: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = html;
        let open = r#"<span class="sec-title">"#;
        while let Some(a) = rest.find(open) {
            let after = &rest[a + open.len()..];
            // A title line is wrapped whole: up to the next <br> (or the end), minus </span>.
            let end = after.find("<br>").unwrap_or(after.len());
            let line = after[..end].strip_suffix("</span>").unwrap_or(&after[..end]);
            out.push(line_key(line));
            rest = &after[end..];
        }
        out
    }

    fn shown(raw: &str, rules: &TitleRules) -> String {
        let html = clean_esword_text(raw);
        let wrapped = wrap_titles(&html, raw, rules);
        assert_eq!(to_plain(&wrapped), to_plain(&html), "the same text as before, plus the wrappers");
        wrapped
    }

    #[test]
    fn candidates_are_whole_bold_lines_or_bold_then_italic() {
        let mut sigs = Vec::new();
        let lines = emphasis_lines(
            "<p><b>Glosa</b></p><p><b>San Agustín, </b><i>de sermone Domini, 1,1</i></p>\
             <p><b>Ojo:</b> texto normal</p><p><b>Etiqueta:</b> <i>valor</i></p>",
            &mut sigs,
        );
        let lines: Vec<&String> = lines.iter().filter(|l| !strip_emphasis(l).trim().is_empty()).collect();
        let c0 = candidate(lines[0]).expect("Glosa");
        assert_eq!((c0.text.as_str(), c0.lead.is_none()), ("Glosa", true));
        let c1 = candidate(lines[1]).expect("author line");
        assert_eq!(c1.key, "San Agustín");
        assert_eq!(c1.lead, Some(("San Agustín,".to_string(), "de sermone Domini, 1,1".to_string())));
        assert!(candidate(lines[2]).is_none(), "bold label + plain text");
        assert!(candidate(lines[3]).is_none(), "label: value");
    }

    #[test]
    fn titlelike_keeps_titles_and_drops_headwords_fields_and_notices() {
        for t in ["Diccionario Strong", "BDB-THAYER", "San Agustín", "NOTAS", "Thayer Definición:", "A. Nombre",
                  "¿Quién, dónde y cuándo?", "CAPITULO 1", "Notas al pie De La Biblia de las Americas (1997)"] {
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
        assert_eq!(titles(&shown(&raw, &rules)), ["Diccionario Strong", "Palabras Griegas del Nuevo Testamento (Barclay)"]);
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
        assert_eq!(titles(&shown(raw, &rules)), ["ARQUEOLÓGICAS"]);
    }

    #[test]
    fn the_last_line_is_never_a_title() {
        let mut entries = Vec::new();
        for i in 0..40 {
            entries.push(format!("<p>texto {i}</p><p><b>Diccionario de filosofía en CD-ROM.</b></p>"));
        }
        assert!(learn(&entries).is_empty());
    }

    #[test]
    fn an_author_line_is_wrapped_whole_with_its_work() {
        let author = |a: &str, w: &str| format!("<p><b>{a}, </b><i>{w}</i></p>");
        let mut entries = Vec::new();
        for i in 0..40 {
            entries.push(format!("{}<p>texto {i}</p>{}<p>más {i}</p>", author("San Agustín", "de sermone Domini, 1,1"), author("Crisóstomo", "in Matthaeum, hom. 15")));
        }
        let rules = learn(&entries);
        let raw = format!("{}<p>Dichosos…</p>{}<p>fin</p>", author("San Agustín", "de civitate Dei, 4"), author("Rabano", "in Matthaeum"));
        let html = shown(&raw, &rules);
        assert_eq!(titles(&html), ["San Agustín, de civitate Dei, 4"], "a recurring author: the line wrapped whole");
        assert!(html.contains(r#"<span class="sec-title"><strong>San Agustín, </strong><em>de civitate Dei, 4</em></span>"#), "{html}");
    }

    #[test]
    fn an_unbalanced_line_is_never_wrapped() {
        assert!(balanced("<strong>Diccionario Strong</strong>"));
        assert!(balanced("<strong>A</strong><em>b</em>"));
        assert!(!balanced("<strong>Diccionario"));
        assert!(!balanced("Strong</strong>"));
        assert!(!balanced("<em><strong>x</em></strong>"));
    }

    #[test]
    fn a_title_that_is_not_a_line_of_its_own_does_not_block_the_next() {
        let title = |t: &str| format!(r#"<p style="text-align:center;"><span style="color:#008080;font-weight:bold;">{t}</span></p>"#);
        let mut entries = Vec::new();
        for i in 0..40 {
            entries.push(format!("{}<p>a {i}</p>{}<p>b {i}</p>", title("Diccionario Strong"), title("Diccionario Tuggy")));
        }
        let rules = learn(&entries);
        // After <hr> the display glues «Diccionario Strong» to the text before it; Tuggy is still wrapped.
        let raw = format!("<p>intro</p><hr>{}<p>a</p>{}<p>b</p>", title("Diccionario Strong"), title("Diccionario Tuggy"));
        let html = shown(&raw, &rules);
        assert!(titles(&html).contains(&"Diccionario Tuggy".to_string()), "{html}");
    }

    #[test]
    fn no_rules_change_nothing() {
        let raw = "<p><b>Título</b></p><p>texto</p>";
        let html = clean_esword_text(raw);
        assert_eq!(wrap_titles(&html, raw, &TitleRules::default()), html);
    }
}
