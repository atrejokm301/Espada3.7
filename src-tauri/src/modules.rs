//! e-Sword-compatible SQLite module scanning and queries.

use crate::text::{
    clean_bible_text, clean_bible_verse_tagged, clean_esword_text, looks_encrypted, own_tag_layout_stats,
    own_tag_units, to_plain,
};
use once_cell::sync::{Lazy, OnceCell};
use parking_lot::Mutex;
use regex::Regex;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

static MODULE_LIST_CACHE: Lazy<Mutex<Option<ModuleListCache>>> = Lazy::new(|| Mutex::new(None));

#[derive(Clone)]
struct ModuleListCache {
    path: String,
    fingerprint: u64,
    modules: Vec<ModuleInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleInfo {
    pub path: String,
    pub filename: String,
    pub title: String,
    pub abbreviation: String,
    pub kind: String, // bible | commentary | dictionary | lexicon | reference | devotional
    pub encrypted: bool,
    pub has_strongs: bool,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerseRow {
    pub verse: i32,
    /// The verse without its section titles (those are in `titles`).
    pub html: String,
    pub plain: String,
    /// Section titles the module put inside this verse's text (bible_titles.rs), in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub titles: Vec<VerseTitle>,
}

/// One section title of a verse: `lead` (drawn above the verse), `minor` (italic-only, psalm
/// titles: drawn lighter) or `next` (stored at the end of this verse, drawn above the next one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerseTitle {
    pub kind: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterPayload {
    pub book: i32,
    pub chapter: i32,
    pub module: String,
    pub abbreviation: String,
    pub title: String,
    pub verses: Vec<VerseRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyEntry {
    pub scope: String, // verse | chapter | book
    pub label: String,
    pub html: String,
    pub plain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictEntry {
    pub topic: String,
    pub html: String,
    pub plain: String,
    /// ISS-08: `exact` | `headword` | `lemma` | `verbform` | `contains`. Empty (omitted) for
    /// paths that do not rank (reference modules); the UI treats a missing value as exact.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub match_kind: String,
    /// ISS-08: the article was cut at a block boundary (it was larger than 400 KB).
    #[serde(default)]
    pub truncated: bool,
}

// ─── Errors (ISS-26) ───

/// Map an SQLite / open error to a Spanish message for the UI. Unknown errors pass through.
pub fn es_error(msg: &str) -> String {
    let m = msg.to_ascii_lowercase();
    if m.contains("no such table") || m.contains("no such column") {
        "Formato de módulo no compatible".into()
    } else if m.contains("unable to open") {
        "El módulo ya no está en la carpeta; pulse «Actualizar lista de módulos»".into()
    } else if m.contains("not a database")
        || m.contains("file is encrypted")
        || m.contains("malformed")
    {
        "El archivo no es un módulo válido".into()
    } else {
        msg.to_string()
    }
}

// ─── Connection cache (ISS-28) ───

/// Bounded LRU of open read-only connections.
const CONN_CACHE_CAP: usize = 64;

/// File identity, so a module replaced under the same name is reopened (ISS-28).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdent {
    len: u64,
    mtime_ns: u128,
}

fn file_ident(path: &str) -> Option<FileIdent> {
    let meta = fs::metadata(path).ok()?;
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Some(FileIdent {
        len: meta.len(),
        mtime_ns,
    })
}

struct ConnSlot {
    conn: Mutex<Connection>,
    schema: OnceCell<Schema>,
}

struct CacheEntry {
    slot: Arc<ConnSlot>,
    ident: FileIdent,
    last_used: u64,
}

#[derive(Default)]
struct ConnCache {
    map: HashMap<String, CacheEntry>,
    tick: u64,
}

static CONN_CACHE: Lazy<Mutex<ConnCache>> = Lazy::new(|| Mutex::new(ConnCache::default()));
/// ISS-28 review: detected schema per (path, file identity). A module evicted from the LRU
/// reopens without the sqlite_master check and the detect_schema PRAGMAs (tiny entries).
static SCHEMA_CACHE: Lazy<Mutex<HashMap<String, (FileIdent, Schema)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static PINNED: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

/// ISS-28: paths the connection LRU never evicts (the interlinear and the current Bible).
/// Replaces the previous pinned set. Empty strings are ignored.
#[allow(dead_code)] // additive API for WP5 (lib.rs)
pub fn set_pinned_modules(paths: &[String]) {
    let mut p = PINNED.lock();
    p.clear();
    p.extend(paths.iter().filter(|s| !s.is_empty()).cloned());
}

#[cfg(test)]
thread_local! {
    /// Opens on the current thread (exact per-test counts while other tests run).
    static THREAD_OPENS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// detect_schema runs on the current thread.
    static THREAD_SCHEMA_DETECTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Failed opens on the current thread (EMFILE under a low fd limit, …).
    static THREAD_OPEN_FAILS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn open_ro(path: &str) -> Result<Connection, String> {
    #[cfg(test)]
    THREAD_OPENS.with(|c| c.set(c.get() + 1));
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| {
        #[cfg(test)]
        THREAD_OPEN_FAILS.with(|c| c.set(c.get() + 1));
        es_error(&format!("No se pudo abrir módulo: {e}"))
    })?;
    // ISS-28: bound the page cache (256 KiB per connection).
    let _ = conn.execute_batch("PRAGMA cache_size=-256;");
    Ok(conn)
}

fn cached_slot(path: &str, ident: Option<FileIdent>) -> Option<Arc<ConnSlot>> {
    let mut cache = CONN_CACHE.lock();
    cache.tick += 1;
    let tick = cache.tick;
    let stale = match cache.map.get_mut(path) {
        Some(e) if Some(e.ident) == ident => {
            e.last_used = tick;
            return Some(e.slot.clone());
        }
        Some(_) => true,
        None => false,
    };
    if stale {
        cache.map.remove(path);
    }
    None
}

fn insert_slot(path: &str, ident: FileIdent, slot: Arc<ConnSlot>) {
    let pinned = PINNED.lock().clone();
    let mut cache = CONN_CACHE.lock();
    cache.tick += 1;
    let tick = cache.tick;
    while cache.map.len() >= CONN_CACHE_CAP && !cache.map.contains_key(path) {
        let victim = cache
            .map
            .iter()
            .filter(|(k, _)| !pinned.contains(*k))
            .min_by_key(|(_, e)| e.last_used)
            .map(|(k, _)| k.clone());
        match victim {
            Some(k) => {
                cache.map.remove(&k);
            }
            None => break,
        }
    }
    cache.map.insert(
        path.to_string(),
        CacheEntry {
            slot,
            ident,
            last_used: tick,
        },
    );
}

/// Run `f` with a connection and its detected schema. The map lock is released before the
/// connection is locked, so slow queries never block other modules (ISS-28).
/// `cache_new = false` (probes) uses a cached connection when present but never inserts one.
fn with_slot_mode<T, F>(path: &str, cache_new: bool, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection, &Schema) -> Result<T, String>,
{
    let ident = file_ident(path);
    let slot = match cached_slot(path, ident) {
        Some(s) => s,
        None => {
            let known = ident.and_then(|id| match SCHEMA_CACHE.lock().get(path) {
                Some((i, s)) if *i == id => Some(s.clone()),
                _ => None,
            });
            let conn = open_ro(path)?;
            // ISS-26: SQLite opens lazily, so a non-database file only fails here; surface
            // «El archivo no es un módulo válido» instead of a misleading missing-table error.
            // Skipped when this exact file already passed it (schema cached, ISS-28 review).
            if known.is_none() {
                conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
                    .map_err(|e| es_error(&e.to_string()))?;
            }
            let schema = OnceCell::new();
            if let Some(k) = known {
                let _ = schema.set(k);
            }
            let slot = Arc::new(ConnSlot {
                conn: Mutex::new(conn),
                schema,
            });
            if cache_new {
                if let Some(id) = ident {
                    insert_slot(path, id, slot.clone());
                }
            }
            slot
        }
    };
    let conn = slot.conn.lock();
    let schema = slot.schema.get_or_init(|| {
        #[cfg(test)]
        THREAD_SCHEMA_DETECTS.with(|c| c.set(c.get() + 1));
        let s = detect_schema(&conn);
        if let Some(id) = ident {
            SCHEMA_CACHE.lock().insert(path.to_string(), (id, s.clone()));
        }
        s
    });
    f(&conn, schema)
}

fn with_slot<T, F>(path: &str, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection, &Schema) -> Result<T, String>,
{
    with_slot_mode(path, true, f)
}

fn with_probe_slot<T, F>(path: &str, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection, &Schema) -> Result<T, String>,
{
    with_slot_mode(path, false, f)
}

fn with_conn<T, F>(path: &str, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection) -> Result<T, String>,
{
    with_slot(path, |c, _| f(c))
}

#[cfg(test)]
fn conn_cache_len() -> usize {
    CONN_CACHE.lock().map.len()
}

#[cfg(test)]
fn conn_cache_has(path: &str) -> bool {
    CONN_CACHE.lock().map.contains_key(path)
}

// ─── Schema detection (ISS-12) ───

/// Column layout of a Topic/Definition (or legacy word/data) table. Names come only from
/// the static allowlist below, never from module data.
#[derive(Debug, Clone, Copy)]
struct TopicSchema {
    table: &'static str,
    topic: &'static str,
    def: &'static str,
    /// An index whose first column is the topic column exists (index seeks are possible).
    indexed: bool,
}

#[derive(Debug, Clone, Default)]
struct Schema {
    verse_cmt: Option<&'static str>,
    chapter_cmt: Option<&'static str>,
    book_cmt: Option<&'static str>,
    dict: Option<TopicSchema>,
    lex: Option<TopicSchema>,
}

fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(mut st) = conn.prepare("SELECT name FROM pragma_table_info(?1)") {
        if let Ok(rows) = st.query_map(rusqlite::params![table], |r| r.get::<_, String>(0)) {
            out.extend(rows.flatten().map(|s| s.to_ascii_lowercase()));
        }
    }
    out
}

fn topic_indexed(conn: &Connection, table: &str, topic: &str) -> bool {
    let mut idx_names = Vec::new();
    if let Ok(mut st) = conn.prepare("SELECT name FROM pragma_index_list(?1)") {
        if let Ok(rows) = st.query_map(rusqlite::params![table], |r| r.get::<_, String>(0)) {
            idx_names.extend(rows.flatten());
        }
    }
    idx_names.iter().any(|ix| {
        conn.query_row(
            "SELECT name FROM pragma_index_info(?1) WHERE seqno=0",
            rusqlite::params![ix],
            |r| r.get::<_, String>(0),
        )
        .map(|c| c.eq_ignore_ascii_case(topic))
        .unwrap_or(false)
    })
}

fn detect_schema(conn: &Connection) -> Schema {
    let mut tables: HashSet<String> = HashSet::new();
    if let Ok(mut st) = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'") {
        if let Ok(rows) = st.query_map([], |r| r.get::<_, String>(0)) {
            tables.extend(rows.flatten().map(|s| s.to_ascii_lowercase()));
        }
    }
    let has = |t: &str| tables.contains(&t.to_ascii_lowercase());
    let pick_cmt = |cands: &[&'static str], need: &[&str]| -> Option<&'static str> {
        cands.iter().copied().find(|t| {
            has(t) && {
                let cols = table_columns(conn, t);
                need.iter().all(|n| cols.iter().any(|c| c == n))
            }
        })
    };
    let topic_schema = |t: &'static str| -> Option<TopicSchema> {
        if !has(t) {
            return None;
        }
        let cols = table_columns(conn, t);
        let c = |n: &str| cols.iter().any(|x| x == n);
        let (topic, def) = if c("topic") && c("definition") {
            ("Topic", "Definition")
        } else if c("word") && c("data") {
            ("word", "data")
        } else {
            return None;
        };
        Some(TopicSchema {
            table: t,
            topic,
            def,
            indexed: topic_indexed(conn, t, topic),
        })
    };
    let d = topic_schema("Dictionary");
    let l = topic_schema("Lexicon");
    Schema {
        verse_cmt: pick_cmt(
            &["VerseCommentary", "Verses"],
            &["comments", "chapterbegin", "versebegin"],
        ),
        chapter_cmt: pick_cmt(&["ChapterCommentary", "Chapters"], &["comments", "chapter"]),
        book_cmt: pick_cmt(&["BookCommentary", "Books"], &["comments", "book"]),
        dict: d.or(l),
        lex: l.or(d),
    }
}

fn kind_from_ext(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "bbli" | "bblx" | "bbl" => Some("bible"),
        "cmti" | "cmtx" | "cmt" => Some("commentary"),
        "dcti" | "dctx" | "dct" => Some("dictionary"),
        "lexi" | "lexx" | "lex" => Some("lexicon"),
        "refi" | "refx" | "ref" => Some("reference"),
        // Devotionals: one reading per calendar day (2026-11-18, from the Windows build).
        "devi" | "devx" => Some("devotional"),
        _ => None,
    }
}

/// The old rule (a legacy extension meant 🔒); kept for the scan-comparison golden test.
#[cfg(test)]
fn is_legacy_encrypted_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "bblx" | "cmtx" | "dctx" | "lexx" | "refx" | "devx"
    )
}

fn fingerprint_dir(dir: &Path) -> u64 {
    let mut fp: u64 = 0xcbf29ce484222325;
    if let Ok(rd) = fs::read_dir(dir) {
        for ent in rd.flatten() {
            let path = ent.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if kind_from_ext(&ext).is_none() {
                continue;
            }
            if let Ok(meta) = ent.metadata() {
                let size = meta.len();
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                for b in name.as_bytes() {
                    fp ^= *b as u64;
                    fp = fp.wrapping_mul(0x100000001b3);
                }
                fp ^= size;
                fp = fp.wrapping_mul(0x100000001b3);
                fp ^= mtime;
                fp = fp.wrapping_mul(0x100000001b3);
            }
        }
    }
    fp
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn read_details(conn: &Connection) -> (String, String, i32, bool) {
    let mut title = String::new();
    let mut abbr = String::new();
    let mut version = 0i32;
    let mut strongs = false;

    let _ = conn.query_row("SELECT Title FROM Details LIMIT 1", [], |r| {
        title = r.get::<_, String>(0).unwrap_or_default();
        Ok(())
    });
    // ISS-12: legacy .cmtx modules keep the name in Description.
    if title.trim().is_empty() {
        let _ = conn.query_row("SELECT Description FROM Details LIMIT 1", [], |r| {
            title = r.get::<_, String>(0).unwrap_or_default();
            Ok(())
        });
    }
    let title = collapse_ws(&title);
    let _ = conn.query_row("SELECT Abbreviation FROM Details LIMIT 1", [], |r| {
        abbr = r.get::<_, String>(0).unwrap_or_default();
        Ok(())
    });
    let _ = conn.query_row("SELECT Version FROM Details LIMIT 1", [], |r| {
        version = r.get::<_, i32>(0).unwrap_or(0);
        Ok(())
    });
    // Strongs column may be BOOL/INT
    if let Ok(v) = conn.query_row("SELECT Strongs FROM Details LIMIT 1", [], |r| {
        r.get::<_, i32>(0)
    }) {
        strongs = v != 0;
    } else if let Ok(v) = conn.query_row("SELECT Strongs FROM Details LIMIT 1", [], |r| {
        r.get::<_, bool>(0)
    }) {
        strongs = v;
    }

    (title, abbr, version, strongs)
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 COLLATE NOCASE LIMIT 1",
        rusqlite::params![name],
        |_| Ok(()),
    )
    .is_ok()
}

/// Read TEXT or BLOB_TEXT columns as a Rust String (UTF-8 / UTF-16 LE).
fn cell_to_string(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<String> {
    if let Ok(s) = row.get::<_, String>(idx) {
        return Ok(s);
    }
    if let Ok(bytes) = row.get::<_, Vec<u8>>(idx) {
        return Ok(bytes_to_text(&bytes));
    }
    // NULL or unsupported
    Ok(String::new())
}

fn bytes_to_text(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    // UTF-8
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    // UTF-16 LE (common in older e-Sword modules)
    if bytes.len() >= 2 && bytes.len() % 2 == 0 {
        let mut u16s = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.chunks_exact(2) {
            u16s.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        // strip BOM
        let slice = if u16s.first() == Some(&0xFEFF) {
            &u16s[1..]
        } else {
            &u16s[..]
        };
        return String::from_utf16_lossy(slice);
    }
    // Windows-1252 fallback
    encoding_rs::WINDOWS_1252
        .decode(bytes)
        .0
        .into_owned()
}

fn sample_encrypted(conn: &Connection, kind: &str) -> bool {
    let schema = detect_schema(conn);
    let mut samples: Vec<String> = Vec::new();
    match kind {
        "bible" => samples.push("SELECT Scripture FROM Bible LIMIT 1".into()),
        "commentary" => {
            for t in [schema.verse_cmt, schema.chapter_cmt, schema.book_cmt]
                .into_iter()
                .flatten()
            {
                samples.push(format!(
                    "SELECT Comments FROM \"{t}\" WHERE length(Comments)>0 LIMIT 1"
                ));
            }
        }
        "dictionary" | "lexicon" => {
            for ts in [schema.lex, schema.dict].into_iter().flatten() {
                samples.push(format!(
                    "SELECT \"{}\" FROM \"{}\" LIMIT 1",
                    ts.def, ts.table
                ));
            }
        }
        "reference" => samples.push("SELECT Content FROM Reference LIMIT 1".into()),
        "devotional" => samples.push("SELECT Devotion FROM Devotional WHERE length(Devotion)>0 LIMIT 1".into()),
        _ => return false,
    }
    for sql in samples {
        if let Ok(sample) = conn.query_row(&sql, [], |r| cell_to_string(r, 0)) {
            if !sample.is_empty() {
                return looks_encrypted(&sample);
            }
        }
    }
    false
}

/// One content cell of e-Sword's «sqliteplus» ciphertext: binary, a whole number of AES blocks,
/// neither UTF-8 nor UTF-16 text nor zlib/gzip, with control bytes, and about as many distinct
/// byte values as random bytes of that length. Short cells count too: «Testigos de Jehova
/// (TNM).bblx» stores 80–176-byte blobs, under the 181 distinct values the Windows check needs.
fn blob_is_ciphertext(b: &[u8]) -> bool {
    let n = b.len();
    if n < 32 || n % 16 != 0 {
        return false;
    }
    if (b[0] == 0x78 && matches!(b[1], 0x01 | 0x5e | 0x9c | 0xda)) || (b[0] == 0x1f && b[1] == 0x8b) {
        return false;
    }
    if std::str::from_utf8(b).is_ok() {
        return false;
    }
    // UTF-16LE text: its high bytes are mostly zero.
    let odd_zero = b.iter().skip(1).step_by(2).filter(|&&x| x == 0).count();
    if odd_zero * 10 >= (n / 2) * 4 {
        return false;
    }
    let ctrl = b.iter().filter(|&&x| x < 0x20 && !matches!(x, 9 | 10 | 13)).count();
    let mut seen = [false; 256];
    for &x in b {
        seen[x as usize] = true;
    }
    let distinct = seen.iter().filter(|&&x| x).count() as f64;
    let expected = 256.0 * (1.0 - (255.0f64 / 256.0).powi(n as i32));
    ctrl * 25 >= n && distinct >= 0.6 * expected
}

/// The module's text cells look like e-Sword ciphertext: most of up to 3 binary cells of its
/// main text table are (see [`blob_is_ciphertext`]).
fn sample_ciphertext(conn: &Connection, kind: &str) -> bool {
    let schema = detect_schema(conn);
    let mut cells: Vec<(&str, &str)> = Vec::new();
    match kind {
        "bible" => cells.push(("Bible", "Scripture")),
        "commentary" => {
            for t in [schema.verse_cmt, schema.chapter_cmt, schema.book_cmt].into_iter().flatten() {
                cells.push((t, "Comments"));
            }
        }
        "dictionary" | "lexicon" => {
            for ts in [schema.lex, schema.dict].into_iter().flatten() {
                cells.push((ts.table, ts.def));
            }
        }
        "reference" => cells.push(("Reference", "Content")),
        "devotional" => cells.push(("Devotional", "Devotion")),
        _ => return false,
    }
    for (table, col) in cells {
        let sql = format!("SELECT \"{col}\" FROM \"{table}\" WHERE typeof(\"{col}\")='blob' AND length(\"{col}\")>0 LIMIT 3");
        let Ok(mut st) = conn.prepare(&sql) else { continue };
        let Ok(rows) = st.query_map([], |r| r.get::<_, Vec<u8>>(0)) else { continue };
        let votes: Vec<bool> = rows.flatten().map(|b| blob_is_ciphertext(&b)).collect();
        if !votes.is_empty() {
            return votes.iter().filter(|&&v| v).count() * 2 > votes.len();
        }
    }
    false
}

// ─── Strong codes (ISS-03) ───

/// Strict Strong code parser. Accepts only a whole trimmed, case-insensitive
/// `H|G`, optional spaces, leading zeros, then 1–5 digits whose value is > 0.
/// Returns the canonical form (`"H430"`), or `None` for words like «hijo», «H0», «H1254a».
pub fn parse_strong_code(s: &str) -> Option<String> {
    let t = s.trim();
    let mut chars = t.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    if letter != 'H' && letter != 'G' {
        return None;
    }
    let rest = chars.as_str().trim_start_matches(' ');
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits = rest.trim_start_matches('0');
    if digits.is_empty() || digits.len() > 5 {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    if n == 0 {
        return None;
    }
    Some(format!("{letter}{n}"))
}

/// Topic spellings of a canonical code that Strong lexicons use (`H430`, `H0430`, `H00430`).
fn strong_code_forms(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(c) = parse_strong_code(code) else {
        return out;
    };
    let (p, d) = c.split_at(1);
    let n: u32 = d.parse().unwrap_or(0);
    for f in [c.clone(), format!("{p}{n:04}"), format!("{p}{n:05}")] {
        if !out.contains(&f) {
            out.push(f);
        }
    }
    let lower: Vec<String> = out.iter().map(|s| s.to_ascii_lowercase()).collect();
    out.extend(lower);
    out
}

/// Build Strong’s number lookup variants (G26, H430, zero-padded, bare digits).
fn strong_candidates(raw: &str) -> Vec<String> {
    let s = raw.trim().to_uppercase().replace(' ', "");
    if s.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    let push = |v: &mut Vec<String>, x: String| {
        if !x.is_empty() && !v.iter().any(|e| e == &x) {
            v.push(x);
        }
    };
    push(&mut out, s.clone());

    let (prefix, digits) = if s.starts_with('G') || s.starts_with('H') {
        (
            s.chars().next().unwrap(),
            s[1..].trim_start_matches('0').to_string(),
        )
    } else if s.chars().all(|c| c.is_ascii_digit()) {
        // bare number — try both G and H
        ('?', s.trim_start_matches('0').to_string())
    } else {
        return out;
    };
    // ISS-03: 'H0' / 'HIJO' have no digit part — never add a bare 'H' or letter junk.
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return out;
    }

    if prefix == '?' {
        for p in ['G', 'H'] {
            push(&mut out, format!("{p}{digits}"));
            if let Ok(n) = digits.parse::<u32>() {
                push(&mut out, format!("{p}{n:04}"));
                push(&mut out, format!("{p}{n:05}"));
            }
        }
        push(&mut out, digits.clone());
    } else {
        push(&mut out, format!("{prefix}{digits}"));
        if let Ok(n) = digits.parse::<u32>() {
            push(&mut out, format!("{prefix}{n}"));
            push(&mut out, format!("{prefix}{n:04}"));
            push(&mut out, format!("{prefix}{n:05}"));
        }
        push(&mut out, digits);
    }
    out
}

static UNREADABLE: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// ISS-28: modules whose file could not be opened during the last scan. They keep
/// `encrypted = true` (so they are never picked as defaults) but are not encrypted; the UI can
/// show «no se pudo leer» for these instead of 🔒. «Actualizar lista» (clear_caches + scan)
/// retries them.
#[allow(dead_code)] // additive API for WP5 (lib.rs)
pub fn scan_unreadable_paths() -> Vec<String> {
    UNREADABLE.lock().clone()
}

/// Unreadable files per scanned folder (review round 2: a cached scan of another folder no
/// longer leaves the previous folder's list behind).
static UNREADABLE_BY_DIR: Lazy<Mutex<HashMap<String, Vec<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// ISS-28: files of `modules_dir` that could not be read as SQLite in its last scan
/// (not a database, e.g. e-Sword 7 MS-Access «.bbl», or open failure). Empty before a scan.
#[allow(dead_code)] // additive API for WP5 (lib.rs)
pub fn scan_unreadable_paths_in(modules_dir: &str) -> Vec<String> {
    UNREADABLE_BY_DIR
        .lock()
        .get(modules_dir)
        .cloned()
        .unwrap_or_default()
}

/// True when an opened file cannot be read as SQLite (not a database, truncated header).
fn db_unreadable(conn: &Connection) -> bool {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
        .is_err()
}

pub fn scan_modules(modules_dir: &str) -> Result<Vec<ModuleInfo>, String> {
    let dir = PathBuf::from(modules_dir);
    if !dir.is_dir() {
        return Err(format!("Carpeta de módulos no válida: {modules_dir}"));
    }

    let fp = fingerprint_dir(&dir);
    {
        let cache = MODULE_LIST_CACHE.lock();
        if let Some(c) = cache.as_ref() {
            if c.path == modules_dir && c.fingerprint == fp {
                if let Some(u) = UNREADABLE_BY_DIR.lock().get(modules_dir) {
                    *UNREADABLE.lock() = u.clone();
                }
                return Ok(c.modules.clone());
            }
        }
    }

    let mut modules = Vec::new();
    let mut unreadable = Vec::new();
    let rd = fs::read_dir(&dir).map_err(|e| format!("No se pudo leer carpeta: {e}"))?;

    for ent in rd.flatten() {
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        let Some(kind) = kind_from_ext(&ext) else {
            continue;
        };
        let path_str = path.to_string_lossy().to_string();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let mut title = filename.clone();
        let mut abbreviation = filename
            .trim_end_matches(&format!(".{ext}"))
            .to_string();
        let mut version = 0;
        let mut has_strongs = false;
        // Every module is judged by its content, as the Windows build does (2026-11-18): the
        // legacy extensions (.bblx/.cmtx/.dctx/.lexx/.refx/.devx) are mostly plain text — 70 of
        // this library's 72 .bblx Bibles. Only e-Sword's ciphertext is 🔒 (`sample_ciphertext`).
        let encrypted: bool;

        // Skip bad/unreadable files without aborting the whole scan
        match open_ro(&path_str) {
            Ok(conn) => {
                // SQLite opens lazily: a non-database file only fails on its first read.
                // Listed as unreadable; `encrypted` keeps today's value (review round 2).
                if db_unreadable(&conn) {
                    unreadable.push(path_str.clone());
                }
                let (t, a, v, s) = read_details(&conn);
                if !t.is_empty() {
                    title = t;
                }
                if !a.is_empty() {
                    abbreviation = a;
                }
                version = v;
                has_strongs = s || kind == "lexicon";
                encrypted = sample_encrypted(&conn, kind) || sample_ciphertext(&conn, kind);
            }
            Err(_) => {
                encrypted = true;
                unreadable.push(path_str.clone());
            }
        }

        modules.push(ModuleInfo {
            path: path_str,
            filename,
            title,
            abbreviation,
            kind: kind.to_string(),
            encrypted,
            has_strongs,
            version,
        });
    }

    modules.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(a.abbreviation.to_lowercase().cmp(&b.abbreviation.to_lowercase()))
    });

    UNREADABLE_BY_DIR
        .lock()
        .insert(modules_dir.to_string(), unreadable.clone());
    *UNREADABLE.lock() = unreadable;
    {
        let mut cache = MODULE_LIST_CACHE.lock();
        *cache = Some(ModuleListCache {
            path: modules_dir.to_string(),
            fingerprint: fp,
            modules: modules.clone(),
        });
    }

    Ok(modules)
}

// ─── Strong-tagged Bible layout (ISS-36) ───

static OWN_TAG_CACHE: Lazy<Mutex<HashMap<String, (FileIdent, bool)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Sample verses spread over both testaments for layout detection.
const LAYOUT_SAMPLE: &[(i32, i32, i32)] = &[
    (1, 1, 1),
    (1, 12, 3),
    (2, 20, 2),
    (3, 19, 18),
    (5, 6, 5),
    (6, 1, 9),
    (9, 16, 7),
    (19, 23, 1),
    (20, 3, 5),
    (23, 53, 5),
    (24, 29, 11),
    (26, 36, 26),
    (39, 3, 10),
    (40, 5, 3),
    (41, 1, 1),
    (42, 2, 11),
    (43, 3, 16),
    (44, 2, 38),
    (45, 8, 28),
    (66, 22, 21),
];

/// Pure layout rule: ≥90% of `<num>` groups follow a `<tvm>` (word, then morph, then number),
/// with enough groups to judge. Rejects AA-rv1960+ (number before word) and untagged Bibles.
fn own_tag_layout_ok(num_groups: usize, after_tvm: usize) -> bool {
    num_groups >= 10 && after_tvm * 100 >= num_groups * 90
}

/// ISS-36: true when this Bible carries its own `palabra <tvm>m</tvm> <num>X</num>` tags
/// (00RVR1960+, 10LBLA+, 00LBLA+). Detected once per file from 20 sampled verses.
pub fn bible_has_own_tags(module_path: &str) -> bool {
    let ident = file_ident(module_path);
    if let Some(id) = ident {
        if let Some((cid, v)) = OWN_TAG_CACHE.lock().get(module_path) {
            if *cid == id {
                return *v;
            }
        }
    }
    let verdict = with_conn(module_path, |conn| {
        let mut groups = 0usize;
        let mut after = 0usize;
        let mut st = conn
            .prepare("SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3")
            .map_err(|e| es_error(&e.to_string()))?;
        for (b, c, v) in LAYOUT_SAMPLE {
            if let Ok(raw) = st.query_row(rusqlite::params![b, c, v], |r| cell_to_string(r, 0)) {
                let s = own_tag_layout_stats(&raw);
                groups += s.num_groups;
                after += s.after_tvm;
            }
        }
        Ok(own_tag_layout_ok(groups, after))
    })
    .unwrap_or(false);
    if let Some(id) = ident {
        OWN_TAG_CACHE
            .lock()
            .insert(module_path.to_string(), (id, verdict));
    }
    verdict
}

pub fn load_bible_chapter(
    module_path: &str,
    book: i32,
    chapter: i32,
) -> Result<ChapterPayload, String> {
    // Detected outside with_conn: the connection mutex is not re-entrant.
    let tagged = bible_has_own_tags(module_path);
    let ident = file_ident(module_path);
    with_conn(module_path, |conn| {
        let (title, abbr, _, _) = read_details(conn);
        let rows = chapter_raw_rows(conn, book, chapter)?;
        if rows.iter().any(|(_, s)| looks_encrypted(s)) {
            return Err("Este módulo parece cifrado (🔒). Use un módulo .bbli moderno sin DRM.".into());
        }
        let raws: Vec<&str> = rows.iter().map(|(_, s)| s.as_str()).collect();
        let text_lines = chapter_reads_text_titles(&raws);
        remember_text_titles(module_path, ident, book, chapter, text_lines);
        let mut verses = Vec::with_capacity(rows.len());
        for (verse, scripture) in &rows {
            let (html, titles) = render_bible_verse(scripture, tagged, text_lines);
            let plain = to_plain(&html);
            verses.push(VerseRow { verse: *verse, html, plain, titles });
        }

        Ok(ChapterPayload {
            book,
            chapter,
            module: module_path.to_string(),
            abbreviation: abbr,
            title,
            verses,
        })
    })
}

pub fn load_verse_text(
    module_path: &str,
    book: i32,
    chapter: i32,
    verse: i32,
) -> Result<String, String> {
    let scripture: String = with_conn(module_path, |conn| {
        conn.query_row(
            "SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3",
            rusqlite::params![book, chapter, verse],
            |row| cell_to_string(row, 0),
        )
        .map_err(|_| "Versículo no encontrado".to_string())
    })?;
    // The verse as the Bible tab shows it: without its section titles.
    let text_lines = chapter_text_titles(module_path, book, chapter);
    let (html, _) = render_bible_verse(&scripture, false, text_lines);
    Ok(to_plain(&html))
}

// ─── Section titles inside verse text (bible_titles.rs, ported from Windows) ───

/// A verse as the Bible tab shows it: its HTML without its section titles, and the titles.
/// `text_lines`: read plain-text lines as titles (the chapter's guard, see below).
pub(crate) fn render_bible_verse(raw: &str, tagged: bool, text_lines: bool) -> (String, Vec<VerseTitle>) {
    let clean = |s: &str| if tagged { clean_bible_verse_tagged(s) } else { clean_bible_text(s) };
    let found = crate::bible_titles::find_titles(raw, text_lines);
    if found.is_empty() {
        return (clean(raw), Vec::new());
    }
    let (pieces, body) = found.split(raw);
    let titles = pieces
        .into_iter()
        .filter_map(|(kind, piece)| {
            let html = clean(piece);
            (!to_plain(&html).is_empty()).then(|| VerseTitle { kind: kind.as_str().to_string(), html })
        })
        .collect();
    (clean(&body), titles)
}

/// Plain-text titles are read only when few verses of the chapter carry them: a module that
/// opens every verse on a new line would turn its first lines into titles.
fn chapter_reads_text_titles(raws: &[&str]) -> bool {
    let leads = raws
        .iter()
        // Only plain-text verses can open with title lines (HTML and RTF never count here).
        .filter(|raw| !raw.contains('<') && !raw.contains("{\\"))
        .filter(|raw| crate::bible_titles::find_titles(raw, true).has_text_lead(raw))
        .count();
    leads <= 3 || leads * 5 <= raws.len() * 2
}

fn chapter_raw_rows(conn: &Connection, book: i32, chapter: i32) -> Result<Vec<(i32, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT Verse, Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 ORDER BY Verse")
        .map_err(|e| es_error(&e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params![book, chapter], |row| Ok((row.get::<_, i32>(0)?, cell_to_string(row, 1)?)))
        .map_err(|e| es_error(&e.to_string()))?;
    Ok(rows.flatten().collect())
}

/// The chapter guard per (module, book, chapter), so a single verse (a tap, a preview) is cut
/// exactly as its chapter is shown. Small and cleared when full: one chapter read each.
static TEXT_TITLES_CACHE: Lazy<Mutex<HashMap<(String, i32, i32), (Option<FileIdent>, bool)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
const TEXT_TITLES_CACHE_MAX: usize = 2048;

fn remember_text_titles(module_path: &str, ident: Option<FileIdent>, book: i32, chapter: i32, on: bool) {
    let mut cache = TEXT_TITLES_CACHE.lock();
    if cache.len() >= TEXT_TITLES_CACHE_MAX {
        cache.clear();
    }
    cache.insert((module_path.to_string(), book, chapter), (ident, on));
}

fn chapter_text_titles(module_path: &str, book: i32, chapter: i32) -> bool {
    let ident = file_ident(module_path);
    if let Some((id, on)) = TEXT_TITLES_CACHE.lock().get(&(module_path.to_string(), book, chapter)) {
        if ident.is_some() && *id == ident {
            return *on;
        }
    }
    let on = with_conn(module_path, |conn| {
        let rows = chapter_raw_rows(conn, book, chapter)?;
        let raws: Vec<&str> = rows.iter().map(|(_, s)| s.as_str()).collect();
        Ok(chapter_reads_text_titles(&raws))
    })
    .unwrap_or(true);
    remember_text_titles(module_path, ident, book, chapter, on);
    on
}

fn load_raw_verse(module_path: &str, book: i32, chapter: i32, verse: i32) -> Option<String> {
    with_conn(module_path, |conn| {
        conn.query_row(
            "SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3",
            rusqlite::params![book, chapter, verse],
            |row| cell_to_string(row, 0),
        )
        .map_err(|e| e.to_string())
    })
    .ok()
}

// ─── Commentary (ISS-10, ISS-12, ISS-17) ───

/// One VerseCommentary row header, decoded exactly like the original loader
/// (`get::<i32>` falling back to 0; e.g. CEE2011 stores ChapterEnd 'º' as TEXT → 0).
#[derive(Debug, Clone, Copy)]
struct CmtRow {
    rowid: i64,
    cb: i32,
    vb: i32,
    ce: i32,
    ve: i32,
}

impl CmtRow {
    fn end(&self) -> (i32, i32) {
        (
            if self.ce == 0 { self.cb } else { self.ce },
            if self.ve == 0 { self.vb } else { self.ve },
        )
    }

    /// The original range rule (unchanged semantics).
    fn covers(&self, chapter: i32, verse: i32) -> bool {
        let (end_ch, end_v) = self.end();
        let after_start = chapter > self.cb || (chapter == self.cb && verse >= self.vb);
        let before_end = chapter < end_ch || (chapter == end_ch && verse <= end_v);
        let single = self.ce == 0 && self.ve == 0 && self.cb == chapter && self.vb == verse;
        single || (after_start && before_end)
    }

    fn label(&self) -> String {
        let (end_ch, end_v) = self.end();
        if self.cb == end_ch && self.vb == end_v {
            format!("v.{}", self.vb)
        } else if self.cb == end_ch {
            format!("v.{}–{}", self.vb, end_v)
        } else {
            format!("{}:{}–{}:{}", self.cb, self.vb, end_ch, end_v)
        }
    }

    fn is_exact_single(&self, chapter: i32, verse: i32) -> bool {
        let (end_ch, end_v) = self.end();
        self.cb == end_ch && self.vb == end_v && self.cb == chapter && self.vb == verse
    }
}

/// Matching verse rows without reading Comments. The SQL predicate is a strict superset of
/// the Rust rule (only rows whose integer ChapterBegin is after the chapter are dropped), so
/// range semantics are exactly the original ones. Sorted: exact single-verse rows first,
/// then (ChapterBegin, VerseBegin, rowid) (ISS-17).
fn verse_cmt_rows(
    conn: &Connection,
    table: &str,
    book: i32,
    chapter: i32,
    verse: i32,
) -> Result<Vec<CmtRow>, String> {
    let sql = format!(
        "SELECT rowid, ChapterBegin, VerseBegin, ChapterEnd, VerseEnd FROM \"{table}\"
         WHERE Book=?1 AND (typeof(ChapterBegin)<>'integer' OR ChapterBegin<=?2)"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| es_error(&e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params![book, chapter], |row| {
            Ok(CmtRow {
                rowid: row.get::<_, i64>(0)?,
                cb: row.get::<_, i32>(1).unwrap_or(0),
                vb: row.get::<_, i32>(2).unwrap_or(0),
                ce: row.get::<_, Option<i32>>(3).ok().flatten().unwrap_or(0),
                ve: row.get::<_, Option<i32>>(4).ok().flatten().unwrap_or(0),
            })
        })
        .map_err(|e| es_error(&e.to_string()))?;
    let mut out: Vec<CmtRow> = rows.flatten().filter(|r| r.covers(chapter, verse)).collect();
    out.sort_by_key(|r| (!r.is_exact_single(chapter, verse), r.cb, r.vb, r.rowid));
    Ok(out)
}

fn cmt_by_rowid(conn: &Connection, table: &str, rowid: i64) -> String {
    conn.query_row(
        &format!("SELECT Comments FROM \"{table}\" WHERE rowid=?1"),
        rusqlite::params![rowid],
        |r| cell_to_string(r, 0),
    )
    .unwrap_or_default()
}

fn dedupe_key(html: &str, plain: &str) -> String {
    let p = collapse_ws(plain);
    if p.is_empty() {
        format!("html:{}", collapse_ws(html))
    } else {
        p
    }
}

#[allow(clippy::too_many_arguments)]
fn load_commentary_in(
    conn: &Connection,
    schema: &Schema,
    book: i32,
    chapter: i32,
    verse: i32,
    include_verse: bool,
    include_chapter: bool,
    include_book: bool,
) -> Result<Vec<StudyEntry>, String> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push = |out: &mut Vec<StudyEntry>, scope: &str, label: String, html: String| {
        let plain = to_plain(&html);
        if seen.insert(dedupe_key(&html, &plain)) {
            out.push(StudyEntry {
                scope: scope.into(),
                label,
                plain,
                html,
            });
        }
    };

    if include_verse {
        let table = schema
            .verse_cmt
            .ok_or_else(|| es_error("no such table: VerseCommentary"))?;
        for r in verse_cmt_rows(conn, table, book, chapter, verse)? {
            let comments = cmt_by_rowid(conn, table, r.rowid);
            if comments.trim().is_empty() {
                continue;
            }
            if looks_encrypted(&comments) {
                return Err("Módulo de comentario cifrado (🔒).".into());
            }
            push(&mut out, "verse", r.label(), titled(clean_esword_text(&comments), &comments));
        }
    }

    if include_chapter {
        if let Some(t) = schema.chapter_cmt {
            if let Ok(comments) = conn.query_row(
                &format!("SELECT Comments FROM \"{t}\" WHERE Book=?1 AND Chapter=?2"),
                rusqlite::params![book, chapter],
                |row| cell_to_string(row, 0),
            ) {
                if !comments.trim().is_empty() {
                    push(
                        &mut out,
                        "chapter",
                        format!("Cap. {chapter}"),
                        titled(clean_esword_text(&comments), &comments),
                    );
                }
            }
        }
    }

    if include_book {
        if let Some(t) = schema.book_cmt {
            if let Ok(comments) = conn.query_row(
                &format!("SELECT Comments FROM \"{t}\" WHERE Book=?1"),
                rusqlite::params![book],
                |row| cell_to_string(row, 0),
            ) {
                if !comments.trim().is_empty() {
                    push(&mut out, "book", "Libro".into(), titled(clean_esword_text(&comments), &comments));
                }
            }
        }
    }

    Ok(out)
}

pub fn load_commentary(
    module_path: &str,
    book: i32,
    chapter: i32,
    verse: i32,
    include_verse: bool,
    include_chapter: bool,
    include_book: bool,
) -> Result<Vec<StudyEntry>, String> {
    with_slot(module_path, |conn, schema| {
        load_commentary_in(
            conn,
            schema,
            book,
            chapter,
            verse,
            include_verse,
            include_chapter,
            include_book,
        )
    })
}

/// Cheap non-empty check: `octet_length` does not read large values; only short values are
/// loaded to rule out whitespace-only or encrypted text.
fn cmt_text_ok(conn: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Option<bool> {
    conn.query_row(sql, params, |r| {
        let n: i64 = r.get::<_, Option<i64>>(0)?.unwrap_or(0);
        let small = cell_to_string(r, 1).unwrap_or_default();
        Ok((n, small))
    })
    .ok()
    .map(|(n, small)| {
        if n <= 0 {
            false
        } else if n > 4096 {
            true
        } else {
            !small.trim().is_empty() && !looks_encrypted(&small)
        }
    })
}

// ─── Topic lookups: shared helpers ───

/// Escape `\`, `%` and `_` for `LIKE … ESCAPE '\'` (ISS-32).
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// ISS-08: articles above this size are cut at a block boundary.
const MAX_ENTRY_HTML: usize = 400 * 1024;

fn truncate_entry_html(html: String) -> (String, bool) {
    if html.len() <= MAX_ENTRY_HTML {
        return (html, false);
    }
    let mut cut = MAX_ENTRY_HTML;
    while !html.is_char_boundary(cut) {
        cut -= 1;
    }
    let head = &html[..cut];
    let boundary = ["<br", "</p>", "<p", "\n"]
        .iter()
        .filter_map(|m| head.rfind(m))
        .max()
        .filter(|&i| i > MAX_ENTRY_HTML / 2)
        .unwrap_or_else(|| tag_safe_cut(head));
    let kb = html.len().div_ceil(1024);
    (
        format!(
            "{}<p>… (texto truncado, {kb} KB)</p>",
            &html[..boundary]
        ),
        true,
    )
}

/// Byte length of `head` without a trailing unfinished tag (`…<a href="x`), so a hard cut
/// with no block boundary nearby never leaves half a tag in the article.
fn tag_safe_cut(head: &str) -> usize {
    match (head.rfind('<'), head.rfind('>')) {
        (Some(lt), gt) if gt.map_or(true, |g| g < lt) && lt > 0 => lt,
        _ => head.len(),
    }
}

/// Displayed topic: legacy word/data dictionaries number homographs `AARÓN_1` (ISS-12).
fn display_topic(topic: &str) -> String {
    if let Some(i) = topic.rfind('_') {
        let d = &topic[i + 1..];
        if i > 0 && !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()) {
            return topic[..i].to_string();
        }
    }
    topic.to_string()
}

fn plain_hash(s: &str) -> u64 {
    let mut fp: u64 = 0xcbf29ce484222325;
    for b in collapse_ws(s).as_bytes() {
        fp ^= *b as u64;
        fp = fp.wrapping_mul(0x100000001b3);
    }
    fp
}

// ─── Section titles in combined modules (sections.rs, ported from Windows) ───
//
// Only the main panel asks for them (`with_section_titles` around one lookup): the rules are
// learned (or loaded) before the lookup, then every article built on this thread while it runs
// gets its title lines wrapped. Probes, the side pane and tooltips never set them.

thread_local! {
    static SECTION_RULES: std::cell::RefCell<Option<Arc<crate::sections::TitleRules>>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` with `module_path`'s section titles on (`on`), off otherwise. The previous setting is
/// put back when `f` ends, even by unwinding (the blocking threads are reused).
pub fn with_section_titles<T>(module_path: &str, on: bool, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<crate::sections::TitleRules>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let prev = self.0.take();
            SECTION_RULES.with(|r| *r.borrow_mut() = prev);
        }
    }
    let rules = if on { section_rules(module_path) } else { None };
    let _restore = Restore(SECTION_RULES.with(|r| r.replace(rules)));
    f()
}

/// The module's title rules: known (memory or saved file), else learned now from a sample of its
/// text tables (the main one first) and kept.
fn section_rules(module_path: &str) -> Option<Arc<crate::sections::TitleRules>> {
    if let Some(r) = crate::sections::known_rules(module_path) {
        return Some(r);
    }
    let entries = with_slot(module_path, |conn, schema| {
        let mut tables: Vec<(&str, &str)> = Vec::new();
        for ts in [schema.lex, schema.dict].into_iter().flatten() {
            if !tables.iter().any(|t| t.0 == ts.table) {
                tables.push((ts.table, ts.def));
            }
        }
        for t in [schema.verse_cmt, schema.chapter_cmt, schema.book_cmt].into_iter().flatten() {
            tables.push((t, "Comments"));
        }
        Ok(crate::sections::sample_entries(conn, &tables))
    })
    .ok()?;
    Some(crate::sections::remember_rules(module_path, &entries))
}

/// `html` (cleaned from `raw`) with its section-title lines wrapped, when the main panel asked.
fn titled(html: String, raw: &str) -> String {
    SECTION_RULES.with(|r| match &*r.borrow() {
        Some(rules) if !rules.is_empty() => crate::sections::wrap_titles(&html, raw, rules),
        _ => html,
    })
}

/// Build a DictEntry from a raw definition. `Ok(None)` for empty definitions or entries that
/// clean to nothing (placeholders); `Err` for encrypted text.
fn make_entry(topic: &str, def: &str, kind: &str, enc_msg: &str) -> Result<Option<DictEntry>, String> {
    if def.trim().is_empty() {
        return Ok(None);
    }
    if looks_encrypted(def) {
        return Err(enc_msg.to_string());
    }
    let (html, truncated) = truncate_entry_html(titled(clean_esword_text(def), def));
    let plain = to_plain(&html);
    if plain.trim().is_empty() && !html.contains("<img") {
        return Ok(None);
    }
    Ok(Some(DictEntry {
        topic: display_topic(topic),
        html,
        plain,
        match_kind: kind.to_string(),
        truncated,
    }))
}

fn def_by_rowid(conn: &Connection, ts: &TopicSchema, rowid: i64) -> String {
    conn.query_row(
        &format!("SELECT \"{}\" FROM \"{}\" WHERE rowid=?1", ts.def, ts.table),
        rusqlite::params![rowid],
        |r| cell_to_string(r, 0),
    )
    .unwrap_or_default()
}

/// Original headword lookup (prefix/contains LIKE), now with escaped wildcards and empty
/// definitions filtered before LIMIT (ISS-32). Used only for non-code lexicon queries.
fn lookup_in_topic_table(
    conn: &Connection,
    ts: &TopicSchema,
    query: &str,
    limit: i32,
) -> Result<Vec<DictEntry>, String> {
    let (table, topic, def) = (ts.table, ts.topic, ts.def);
    let sql = format!(
        "SELECT \"{topic}\", \"{def}\" FROM \"{table}\"
         WHERE (\"{topic}\" = ?1 COLLATE NOCASE
            OR \"{topic}\" LIKE ?2 ESCAPE '\\'
            OR \"{topic}\" LIKE ?3 ESCAPE '\\')
           AND length(trim(\"{def}\"))>0
         ORDER BY
           CASE
             WHEN \"{topic}\" = ?1 COLLATE NOCASE THEN 0
             WHEN \"{topic}\" LIKE ?2 ESCAPE '\\' THEN 1
             ELSE 2
           END,
           length(\"{topic}\"),
           \"{topic}\"
         LIMIT ?4"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| es_error(&e.to_string()))?;
    let esc = like_escape(query);
    let prefix = format!("{esc}%");
    let contains = format!("%{esc}%");
    let rows = stmt
        .query_map(rusqlite::params![query, prefix, contains, limit], |row| {
            Ok((cell_to_string(row, 0)?, cell_to_string(row, 1)?))
        })
        .map_err(|e| es_error(&e.to_string()))?;

    let mut out = Vec::new();
    for r in rows.flatten() {
        let (topic, def) = r;
        if let Some(e) = make_entry(
            &topic,
            &def,
            "headword",
            "Módulo cifrado (🔒). Use un .lexi/.dcti moderno sin DRM.",
        )? {
            out.push(e);
        }
    }
    Ok(out)
}

// ─── Headword variants and Spanish folding (ISS-07) ───

/// Compose base letter + combining acute / grave / tilde / diaeresis (NFC for Spanish text).
fn compose_nfc_es(s: &str) -> String {
    fn pair(b: char, m: char) -> Option<char> {
        let set: &[(char, char, char, char)] = match m {
            '\u{301}' => &[
                ('a', 'á', 'A', 'Á'),
                ('e', 'é', 'E', 'É'),
                ('i', 'í', 'I', 'Í'),
                ('o', 'ó', 'O', 'Ó'),
                ('u', 'ú', 'U', 'Ú'),
                ('y', 'ý', 'Y', 'Ý'),
            ],
            '\u{300}' => &[
                ('a', 'à', 'A', 'À'),
                ('e', 'è', 'E', 'È'),
                ('i', 'ì', 'I', 'Ì'),
                ('o', 'ò', 'O', 'Ò'),
                ('u', 'ù', 'U', 'Ù'),
            ],
            '\u{303}' => &[('n', 'ñ', 'N', 'Ñ'), ('a', 'ã', 'A', 'Ã'), ('o', 'õ', 'O', 'Õ')],
            '\u{308}' => &[
                ('a', 'ä', 'A', 'Ä'),
                ('e', 'ë', 'E', 'Ë'),
                ('i', 'ï', 'I', 'Ï'),
                ('o', 'ö', 'O', 'Ö'),
                ('u', 'ü', 'U', 'Ü'),
            ],
            _ => return None,
        };
        set.iter().find_map(|(lo, lo_c, up, up_c)| {
            if b == *lo {
                Some(*lo_c)
            } else if b == *up {
                Some(*up_c)
            } else {
                None
            }
        })
    }
    if !s.chars().any(|c| matches!(c, '\u{300}' | '\u{301}' | '\u{303}' | '\u{308}')) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if let Some(&m) = it.peek() {
            if let Some(comp) = pair(c, m) {
                out.push(comp);
                it.next();
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Remove Spanish acute/grave/diaeresis accents (ñ is kept).
fn strip_accents_es(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'Á' | 'À' | 'Ä' | 'Â' => 'A',
            'É' | 'È' | 'Ë' | 'Ê' => 'E',
            'Í' | 'Ì' | 'Ï' | 'Î' => 'I',
            'Ó' | 'Ò' | 'Ö' | 'Ô' => 'O',
            'Ú' | 'Ù' | 'Ü' | 'Û' => 'U',
            c => c,
        })
        .collect()
}

/// Diacritic monosyllables and demonstratives whose accent changes the word (ISS-07).
const ACCENT_PROTECTED: &[&str] = &[
    "él", "mí", "tú", "sí", "sé", "dé", "más", "aún", "sólo", "té", "aquél", "aquélla",
    "aquéllos", "aquéllas", "éste", "ésta", "éstos", "éstas", "ése", "ésa", "ésos", "ésas",
];

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// Headword spellings for one query (ISS-07).
#[derive(Debug, Clone)]
struct Variants {
    /// NFC as typed.
    q: String,
    lower: String,
    /// Accent-folded lowercase (ñ kept).
    fold: String,
    /// As typed, lowercase, UPPERCASE, Titlecase.
    primary: Vec<String>,
    /// Accent-stripped copies (only for capitalised, unprotected queries).
    stripped: Vec<String>,
    /// Accent-insensitive comparison allowed (stripping or adding accents).
    fold_ok: bool,
    /// Query is unaccented ASCII of 4+ letters: may find accented headwords (Jehova → JEHOVÁ).
    add_accents: bool,
    qlen: usize,
}

impl Variants {
    fn new(query: &str) -> Variants {
        let q = compose_nfc_es(query.trim());
        let lower = q.to_lowercase();
        let mut primary = Vec::new();
        for v in [q.clone(), lower.clone(), q.to_uppercase(), title_case(&q)] {
            if !v.is_empty() && !primary.contains(&v) {
                primary.push(v);
            }
        }
        let protected = ACCENT_PROTECTED.contains(&lower.as_str());
        let capitalised = q.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        let has_accent = strip_accents_es(&q) != q;
        let mut stripped = Vec::new();
        if capitalised && has_accent && !protected {
            for v in &primary {
                let s = strip_accents_es(v);
                if !primary.contains(&s) && !stripped.contains(&s) {
                    stripped.push(s);
                }
            }
        }
        let qlen = q.chars().count();
        let add_accents = !protected
            && qlen >= 4
            && q.chars().all(|c| c.is_ascii_alphabetic());
        Variants {
            fold: strip_accents_es(&lower),
            fold_ok: !stripped.is_empty() || add_accents,
            q,
            lower,
            primary,
            stripped,
            add_accents,
            qlen,
        }
    }

    fn all(&self) -> Vec<String> {
        let mut v = self.primary.clone();
        v.extend(self.stripped.iter().cloned());
        v
    }
}

/// ISS-07: every headword spelling tried for a query: as typed (NFC), lowercase, UPPERCASE,
/// Titlecase, then accent-stripped copies for capitalised words (never for él/mí/tú/sí/…).
/// The exact tier also tries NBSP- and space-prefixed copies of each.
#[allow(dead_code)] // additive API (tests, WP5)
pub fn headword_variants(q: &str) -> Vec<String> {
    Variants::new(q).all()
}

/// Ranking tiers, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Exact,
    Headword,
    Lemma,
    Phrase,
    LemmaPhrase,
    /// ISS-14 review: masculine guess for a feminine word (puertas → Puerto). Tagged
    /// `lemma`, but never a full match, so tap panes never show a wrong-sense article.
    LemmaGender,
    Verbform,
    Contains,
}

impl Tier {
    fn kind(self) -> &'static str {
        match self {
            Tier::Exact => "exact",
            Tier::Headword | Tier::Phrase => "headword",
            Tier::Lemma | Tier::LemmaPhrase | Tier::LemmaGender => "lemma",
            Tier::Verbform => "verbform",
            Tier::Contains => "contains",
        }
    }
    fn full(self) -> bool {
        matches!(
            self,
            Tier::Exact | Tier::Headword | Tier::Lemma | Tier::Phrase | Tier::LemmaPhrase
        )
    }
}

#[derive(Debug, Clone)]
struct Cand {
    rowid: i64,
    topic: String,
    tier: Tier,
    /// 0 as typed, 1 case-only, 2 accent-insensitive.
    sub: u8,
    suffix: usize,
}

fn classify_suffix(s: &str, qlen: usize) -> Option<Tier> {
    let is_ws = |c: char| c.is_whitespace() || c == '\u{a0}';
    if s.chars().all(is_ws) {
        return Some(Tier::Headword);
    }
    let first = s.chars().next()?;
    if matches!(first, ',' | '.' | ';' | ':' | '(' | '[' | '—' | '–' | '/') {
        return Some(Tier::Headword);
    }
    if first == '_' && s.len() > 1 && s[1..].chars().all(|c| c.is_ascii_digit()) {
        return Some(Tier::Headword);
    }
    if is_ws(first) {
        let rest = s.trim_start_matches(is_ws);
        let c2 = rest.chars().next()?;
        if matches!(c2, '(' | '[' | '—' | '–' | '-' | ',' | ';' | ':') || c2.is_ascii_digit() {
            return Some(Tier::Headword);
        }
        return Some(Tier::Phrase);
    }
    if qlen >= 4 && s.chars().count() <= 3 && s.chars().all(|c| c.is_alphabetic()) {
        return Some(Tier::Verbform);
    }
    None
}

/// Classify one topic against a query: exact (as typed / case-only / accent-insensitive),
/// headword (`VIEJO, A`, `Pan\t`, `HAYA (v.)`, `AARÓN_1`), phrase (`REINO DE DIOS`),
/// verbform (`REPUDIAR` for «repudia»). Mid-word matches (FEBE for «fe», odioso) are never kept.
fn classify_topic(topic: &str, v: &Variants) -> Option<(Tier, u8, usize)> {
    let t = topic.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{a0}');
    if t.is_empty() {
        return None;
    }
    let tn = compose_nfc_es(t);
    if tn == v.q {
        return Some((Tier::Exact, 0, 0));
    }
    let tl = tn.to_lowercase();
    if tl == v.lower {
        return Some((Tier::Exact, 1, 0));
    }
    let tf = if v.fold_ok {
        Some(strip_accents_es(&tl))
    } else {
        None
    };
    if tf.as_deref() == Some(v.fold.as_str()) {
        return Some((Tier::Exact, 2, 0));
    }
    let (suffix, sub) = if let Some(rest) = tl.strip_prefix(v.lower.as_str()) {
        (rest.to_string(), 0u8)
    } else {
        let rest = tf.as_deref().and_then(|f| f.strip_prefix(v.fold.as_str()))?;
        (rest.to_string(), 2u8)
    };
    classify_suffix(&suffix, v.qlen).map(|tier| (tier, sub, suffix.chars().count()))
}

fn topic_rows(conn: &Connection, sql: &str, params: &[String]) -> Vec<(i64, String)> {
    let Ok(mut st) = conn.prepare(sql) else {
        return vec![];
    };
    let Ok(rows) = st.query_map(rusqlite::params_from_iter(params.iter()), |r| {
        Ok((r.get::<_, i64>(0)?, cell_to_string(r, 1)?))
    }) else {
        return vec![];
    };
    rows.flatten().collect()
}

/// Topic rows (rowid, topic) whose topic is a case/accent spelling of `v` or starts with one.
/// Index seeks only (BINARY `IN` + range per variant) when the topic column is indexed;
/// one topic-only scan otherwise (03DRAE2014, OGNTe, Peshito).
fn gather_topic_rows(conn: &Connection, ts: &TopicSchema, v: &Variants) -> Vec<(i64, String)> {
    let (t, c) = (ts.table, ts.topic);
    if !ts.indexed {
        return topic_rows(conn, &format!("SELECT rowid, \"{c}\" FROM \"{t}\""), &[]);
    }
    let mut out = Vec::new();
    let mut exact = v.all();
    for s in v.all() {
        exact.push(format!("\u{a0}{s}"));
        exact.push(format!(" {s}"));
    }
    let ph = (1..=exact.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    out.extend(topic_rows(
        conn,
        &format!("SELECT rowid, \"{c}\" FROM \"{t}\" WHERE \"{c}\" IN ({ph})"),
        &exact,
    ));
    let range_sql =
        format!("SELECT rowid, \"{c}\" FROM \"{t}\" WHERE \"{c}\" >= ?1 AND \"{c}\" < ?2 LIMIT 300");
    for s in v.all() {
        out.extend(topic_rows(conn, &range_sql, &[s.clone(), format!("{s}\u{10FFFF}")]));
    }
    out.extend(gather_phrase_nocase_rows(conn, ts, v));
    out
}

/// ISS-07 review: a multi-token query («reino de dios», «ja-matzot») can differ in case
/// word by word from its headword («Reino de Dios», «Ja-Matzot»), which no single
/// lower/UPPER/Title variant reaches. Legacy (NOCASE / LIKE) found those, so multi-token
/// queries also get one ASCII-case-insensitive prefix scan (LIKE folds ASCII exactly like
/// NOCASE); `classify_topic` then ranks the rows. Single words (every tap) never run it.
/// The scan reads only the covering topic index (≈11 ms on Webster 1913, 116k rows).
fn gather_phrase_nocase_rows(conn: &Connection, ts: &TopicSchema, v: &Variants) -> Vec<(i64, String)> {
    if !ts.indexed || !is_multi_token(&v.q) {
        return vec![];
    }
    let mut pats: Vec<String> = Vec::new();
    let mut keys: HashSet<String> = HashSet::new();
    for s in v.all() {
        for pad in ["", " ", "\u{a0}"] {
            let p = format!("{pad}{}%", like_escape(&s));
            // LIKE is ASCII-case-insensitive: one pattern per ASCII-folded spelling.
            if keys.insert(p.to_ascii_lowercase()) {
                pats.push(p);
            }
        }
    }
    let c = ts.topic;
    let cond = (1..=pats.len())
        .map(|i| format!("\"{c}\" LIKE ?{i} ESCAPE '\\'"))
        .collect::<Vec<_>>()
        .join(" OR ");
    topic_rows(
        conn,
        &format!("SELECT rowid, \"{c}\" FROM \"{}\" WHERE {cond} LIMIT 2000", ts.table),
        &pats,
    )
}

/// More than one word: whitespace (incl. NBSP) or a hyphen.
fn is_multi_token(q: &str) -> bool {
    q.trim().chars().any(|c| c.is_whitespace() || c == '\u{a0}' || c == '-')
}

/// Accent-adding ranges for unaccented queries (Jehova → JEHOVÁ, espiritu → Espíritu):
/// the prefix up to the first vowel after the first letter, in each case form.
fn gather_add_accent_rows(conn: &Connection, ts: &TopicSchema, v: &Variants) -> Vec<(i64, String)> {
    if !ts.indexed || !v.add_accents {
        return vec![];
    }
    let chars: Vec<char> = v.lower.chars().collect();
    let k = chars
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, c)| "aeiou".contains(**c))
        .map(|(i, _)| i)
        .unwrap_or(chars.len());
    let mut prefixes: Vec<String> = Vec::new();
    let base: String = chars[..k].iter().collect();
    if "aeiou".contains(chars[0]) {
        // A leading vowel may itself carry the accent (Éfeso).
        let acc = strip_accents_es(&base);
        for (plain, accented) in [('a', 'á'), ('e', 'é'), ('i', 'í'), ('o', 'ó'), ('u', 'ú')] {
            if acc.starts_with(plain) {
                let mut s = String::new();
                s.push(accented);
                s.push_str(&base[plain.len_utf8()..]);
                prefixes.push(s);
            }
        }
    }
    prefixes.push(base);
    let range_sql = format!(
        "SELECT rowid, \"{c}\" FROM \"{t}\" WHERE \"{c}\" >= ?1 AND \"{c}\" < ?2 LIMIT 600",
        c = ts.topic,
        t = ts.table
    );
    let mut out = Vec::new();
    for p in prefixes {
        let mut forms = vec![p.clone(), p.to_uppercase(), title_case(&p)];
        forms.dedup();
        for f in forms {
            // Same padded spellings as the exact tier (DiccBibl1 stores « JERUSALÉN»).
            for pad in ["", " ", "\u{a0}"] {
                let g = format!("{pad}{f}");
                out.extend(topic_rows(conn, &range_sql, &[g.clone(), format!("{g}\u{10FFFF}")]));
            }
        }
    }
    out
}

/// ISS-14: singular / masculine candidates for a plural or feminine Spanish word.
/// Number candidates first, then gender candidates (see [`lemma_candidates_staged`]).
#[allow(dead_code)] // public API kept (tests, WP5); lookups use lemma_candidates_staged
pub fn lemma_candidates(word: &str) -> Vec<String> {
    let (mut number, gender) = lemma_candidates_staged(word);
    for g in gender {
        if !number.contains(&g) {
            number.push(g);
        }
    }
    number
}

/// ISS-14 review: (number candidates, gender candidates). Number: -ces→z, -es after a
/// consonant, -s. Gender: -as→-o, -a→-o; tried only when no number candidate is a headword
/// of the module (puertas → Puerta, never also Puerto). Candidates have ≥3 letters.
fn lemma_candidates_staged(word: &str) -> (Vec<String>, Vec<String>) {
    let w = compose_nfc_es(word.trim()).to_lowercase();
    let mut number: Vec<String> = Vec::new();
    let mut gender: Vec<String> = Vec::new();
    let ok = |s: &String, out: &Vec<String>| s.chars().count() >= 3 && *s != w && !out.contains(s);
    let is_cons = |c: char| c.is_alphabetic() && !"aeiouáéíóúü".contains(c);
    let mut push_n = |s: String| {
        if ok(&s, &number) {
            number.push(s);
        }
    };
    if let Some(stem) = w.strip_suffix("ces") {
        push_n(format!("{stem}z"));
    }
    if let Some(stem) = w.strip_suffix("es") {
        if stem.chars().last().map(is_cons).unwrap_or(false) {
            push_n(stem.to_string());
        }
    }
    if let Some(stem) = w.strip_suffix('s') {
        push_n(stem.to_string());
    }
    let g = if let Some(stem) = w.strip_suffix("as") {
        Some(format!("{stem}o"))
    } else {
        w.strip_suffix('a').map(|stem| format!("{stem}o"))
    };
    if let Some(g) = g {
        if ok(&g, &gender) && !number.contains(&g) {
            gender.push(g);
        }
    }
    (number, gender)
}

/// All ranked candidates of one module for a query (ISS-07/08/14). `search` enables the
/// whole-token `contains` tier (explicit search box only).
fn ranked_candidates(conn: &Connection, ts: &TopicSchema, query: &str, search: bool) -> Vec<Cand> {
    let v = Variants::new(query);
    if v.q.is_empty() {
        return vec![];
    }
    let mut by_row: HashMap<i64, Cand> = HashMap::new();
    let add = |by_row: &mut HashMap<i64, Cand>, rowid: i64, topic: String, tier: Tier, sub: u8, suffix: usize| {
        let c = Cand {
            rowid,
            topic,
            tier,
            sub,
            suffix,
        };
        match by_row.get(&rowid) {
            Some(old) if (old.tier, old.sub, old.suffix) <= (c.tier, c.sub, c.suffix) => {}
            _ => {
                by_row.insert(rowid, c);
            }
        }
    };
    for (rowid, topic) in gather_topic_rows(conn, ts, &v) {
        if let Some((tier, sub, suf)) = classify_topic(&topic, &v) {
            add(&mut by_row, rowid, topic, tier, sub, suf);
        }
    }
    let has_cased_exact = by_row.values().any(|c| c.tier == Tier::Exact && c.sub < 2);
    if !has_cased_exact && v.add_accents {
        for (rowid, topic) in gather_add_accent_rows(conn, ts, &v) {
            if let Some((tier, sub, suf)) = classify_topic(&topic, &v) {
                add(&mut by_row, rowid, topic, tier, sub, suf);
            }
        }
    }
    // Accent-insensitive hits count only when the module has no as-typed / case-only hit.
    if by_row
        .values()
        .any(|c| c.sub < 2 && matches!(c.tier, Tier::Exact | Tier::Headword))
    {
        by_row.retain(|_, c| c.sub < 2);
    }
    // ISS-14: singular / masculine fallback when there is no exact or tight headword hit.
    // Two stages (review): gender guesses run only when no number candidate is a headword
    // here, and are never full matches (casas → Casa, never Caso; reina ↛ full Reino).
    let blocked = by_row
        .values()
        .any(|c| matches!(c.tier, Tier::Exact | Tier::Headword));
    if !blocked {
        let (number, gender) = lemma_candidates_staged(&v.q);
        let mut number_hit = false;
        for (stage, cands) in [(0u8, number), (1u8, gender)] {
            if stage == 1 && number_hit {
                break;
            }
            for lc in cands {
                let lv = Variants::new(&lc);
                let mut rows = gather_topic_rows(conn, ts, &lv);
                rows.extend(gather_add_accent_rows(conn, ts, &lv));
                for (rowid, topic) in rows {
                    if let Some((tier, sub, suf)) = classify_topic(&topic, &lv) {
                        let t2 = match (stage, tier) {
                            (0, Tier::Exact | Tier::Headword) => {
                                number_hit = true;
                                Tier::Lemma
                            }
                            (0, Tier::Phrase) => Tier::LemmaPhrase,
                            // Gender: headwords only (puertas ↛ «Puerto Rico»).
                            (1, Tier::Exact | Tier::Headword) => Tier::LemmaGender,
                            _ => continue,
                        };
                        add(&mut by_row, rowid, topic, t2, sub, suf);
                    }
                }
            }
        }
    }
    if search && v.qlen >= 4 {
        let pat = format!("%{}%", like_escape(&v.q));
        let sql = format!(
            "SELECT rowid, \"{c}\" FROM \"{t}\" WHERE \"{c}\" LIKE ?1 ESCAPE '\\' LIMIT 500",
            c = ts.topic,
            t = ts.table
        );
        for (rowid, topic) in topic_rows(conn, &sql, &[pat]) {
            if by_row.contains_key(&rowid) {
                continue;
            }
            let tf = strip_accents_es(&compose_nfc_es(&topic).to_lowercase());
            // whole token, or its simple plural («Acción de Gracias» for «gracia»)
            let whole = tf.split(|c: char| !c.is_alphanumeric()).any(|tok| {
                tok == v.fold
                    || tok.strip_suffix('s') == Some(v.fold.as_str())
                    || tok.strip_suffix("es") == Some(v.fold.as_str())
            });
            if whole {
                add(&mut by_row, rowid, topic, Tier::Contains, 0, 0);
            }
        }
    }
    let mut out: Vec<Cand> = by_row.into_values().collect();
    out.sort_by(|a, b| {
        (a.tier, a.sub, a.suffix, a.topic.chars().count(), a.rowid).cmp(&(
            b.tier,
            b.sub,
            b.suffix,
            b.topic.chars().count(),
            b.rowid,
        ))
    });
    out
}

/// Fetch, clean and dedupe definitions for ranked candidates until `limit` entries.
fn entries_from_cands(
    conn: &Connection,
    ts: &TopicSchema,
    cands: &[Cand],
    limit: usize,
    full_only: bool,
) -> Result<Vec<DictEntry>, String> {
    let mut out = Vec::new();
    let mut seen: HashSet<(String, u64)> = HashSet::new();
    for c in cands {
        if out.len() >= limit {
            break;
        }
        if full_only && !c.tier.full() {
            continue;
        }
        let def = def_by_rowid(conn, ts, c.rowid);
        if let Some(e) = make_entry(
            &c.topic,
            &def,
            c.tier.kind(),
            "Módulo cifrado (🔒). Use un .lexi/.dcti moderno sin DRM.",
        )? {
            if seen.insert((e.topic.clone(), plain_hash(&e.plain))) {
                out.push(e);
            }
        }
    }
    Ok(out)
}

/// Options for [`dictionary_lookup_ranked`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DictLookupOpts {
    /// Explicit search-box query: adds whole-token `contains` matches (never on taps/probes).
    pub search: bool,
}

/// ISS-07/08/14/32: ranked dictionary lookup. Entries come in tier order
/// exact → headword → lemma → phrase headword → verbform → contains, each tagged with
/// `match_kind`; empty and placeholder definitions are skipped before `limit` applies.
pub fn dictionary_lookup_ranked(
    module_path: &str,
    query: &str,
    limit: i32,
    opts: DictLookupOpts,
) -> Result<Vec<DictEntry>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let limit = if limit <= 0 { 200 } else { limit as usize };
    with_slot(module_path, |conn, schema| {
        let ts = schema
            .dict
            .ok_or_else(|| "Este módulo no tiene tabla Dictionary/Lexicon.".to_string())?;
        let cands = ranked_candidates(conn, &ts, q, opts.search);
        entries_from_cands(conn, &ts, &cands, limit, false)
    })
}

/// Dictionary lookup for taps and the current UI (no `contains` tier). Same as
/// [`dictionary_lookup_ranked`] with default options.
pub fn dictionary_lookup(module_path: &str, query: &str, limit: i32) -> Result<Vec<DictEntry>, String> {
    dictionary_lookup_ranked(module_path, query, limit, DictLookupOpts::default())
}

// ─── Dictionary topic index, e-Sword style (user choice 2026-11-11) ───
//
// The Diccionario view lists the module's topics alphabetically beside the article, like
// e-Sword: a tap jumps to the exact topic or marks where the word would be, and the reader
// opens any topic with a click. Big modules have tens of thousands of topics, so the UI asks
// for windows of the sorted list; the sorted list is built once per file and cached.

/// One row of a dictionary's (or lexicon's) alphabetical topic index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRow {
    /// Position in the sorted index (0-based).
    pub pos: u32,
    /// The topic's row in the module (opens exactly this article).
    pub rowid: i64,
    pub topic: String,
    /// Léxico: the Greek/Hebrew headword of a Strong's-keyed row («G26» → «ἀγάπη»); empty when
    /// the entry does not print one at its start.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lemma: String,
}

/// A window of the topic index, around a query or an anchor position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexSlice {
    pub total: u32,
    /// Position of `rows[0]`.
    pub start: u32,
    /// Where the query is in the index: its topic, or the position it would take.
    pub focus: Option<u32>,
    /// The topic at `focus` is the query itself (case and accents aside).
    pub exact: bool,
    pub rows: Vec<IndexRow>,
    /// Most topics are Strong's codes (a Strong lexicon, or Vine with numbers).
    #[serde(default)]
    pub strong_keyed: bool,
    /// First `G…` / `H…` row, for the Léxico's «Griego» / «Hebreo» jumps.
    #[serde(default)]
    pub greek_start: Option<u32>,
    #[serde(default)]
    pub hebrew_start: Option<u32>,
}

struct TopicIndex {
    ts: TopicSchema,
    keys: Vec<String>,
    rows: Vec<(i64, String)>,
    /// `b'G'` / `b'H'` for a Strong's-coded topic, 0 otherwise (same order as `rows`).
    tags: Vec<u8>,
    greek_start: Option<u32>,
    hebrew_start: Option<u32>,
    strong_rows: u32,
    /// rowid → headword, read lazily for the rows of the windows shown ("" = none).
    lemmas: Mutex<HashMap<i64, String>>,
}

/// Key: `(module path, lexicon table first)` — a module with both tables indexes each one.
static TOPIC_INDEX: Lazy<Mutex<HashMap<(String, bool), (Option<FileIdent>, Arc<TopicIndex>)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Upper bound for one window (the UI asks for ~120 rows).
const INDEX_MAX_WINDOW: u32 = 1000;

/// Spanish dictionary order: case and accents ignored, ñ right after n, leading spaces and
/// punctuation («¡», «"», «(», «*») ignored, inner spaces collapsed, and numbers in natural
/// order (Thompson's chain topics 1, 2, 3 … 10, never 1, 10, 100, 1000, 2).
pub(crate) fn index_key(topic: &str) -> String {
    const NUM_WIDTH: usize = 12;
    let t = compose_nfc_es(topic);
    let t = t.trim_start_matches(|c: char| !c.is_alphanumeric());
    let folded = strip_accents_es(&collapse_ws(t).to_lowercase());
    let mut out = String::with_capacity(folded.len() + 16);
    let mut digits = String::new();
    let flush = |digits: &mut String, out: &mut String| {
        if !digits.is_empty() {
            // Left-pad each digit run so 2 < 10 (runs longer than the width keep their order).
            for _ in digits.len()..NUM_WIDTH {
                out.push('0');
            }
            out.push_str(digits);
            digits.clear();
        }
    };
    for c in folded.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        flush(&mut digits, &mut out);
        if c == 'ñ' {
            out.push_str("n~"); // '~' sorts after every letter: n < ñ < o
        } else {
            out.push(c);
        }
    }
    flush(&mut digits, &mut out);
    out
}

/// `Some('G' | 'H')` when a topic is a Strong's code: G26, h1254, "G 26", G00001, H1254a.
/// Bare numbers ("1", "2") and other prefixes (Peshito's A200001) are not.
pub(crate) fn strong_prefix(topic: &str) -> Option<char> {
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
    let mut tail = rest[digits..].chars();
    match (tail.next(), tail.next()) {
        (None, _) => Some(p),
        (Some(c), None) if c.is_ascii_alphabetic() => Some(p),
        _ => None,
    }
}

/// The key a typed query is looked up by: «H 01254» is the code H1254.
fn index_query_key(q: &str) -> String {
    if strong_prefix(q).is_some() {
        index_key(&q.split_whitespace().collect::<String>())
    } else {
        index_key(q)
    }
}

fn build_topic_index(conn: &Connection, ts: &TopicSchema) -> Result<TopicIndex, String> {
    let sql = format!("SELECT rowid, \"{}\" FROM \"{}\"", ts.topic, ts.table);
    let mut st = conn.prepare(&sql).map_err(|e| es_error(&e.to_string()))?;
    let rows = st
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, cell_to_string(r, 1)?)))
        .map_err(|e| es_error(&e.to_string()))?;
    let mut all: Vec<(String, i64, String)> = rows
        .flatten()
        .filter_map(|(rowid, topic)| {
            let shown = display_topic(&topic).trim().to_string();
            let key = index_key(&shown);
            (!key.is_empty()).then_some((key, rowid, shown))
        })
        .collect();
    all.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    let n = all.len();
    let mut idx = TopicIndex {
        ts: *ts,
        keys: Vec::with_capacity(n),
        rows: Vec::with_capacity(n),
        tags: Vec::with_capacity(n),
        greek_start: None,
        hebrew_start: None,
        strong_rows: 0,
        lemmas: Mutex::new(HashMap::new()),
    };
    for (pos, (k, rowid, topic)) in all.into_iter().enumerate() {
        let tag = strong_prefix(&topic).map_or(0, |c| c as u8);
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
        idx.keys.push(k);
        idx.rows.push((rowid, topic));
        idx.tags.push(tag);
    }
    Ok(idx)
}

/// The topic table an index (and its entries) reads: a lexicon reads `Lexicon` first, like
/// `lexicon_lookup`; a dictionary `Dictionary` first, like `dictionary_lookup`.
fn index_table(schema: &Schema, lexicon: bool) -> Result<TopicSchema, String> {
    let ts = if lexicon { schema.lex } else { schema.dict };
    ts.ok_or_else(|| "Este módulo no tiene tabla Dictionary/Lexicon.".to_string())
}

fn topic_index_for(module_path: &str, lexicon: bool) -> Result<Arc<TopicIndex>, String> {
    let ident = file_ident(module_path);
    let key = (module_path.to_string(), lexicon);
    if let Some((id, idx)) = TOPIC_INDEX.lock().get(&key) {
        if ident.is_some() && *id == ident {
            return Ok(idx.clone());
        }
    }
    let idx = with_slot(module_path, |conn, schema| build_topic_index(conn, &index_table(schema, lexicon)?))?;
    let idx = Arc::new(idx);
    TOPIC_INDEX.lock().insert(key, (ident, idx.clone()));
    Ok(idx)
}

/// Raw characters of a definition read to find its headword.
const LEMMA_SOURCE_CHARS: u32 = 2000;

/// Headwords for the Strong's rows in `start..end`, reading only rows not seen before.
fn index_lemmas(module_path: &str, idx: &TopicIndex, start: u32, end: u32) -> HashMap<i64, String> {
    let mut out = HashMap::new();
    let wanted: Vec<(i64, u8, String)> = {
        let known = idx.lemmas.lock();
        (start..end)
            .filter_map(|p| {
                let (rowid, topic) = &idx.rows[p as usize];
                let tag = idx.tags[p as usize];
                if tag == 0 {
                    return None;
                }
                match known.get(rowid) {
                    Some(l) => {
                        if !l.is_empty() {
                            out.insert(*rowid, l.clone());
                        }
                        None
                    }
                    None => Some((*rowid, tag, topic.clone())),
                }
            })
            .collect()
    };
    if wanted.is_empty() {
        return out;
    }
    let ts = idx.ts;
    let read = with_slot(module_path, |conn, _| {
        let sql = format!(
            "SELECT CASE WHEN typeof(\"{d}\") = 'text' THEN substr(\"{d}\", 1, {LEMMA_SOURCE_CHARS}) ELSE \"{d}\" END \
             FROM \"{t}\" WHERE rowid=?1",
            d = ts.def,
            t = ts.table
        );
        let mut st = conn.prepare_cached(&sql).map_err(|e| e.to_string())?;
        let mut got = Vec::with_capacity(wanted.len());
        for (rowid, tag, code) in &wanted {
            let def: String = st.query_row([rowid], |r| cell_to_string(r, 0)).unwrap_or_default();
            let lemma = if def.is_empty() {
                String::new()
            } else {
                extract_lemma(&to_plain(&clean_esword_text(&def)), *tag as char, code).unwrap_or_default()
            };
            got.push((*rowid, lemma));
        }
        Ok(got)
    })
    .unwrap_or_default();
    let mut known = idx.lemmas.lock();
    for (rowid, lemma) in read {
        if !lemma.is_empty() {
            out.insert(rowid, lemma.clone());
        }
        known.insert(rowid, lemma);
    }
    out
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

/// A headword printed later than this (in characters) is not the entry's own word: Vine's
/// G26 is a 274-character preface before any Greek.
const LEMMA_WINDOW: usize = 40;
/// Longest headword accepted, in characters (pointed Hebrew counts its vowel points).
const LEMMA_MAX_CHARS: usize = 40;

/// The Strong's code right after a word, as in «ἀγάπη G26» or «λαλέω[G2980]».
fn adjacent_code(rest: &[char]) -> Option<String> {
    let mut i = 0;
    while i < rest.len() && matches!(rest[i], ' ' | '[' | '(') {
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

/// The headword of a Strong's entry (ported from the Windows build): the first Hebrew word (H
/// codes) or Greek word (G codes) of its plain text, accepted only when it starts within the
/// first [`LEMMA_WINDOW`] characters and stands as a word of its own («ἀγάπη agápe; de G25…»,
/// «Número Strong: G26 Lexema: ἀγάπη»). Rejected, because each would show a wrong word: a word
/// glued to letters of another script (Swanson's «ἀγάπē»), a form from an occurrence list
/// («θεε (16)»), and a word labelled with another code («λαλέω[G2980]» heading G3056).
pub(crate) fn extract_lemma(text: &str, prefix: char, code: &str) -> Option<String> {
    let hebrew = prefix.eq_ignore_ascii_case(&'H');
    let is_letter = |c: char| if hebrew { is_hebrew_letter(c) } else { is_greek_letter(c) };
    let is_mark = |c: char| if hebrew { is_hebrew_mark(c) } else { is_combining_mark(c) };
    let chars: Vec<char> = text.chars().take(LEMMA_WINDOW + LEMMA_MAX_CHARS + 16).collect();
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
    let after = chars.get(end).copied();
    let after_hyphen = chars.get(end + 1).copied();
    if after.is_some_and(char::is_alphabetic) || (after.is_some_and(is_hyphen) && after_hyphen.is_some_and(char::is_alphabetic)) {
        return None;
    }
    let rest = &chars[end..];
    let spaced: Vec<char> = rest.iter().skip_while(|c| **c == ' ').copied().collect();
    if spaced.first() == Some(&'(') {
        let n = spaced[1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if n > 0 && spaced.get(1 + n) == Some(&')') {
            return None;
        }
    }
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

/// A window of the module's alphabetical topic index: `before` rows above and `after` rows from
/// the query's position (or `anchor`, for «Anteriores…» / «Siguientes…»). The Diccionario's
/// index; the app calls [`topic_index`], the probes in accuracy_tests.rs this.
#[cfg(test)]
pub fn dictionary_index(
    module_path: &str,
    query: Option<&str>,
    anchor: Option<u32>,
    before: u32,
    after: u32,
) -> Result<IndexSlice, String> {
    topic_index(module_path, query, anchor, before, after, false)
}

/// [`dictionary_index`] for the Diccionario (`lexicon = false`) or the Léxico (`true`: the
/// `Lexicon` table first, Strong's rows with their headword, and the Griego/Hebreo starts).
pub fn topic_index(
    module_path: &str,
    query: Option<&str>,
    anchor: Option<u32>,
    before: u32,
    after: u32,
    lexicon: bool,
) -> Result<IndexSlice, String> {
    let idx = topic_index_for(module_path, lexicon)?;
    let total = idx.rows.len() as u32;
    let (focus, exact) = match query.map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => {
            let k = index_query_key(q);
            let pos = idx.keys.partition_point(|x| x.as_str() < k.as_str());
            (Some(pos as u32), idx.keys.get(pos).is_some_and(|x| *x == k))
        }
        None => (None, false),
    };
    let center = anchor.or(focus).unwrap_or(0).min(total);
    let start = center.saturating_sub(before.min(INDEX_MAX_WINDOW));
    let end = center.saturating_add(after.min(INDEX_MAX_WINDOW)).min(total);
    let lemmas = if lexicon { index_lemmas(module_path, &idx, start, end) } else { HashMap::new() };
    let rows = (start..end)
        .map(|p| {
            let (rowid, topic) = &idx.rows[p as usize];
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
        rows,
        strong_keyed: total > 0 && idx.strong_rows.saturating_mul(2) >= total,
        greek_start: idx.greek_start,
        hebrew_start: idx.hebrew_start,
    })
}

/// One topic's article by its row (a click in the index opens exactly that topic).
#[cfg(test)]
pub fn dictionary_entry_by_rowid(module_path: &str, rowid: i64) -> Result<Option<DictEntry>, String> {
    topic_entry_by_rowid(module_path, rowid, false)
}

/// [`dictionary_entry_by_rowid`] on the table [`topic_index`] read (`lexicon`).
pub fn topic_entry_by_rowid(module_path: &str, rowid: i64, lexicon: bool) -> Result<Option<DictEntry>, String> {
    with_slot(module_path, |conn, schema| {
        let ts = index_table(schema, lexicon)?;
        let topic: String = conn
            .query_row(
                &format!("SELECT \"{}\" FROM \"{}\" WHERE rowid=?1", ts.topic, ts.table),
                rusqlite::params![rowid],
                |r| cell_to_string(r, 0),
            )
            .unwrap_or_default();
        if topic.trim().is_empty() {
            return Ok(None);
        }
        let def = def_by_rowid(conn, &ts, rowid);
        make_entry(&topic, &def, "exact", "Módulo cifrado (🔒). Use un .lexi/.dcti moderno sin DRM.")
    })
}

// ─── Devotionals (.devi): one reading per calendar day (ported from the Windows build) ───
//
// A single table, unlike the verse-addressed modules: `Devotional(Month, Day, Devotion)`.
// The Devocional tab opens on today's date, steps day by day, and lists the same day in the
// folder's other devotionals.

/// One day's reading of a devotional.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevotionalEntry {
    pub month: i32,
    pub day: i32,
    /// The reading, cleaned like any study text.
    pub html: String,
    pub title: String,
    pub abbreviation: String,
}

fn devotional_table_ok(conn: &Connection) -> Result<(), String> {
    if table_exists(conn, "Devotional") {
        Ok(())
    } else {
        Err("Este módulo no tiene tabla Devotional: no es un devocional de e-Sword.".to_string())
    }
}

/// The (month, day) pairs a devotional has a reading for, in calendar order.
pub fn devotional_days(module_path: &str) -> Result<Vec<(i32, i32)>, String> {
    with_conn(module_path, |conn| {
        devotional_table_ok(conn)?;
        let mut st = conn
            .prepare(
                "SELECT CAST(Month AS INTEGER) AS m, CAST(Day AS INTEGER) AS d FROM Devotional \
                 WHERE m BETWEEN 1 AND 12 AND d BETWEEN 1 AND 31 GROUP BY m, d ORDER BY m, d",
            )
            .map_err(|e| es_error(&e.to_string()))?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, i32>(0)?, r.get::<_, i32>(1)?)))
            .map_err(|e| es_error(&e.to_string()))?;
        Ok(rows.flatten().collect())
    })
}

/// The reading for one calendar day, or `None` when the devotional has none for it.
pub fn devotional_entry(module_path: &str, month: i32, day: i32) -> Result<Option<DevotionalEntry>, String> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Ok(None);
    }
    with_conn(module_path, |conn| {
        devotional_table_ok(conn)?;
        let raw: Option<String> = match conn.query_row(
            "SELECT Devotion FROM Devotional WHERE CAST(Month AS INTEGER)=?1 AND CAST(Day AS INTEGER)=?2 \
             AND length(Devotion)>0 ORDER BY rowid LIMIT 1",
            rusqlite::params![month, day],
            |r| cell_to_string(r, 0),
        ) {
            Ok(text) => Some(text),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            // A table without the expected columns, a damaged file: say so, not «no reading».
            Err(e) => return Err(es_error(&e.to_string())),
        };
        let Some(raw) = raw.filter(|t| !t.trim().is_empty()) else {
            return Ok(None);
        };
        if looks_encrypted(&raw) {
            return Err("Este devocional está cifrado (🔒). Use un módulo .devi moderno sin DRM.".to_string());
        }
        let (title, abbreviation, _, _) = read_details(conn);
        let (html, _) = truncate_entry_html(clean_esword_text(&raw));
        Ok(Some(DevotionalEntry { month, day, html, title, abbreviation }))
    })
}

// ─── «ES» line: a Strong's number's Reina-Valera renderings (ported from Windows) ───

static STRONG_GLOSS_CACHE: Lazy<Mutex<HashMap<(String, String), Option<crate::strong_gloss::StrongGloss>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
const STRONG_GLOSS_CACHE_MAX: usize = 4096;
/// Modules tried for one number (the page sends its 4 preferred Spanish Strong's dictionaries).
const STRONG_GLOSS_MAX_SOURCES: usize = 4;

/// Text of cleaned entry HTML, one line per `<br>` (the parser reads line breaks).
fn entry_lines(html: &str) -> String {
    html.split("<br>").map(to_plain).collect::<Vec<_>>().join("\n")
}

/// The Spanish renderings of `code` from the first of `module_paths` that has a readable entry
/// with a list for it. A module that is missing, unreadable or lacks the entry is skipped; when
/// none of them could be read at all the answer is an error (the page asks again later) rather
/// than «no list».
pub fn strong_gloss(code: &str, module_paths: &[String]) -> Result<Option<crate::strong_gloss::StrongGloss>, String> {
    let Some(code) = crate::strong_gloss::strong_code(code) else {
        return Ok(None);
    };
    let mut last_err = None;
    let mut read_any = false;
    for p in module_paths.iter().take(STRONG_GLOSS_MAX_SOURCES) {
        match strong_gloss_in(p, &code) {
            Ok(Some(g)) => return Ok(Some(g)),
            Ok(None) => read_any = true,
            Err(e) => last_err = Some(e),
        }
    }
    match last_err {
        Some(e) if !read_any => Err(e),
        _ => Ok(None),
    }
}

fn strong_gloss_in(path: &str, code: &str) -> Result<Option<crate::strong_gloss::StrongGloss>, String> {
    let key = (path.to_string(), code.to_string());
    if let Some(hit) = STRONG_GLOSS_CACHE.lock().get(&key) {
        return Ok(hit.clone());
    }
    let found = with_slot(path, |conn, schema| {
        let Some(ts) = schema.lex.or(schema.dict) else { return Ok(None) };
        // Exact topics only, as the modules spell them (G26, G0026, G00026): a prefix match
        // would hand G260 the words of G26's neighbour.
        let forms = strong_code_forms(code);
        let ph = (1..=forms.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",");
        let coll = if ts.indexed { "" } else { " COLLATE NOCASE" };
        let sql = format!("SELECT rowid, \"{c}\" FROM \"{t}\" WHERE \"{c}\"{coll} IN ({ph})", c = ts.topic, t = ts.table);
        let mut rows = topic_rows(conn, &sql, &forms);
        rows.sort_by_key(|(rowid, _)| *rowid);
        let Some((rowid, _)) = rows.first().cloned() else { return Ok(None) };
        let def = def_by_rowid(conn, &ts, rowid);
        if def.trim().is_empty() || looks_encrypted(&def) {
            return Ok(None);
        }
        let Some(entry) = crate::strong_gloss::parse_entry(&entry_lines(&clean_esword_text(&def))) else {
            return Ok(None);
        };
        let (title, abbr, _, _) = read_details(conn);
        Ok(Some(crate::strong_gloss::StrongGloss {
            code: code.to_string(),
            spanish: entry.spanish,
            translit: entry.translit,
            original: entry.original,
            source: if abbr.trim().is_empty() { title } else { abbr },
            module: Path::new(path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        }))
    })?; // not readable now (busy, missing): not remembered
    let mut cache = STRONG_GLOSS_CACHE.lock();
    if cache.len() >= STRONG_GLOSS_CACHE_MAX {
        cache.clear();
    }
    cache.insert(key, found.clone());
    Ok(found)
}

// ─── Strong bridge: a verb or adjective tap reaches its concept's headword ───
//
// Bible dictionaries list concepts (AMOR, CREACIÓN, BENDICIÓN), rarely the tapped form
// (amó, creó, bendijo). When the word itself has no full match in a module, the glosses of its
// Strong family in a Spanish Strong lexicon (STRONGes: «… : — amado, amada, amar, amor.»,
// «de G25» for derived words) name the headword. A gloss is used only when it shares the
// word's beginning, so other senses of the same Hebrew word (beraká: «estanque») never come in.
// Probe of 72 real taps (2026-11-11): 25 showed no dictionary entry; this finds the right
// article for 9 of them (amó → AMOR, creó → CREACIÓN, arrepentíos → ARREPENTIMIENTO, …).

#[derive(Debug, Default)]
struct GlossIndex {
    /// Strong code → single-word Spanish glosses, in lexicon order.
    glosses: HashMap<String, Vec<String>>,
    /// Strong code → codes it is derived from («de G25»).
    roots: HashMap<String, Vec<String>>,
    /// Strong code → codes derived from it.
    derived: HashMap<String, Vec<String>>,
    /// Codes whose gloss list names a person or place (a capitalised gloss: Cristo, Jehová).
    proper: HashSet<String>,
}

static GLOSS_INDEX: Lazy<Mutex<HashMap<String, (Option<FileIdent>, Option<Arc<GlossIndex>>)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static RE_GLOSS_ROOT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\bde\s+([GH]\d{1,5})\b").unwrap());
static RE_GLOSS_PAREN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\([^)]*\)").unwrap());

fn gloss_plain(def: &str) -> String {
    collapse_ws(&html_escape::decode_html_entities(&strip_tags_quick(def)))
}

/// STRONGes gloss list: the text after the last «—» up to the sentence end («Comp. G5368»
/// and «Véase» tails dropped); single-word items only, lowercased, parentheses removed.
fn parse_gloss_tail(plain: &str) -> Vec<String> {
    parse_gloss_tail_names(plain).0
}

/// [`parse_gloss_tail`] plus whether the list had a capitalised (proper-name) gloss.
fn parse_gloss_tail_names(plain: &str) -> (Vec<String>, bool) {
    let mut proper = false;
    let Some(pos) = plain.rfind('—') else {
        return (vec![], false);
    };
    let tail = &plain[pos + '—'.len_utf8()..];
    let tail = tail.split(". ").next().unwrap_or(tail);
    let tail = tail.split("Comp.").next().unwrap_or(tail);
    let tail = tail.split("Véase").next().unwrap_or(tail);
    let mut out: Vec<String> = Vec::new();
    for item in tail.split(',') {
        let item = RE_GLOSS_PAREN.replace_all(item, "");
        let item = item.trim().trim_matches(|c: char| c == '.' || c == ';' || c == ':').trim();
        if item.is_empty() || item.contains(char::is_whitespace) || !item.chars().all(|c| c.is_alphabetic()) {
            continue;
        }
        // A capitalised gloss is a proper name (Jehú for H3058 «de H3068»): never a concept.
        if item.chars().next().is_some_and(char::is_uppercase) {
            proper = true;
            continue;
        }
        let w = item.to_lowercase();
        if !out.contains(&w) {
            out.push(w);
        }
    }
    (out, proper)
}

/// «de G25» in the head of the entry (before the definition proper): the codes it derives from.
fn parse_gloss_roots(plain: &str) -> Vec<String> {
    let head: String = plain.split(';').take(3).collect::<Vec<_>>().join(";");
    let mut out: Vec<String> = Vec::new();
    for c in RE_GLOSS_ROOT.captures_iter(&head) {
        if let Some(code) = parse_strong_code(&c[1]) {
            if !out.contains(&code) {
                out.push(code);
            }
        }
    }
    out
}

fn build_gloss_index(conn: &Connection, ts: &TopicSchema) -> Option<GlossIndex> {
    // Cheap format check first: STRONGes-style entries end in «: — glosa, glosa.».
    let sample = ["G26", "H1293", "G1347"];
    let ok = sample.iter().any(|code| {
        conn.query_row(
            &format!("SELECT \"{}\" FROM \"{}\" WHERE \"{}\"=?1", ts.def, ts.table, ts.topic),
            rusqlite::params![code],
            |r| cell_to_string(r, 0),
        )
        .map(|d| !parse_gloss_tail(&gloss_plain(&d)).is_empty())
        .unwrap_or(false)
    });
    if !ok {
        return None;
    }
    let mut st = conn
        .prepare(&format!("SELECT \"{}\", \"{}\" FROM \"{}\"", ts.topic, ts.def, ts.table))
        .ok()?;
    let rows = st.query_map([], |r| Ok((cell_to_string(r, 0)?, cell_to_string(r, 1)?))).ok()?;
    let mut idx = GlossIndex::default();
    for (topic, def) in rows.flatten() {
        let Some(code) = parse_strong_code(topic.trim()) else { continue };
        let plain = gloss_plain(&def);
        let (g, proper) = parse_gloss_tail_names(&plain);
        if proper {
            idx.proper.insert(code.clone());
        }
        if !g.is_empty() {
            idx.glosses.insert(code.clone(), g);
        }
        for root in parse_gloss_roots(&plain) {
            if root != code {
                idx.derived.entry(root.clone()).or_default().push(code.clone());
                idx.roots.entry(code.clone()).or_default().push(root);
            }
        }
    }
    (idx.glosses.len() >= 1000).then_some(idx)
}

fn gloss_index_for(path: &str) -> Option<Arc<GlossIndex>> {
    let ident = file_ident(path);
    if let Some((id, idx)) = GLOSS_INDEX.lock().get(path) {
        if ident.is_some() && *id == ident {
            return idx.clone();
        }
    }
    let idx = with_probe_slot(path, |conn, schema| {
        Ok(schema.lex.or(schema.dict).and_then(|ts| build_gloss_index(conn, &ts)).map(Arc::new))
    })
    .ok()
    .flatten();
    GLOSS_INDEX.lock().insert(path.to_string(), (ident, idx.clone()));
    idx
}

/// The first lexicon with a STRONGes-style gloss list (03/04STRONGes), if any.
fn gloss_source(lex_paths: &[String]) -> Option<Arc<GlossIndex>> {
    lex_paths.iter().find_map(|p| gloss_index_for(p))
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// Conservative nouns for a verb gloss, tried only as existing headwords:
/// -ificar → -ificación, -izar → -ismo, -ar → -ación, -er/-ir(se) → -imiento.
fn gloss_nominalizations(verb: &str) -> Vec<String> {
    let v = verb.strip_suffix("se").filter(|s| s.ends_with('r')).unwrap_or(verb);
    let real = |stem: &str| stem.chars().count() >= 3; // «ser» gives nothing
    let mut out = Vec::new();
    if let Some(stem) = v.strip_suffix("ificar").filter(|s| real(s)) {
        out.push(format!("{stem}ificación"));
    } else if let Some(stem) = v.strip_suffix("izar").filter(|s| real(s)) {
        out.push(format!("{stem}ismo"));
    } else if let Some(stem) = v.strip_suffix("ar").filter(|s| real(s)) {
        out.push(format!("{stem}ación"));
    } else if let Some(stem) = v.strip_suffix("er").or_else(|| v.strip_suffix("ir")).filter(|s| real(s)) {
        out.push(format!("{stem}imiento"));
    }
    out
}

/// Headword candidates for `word` from its Strong family, best first, each with a priority:
/// 0 glosses of its own code and of codes derived from it, 1 nouns of those verb glosses,
/// 2 glosses of the codes it derives from (broader: glorificó → gloria). Only glosses that
/// share the word's beginning (≥4 letters, ≥3 for short words) count.
pub(crate) fn strong_bridge_candidates(word: &str, strongs: &[String], lex_paths: &[String]) -> Vec<BridgeCand> {
    let fw = strip_accents_es(&compose_nfc_es(word.trim()).to_lowercase());
    if fw.chars().count() < 3 || strongs.is_empty() {
        return vec![];
    }
    let Some(idx) = gloss_source(lex_paths) else {
        return vec![];
    };
    // A name (Cristo, Jehová) matches its own headword or nothing: no related concepts.
    if idx.proper.contains(&strongs[0]) {
        return vec![];
    }
    // Letters the gloss must share with the word's beginning: 4 (3 when the tapped word itself
    // is that short: amó → amor); generated nouns 7 (predestinó → predestinación, never
    // ordeno → ordenación); a root's glosses 5 (glorificó → gloria).
    let base_need = if fw.chars().count() <= 4 { 3 } else { 4 };
    // Verb lemmas of the tapped word among its own family's infinitive glosses (pierda →
    // perder, murió → morir, creyó → creer): their stems begin related nouns that the
    // irregular form does not (perder → PERDICIÓN, morir → MUERTO). Anchors need 4 letters.
    let mut lemma_anchors: Vec<String> = Vec::new();
    for code in strongs.iter().take(2) {
        let mut close: Vec<&String> = vec![code];
        if let Some(d) = idx.derived.get(code) {
            close.extend(d.iter());
        }
        for c in close {
            for g in idx.glosses.get(c).map(Vec::as_slice).unwrap_or(&[]) {
                if crate::conjugation::is_infinitive(g) && crate::conjugation::is_form_of(&fw, g) {
                    for a in crate::conjugation::lemma_anchors(g) {
                        if !lemma_anchors.contains(&a) {
                            lemma_anchors.push(a);
                        }
                    }
                }
            }
        }
    }
    // Shared beginning with the word (`need`) or with one of its verb lemmas (`lemma_need`).
    let shares_at = |g: &str, need: usize, lemma_need: usize| -> Option<usize> {
        let fg = strip_accents_es(g);
        let cp = common_prefix(&fg, &fw);
        let lcp = lemma_anchors.iter().map(|a| common_prefix(&fg, a)).max().unwrap_or(0);
        if cp >= need {
            Some(cp.max(lcp))
        } else if lcp >= lemma_need {
            Some(lcp)
        } else {
            None
        }
    };
    // Glosses of derived codes on long words need 5 (reinar ↛ reina «queen»).
    let derived_need = if fw.chars().count() >= 6 { base_need.max(5) } else { base_need };
    // criado ↛ Criada, muchacho ↛ Muchacha: the same stem with the other gender ending.
    let gender_flip = |g: &str| -> bool {
        let a = strip_accents_es(g);
        let (a, b) = (a.trim_end_matches('s'), fw.trim_end_matches('s'));
        a.chars().count() == b.chars().count()
            && a.chars().count() >= 3
            && a.chars().zip(b.chars()).filter(|(x, y)| x != y).count() == 1
            && matches!((a.chars().last(), b.chars().last()), (Some('a'), Some('o')) | (Some('o'), Some('a')))
    };
    // (priority, -shared, order, gloss)
    let mut cands: Vec<(u8, usize, usize, String)> = Vec::new();
    let push = |cands: &mut Vec<(u8, usize, usize, String)>, pri: u8, cp: usize, g: String| {
        if strip_accents_es(&g) == fw {
            return; // the word itself was already looked up
        }
        let n = cands.len();
        match cands.iter_mut().find(|c| c.3 == g) {
            Some(c) if (pri, std::cmp::Reverse(cp)) < (c.0, std::cmp::Reverse(c.1)) => {
                c.0 = pri;
                c.1 = cp;
            }
            Some(_) => {}
            None => cands.push((pri, cp, n, g)),
        }
    };
    for code in strongs.iter().take(2) {
        let mut close: Vec<&String> = vec![code];
        if let Some(d) = idx.derived.get(code) {
            close.extend(d.iter());
        }
        for (k, c) in close.iter().enumerate() {
            let need = if k == 0 { base_need } else { derived_need };
            for g in idx.glosses.get(*c).map(Vec::as_slice).unwrap_or(&[]) {
                if gender_flip(g) {
                    continue;
                }
                if let Some(cp) = shares_at(g, need, 4) {
                    push(&mut cands, 0, cp, g.clone());
                }
                for n in gloss_nominalizations(g) {
                    if gender_flip(&n) {
                        continue;
                    }
                    if let Some(cp) = shares_at(&n, 7, 7) {
                        push(&mut cands, 1, cp, n);
                    }
                }
            }
        }
        for r in idx.roots.get(code).map(Vec::as_slice).unwrap_or(&[]) {
            for g in idx.glosses.get(r).map(Vec::as_slice).unwrap_or(&[]) {
                if gender_flip(g) {
                    continue;
                }
                if let Some(cp) = shares_at(g, base_need.max(5), 5) {
                    push(&mut cands, 2, cp, g.clone());
                }
            }
        }
    }
    cands.sort_by(|a, b| (a.0, std::cmp::Reverse(a.1), a.2).cmp(&(b.0, std::cmp::Reverse(b.1), b.2)));
    cands
        .into_iter()
        .map(|c| BridgeCand { gloss: c.3, priority: c.0, shared: c.1 })
        .collect()
}

/// One Strong bridge headword candidate (see [`strong_bridge_candidates`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BridgeCand {
    pub gloss: String,
    /// 0 own/derived gloss, 1 noun of a verb gloss, 2 gloss of a root code.
    pub priority: u8,
    /// Letters shared with the tapped word's beginning (accents off).
    pub shared: usize,
}

/// Full-headword entries (exact or headword tier) for bridge candidates in one module, tagged
/// `strong`, at most `limit`. Root glosses (priority 2) only when nothing closer matched; a
/// second entry only when it shares as much of the word as the first (creó: CREACIÓN and
/// CREADOR, but justificados: JUSTIFICACIÓN without JUSTO).
fn bridge_entries(conn: &Connection, ts: &TopicSchema, cands: &[BridgeCand], limit: usize) -> Result<Vec<DictEntry>, String> {
    let mut out: Vec<DictEntry> = Vec::new();
    let mut seen: HashSet<(String, u64)> = HashSet::new();
    let mut first_shared: Option<usize> = None;
    for c in cands {
        if out.len() >= limit || (c.priority == 2 && !out.is_empty()) {
            break;
        }
        if first_shared.is_some_and(|f| c.shared < f) {
            continue;
        }
        let hits: Vec<Cand> = ranked_candidates(conn, ts, &c.gloss, false)
            .into_iter()
            .filter(|h| h.tier == Tier::Exact || (h.tier == Tier::Headword && synonym_list_headword(&h.topic, &c.gloss)))
            .collect();
        for mut e in entries_from_cands(conn, ts, &hits, 1, true)? {
            if seen.insert((e.topic.clone(), plain_hash(&e.plain))) {
                e.match_kind = "strong".into();
                out.push(e);
                first_shared.get_or_insert(c.shared);
            }
        }
    }
    Ok(out)
}

/// A headword the bridge may use besides the exact topic: `GLOSA, SINÓNIMO[, …]` — a comma list
/// of single words. Qualified subtopics are other articles: «Ofrenda (Iglesias Paulinas)»,
/// «PALOMA, ESTIÉRCOL DE», «DEDICACIÓN, FIESTA DE LA», «Sabiduría (Libro de)».
fn synonym_list_headword(topic: &str, gloss: &str) -> bool {
    let t = strip_accents_es(&compose_nfc_es(topic.trim()).to_lowercase());
    let g = strip_accents_es(gloss);
    let Some(rest) = t.strip_prefix(g.as_str()) else {
        return false;
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return true;
    }
    let Some(list) = rest.strip_prefix(',') else {
        return false;
    };
    const SMALL: &[&str] = &["de", "del", "la", "las", "el", "los", "un", "una", "y", "en"];
    list.split(',').all(|item| {
        let item = item.trim();
        !item.is_empty()
            && !item.contains(char::is_whitespace)
            && item.chars().all(char::is_alphabetic)
            && !SMALL.contains(&item)
    })
}

/// [`dictionary_lookup`] for the Diccionario view: when the query has no full match in this
/// module and the tapped word's Strong codes are known, the Strong bridge's headwords come
/// first (tagged `strong`); the rest of the list is unchanged.
pub fn dictionary_lookup_bridged(
    module_path: &str,
    query: &str,
    limit: i32,
    strongs: &[String],
    lex_paths: &[String],
) -> Result<Vec<DictEntry>, String> {
    let mut entries = dictionary_lookup(module_path, query, limit)?;
    let full = |k: &str| matches!(k, "exact" | "headword" | "lemma");
    if strongs.is_empty() || entries.iter().any(|e| full(&e.match_kind)) {
        return Ok(entries);
    }
    let cands = strong_bridge_candidates(query, strongs, lex_paths);
    if cands.is_empty() {
        return Ok(entries);
    }
    let bridged = with_slot(module_path, |conn, schema| {
        let Some(ts) = schema.dict else { return Ok(vec![]) };
        bridge_entries(conn, &ts, &cands, 3)
    })?;
    if bridged.is_empty() {
        return Ok(entries);
    }
    let mut out = bridged;
    out.append(&mut entries);
    Ok(out)
}

fn lexicon_exact_code(conn: &Connection, ts: &TopicSchema, code: &str) -> Result<Option<DictEntry>, String> {
    let forms = strong_code_forms(code);
    if forms.is_empty() {
        return Ok(None);
    }
    let ph = (1..=forms.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let coll = if ts.indexed { "" } else { " COLLATE NOCASE" };
    let sql = format!(
        "SELECT rowid, \"{c}\" FROM \"{t}\" WHERE \"{c}\"{coll} IN ({ph})",
        c = ts.topic,
        t = ts.table
    );
    let mut rows = topic_rows(conn, &sql, &forms);
    rows.sort_by_key(|(rowid, topic)| {
        let up = topic.trim().to_ascii_uppercase();
        (forms.iter().position(|f| *f == up).unwrap_or(99), *rowid)
    });
    for (rowid, topic) in rows {
        let def = def_by_rowid(conn, ts, rowid);
        if let Some(e) = make_entry(&topic, &def, "exact", "Léxico cifrado (🔒).")? {
            return Ok(Some(e));
        }
    }
    Ok(None)
}

fn lexicon_lookup_mode(module_path: &str, strong: &str, cache: bool) -> Result<Option<DictEntry>, String> {
    let raw = strong.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    with_slot_mode(module_path, cache, |conn, schema| {
        // Prefer Lexicon table (.lexi / .lexx), fall back to Dictionary (some Strong packs as .dcti)
        let ts = schema
            .lex
            .ok_or_else(|| "Este módulo no tiene tabla Lexicon/Dictionary.".to_string())?;

        // ISS-04: a Strong code matches only its own canonical spellings; a miss is None so the
        // caller moves on to the next lexicon instead of showing a neighbouring number.
        if let Some(code) = parse_strong_code(raw) {
            return lexicon_exact_code(conn, &ts, &code);
        }

        // Non-code queries (Vine-style lemma topics, bare digits): original behaviour.
        for c in strong_candidates(raw) {
            let sql = format!(
                "SELECT \"{}\", \"{}\" FROM \"{}\" WHERE \"{}\" = ?1 COLLATE NOCASE
                   AND length(trim(\"{}\"))>0 LIMIT 1",
                ts.topic, ts.def, ts.table, ts.topic, ts.def
            );
            if let Ok((topic, def)) = conn.query_row(&sql, rusqlite::params![c], |row| {
                Ok((cell_to_string(row, 0)?, cell_to_string(row, 1)?))
            }) {
                if let Some(e) = make_entry(&topic, &def, "exact", "Léxico cifrado (🔒).")? {
                    return Ok(Some(e));
                }
            }
        }
        let mut entries = lookup_in_topic_table(conn, &ts, raw, 8)?;
        if let Some(i) = entries.iter().position(|e| {
            e.topic.eq_ignore_ascii_case(raw)
                || strong_candidates(raw)
                    .iter()
                    .any(|c| e.topic.eq_ignore_ascii_case(c))
        }) {
            return Ok(Some(entries.swap_remove(i)));
        }
        Ok(entries.into_iter().next())
    })
}

pub fn lexicon_lookup(module_path: &str, strong: &str) -> Result<Option<DictEntry>, String> {
    lexicon_lookup_mode(module_path, strong, true)
}

/// List/search e-Sword Reference modules (.refi) — topic = Chapter column.
pub fn reference_lookup(module_path: &str, query: &str, limit: i32) -> Result<Vec<DictEntry>, String> {
    let q = query.trim();
    with_conn(module_path, |conn| {
        if !table_exists(conn, "Reference") {
            return Err("No es un módulo de referencia (.refi).".into());
        }
        if q.is_empty() {
            // First topics for browsing
            let mut stmt = conn
                .prepare(
                    "SELECT Chapter, Content FROM Reference ORDER BY Chapter LIMIT ?1",
                )
                .map_err(|e| es_error(&e.to_string()))?;
            let rows = stmt
                .query_map(rusqlite::params![limit], |row| {
                    Ok((cell_to_string(row, 0)?, cell_to_string(row, 1)?))
                })
                .map_err(|e| es_error(&e.to_string()))?;
            let mut out = Vec::new();
            for r in rows.flatten() {
                let (topic, def) = r;
                if looks_encrypted(&def) {
                    return Err("Referencia cifrada (🔒).".into());
                }
                let html = clean_esword_text(&def);
                out.push(DictEntry {
                    topic,
                    plain: to_plain(&html),
                    html,
                    match_kind: String::new(),
                    truncated: false,
                });
            }
            return Ok(out);
        }
        let mut stmt = conn
            .prepare(
                "SELECT Chapter, Content FROM Reference
                 WHERE Chapter = ?1 COLLATE NOCASE
                    OR Chapter LIKE ?2 ESCAPE '\\'
                    OR Chapter LIKE ?3 ESCAPE '\\'
                 ORDER BY
                   CASE WHEN Chapter = ?1 COLLATE NOCASE THEN 0
                        WHEN Chapter LIKE ?2 ESCAPE '\\' THEN 1
                        ELSE 2 END,
                   Chapter
                 LIMIT ?4",
            )
            .map_err(|e| es_error(&e.to_string()))?;
        let esc = like_escape(q);
        let prefix = format!("{esc}%");
        let contains = format!("%{esc}%");
        let rows = stmt
            .query_map(rusqlite::params![q, prefix, contains, limit], |row| {
                Ok((cell_to_string(row, 0)?, cell_to_string(row, 1)?))
            })
            .map_err(|e| es_error(&e.to_string()))?;
        let mut out = Vec::new();
        for r in rows.flatten() {
            let (topic, def) = r;
            if looks_encrypted(&def) {
                return Err("Referencia cifrada (🔒).".into());
            }
            let html = clean_esword_text(&def);
            out.push(DictEntry {
                topic,
                plain: to_plain(&html),
                html,
                match_kind: String::new(),
                truncated: false,
            });
        }
        Ok(out)
    })
}

/// ISS-10: EXISTS-style probe. Reads only row headers and, for short notes, the text itself;
/// never cleans HTML. Same answer as `!load_commentary(..).is_empty()`.
pub fn probe_commentary_has(
    module_path: &str,
    book: i32,
    chapter: i32,
    verse: i32,
    include_verse: bool,
    include_chapter: bool,
    include_book: bool,
) -> bool {
    with_probe_slot(module_path, |conn, schema| {
        if include_verse {
            if let Some(t) = schema.verse_cmt {
                let sql = format!(
                    "SELECT octet_length(Comments),
                            CASE WHEN octet_length(Comments) <= 4096 THEN Comments END
                     FROM \"{t}\" WHERE rowid=?1"
                );
                for r in verse_cmt_rows(conn, t, book, chapter, verse)? {
                    if cmt_text_ok(conn, &sql, &[&r.rowid]) == Some(true) {
                        return Ok(true);
                    }
                }
            } else {
                return Ok(false);
            }
        }
        if include_chapter {
            if let Some(t) = schema.chapter_cmt {
                let sql = format!(
                    "SELECT octet_length(Comments),
                            CASE WHEN octet_length(Comments) <= 4096 THEN Comments END
                     FROM \"{t}\" WHERE Book=?1 AND Chapter=?2"
                );
                if cmt_text_ok(conn, &sql, &[&book, &chapter]) == Some(true) {
                    return Ok(true);
                }
            }
        }
        if include_book {
            if let Some(t) = schema.book_cmt {
                let sql = format!(
                    "SELECT octet_length(Comments),
                            CASE WHEN octet_length(Comments) <= 4096 THEN Comments END
                     FROM \"{t}\" WHERE Book=?1"
                );
                if cmt_text_ok(conn, &sql, &[&book]) == Some(true) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    })
    .unwrap_or(false)
}

/// ISS-08/10: true only for an exact, headword or lemma entry (never prefix/substring junk).
pub fn probe_dictionary_has(module_path: &str, topic: &str) -> bool {
    let q = topic.trim();
    if q.is_empty() {
        return false;
    }
    with_probe_slot(module_path, |conn, schema| {
        let Some(ts) = schema.dict else {
            return Ok(false);
        };
        let cands = ranked_candidates(conn, &ts, q, false);
        for c in cands.iter().filter(|c| c.tier.full()).take(8) {
            let def = def_by_rowid(conn, &ts, c.rowid);
            if def.trim().is_empty() || looks_encrypted(&def) {
                continue;
            }
            if def.len() > 4096 || !to_plain(&clean_esword_text(&def)).trim().is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    })
    .unwrap_or(false)
}

pub fn probe_lexicon_has(module_path: &str, strong: &str) -> bool {
    lexicon_lookup_mode(module_path, strong, false)
        .map(|v| v.is_some())
        .unwrap_or(false)
}

pub fn verse_count(module_path: &str, book: i32, chapter: i32) -> Result<i32, String> {
    with_conn(module_path, |conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM Bible WHERE Book=?1 AND Chapter=?2",
            rusqlite::params![book, chapter],
            |row| row.get(0),
        )
        .map_err(|e| es_error(&e.to_string()))
    })
}

pub fn default_modules_candidates() -> Vec<String> {
    let mut out = Vec::new();
    if let Some(home) = dirs_home() {
        // Preferred Mac library folder for this install
        out.push(format!("{home}/Downloads/for mac espada"));
        out.push(format!("{home}/Downloads/for Mac Espada"));
        out.push(format!("{home}/Documents/e-Sword"));
        out.push(format!("{home}/Documents/eSword"));
        out.push(format!("{home}/Documents/e-sword"));
        out.push(format!("{home}/Library/Application Support/e-Sword"));
        out.push(format!("{home}/Library/Application Support/eSword"));
        out.push(format!("{home}/e-Sword"));
        out.push(format!("{home}/Espada3.7/modules"));
        out.push(format!("{home}/Desktop"));
    }
    out.push("/Applications/e-Sword.app/Contents/Resources".into());
    out
}

fn dirs_home() -> Option<String> {
    std::env::var("HOME").ok()
}

pub fn find_default_modules_dir() -> Option<String> {
    for c in default_modules_candidates() {
        let p = Path::new(&c);
        if !p.is_dir() {
            continue;
        }
        if let Ok(rd) = fs::read_dir(p) {
            let has = rd.flatten().any(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|ext| kind_from_ext(ext).is_some())
                    .unwrap_or(false)
            });
            if has {
                return Some(c);
            }
        }
    }
    None
}

pub fn clear_caches() {
    CONN_CACHE.lock().map.clear();
    SCHEMA_CACHE.lock().clear();
    *MODULE_LIST_CACHE.lock() = None;
    OWN_TAG_CACHE.lock().clear();
    LEMMA_INDEX.lock().clear();
    REMAP_CACHE.lock().clear();
    IL_VALID_CACHE.lock().clear();
    FALLBACK_CACHE.lock().clear();
    ABBR_CACHE.lock().clear();
    PROSE_CACHE.lock().clear();
    STRONG_GLOSS_CACHE.lock().clear();
    TEXT_TITLES_CACHE.lock().clear();
}

// ─── Module ranking (ISS-13) ───

/// Pure usefulness rank for a module (lower is better), from its filename and title.
/// Lexicons: Spanish full-Strong lexicons first (STRONGes, Strong SPA, Strong °, StrongPLUS,
/// Multiléxico, BDB-THAYERes, Swanson, Tuggy, Chávez), other lexicons, English last.
/// Dictionaries: Nelson, Vila-Escuain, Certeza, Mundo Hispano, other Spanish Bible
/// dictionaries, the rest, English last.
pub fn rank_module(path: &str, title: &str) -> i32 {
    let file = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_lowercase();
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let hay = format!("{file} | {}", title.to_lowercase());
    let find = |list: &[&str]| list.iter().position(|k| hay.contains(k));
    match kind_from_ext(ext) {
        Some("lexicon") => {
            const TOP: &[&str] = &[
                "stronges",
                "strongspa",
                "strong spa",
                "diccionario strong.lexi",
                "strongsplus",
                "strong plus",
                "multil",
                "bdb-thayeres",
                "swanson",
                "tuggy",
                "chavez",
                "chávez",
            ];
            const ENGLISH: &[&str] = &[
                "ahlb",
                "ancient hebrew",
                "peshito",
                "aramaic-english",
                "thayer",
                "vgnt",
                "accurate_new_testament",
                "ant key",
                "bdb.lexi",
                "bdbthayerstrongkjctvm",
                "brown-driver-briggs' hebrew",
            ];
            if let Some(i) = find(TOP) {
                return i as i32;
            }
            if hay.contains("strong-pt") || hay.contains("adventista") {
                return 600;
            }
            if find(ENGLISH).is_some() {
                return 900;
            }
            500
        }
        _ => {
            const TOP: &[&str] = &[
                "nelson",
                "vila",
                "certeza",
                "mundo hispano",
                "mundohispano",
                "douglas",
                "lockward",
                "constronges",
                "concordancia de las sagradas escrituras",
                "cdserv1960",
            ];
            const SPANISH_BIBLE: &[&str] = &[
                "diccionario bíblico",
                "diccionario biblico",
                "dicc",
                "dbdig",
                "enciclopedia b",
                "hitchcock",
                "hitesp",
                "enlow",
                "martens",
                "concordancia",
                "strongc",
                "nc-strong",
                "vine",
                "perspicacia",
                "nave",
                "thompson",
                "lbla",
                "rvr1995",
                "pdt",
                "btx",
                "macarthur",
                "bo-la",
                "rv 1909",
                "minibiograf",
                "usos_y_costumbres",
                "vocabulario_b",
                "nombres",
            ];
            const ENGLISH: &[&str] = &[
                "ahlb",
                "ancient hebrew",
                "ahrc",
                "albg",
                "analytical lexicon",
                "babylonian",
                "catholic",
                "christ gospels",
                "hastings",
                "cyclopedia",
                "easy to read",
                "english",
                "jokes",
                "isbe",
                "international standard",
                "new_believers",
                "new believer",
                "synonymns",
                "synonyms",
                "old_testament",
                "prophecies",
                "rmac",
                "sahidic",
                "small_theological",
                "testimony_of",
                "the_voice",
                "theological_and_philosophical",
                "topical_bible",
                "two_babylons",
                "webster",
                "wulfila",
                "gothic",
                "hebraic_roots",
                "tcr.dcti",
            ];
            if let Some(i) = find(TOP) {
                return i as i32;
            }
            if find(ENGLISH).is_some() {
                return 900;
            }
            if find(SPANISH_BIBLE).is_some() {
                return 100;
            }
            500
        }
    }
}

/// Order module paths by [`rank_module`] (stable), with `primary` first when present.
/// Titles are read from each module's Details table (cached per file).
pub fn rank_module_paths(paths: &[String], primary: Option<&str>) -> Vec<String> {
    let mut v: Vec<(bool, i32, usize, String)> = paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let is_primary = primary.map(|x| x == p).unwrap_or(false);
            (!is_primary, rank_module(p, &module_title(p)), i, p.clone())
        })
        .collect();
    v.sort();
    let mut seen = HashSet::new();
    v.into_iter()
        .filter(|(_, _, _, p)| seen.insert(p.clone()))
        .map(|(_, _, _, p)| p)
        .collect()
}

/// path → (file identity, abbreviation, title)
type DetailsCache = HashMap<String, (FileIdent, String, String)>;
static ABBR_CACHE: Lazy<Mutex<DetailsCache>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn module_details_cached(path: &str) -> (String, String) {
    let ident = file_ident(path);
    if let Some(id) = ident {
        if let Some((cid, a, t)) = ABBR_CACHE.lock().get(path) {
            if *cid == id {
                return (a.clone(), t.clone());
            }
        }
    }
    let (abbr, title) = with_probe_slot(path, |conn, _| {
        let (t, a, _, _) = read_details(conn);
        Ok((a, t))
    })
    .unwrap_or_default();
    let abbr = if abbr.is_empty() {
        Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string()
    } else {
        abbr
    };
    if let Some(id) = ident {
        ABBR_CACHE
            .lock()
            .insert(path.to_string(), (id, abbr.clone(), title.clone()));
    }
    (abbr, title)
}

fn module_title(path: &str) -> String {
    module_details_cached(path).1
}

// ─── Word interconnection (Spanish ↔ Strong ↔ dict ↔ lexicon) ───

static RE_STRONG_ONLY: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[HG]\d+$").unwrap());
/// Tag markers for sequential interlinear walk (iRV / e-Sword style). Grey `<span>`s are
/// RV1960 filler words with no original (kept only for occurrence alignment).
static RE_IL_TAG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?is)<num\s*>\s*([^<]*?)\s*</num>|<blu>(.*?)</blu>|<heb>(.*?)</heb>|<grk>(.*?)</grk>|<span style="color:#C0C0C0;">([^<]*)</span>"#,
    )
    .unwrap()
});
static RE_TAGS: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedEntry {
    pub module_path: String,
    pub module_abbr: String,
    pub topic: String,
    pub html: String,
    pub plain: String,
    /// ISS-08: match kind of a dictionary/lexicon entry (`exact`, `headword`, `lemma`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub match_kind: String,
}

/// One interlinear “column”: original (H/G script) + Spanish + Strong (+ translit).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterlinearWordHit {
    pub spanish: String,
    pub strong: String,
    /// Hebrew or Greek lemma (script), digits/arrows stripped.
    pub original: String,
    /// Latin transliteration when present (e.g. `Houtô`, `êgapêsen`). Never set for Hebrew.
    pub translit: String,
    /// `"hebrew"` | `"greek"` | `""`
    pub script: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordLinkResult {
    pub word: String,
    pub normalized: String,
    pub strongs: Vec<String>,
    pub hebrew: Vec<String>,
    pub greek: Vec<String>,
    pub dictionary: Vec<LinkedEntry>,
    pub lexicon: Vec<LinkedEntry>,
    pub interlinear_used: bool,
    /// Full verse interlinear map (for lexicon card + analysis).
    #[serde(default)]
    pub interlinear_hits: Vec<InterlinearWordHit>,
    /// Best match for the clicked word (if any).
    #[serde(default)]
    pub matched_hit: Option<InterlinearWordHit>,
    /// ISS-02: how `strongs` was found: `tagged` | `code` | `alias` | `gloss` | `stem` |
    /// `translit` | `original` | `lemma` | `""` (nothing matched).
    #[serde(default)]
    pub match_kind: String,
    /// ISS-04: Strong-like topics from the lemma pass. Never primary, never in `strongs`.
    #[serde(default)]
    pub related: Vec<String>,
    /// ISS-38: abbreviation of the own-tagged Bible used when the interlinear verse was
    /// missing or unparseable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interlinear_fallback: Option<String>,
    /// ISS-43: `strongs` came from the Greek/Hebrew/transliteration lemma index.
    #[serde(default)]
    pub lemma_index_used: bool,
    /// ISS-35: interlinear verse actually consulted, when versification remapped it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interlinear_verse: Option<[i32; 3]>,
    /// Abbreviation of the Strong-tagged edition of the reading Bible whose own tags answered
    /// an untagged tap (plain RVR1960 → 00RVR1960+).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagged_twin: Option<String>,
}

fn strip_tags_quick(s: &str) -> String {
    RE_TAGS.replace_all(s, "").to_string()
}

/// Greek letter without accents, breathings or iota subscript (lowercase in, base out);
/// final sigma folds to σ (ISS-19).
fn greek_base(c: char) -> char {
    match c as u32 {
        0x0386 | 0x03AC => 'α',
        0x0388 | 0x03AD => 'ε',
        0x0389 | 0x03AE => 'η',
        0x038A | 0x03AF | 0x0390 | 0x03AA | 0x03CA => 'ι',
        0x038C | 0x03CC => 'ο',
        0x038E | 0x03CD | 0x03B0 | 0x03AB | 0x03CB => 'υ',
        0x038F | 0x03CE => 'ω',
        0x03C2 => 'σ',
        0x1F00..=0x1F0F | 0x1F70 | 0x1F71 | 0x1F80..=0x1F8F | 0x1FB0..=0x1FBC => 'α',
        0x1F10..=0x1F1D | 0x1F72 | 0x1F73 | 0x1FC8 | 0x1FC9 => 'ε',
        0x1F20..=0x1F2F
        | 0x1F74
        | 0x1F75
        | 0x1F90..=0x1F9F
        | 0x1FC2..=0x1FC4
        | 0x1FC6
        | 0x1FC7
        | 0x1FCA..=0x1FCC => 'η',
        0x1F30..=0x1F3F | 0x1F76 | 0x1F77 | 0x1FD0..=0x1FDB => 'ι',
        0x1F40..=0x1F4D | 0x1F78 | 0x1F79 | 0x1FF8 | 0x1FF9 => 'ο',
        0x1F50..=0x1F5F | 0x1F7A | 0x1F7B | 0x1FE0..=0x1FE3 | 0x1FE6..=0x1FEB => 'υ',
        0x1F60..=0x1F6F
        | 0x1F7C
        | 0x1F7D
        | 0x1FA0..=0x1FAF
        | 0x1FF2..=0x1FF4
        | 0x1FF6
        | 0x1FF7
        | 0x1FFA..=0x1FFC => 'ω',
        0x1FE4 | 0x1FE5 | 0x1FEC => 'ρ',
        _ => c,
    }
}

/// Normalize Spanish/Greek/Hebrew headwords for comparison: lowercase, Spanish accents and
/// Greek diacritics folded, Hebrew points dropped, punctuation trimmed.
pub fn normalize_word(s: &str) -> String {
    let s = s.trim();
    let s = strip_tags_quick(s);
    let s = s.trim_matches(|c: char| {
        c.is_ascii_punctuation()
            || matches!(c, '«' | '»' | '“' | '”' | '‘' | '’' | '…' | '—' | '¿' | '¡')
    });
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    for ch in lower.chars() {
        match ch {
            'á' | 'à' | 'ä' | 'â' => out.push('a'),
            'é' | 'è' | 'ë' | 'ê' => out.push('e'),
            'í' | 'ì' | 'ï' | 'î' => out.push('i'),
            'ó' | 'ò' | 'ö' | 'ô' => out.push('o'),
            'ú' | 'ù' | 'ü' | 'û' => out.push('u'),
            'ñ' => out.push('n'),
            // Hebrew points/cantillation and combining marks count as alphabetic in Unicode.
            '\u{0591}'..='\u{05C7}' | '\u{0300}'..='\u{036F}' => {}
            c if c.is_alphanumeric() || c == '\'' => out.push(greek_base(c)),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
fn words_match(a: &str, b: &str) -> bool {
    let na = normalize_word(a);
    let nb = normalize_word(b);
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    na == nb || na.starts_with(&nb) || nb.starts_with(&na) || na.contains(&nb) || nb.contains(&na)
}

/// ISS-30: the iRV stores NT capitals Π..Ω one codepoint late (Ψριστοῦ → Χριστοῦ).
fn irv_fix_greek_caps(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{03A1}' => '\u{03A0}',
            '\u{03A3}' => '\u{03A1}',
            '\u{03A4}' => '\u{03A3}',
            '\u{03A5}' => '\u{03A4}',
            '\u{03A6}' => '\u{03A5}',
            '\u{03A7}' => '\u{03A6}',
            '\u{03A8}' => '\u{03A7}',
            '\u{03A9}' => '\u{03A8}',
            '\u{03AA}' => '\u{03A9}',
            c => c,
        })
        .collect()
}

fn is_irv_interlinear(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_lowercase().contains("interlineal-irv"))
        .unwrap_or(false)
}

/// Clean original-language token from interlinear tags.
/// e.g. `γὰρ2` → `γὰρ`, `ὁ4 Θεὸς5` → `ὁ Θεὸς`, `→` / empty → skip.
fn clean_original_script(raw: &str) -> String {
    let t = strip_tags_quick(raw);
    let t = t.trim();
    // Drop pure arrows / bullets used as alignment placeholders
    if t.is_empty() || t == "→" || t == "←" || t == "•" || t == "►" || t == "‹" || t == "›" {
        return String::new();
    }
    // iRV appends a digit index per word: γὰρ2, Οὕτω1, ὁ4 Θεὸς5
    let cleaned: Vec<String> = t
        .split_whitespace()
        .map(|tok| {
            let mut s = tok
                .trim_matches(|c: char| matches!(c, '‹' | '›' | '→' | '←' | '•' | '►'))
                .to_string();
            while s
                .chars()
                .last()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
            {
                s.pop();
            }
            s
        })
        .filter(|s| !s.is_empty() && s != "→" && s != "←")
        .collect();
    let t = cleaned.join(" ");
    if t.is_empty() {
        return String::new();
    }
    // Drop multi-token noise that is only punctuation
    if t.chars().all(|c| !c.is_alphanumeric() && !is_heb_or_grk(c)) {
        return String::new();
    }
    t
}

fn is_heb_or_grk(c: char) -> bool {
    matches!(c,
        '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}'
        | '\u{0370}'..='\u{03FF}' | '\u{1F00}'..='\u{1FFF}'
    )
}

fn script_kind(s: &str) -> &'static str {
    if s.chars().any(|c| matches!(c, '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}')) {
        "hebrew"
    } else if s.chars().any(|c| {
        matches!(c, '\u{0370}'..='\u{03FF}' | '\u{1F00}'..='\u{1FFF}')
    }) {
        "greek"
    } else {
        ""
    }
}

fn is_plausible_translit(s: &str) -> bool {
    let t = s.trim();
    if t.len() < 2 || t.len() > 40 {
        return false;
    }
    // Latin letters + macrons / circumflex common in e-Sword translit (ê, ô, ē)
    let has_letter = t.chars().any(|c| {
        c.is_ascii_alphabetic()
            || matches!(c, 'ā' | 'ē' | 'ī' | 'ō' | 'ū' | 'â' | 'ê' | 'î' | 'ô' | 'û' | 'ä' | 'ë' | 'ï' | 'ö' | 'ü')
    });
    let has_script = t.chars().any(is_heb_or_grk);
    let morphish = t.chars().any(|c| c.is_ascii_digit()) && t.chars().any(|c| c.is_ascii_alphabetic()) && t.len() <= 12;
    has_letter && !has_script && !morphish && !RE_STRONG_ONLY.is_match(&t.to_uppercase())
}

fn is_plausible_spanish_gloss(s: &str) -> bool {
    let t = s.trim().trim_matches(|c: char| c.is_ascii_punctuation() || matches!(c, '«' | '»' | '•'));
    if t.is_empty() || t == "•" {
        return false;
    }
    if t.chars().any(is_heb_or_grk) {
        return false;
    }
    // Morph codes: VAAI3S, NCcSFC, DNSM, etc.
    let u = t.to_uppercase();
    if u.len() <= 14
        && u.chars().all(|c| c.is_ascii_alphanumeric())
        && u.chars().any(|c| c.is_ascii_digit())
        && u.chars().any(|c| c.is_ascii_alphabetic())
    {
        return false;
    }
    true
}

/// One interlinear unit: a Spanish gloss and its hits (several for multi-`<num>` groups).
/// Filler units (grey/• RV1960 words with no original) have no hits.
#[derive(Debug, Clone)]
struct IlUnit {
    gloss: String,
    hits: Vec<InterlinearWordHit>,
}

impl IlUnit {
    fn codes(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for h in &self.hits {
            if !h.strong.is_empty() && !v.contains(&h.strong) {
                v.push(h.strong.clone());
            }
        }
        v
    }
}

/// ISS-02/30: sequential walk over an interlinear verse into units, in verse order, with no
/// dedupe. Transliterations only for Greek, between `</grk>` and the first `<num>`; morph
/// codes and markup never become transliterations. In an article + content group the
/// content code comes first (G3588 stays as a secondary code).
fn parse_interlinear_units(raw: &str, fix_greek_caps: bool) -> Vec<IlUnit> {
    let mut units: Vec<IlUnit> = Vec::new();
    if raw.is_empty() {
        return units;
    }
    let mut pending_original = String::new();
    let mut pending_script = String::new();
    let mut pending_translit = String::new();
    let mut pending_strongs: Vec<String> = Vec::new();
    let mut last_end = 0usize;

    for cap in RE_IL_TAG.captures_iter(raw) {
        let m = cap.get(0).unwrap();
        if pending_script == "greek"
            && pending_strongs.is_empty()
            && pending_translit.is_empty()
            && !pending_original.is_empty()
        {
            let between = raw.get(last_end..m.start()).unwrap_or("");
            let txt = RE_TAGS.replace_all(between, " ");
            let mut toks: Vec<String> = Vec::new();
            for tok in txt.split_whitespace() {
                if tok.starts_with(':') {
                    break;
                }
                let clean = tok.trim_matches(|c: char| {
                    c.is_ascii_punctuation() || matches!(c, '‹' | '›' | '•' | '→' | '←' | '►')
                });
                if is_plausible_translit(clean) {
                    toks.push(clean.to_string());
                } else if !toks.is_empty() {
                    break;
                }
            }
            pending_translit = toks.join(" ");
        }
        last_end = m.end();

        if let Some(num) = cap.get(1) {
            if let Some(code) = parse_strong_code(num.as_str()) {
                if !pending_strongs.contains(&code) {
                    pending_strongs.push(code);
                }
            }
            continue;
        }
        if let Some(blu) = cap.get(2) {
            let spanish = strip_tags_quick(blu.as_str())
                .trim()
                .trim_matches(|c: char| {
                    c.is_ascii_punctuation() || matches!(c, '«' | '»' | '“' | '”')
                })
                .to_string();
            if is_plausible_spanish_gloss(&spanish) {
                let mut hits = Vec::new();
                if !pending_strongs.is_empty() {
                    // ISS-02: article + content → content first, G3588 secondary.
                    let mut codes = pending_strongs.clone();
                    if codes.len() > 1 && codes.iter().any(|c| c == "G3588") {
                        codes.retain(|c| c != "G3588");
                        codes.push("G3588".into());
                    }
                    for (i, strong) in codes.iter().enumerate() {
                        hits.push(InterlinearWordHit {
                            spanish: spanish.clone(),
                            strong: strong.clone(),
                            original: if i == 0 {
                                pending_original.clone()
                            } else {
                                String::new()
                            },
                            translit: if i == 0 {
                                pending_translit.clone()
                            } else {
                                String::new()
                            },
                            script: pending_script.clone(),
                        });
                    }
                } else if !pending_original.is_empty() {
                    hits.push(InterlinearWordHit {
                        spanish: spanish.clone(),
                        strong: String::new(),
                        original: pending_original.clone(),
                        translit: pending_translit.clone(),
                        script: pending_script.clone(),
                    });
                }
                units.push(IlUnit {
                    gloss: spanish,
                    hits,
                });
            }
            pending_original.clear();
            pending_script.clear();
            pending_translit.clear();
            pending_strongs.clear();
            continue;
        }
        if let Some(o) = cap.get(3).or_else(|| cap.get(4)) {
            let greek = cap.get(4).is_some();
            let mut o = clean_original_script(o.as_str());
            if greek && fix_greek_caps {
                o = irv_fix_greek_caps(&o);
            }
            if !o.is_empty() {
                if pending_original.is_empty() {
                    pending_original = o;
                } else {
                    // multi-word original (e.g. ὁ Θεὸς)
                    pending_original.push(' ');
                    pending_original.push_str(&o);
                }
                pending_script = if greek { "greek" } else { "hebrew" }.into();
            }
            continue;
        }
        if let Some(fill) = cap.get(5) {
            let g = fill.as_str().trim().to_string();
            if is_plausible_spanish_gloss(&g) {
                units.push(IlUnit {
                    gloss: g,
                    hits: vec![],
                });
            }
        }
    }
    units
}

fn flatten_units(units: &[IlUnit]) -> Vec<InterlinearWordHit> {
    let mut seen = HashSet::new();
    units
        .iter()
        .flat_map(|u| u.hits.iter().cloned())
        .filter(|h| seen.insert(format!("{}|{}|{}", h.spanish, h.strong, h.original)))
        .collect()
}

/// Parse e-Sword interlinear verse into Spanish ↔ Strong ↔ original (H/G) hits.
///
/// NT iRV style:
///   `<grk>ἠγάπησεν3</grk> êgapêsen <num>G25</num> VAAI3S <blu>amó</blu>`
/// OT iRV style:
///   `<heb>רֵאשִׁית</heb>2 <num>H7225</num>:NCcSFC <blu>principio</blu>`
pub fn parse_interlinear_tokens(raw: &str) -> Vec<InterlinearWordHit> {
    flatten_units(&parse_interlinear_units(raw, false))
}

/// Back-compat helper used by older call sites / tests.
/// Returns (spanish_or_token, strong, optional hebrew/greek).
#[allow(dead_code)]
pub fn parse_interlinear_links(raw: &str) -> Vec<(String, String, Option<String>)> {
    parse_interlinear_tokens(raw)
        .into_iter()
        .map(|h| {
            let orig = if h.original.is_empty() {
                None
            } else {
                Some(h.original)
            };
            (h.spanish, h.strong, orig)
        })
        .collect()
}

fn module_abbr(path: &str) -> String {
    module_details_cached(path).0
}

// ─── Gloss matching (ISS-02) ───

const ENCLITICS: &[&str] = &["les", "los", "las", "nos", "le", "la", "lo", "se", "me", "te"];
/// Stems that would turn a pronoun into another word (aquellos → aquel).
const STEM_STOP: &[&str] = &["aquel", "ell", "nosotr", "vosotr"];

fn gloss_tokens(gloss: &str) -> Vec<String> {
    gloss
        .split(|c: char| c.is_whitespace() || c == '-')
        .map(normalize_word)
        .filter(|s| !s.is_empty())
        .collect()
}

/// ISS-02: how a tapped (normalized) word matches a Spanish gloss.
/// `Some(1)`: whole-token equality (T1). `Some(2)`: enclitic stem (díjole → dijo, dióle → dio)
/// or a close prefix (Levántate ↔ levanta), only for taps of 4+ letters. Never substring.
pub fn match_tier(tap_norm: &str, gloss: &str) -> Option<u8> {
    if tap_norm.is_empty() {
        return None;
    }
    let toks = gloss_tokens(gloss);
    if toks.iter().any(|t| t == tap_norm) {
        return Some(1);
    }
    let n = tap_norm.chars().count();
    if n < 4 {
        return None;
    }
    for suf in ENCLITICS {
        if let Some(stem) = tap_norm.strip_suffix(suf) {
            if stem.chars().count() >= 3 && !STEM_STOP.contains(&stem) && toks.iter().any(|t| t == stem) {
                return Some(2);
            }
        }
    }
    for t in &toks {
        let tn = t.chars().count();
        if tn < 4 {
            continue;
        }
        let (short, long, sl, ll) = if tn <= n {
            (t.as_str(), tap_norm, tn, n)
        } else {
            (tap_norm, t.as_str(), n, tn)
        };
        if long.starts_with(short) && ll - sl <= 4 && sl >= 4 && !STEM_STOP.contains(&short) {
            return Some(2);
        }
    }
    None
}

/// The gloss token that T1-matched, with its own spelling (for the dictionary headword).
fn t1_gloss_token(gloss: &str, tap_norm: &str) -> Option<String> {
    gloss
        .split(|c: char| c.is_whitespace() || c == '-')
        .find(|tok| normalize_word(tok) == tap_norm)
        .map(|tok| {
            tok.trim_matches(|c: char| {
                c.is_ascii_punctuation()
                    || matches!(c, '«' | '»' | '“' | '”' | '¿' | '¡' | '…' | '—')
            })
            .to_string()
        })
}

fn is_all_caps_word(w: &str) -> bool {
    let letters: Vec<char> = w.chars().filter(|c| c.is_alphabetic()).collect();
    letters.len() > 1 && letters.iter().all(|c| c.is_uppercase())
}

/// ISS-37: divine-name aliases, tried before gloss matching. Returns the Strong codes to
/// look for in the verse, in preference order.
fn divine_alias(word: &str, tap_norm: &str, has_t1: bool) -> Option<&'static [&'static str]> {
    const YHWH: &[&str] = &["H3068", "H3069", "H136"];
    if word.trim().to_lowercase() == "di-s" {
        return Some(&["H430", "H410"]);
    }
    if matches!(tap_norm, "yahve" | "yave" | "yahveh" | "adonai") {
        return Some(YHWH);
    }
    if tap_norm == "senor" && (is_all_caps_word(word) || !has_t1) {
        return Some(YHWH);
    }
    if tap_norm == "dios" && is_all_caps_word(word) {
        return Some(&["H3069"]);
    }
    None
}

// ─── Versification (ISS-35) ───

/// Verified chapter-signature remaps from a Bible's numbering to the interlinear's
/// (RV/KJV). Everything else is identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersificationRemap {
    /// Joel: Bible 4 chapters, iRV 3 (3:1-5 → 2:28-32; 4:n → 3:n).
    pub joel: bool,
    /// Malaquías: Bible ch 3 has 24 verses, no ch 4 (3:19-24 → 4:1-6).
    pub malachi: bool,
    /// 1 Reyes: Bible 4 has 20, 5 has 32 (5:1-14 → 4:21-34; 5:n → 5:n-14).
    pub kings1: bool,
    /// Números: Bible 16 has 35, 17 has 28 (17:1-15 → 16:36-50; 17:n → 17:n-15).
    pub numbers: bool,
}

impl VersificationRemap {
    #[allow(dead_code)] // additive API for WP5 (lib.rs)
    pub fn is_identity(&self) -> bool {
        *self == VersificationRemap::default()
    }

    /// Map a Bible reference to the interlinear's numbering.
    pub fn map(&self, b: i32, c: i32, v: i32) -> (i32, i32, i32) {
        match b {
            29 if self.joel && c == 3 && (1..=5).contains(&v) => (29, 2, v + 27),
            29 if self.joel && c == 4 => (29, 3, v),
            39 if self.malachi && c == 3 && v >= 19 => (39, 4, v - 18),
            11 if self.kings1 && c == 5 && v <= 14 => (11, 4, v + 20),
            11 if self.kings1 && c == 5 => (11, 5, v - 14),
            4 if self.numbers && c == 17 && v <= 15 => (4, 16, v + 35),
            4 if self.numbers && c == 17 => (4, 17, v - 15),
            _ => (b, c, v),
        }
    }
}

/// Pure signature detection from per-chapter verse counts (0 = chapter absent).
fn detect_remap(bible: &dyn Fn(i32, i32) -> i32, il: &dyn Fn(i32, i32) -> i32) -> VersificationRemap {
    VersificationRemap {
        joel: bible(29, 4) > 0
            && bible(29, 3) == 5
            && bible(29, 2) == 27
            && il(29, 4) == 0
            && il(29, 2) == 32,
        malachi: bible(39, 3) == 24 && bible(39, 4) == 0 && il(39, 3) == 18 && il(39, 4) == 6,
        kings1: bible(11, 4) == 20 && bible(11, 5) == 32 && il(11, 4) == 34 && il(11, 5) == 18,
        numbers: bible(4, 16) == 35 && bible(4, 17) == 28 && il(4, 16) == 50 && il(4, 17) == 13,
    }
}

/// Remap per (bible, interlinear), with both files' identities: a module replaced under the
/// same name is re-measured (review round 2), like the connection cache.
type RemapIdent = (Option<FileIdent>, Option<FileIdent>);
static REMAP_CACHE: Lazy<Mutex<HashMap<(String, String), (VersificationRemap, RemapIdent)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// ISS-35: versification remap between a reading Bible and the interlinear, computed once
/// per pair from verse counts.
pub fn versification_remap(bible_path: &str, interlinear_path: &str) -> VersificationRemap {
    let key = (bible_path.to_string(), interlinear_path.to_string());
    if bible_path == interlinear_path {
        return VersificationRemap::default();
    }
    let ident = (file_ident(bible_path), file_ident(interlinear_path));
    if let Some((r, id)) = REMAP_CACHE.lock().get(&key) {
        if *id == ident {
            return *r;
        }
    }
    let counts = |p: &str| {
        let mut m: HashMap<(i32, i32), i32> = HashMap::new();
        let _ = with_conn(p, |conn| {
            let mut st = conn
                .prepare(
                    "SELECT Book, Chapter, COUNT(*) FROM Bible
                     WHERE Book IN (4, 11, 29, 39) GROUP BY Book, Chapter",
                )
                .map_err(|e| e.to_string())?;
            let rows = st
                .query_map([], |r| Ok((r.get::<_, i32>(0)?, r.get::<_, i32>(1)?, r.get::<_, i32>(2)?)))
                .map_err(|e| e.to_string())?;
            for (b, c, n) in rows.flatten() {
                m.insert((b, c), n);
            }
            Ok(())
        });
        m
    };
    let bm = counts(bible_path);
    let im = counts(interlinear_path);
    let r = detect_remap(
        &|b, c| bm.get(&(b, c)).copied().unwrap_or(0),
        &|b, c| im.get(&(b, c)).copied().unwrap_or(0),
    );
    REMAP_CACHE.lock().insert(key, (r, ident));
    r
}

// ─── Own-tag fallback for broken interlinear verses (ISS-38) ───

/// Per folder: the verified fallback Bible and its file identity (review round 2: a
/// replaced file is re-verified).
static FALLBACK_CACHE: Lazy<Mutex<HashMap<String, (Option<String>, Option<FileIdent>)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// An own-tag-verified RVR1960 '+' Bible next to the interlinear, if any.
fn own_tag_fallback_bible(interlinear_path: &str) -> Option<String> {
    let dir = Path::new(interlinear_path).parent()?.to_string_lossy().to_string();
    if let Some((c, id)) = FALLBACK_CACHE.lock().get(&dir) {
        let still = match c {
            Some(p) => id.is_some() && file_ident(p) == *id,
            None => true,
        };
        if still {
            return c.clone();
        }
    }
    let mut cands: Vec<String> = fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            n.contains("rvr1960+") && n.ends_with(".bbli")
        })
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    cands.sort();
    let found = cands
        .into_iter()
        .find(|p| p != interlinear_path && bible_has_own_tags(p));
    let id = found.as_deref().and_then(file_ident);
    FALLBACK_CACHE.lock().insert(dir, (found.clone(), id));
    found
}

fn own_tag_units_as_il(raw: &str) -> Vec<IlUnit> {
    own_tag_units(raw)
        .into_iter()
        .map(|u| IlUnit {
            hits: u
                .strongs
                .iter()
                .filter_map(|s| parse_strong_code(s))
                .map(|s| InterlinearWordHit {
                    spanish: u.word.clone(),
                    strong: s,
                    original: String::new(),
                    translit: String::new(),
                    script: String::new(),
                })
                .collect(),
            gloss: u.word,
        })
        .collect()
}

// ─── Tagged twin: the same text with its own Strong tags (accuracy round 2026-11-11) ───
//
// A reader of a plain Bible (00RVR1960, «RV1960 Reina Valera 1960», 00LBLA, …) gets the
// Strong of the tapped word from the library's Strong-tagged edition of the SAME text
// (00RVR1960+, 00LBLA+, …) by aligning the two verses word by word. Measured on every tagged
// word of RVR1960+: the iRV interlinear's Old Testament codes disagree with it on 13 083
// content words; where Webster 1833 (KJV Strong's) or LBLA+ backs only one side, RVR1960+ is
// right 5 146 times and the iRV 5 times (digit slips such as H8064 for H8034 «nombre»).

/// One tappable word of a rendered verse, as app.v7.js `tokenizeWords` + `tapContext` see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerseTok {
    pub text: String,
    /// Occurrence key: app.v7.js `normalizeLoose(cleanQueryWord(text))`.
    pub key: String,
    /// The word's own Strong tags (`<span class="word" data-strong>`), empty when untagged.
    pub strongs: Vec<String>,
}

/// app.v7.js `normalizeLoose`: lowercase, accents off, letters and digits only.
pub(crate) fn loose_key(s: &str) -> String {
    normalize_word(s).chars().filter(|c| c.is_alphanumeric()).collect()
}

static RE_TOK_WORD: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[\p{L}\p{M}\p{N}]+(?:['’][\p{L}\p{M}\p{N}]+)*").unwrap());
static RE_TOK_TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"<(/?)([A-Za-z][\w-]*)([^>]*)>").unwrap());
static RE_TOK_CLASS: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)\bclass\s*=\s*"([^"]*)""#).unwrap());
static RE_TOK_DATA_STRONG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\bdata-strong\s*=\s*"([^"]*)""#).unwrap());

/// The `.word` tokens of one rendered verse (`load_bible_chapter` html), in order: text in
/// morph / strong-ref / fn / vtitle elements is skipped, bare Strong codes and digit runs are
/// not words, and a backend `<span class="word" data-strong>` carries its own codes.
pub(crate) fn verse_word_tokens(html: &str) -> Vec<VerseTok> {
    struct Open {
        name: String,
        skip: bool,
        own: Option<Vec<String>>,
    }
    fn push_text(text: &str, stack: &[Open], out: &mut Vec<VerseTok>) {
        if text.is_empty() || stack.iter().any(|e| e.skip) {
            return;
        }
        let own = stack.iter().rev().find_map(|e| e.own.clone());
        let text = html_escape::decode_html_entities(text);
        for m in RE_TOK_WORD.find_iter(&text) {
            let w = m.as_str();
            if !w.chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            let bare_code = w.len() > 1
                && matches!(w.as_bytes()[0], b'H' | b'G')
                && w[1..].bytes().all(|b| b.is_ascii_digit());
            if bare_code {
                continue;
            }
            out.push(VerseTok {
                text: w.to_string(),
                key: loose_key(w),
                strongs: own.clone().unwrap_or_default(),
            });
        }
    }
    let mut out: Vec<VerseTok> = Vec::new();
    let mut stack: Vec<Open> = Vec::new();
    let mut last = 0usize;
    for c in RE_TOK_TAG.captures_iter(html) {
        let m = c.get(0).unwrap();
        push_text(&html[last..m.start()], &stack, &mut out);
        last = m.end();
        let name = c[2].to_ascii_lowercase();
        let attrs = &c[3];
        if !c[1].is_empty() {
            if let Some(pos) = stack.iter().rposition(|e| e.name == name) {
                stack.truncate(pos);
            }
            continue;
        }
        if attrs.trim_end().ends_with('/') || matches!(name.as_str(), "br" | "img" | "hr" | "wbr") {
            continue;
        }
        let class = RE_TOK_CLASS
            .captures(attrs)
            .map(|k| k[1].to_ascii_lowercase())
            .unwrap_or_default();
        let has = |k: &str| class.split_whitespace().any(|x| x == k);
        let skip = has("morph") || has("strong-ref") || has("fn") || has("vtitle");
        let own = if has("word") {
            RE_TOK_DATA_STRONG
                .captures(attrs)
                .map(|k| k[1].split_whitespace().filter_map(parse_strong_code).collect::<Vec<_>>())
        } else {
            None
        };
        stack.push(Open { name, skip, own });
    }
    push_text(&html[last..], &stack, &mut out);
    out
}

/// A tag group's codes with the content code first: the Greek article G3588 and the Hebrew
/// object marker H853 go after the others (kept when they are all there is). The iRV parse
/// already does this for article + noun units; own tags store them in Greek/Hebrew order.
pub(crate) fn primary_first(codes: Vec<String>) -> Vec<String> {
    let secondary = |c: &String| c == "G3588" || c == "H853";
    if codes.iter().all(secondary) {
        return codes;
    }
    let (mut first, rest): (Vec<String>, Vec<String>) = codes.into_iter().partition(|c| !secondary(c));
    first.extend(rest);
    first
}

/// Index pairs (a, b) of one longest common subsequence of two key sequences, in order.
pub(crate) fn lcs_pairs(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return Vec::new();
    }
    // dp[i][j] = LCS length of a[i..], b[j..]; verses stay far below u16::MAX tokens.
    let w = m + 1;
    let mut dp = vec![0u16; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * w + j] = if a[i] == b[j] {
                dp[(i + 1) * w + j + 1] + 1
            } else {
                dp[(i + 1) * w + j].max(dp[i * w + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(dp[0] as usize);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if dp[(i + 1) * w + j] >= dp[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Word overlap of two verses: 2·LCS / (|a| + |b|), 0..1.
fn verse_overlap(a: &[VerseTok], b: &[VerseTok], pairs: usize) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    2.0 * pairs as f64 / (a.len() + b.len()) as f64
}

/// One verse rendered the way `load_bible_chapter` renders it, as tokens: the verse itself,
/// without its section titles — the words the page counts for a tap's occurrence.
fn verse_tokens_of(module_path: &str, book: i32, chapter: i32, verse: i32) -> Option<Vec<VerseTok>> {
    let raw = load_raw_verse(module_path, book, chapter, verse)?;
    let text_lines = chapter_text_titles(module_path, book, chapter);
    let (html, _) = render_bible_verse(&raw, bible_has_own_tags(module_path), text_lines);
    Some(verse_word_tokens(&html))
}

/// All the words of one verse's text, section titles included (the twin side of a tap, and
/// twin detection: «Salmo de David.» is verse text in some editions and a title in others).
fn verse_tokens_full(module_path: &str, book: i32, chapter: i32, verse: i32) -> Option<Vec<VerseTok>> {
    let raw = load_raw_verse(module_path, book, chapter, verse)?;
    let html = if bible_has_own_tags(module_path) {
        clean_bible_verse_tagged(&raw)
    } else {
        clean_bible_text(&raw)
    };
    Some(verse_word_tokens(&html))
}

/// Verses compared to recognise the same text (Gn, Ps, Is, Mt, Jn, Rom, Ap).
const TWIN_PROBES: [(i32, i32, i32); 8] = [
    (1, 1, 1),
    (1, 22, 2),
    (19, 23, 1),
    (23, 53, 5),
    (40, 5, 3),
    (43, 3, 16),
    (45, 8, 28),
    (66, 22, 21),
];
/// Mean word overlap on the probes for two files to count as the same text.
const TWIN_MIN_OVERLAP: f64 = 0.9;
/// Per tap: the two verses must still be essentially the same wording.
const TWIN_MIN_VERSE_OVERLAP: f64 = 0.6;
/// Per tap: LCS table bound (≈ 500 × 500 words; real verses are under 120 words).
const TWIN_MAX_LCS_CELLS: usize = 250_000;

/// bible path → (its identity, the tagged twin found)
static TWIN_CACHE: Lazy<Mutex<HashMap<String, (Option<FileIdent>, Option<String>)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Probe one candidate without caching its connection: (own-tag layout verdict, raw probes).
fn twin_candidate_probe(path: &str) -> Option<(bool, Vec<Option<String>>)> {
    with_probe_slot(path, |conn, _| {
        let mut st = conn
            .prepare("SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3")
            .map_err(|e| es_error(&e.to_string()))?;
        let mut get = |b: i32, c: i32, v: i32| st.query_row(rusqlite::params![b, c, v], |r| cell_to_string(r, 0)).ok();
        let (mut groups, mut after) = (0usize, 0usize);
        for &(b, c, v) in LAYOUT_SAMPLE {
            if let Some(raw) = get(b, c, v) {
                let s = own_tag_layout_stats(&raw);
                groups += s.num_groups;
                after += s.after_tvm;
            }
        }
        let tagged = own_tag_layout_ok(groups, after);
        let probes = if tagged { TWIN_PROBES.iter().map(|&(b, c, v)| get(b, c, v)).collect() } else { Vec::new() };
        Ok((tagged, probes))
    })
    .ok()
}

fn find_tagged_twin(bible_path: &str) -> Option<String> {
    if bible_has_own_tags(bible_path) {
        return Some(bible_path.to_string());
    }
    let reading: Vec<Option<Vec<VerseTok>>> = TWIN_PROBES
        .iter()
        .map(|&(b, c, v)| verse_tokens_full(bible_path, b, c, v).filter(|t| !t.is_empty()))
        .collect();
    if reading.iter().flatten().count() < 3 {
        return None;
    }
    let dir = Path::new(bible_path).parent()?;
    let mut cands: Vec<String> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
            n.ends_with(".bbli") && (n.contains('+') || n.contains("strong"))
        })
        .map(|p| p.to_string_lossy().to_string())
        .filter(|p| p != bible_path)
        .collect();
    cands.sort();
    // A probe verse the reading Bible has but the candidate lacks scores 0, so a New
    // Testament-only edition never wins over a whole-Bible one for a whole Bible.
    let present = reading.iter().flatten().count();
    let mut best: Option<(f64, String)> = None;
    for cand in cands {
        let Some((true, probes)) = twin_candidate_probe(&cand) else { continue };
        let (mut sum, mut n) = (0.0, 0usize);
        for (k, raw) in probes.iter().enumerate() {
            let (Some(ra), Some(raw)) = (&reading[k], raw) else { continue };
            let tb = verse_word_tokens(&clean_bible_verse_tagged(raw));
            if tb.is_empty() {
                continue;
            }
            let ka: Vec<&str> = ra.iter().map(|t| t.key.as_str()).collect();
            let kb: Vec<&str> = tb.iter().map(|t| t.key.as_str()).collect();
            sum += verse_overlap(ra, &tb, lcs_pairs(&ka, &kb).len());
            n += 1;
        }
        if n >= 3 {
            let score = sum / present as f64;
            if score >= TWIN_MIN_OVERLAP && best.as_ref().map_or(true, |(s, _)| score > *s) {
                best = Some((score, cand));
                if score >= 0.98 {
                    break; // essentially the same text: no later file can be a better twin
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

/// The library's Strong-tagged edition of the same text as `bible_path` (itself when it is
/// tagged), found once per file and re-checked when the file changes.
pub(crate) fn tagged_twin(bible_path: &str) -> Option<String> {
    let ident = file_ident(bible_path);
    if let Some((id, found)) = TWIN_CACHE.lock().get(bible_path) {
        if ident.is_some() && *id == ident {
            return found.clone();
        }
    }
    let found = find_tagged_twin(bible_path);
    TWIN_CACHE.lock().insert(bible_path.to_string(), (ident, found.clone()));
    found
}

/// Own Strong tags for the tapped word, read from the tagged twin of the reading Bible: the
/// `occurrence`-th word with the tap's key in the reading verse, aligned to the twin's verse.
/// `None` when there is no twin, the verses differ too much, or the twin's word is untagged.
fn twin_tags_for_tap(
    bible_path: &str,
    book: i32,
    chapter: i32,
    verse: i32,
    word: &str,
    occurrence: Option<usize>,
) -> Option<(String, Vec<String>)> {
    let twin = tagged_twin(bible_path)?;
    let key = loose_key(word);
    if key.is_empty() {
        return None;
    }
    // The page counts a tap's occurrence among the verse's own words (section titles apart).
    let body = verse_tokens_of(bible_path, book, chapter, verse)?;
    let idx: Vec<usize> = (0..body.len()).filter(|&i| body[i].key == key).collect();
    let (reading, i) = match idx.get(occurrence.unwrap_or(0)).or(idx.first()) {
        Some(&i) => (body, i),
        None => {
            // A word of a section title («Salmo de David.»): its first place in the verse's
            // whole text, titles included, so the twin still tags it.
            let full = verse_tokens_full(bible_path, book, chapter, verse)?;
            let i = full.iter().position(|t| t.key == key)?;
            (full, i)
        }
    };
    if twin == bible_path {
        let own = reading[i].strongs.clone();
        return (!own.is_empty()).then(|| (twin, own));
    }
    let tagged = verse_tokens_full(&twin, book, chapter, verse)?;
    // A whole chapter stored as one "verse" would make the LCS table huge: interlinear instead.
    if reading.len().saturating_mul(tagged.len()) > TWIN_MAX_LCS_CELLS {
        return None;
    }
    let ka: Vec<&str> = reading.iter().map(|t| t.key.as_str()).collect();
    let kb: Vec<&str> = tagged.iter().map(|t| t.key.as_str()).collect();
    let pairs = lcs_pairs(&ka, &kb);
    if verse_overlap(&reading, &tagged, pairs.len()) < TWIN_MIN_VERSE_OVERLAP {
        return None;
    }
    let j = pairs.iter().find(|p| p.0 == i)?.1;
    let own = tagged[j].strongs.clone();
    (!own.is_empty()).then(|| (twin, own))
}

// ─── Lemma index: Greek / Hebrew / transliteration → Strong (ISS-43) ───

#[derive(Debug, Default)]
struct LemmaIndex {
    exact: HashMap<String, Vec<String>>,
    folded: HashMap<String, Vec<String>>,
    translit: HashMap<String, Vec<String>>,
}

type LemmaCache = HashMap<String, (FileIdent, Arc<LemmaIndex>)>;
static LEMMA_INDEX: Lazy<Mutex<LemmaCache>> = Lazy::new(|| Mutex::new(HashMap::new()));
static RE_LEMMA_RUN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<(grk|heb)>(.*?)</(?:grk|heb)>").unwrap());
static RE_BOLD: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?is)<span style="font-weight:bold;">([^<]{1,40})</span>"#).unwrap());

/// Fold for original-script keys: lowercase, Greek diacritics and final sigma folded,
/// Hebrew points/cantillation removed, spaces and punctuation dropped.
fn fold_original(s: &str) -> String {
    normalize_word(&s.replace(char::is_whitespace, ""))
}

fn lemma_index_source(lex_paths: &[String]) -> Option<String> {
    let mut v: Vec<&String> = lex_paths
        .iter()
        .filter(|p| {
            Path::new(p.as_str())
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase().contains("stronges"))
                .unwrap_or(false)
        })
        .collect();
    v.sort_by_key(|p| {
        let n = Path::new(p.as_str())
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        (!n.starts_with("04"), n)
    });
    v.first().map(|s| s.to_string())
}

fn build_lemma_index(conn: &Connection, ts: &TopicSchema) -> LemmaIndex {
    let mut idx = LemmaIndex::default();
    let sql = format!(
        "SELECT \"{}\", substr(\"{}\", 1, 700) FROM \"{}\"",
        ts.topic, ts.def, ts.table
    );
    let Ok(mut st) = conn.prepare(&sql) else {
        return idx;
    };
    let Ok(rows) = st.query_map([], |r| Ok((cell_to_string(r, 0)?, cell_to_string(r, 1)?))) else {
        return idx;
    };
    let add = |m: &mut HashMap<String, Vec<String>>, k: String, code: &str| {
        if k.is_empty() {
            return;
        }
        let e = m.entry(k).or_default();
        if !e.iter().any(|c| c == code) {
            e.push(code.to_string());
        }
    };
    for (topic, def) in rows.flatten() {
        let Some(code) = parse_strong_code(&topic) else {
            continue;
        };
        let Some(run) = RE_LEMMA_RUN.captures(&def) else {
            continue;
        };
        let whole = run.get(0).unwrap();
        if whole.start() > 300 {
            continue;
        }
        let orig = strip_tags_quick(run.get(2).unwrap().as_str()).trim().to_string();
        if orig.is_empty() {
            continue;
        }
        add(&mut idx.exact, orig.clone(), &code);
        add(&mut idx.folded, fold_original(&orig), &code);
        if let Some(b) = RE_BOLD.captures(&def[whole.end()..]) {
            let tr = normalize_word(b.get(1).unwrap().as_str());
            if tr.chars().count() >= 2 && tr.chars().all(|c| c.is_ascii_alphabetic()) {
                add(&mut idx.translit, tr, &code);
            }
        }
    }
    idx
}

fn lemma_index_for(path: &str) -> Option<Arc<LemmaIndex>> {
    let ident = file_ident(path)?;
    if let Some((cid, idx)) = LEMMA_INDEX.lock().get(path) {
        if *cid == ident {
            return Some(idx.clone());
        }
    }
    let idx = with_slot(path, |conn, schema| {
        let ts = schema.lex.ok_or_else(|| "sin tabla".to_string())?;
        Ok(Arc::new(build_lemma_index(conn, &ts)))
    })
    .ok()?;
    LEMMA_INDEX
        .lock()
        .insert(path.to_string(), (ident, idx.clone()));
    Some(idx)
}

/// ISS-43: Strong codes for a Greek/Hebrew word (pointed form first, then folded) or, when
/// `allow_translit`, a Spanish-style transliteration (agápe → agape). Built lazily from
/// 04STRONGes among `lex_paths`; empty when that lexicon is absent.
pub fn lemma_index_lookup(lex_paths: &[String], query: &str, allow_translit: bool) -> Vec<String> {
    let q = query.trim();
    if q.is_empty() {
        return vec![];
    }
    let Some(src) = lemma_index_source(lex_paths) else {
        return vec![];
    };
    let Some(idx) = lemma_index_for(&src) else {
        return vec![];
    };
    if !script_kind(q).is_empty() {
        let composed = q.to_string();
        if let Some(v) = idx.exact.get(&composed) {
            return v.clone();
        }
        return idx.folded.get(&fold_original(q)).cloned().unwrap_or_default();
    }
    if allow_translit {
        return idx.translit.get(&normalize_word(q)).cloned().unwrap_or_default();
    }
    vec![]
}

// ─── Interlinear validation (ISS-44) ───

static IL_VALID_CACHE: Lazy<Mutex<HashMap<String, (FileIdent, bool)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// True when the module parses as an interlinear: ≥3 Strong hits in Gn 1:1 or Jn 3:16
/// (NT-only interlinears pass on Jn 3:16).
fn interlinear_validated(path: &str) -> bool {
    let Some(ident) = file_ident(path) else {
        return false;
    };
    if let Some((cid, v)) = IL_VALID_CACHE.lock().get(path) {
        if *cid == ident {
            return *v;
        }
    }
    let ok = with_probe_slot(path, |conn, _| {
        for (b, c, v) in [(1, 1, 1), (43, 3, 16)] {
            if let Ok(raw) = conn.query_row(
                "SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3",
                rusqlite::params![b, c, v],
                |r| cell_to_string(r, 0),
            ) {
                let n = parse_interlinear_tokens(&raw)
                    .iter()
                    .filter(|h| !h.strong.is_empty())
                    .count();
                if n >= 3 {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    })
    .unwrap_or(false);
    IL_VALID_CACHE
        .lock()
        .insert(path.to_string(), (ident, ok));
    ok
}

// ─── Resolve ───

/// Options for [`resolve_word_links_at`]. Every field is optional; `Default` reproduces
/// the plain [`resolve_word_links`] call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ResolveOpts {
    /// ISS-02: typed query (Buscar/Enter). Interlinear only on whole-token equality; never
    /// replaces the typed word with a gloss; the lemma index may use transliterations.
    pub typed: bool,
    /// ISS-36: the tapped word's own Strong tags (`data-strong`), primary first.
    pub tagged_strongs: Vec<String>,
    /// ISS-35: the reading Bible, for versification remapping to the interlinear.
    pub bible_path: Option<String>,
    /// ISS-13: the user's primary dictionary, kept ahead of the ranking.
    pub primary_dictionary: Option<String>,
    /// ISS-13: the user's primary lexicon, kept ahead of the ranking.
    pub primary_lexicon: Option<String>,
    /// Accuracy gate only: skip the tagged-twin lookup (measures the interlinear path alone).
    pub skip_twin: bool,
}

const LEXICON_CAP: usize = 8;
const DICTIONARY_CAP: usize = 10;
const DICT_PER_MODULE: usize = 3;

/// Per lexicon path: verdict and file identity (review round 2: replaced files re-checked).
static PROSE_CACHE: Lazy<Mutex<HashMap<String, (bool, Option<FileIdent>)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Lexicons whose topics are prose fragments (DBADV): average topic longer than 40 chars.
fn prose_topic_module(path: &str) -> bool {
    let ident = file_ident(path);
    if let Some((v, id)) = PROSE_CACHE.lock().get(path) {
        if *id == ident {
            return *v;
        }
    }
    let v = with_slot(path, |conn, schema| {
        let Some(ts) = schema.lex else {
            return Ok(true);
        };
        conn.query_row(
            &format!("SELECT avg(length(\"{}\")) FROM \"{}\"", ts.topic, ts.table),
            [],
            |r| r.get::<_, Option<f64>>(0),
        )
        .map(|a| a.unwrap_or(0.0) > 40.0)
        .map_err(|e| e.to_string())
    })
    .unwrap_or(true);
    PROSE_CACHE.lock().insert(path.to_string(), (v, ident));
    v
}

fn linked(path: &str, e: DictEntry) -> LinkedEntry {
    LinkedEntry {
        module_path: path.to_string(),
        module_abbr: module_abbr(path),
        topic: e.topic,
        html: e.html,
        plain: e.plain,
        match_kind: e.match_kind,
    }
}

fn push_original(h: &InterlinearWordHit, hebrew: &mut Vec<String>, greek: &mut Vec<String>) {
    if h.original.is_empty() {
        return;
    }
    let k = if h.script.is_empty() {
        script_kind(&h.original)
    } else {
        h.script.as_str()
    };
    let target = match k {
        "hebrew" => hebrew,
        "greek" => greek,
        _ => return,
    };
    if !target.iter().any(|x| x == &h.original) {
        target.push(h.original.clone());
    }
}

/// Full interconnect: Spanish/Greek/Hebrew/Strong → dictionary + lexicon hits.
/// Back-compat wrapper over [`resolve_word_links_at`] (no occurrence, default options).
pub fn resolve_word_links(
    word: &str,
    dict_paths: &[String],
    lex_paths: &[String],
    interlinear_path: Option<&str>,
    book: i32,
    chapter: i32,
    verse: i32,
) -> WordLinkResult {
    resolve_word_links_at(
        word,
        dict_paths,
        lex_paths,
        interlinear_path,
        book,
        chapter,
        verse,
        None,
        &ResolveOpts::default(),
    )
}

/// ISS-02/03/04/13/35/36/37/38/43: word → Strong → dictionary + lexicon.
///
/// `occurrence` is the 0-based index of the tapped token among same-text tokens of the verse
/// (`Some(n)` picks the n-th whole-token gloss match; past the end it falls back to the first
/// match, never to nothing). `None` keeps the first match that carries a Strong.
#[allow(clippy::too_many_arguments)]
pub fn resolve_word_links_at(
    word: &str,
    dict_paths: &[String],
    lex_paths: &[String],
    interlinear_path: Option<&str>,
    book: i32,
    chapter: i32,
    verse: i32,
    occurrence: Option<usize>,
    opts: &ResolveOpts,
) -> WordLinkResult {
    let word = word.trim();
    let normalized = normalize_word(word);
    let tap_code = parse_strong_code(word);
    let tap_script = script_kind(word);
    let mut strongs: Vec<String> = Vec::new();
    let mut hebrew: Vec<String> = Vec::new();
    let mut greek: Vec<String> = Vec::new();
    let mut interlinear_used = false;
    let mut interlinear_hits: Vec<InterlinearWordHit> = Vec::new();
    let mut matched_hit: Option<InterlinearWordHit> = None;
    let mut match_kind = String::new();
    let mut interlinear_fallback: Option<String> = None;
    let mut lemma_index_used = false;
    let mut interlinear_verse: Option<[i32; 3]> = None;
    let mut dict_head = word.to_string();

    match tap_script {
        "hebrew" => hebrew.push(word.to_string()),
        "greek" => greek.push(word.to_string()),
        _ => {}
    }

    // ISS-36: the word's own tags win.
    let mut tagged: Vec<String> = {
        let mut v: Vec<String> = Vec::new();
        for s in opts.tagged_strongs.iter().filter_map(|s| parse_strong_code(s)) {
            if !v.contains(&s) {
                v.push(s);
            }
        }
        v
    };
    // An untagged word of a plain Bible: the own tags of its Strong-tagged twin (same text).
    let mut tagged_twin_abbr: Option<String> = None;
    if tagged.is_empty() && tap_code.is_none() && tap_script.is_empty() && !opts.typed && !opts.skip_twin {
        if let Some(bp) = opts.bible_path.as_deref().filter(|p| !p.is_empty()) {
            if let Some((twin, codes)) = twin_tags_for_tap(bp, book, chapter, verse, word, occurrence) {
                let mut v: Vec<String> = Vec::new();
                for c in codes.iter().filter_map(|c| parse_strong_code(c)) {
                    if !v.contains(&c) {
                        v.push(c);
                    }
                }
                if !v.is_empty() {
                    tagged = v;
                    tagged_twin_abbr = Some(module_abbr(&twin));
                }
            }
        }
    }
    let tagged = primary_first(tagged);

    // Interlinear units for the (remapped) verse, with the own-tag fallback (ISS-35/38).
    let mut units: Vec<IlUnit> = Vec::new();
    if let Some(ip) = interlinear_path.filter(|p| !p.is_empty()) {
        let (lb, lc, lv) = match opts.bible_path.as_deref().filter(|p| !p.is_empty()) {
            Some(bp) => versification_remap(bp, ip).map(book, chapter, verse),
            None => (book, chapter, verse),
        };
        if (lb, lc, lv) != (book, chapter, verse) {
            interlinear_verse = Some([lb, lc, lv]);
        }
        if let Some(raw) = load_raw_verse(ip, lb, lc, lv) {
            interlinear_used = true;
            units = parse_interlinear_units(&raw, is_irv_interlinear(ip));
        }
        let no_codes = units.iter().all(|u| u.codes().is_empty());
        if no_codes {
            if let Some(fb) = own_tag_fallback_bible(ip) {
                if let Some(raw) = load_raw_verse(&fb, lb, lc, lv) {
                    let fu = own_tag_units_as_il(&raw);
                    if !fu.is_empty() {
                        units = fu;
                        interlinear_used = true;
                        interlinear_fallback = Some(module_abbr(&fb));
                    }
                }
            }
        }
        interlinear_hits = flatten_units(&units);
    }

    // Units matched by the chosen strategy, in verse order.
    let mut matched: Vec<usize> = Vec::new();
    let mut pick_codes_first: Option<String> = None;

    if !tagged.is_empty() {
        strongs = tagged.clone();
        match_kind = "tagged".into();
        if let Some(h) = interlinear_hits.iter().find(|h| h.strong == tagged[0]) {
            matched_hit = Some(h.clone());
            push_original(h, &mut hebrew, &mut greek);
        }
    } else if let Some(code) = tap_code.clone() {
        strongs.push(code.clone());
        match_kind = "code".into();
        if let Some(h) = interlinear_hits.iter().find(|h| h.strong == code) {
            matched_hit = Some(h.clone());
            push_original(h, &mut hebrew, &mut greek);
        }
    } else if !tap_script.is_empty() {
        // Greek/Hebrew script tap: exact (folded) original first.
        let tf = fold_original(word);
        matched = (0..units.len())
            .filter(|&i| {
                units[i].hits.iter().any(|h| {
                    !h.original.is_empty()
                        && (fold_original(&h.original) == tf
                            || h.original.split_whitespace().any(|t| fold_original(t) == tf))
                })
            })
            .collect();
        if !matched.is_empty() {
            match_kind = "original".into();
        } else {
            let codes = lemma_index_lookup(lex_paths, word, false);
            if !codes.is_empty() {
                lemma_index_used = true;
                match_kind = "lemma".into();
                strongs = codes;
            } else {
                // Original behaviour: containment, longest original first.
                let mut m: Vec<(usize, usize)> = Vec::new();
                for (i, u) in units.iter().enumerate() {
                    if let Some(len) = u
                        .hits
                        .iter()
                        .filter(|h| {
                            h.original.chars().count() >= 2
                                && (h.original.contains(word) || word.contains(&h.original))
                        })
                        .map(|h| h.original.chars().count())
                        .max()
                    {
                        m.push((i, len));
                    }
                }
                m.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                matched = m.into_iter().map(|(i, _)| i).collect();
                if !matched.is_empty() {
                    match_kind = "original".into();
                }
            }
        }
    } else if !normalized.is_empty() {
        let t1: Vec<usize> = (0..units.len())
            .filter(|&i| match_tier(&normalized, &units[i].gloss) == Some(1))
            .collect();
        // ISS-37 divine name (before gloss matching).
        if let Some(codes) = divine_alias(word, &normalized, !t1.is_empty()) {
            for code in codes {
                let m: Vec<usize> = (0..units.len())
                    .filter(|&i| units[i].hits.iter().any(|h| h.strong == *code))
                    .collect();
                if !m.is_empty() {
                    matched = m;
                    match_kind = "alias".into();
                    pick_codes_first = Some(code.to_string());
                    break;
                }
            }
        }
        if matched.is_empty() && !t1.is_empty() {
            matched = t1;
            match_kind = "gloss".into();
        }
        if matched.is_empty() && !opts.typed {
            matched = (0..units.len())
                .filter(|&i| match_tier(&normalized, &units[i].gloss) == Some(2))
                .collect();
            if !matched.is_empty() {
                match_kind = "stem".into();
            } else if normalized.chars().count() >= 2 {
                matched = (0..units.len())
                    .filter(|&i| {
                        units[i].hits.iter().any(|h| {
                            h.translit
                                .split_whitespace()
                                .any(|t| normalize_word(t) == normalized)
                        })
                    })
                    .collect();
                if !matched.is_empty() {
                    match_kind = "translit".into();
                }
            }
        }
        if matched.is_empty() && opts.typed {
            let codes = lemma_index_lookup(lex_paths, word, true);
            if !codes.is_empty() {
                lemma_index_used = true;
                match_kind = "lemma".into();
                strongs = codes;
            }
        }
    }

    if !matched.is_empty() {
        let chosen = match occurrence {
            Some(n) => matched[if n < matched.len() { n } else { 0 }],
            None => matched
                .iter()
                .copied()
                .find(|&i| !units[i].codes().is_empty())
                .unwrap_or(matched[0]),
        };
        let unit = &units[chosen];
        let mut codes = unit.codes();
        if let Some(first) = &pick_codes_first {
            if let Some(pos) = codes.iter().position(|c| c == first) {
                let c = codes.remove(pos);
                codes.insert(0, c);
            }
        }
        if !codes.is_empty() {
            strongs = codes;
            for &i in &matched {
                if i == chosen {
                    continue;
                }
                for c in units[i].codes() {
                    if !strongs.contains(&c) {
                        strongs.push(c);
                    }
                }
            }
        }
        matched_hit = unit
            .hits
            .iter()
            .find(|h| Some(&h.strong) == strongs.first())
            .or_else(|| unit.hits.iter().find(|h| !h.strong.is_empty()))
            .or_else(|| unit.hits.first())
            .cloned();
        for h in &unit.hits {
            push_original(h, &mut hebrew, &mut greek);
        }
        if !strongs.is_empty() {
            for &i in &matched {
                for h in &units[i].hits {
                    push_original(h, &mut hebrew, &mut greek);
                }
            }
        }
        if match_kind == "gloss" && !opts.typed {
            if let Some(tok) = t1_gloss_token(&unit.gloss, &normalized) {
                if !tok.is_empty() {
                    dict_head = tok;
                }
            }
        }
    }
    if lemma_index_used && matched_hit.is_none() {
        if let Some(h) = interlinear_hits.iter().find(|h| Some(&h.strong) == strongs.first()) {
            matched_hit = Some(h.clone());
        }
    }
    // A gloss match with no Strong (supplied word) keeps `gloss` so the UI can say so.
    if strongs.is_empty() && match_kind != "gloss" {
        match_kind.clear();
    }

    // ── Dictionary pass (ISS-07/08/13): all modules, ranked, full matches only ──
    // A Spanish word with no full match in a module gets its Strong bridge headwords there.
    let bridge: Vec<BridgeCand> = if tap_code.is_none() && tap_script.is_empty() && !strongs.is_empty() {
        strong_bridge_candidates(&dict_head, &strongs, lex_paths)
    } else {
        Vec::new()
    };
    let mut dictionary: Vec<LinkedEntry> = Vec::new();
    let mut dict_seen: HashSet<(String, String, u64)> = HashSet::new();
    if !dict_head.trim().is_empty() {
        for path in rank_module_paths(dict_paths, opts.primary_dictionary.as_deref()) {
            if dictionary.len() >= DICTIONARY_CAP {
                break;
            }
            let Ok(entries) = with_slot(&path, |conn, schema| {
                let Some(ts) = schema.dict else {
                    return Ok(vec![]);
                };
                let cands = ranked_candidates(conn, &ts, &dict_head, false);
                let direct = entries_from_cands(conn, &ts, &cands, DICT_PER_MODULE, true)?;
                if direct.is_empty() && !bridge.is_empty() {
                    return bridge_entries(conn, &ts, &bridge, 2);
                }
                Ok(direct)
            }) else {
                continue;
            };
            if entries.is_empty() {
                continue;
            }
            let abbr = module_abbr(&path);
            for e in entries {
                let key = (abbr.clone(), e.topic.clone(), plain_hash(&e.plain));
                if !dict_seen.insert(key) {
                    continue;
                }
                dictionary.push(linked(&path, e));
                if dictionary.len() >= DICTIONARY_CAP {
                    break;
                }
            }
        }
    }

    // ── Lexicon pass (ISS-04/13): exact Strong entries, one per lexicon and code ──
    let mut lexicon: Vec<LinkedEntry> = Vec::new();
    let mut related: Vec<String> = Vec::new();
    let ranked_lex = rank_module_paths(lex_paths, opts.primary_lexicon.as_deref());
    if !strongs.is_empty() {
        let codes: Vec<String> = strongs.iter().take(4).cloned().collect();
        let mut memo: HashMap<(String, String), Option<DictEntry>> = HashMap::new();
        let mut lex_seen: HashSet<(String, String, u64)> = HashSet::new();
        let mut used: HashSet<(String, String)> = HashSet::new();
        let get = |memo: &mut HashMap<(String, String), Option<DictEntry>>, p: &str, c: &str| {
            memo.entry((p.to_string(), c.to_string()))
                .or_insert_with(|| lexicon_lookup(p, c).ok().flatten())
                .clone()
        };
        let mut try_push = |lexicon: &mut Vec<LinkedEntry>, p: &str, c: &str, e: DictEntry| -> bool {
            if used.contains(&(p.to_string(), c.to_string())) {
                return false;
            }
            let key = (module_abbr(p), e.topic.to_uppercase(), plain_hash(&e.plain));
            used.insert((p.to_string(), c.to_string()));
            if !lex_seen.insert(key) {
                return false;
            }
            lexicon.push(linked(p, e));
            true
        };
        // Round 1: every code gets its best-ranked entry.
        for c in &codes {
            for p in &ranked_lex {
                if lexicon.len() >= LEXICON_CAP {
                    break;
                }
                if let Some(e) = get(&mut memo, p, c) {
                    if try_push(&mut lexicon, p, c, e) {
                        break;
                    }
                }
            }
        }
        // Round 2: fill with other lexicons, primary code first.
        'fill: for c in &codes {
            for p in &ranked_lex {
                if lexicon.len() >= LEXICON_CAP {
                    break 'fill;
                }
                if let Some(e) = get(&mut memo, p, c) {
                    try_push(&mut lexicon, p, c, e);
                }
            }
        }
    } else if tap_code.is_none() {
        // Lemma pass: exact topic equality, prose-topic modules skipped, codes → related.
        let mut lemma_queries: Vec<String> = vec![word.to_string()];
        if let Some(h) = &matched_hit {
            if !h.spanish.is_empty() {
                lemma_queries.push(h.spanish.clone());
            }
            if !h.original.is_empty() {
                lemma_queries.push(h.original.clone());
            }
        }
        lemma_queries.extend(hebrew.iter().cloned());
        lemma_queries.extend(greek.iter().cloned());
        let mut seen_q = HashSet::new();
        lemma_queries.retain(|q| !q.trim().is_empty() && seen_q.insert(q.clone()));
        let mut lex_seen: HashSet<(String, String, u64)> = HashSet::new();
        'lex: for p in &ranked_lex {
            if prose_topic_module(p) {
                continue;
            }
            for q in &lemma_queries {
                let Ok(entries) = with_slot(p, |conn, schema| {
                    let Some(ts) = schema.lex else {
                        return Ok(vec![]);
                    };
                    let cands: Vec<Cand> = ranked_candidates(conn, &ts, q, false)
                        .into_iter()
                        .filter(|c| c.tier == Tier::Exact)
                        .collect();
                    entries_from_cands(conn, &ts, &cands, 2, true)
                }) else {
                    continue;
                };
                for e in entries {
                    let key = (module_abbr(p), e.topic.clone(), plain_hash(&e.plain));
                    if !lex_seen.insert(key) {
                        continue;
                    }
                    if let Some(c) = parse_strong_code(&e.topic) {
                        if !related.contains(&c) {
                            related.push(c);
                        }
                    }
                    lexicon.push(linked(p, e));
                    if lexicon.len() >= LEXICON_CAP {
                        break 'lex;
                    }
                }
            }
        }
    }

    WordLinkResult {
        word: word.to_string(),
        normalized,
        strongs,
        hebrew,
        greek,
        dictionary,
        lexicon,
        interlinear_used,
        interlinear_hits,
        matched_hit,
        match_kind,
        related,
        interlinear_fallback,
        lemma_index_used,
        interlinear_verse,
        tagged_twin: tagged_twin_abbr,
    }
}

/// Find a preferred interlinear Bible path from scanned modules. Candidates are scored by
/// name and then validated by parsing Gn 1:1 / Jn 3:16 (ISS-44); unparseable files are skipped.
pub fn find_interlinear_path(modules: &[ModuleInfo]) -> Option<String> {
    let mut scored: Vec<(i32, String)> = Vec::new();
    for m in modules.iter().filter(|m| m.kind == "bible" && !m.encrypted) {
        let name = format!("{} {} {}", m.filename, m.title, m.abbreviation).to_lowercase();
        let mut score = 0;
        if name.contains("interlineal") || name.contains("interlinear") {
            score += 10;
        }
        if name.contains("irv") || name.contains("strong") {
            score += 3;
        }
        if m.has_strongs {
            score += 2;
        }
        if score > 0 {
            scored.push((score, m.path.clone()));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
        .into_iter()
        .map(|(_, p)| p)
        .find(|p| interlinear_validated(p))
}

#[cfg(test)]
mod golden_tests;

#[cfg(test)]
mod qa_tests;

#[cfg(test)]
mod accuracy_tests;

#[cfg(test)]
mod port_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_red_woc() {
        let h = crate::text::clean_esword_text("<red>Hola mundo</red>");
        assert!(h.contains("woc"), "{h}");
        assert!(h.contains("Hola mundo"), "{h}");
    }

    #[test]
    fn cleans_strong_num() {
        let h = crate::text::clean_esword_text("amor <num>G26</num>");
        assert!(h.contains("data-strong=\"G26\""), "{h}");
    }

    #[test]
    fn strong_variants_include_g26() {
        let v = strong_candidates("g26");
        assert!(v.iter().any(|x| x == "G26"), "{v:?}");
    }

    #[test]
    fn lexicon_table_lookup_live() {
        let Some(lib) = golden_tests::library() else {
            return;
        };
        let p = lib.join("01 Diccionario strong.lexi");
        let p = p.to_str().unwrap();
        let e = lexicon_lookup(p, "G26").expect("lookup");
        assert!(e.is_some(), "G26 should exist in Strong lexi");
        let e = e.unwrap();
        assert_eq!(e.topic.to_uppercase(), "G26");
        assert!(!e.plain.is_empty());
        eprintln!("PROBE lexicon_table_lookup_live: {} → {} chars", e.topic, e.plain.len());
    }

    #[test]
    fn interlinear_parses_hebrew_spanish_strong() {
        let raw = r#"<heb>יִרְמְיָהוּ</heb>2 <num>H3414</num>:NPHSMN <blu>Jeremías</blu> <heb>בֶּן</heb>3 <num>H1121</num> <blu>hijo</blu>"#;
        let hits = parse_interlinear_tokens(raw);
        assert!(
            hits.iter().any(|h| words_match(&h.spanish, "Jeremías")
                && h.strong == "H3414"
                && h.original.contains('י')
                && h.script == "hebrew"),
            "{hits:?}"
        );
        assert!(
            hits.iter().any(|h| words_match(&h.spanish, "hijo") && h.strong == "H1121"),
            "{hits:?}"
        );
    }

    #[test]
    fn interlinear_parses_greek_spanish_strong() {
        let raw = r#"<grk>ἠγάπησεν3</grk> êgapêsen <num>G25</num> VAAI3S <blu>amó</blu> ‹ <grk>ὁ4 Θεὸς5</grk> › ho Theos <num>G3588</num> <num>G2316</num> DNSM NNSM <blu>Dios</blu>"#;
        let hits = parse_interlinear_tokens(raw);
        let amo = hits.iter().find(|h| words_match(&h.spanish, "amó"));
        assert!(amo.is_some(), "missing amó: {hits:?}");
        let amo = amo.unwrap();
        assert_eq!(amo.strong, "G25");
        assert!(amo.original.contains('ἀ') || amo.original.contains('η') || amo.original.contains('ἠ'), "{amo:?}");
        assert_eq!(amo.script, "greek");
        assert!(
            amo.translit.to_lowercase().contains("gap") || !amo.translit.is_empty(),
            "translit: {amo:?}"
        );
        // Dios may bind two strongs
        assert!(
            hits.iter().any(|h| words_match(&h.spanish, "Dios") && (h.strong == "G2316" || h.strong == "G3588")),
            "{hits:?}"
        );
    }

    #[test]
    fn interlinear_resolve_spanish_to_strong_live() {
        let Some(lib) = golden_tests::library() else {
            return;
        };
        let p = lib.join("00Interlineal-iRV 1960+.bbli");
        let p = p.to_str().unwrap();
        // Juan 3:16 — «mundo»
        let r = resolve_word_links("mundo", &[], &[], Some(p), 43, 3, 16);
        assert!(r.interlinear_used);
        assert!(
            r.strongs.iter().any(|s| s == "G2889"),
            "expected G2889 for mundo, got {:?}",
            r.strongs
        );
        assert!(
            r.greek.iter().any(|g| g.contains('κ') || g.contains('ό') || g.contains('κ')),
            "expected greek lemma, got {:?}",
            r.greek
        );
        assert!(r.matched_hit.is_some(), "matched_hit missing: {r:?}");
        eprintln!("PROBE interlinear_resolve_spanish_to_strong_live: mundo → {:?}", r.strongs);
    }

    #[test]
    fn preserves_esword_ref_tags() {
        let h = crate::text::clean_esword_text(
            r#"Véase <ref>Gén 1:1-2</ref> y también <ref>Mt. 5:12</ref>."#,
        );
        assert!(h.contains("bible-ref"), "{h}");
        assert!(h.contains("data-book"), "{h}");
        assert!(h.contains("Gén") || h.contains("Génesis") || h.contains("1:1"), "{h}");
    }
}
