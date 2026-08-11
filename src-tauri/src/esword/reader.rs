use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::books::book_by_number;
use super::catalog::table_exists;
use super::text::cell_to_text;

/// Per-module connection slots. Global map lock is only held briefly so different
/// modules can be queried in parallel (probes, parallel Bibles, etc.).
static CONN_CACHE: Lazy<Mutex<HashMap<String, Arc<Mutex<Connection>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Schema / labels memoized per module path (avoids repeated sqlite_master / PRAGMA).
struct SchemaMemo {
    tables: HashMap<String, bool>,
    columns: HashMap<String, HashSet<String>>,
    labels: Option<(String, String)>,
}

static SCHEMA_MEMO: Lazy<Mutex<HashMap<String, SchemaMemo>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn open_ro_conn(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("No se pudo abrir {}: {e}", path.display()))?;
    // Read-only performance knobs (safe for study loads)
    let _ = conn.execute_batch(
        "PRAGMA query_only=ON;\
         PRAGMA temp_store=MEMORY;\
         PRAGMA cache_size=-16000;\
         PRAGMA mmap_size=268435456;\
         PRAGMA synchronous=OFF;",
    );
    Ok(conn)
}

fn with_conn<T>(
    path: &Path,
    f: impl FnOnce(&Connection, &str) -> Result<T, String>,
) -> Result<T, String> {
    let key = path.to_string_lossy().to_string();
    let slot = {
        let mut cache = CONN_CACHE
            .lock()
            .map_err(|_| "cache lock poisoned".to_string())?;
        if let Some(existing) = cache.get(&key) {
            existing.clone()
        } else {
            let conn = open_ro_conn(path)?;
            let arc = Arc::new(Mutex::new(conn));
            cache.insert(key.clone(), arc.clone());
            // Bound cache size so we don't keep hundreds of open FDs forever
            if cache.len() > 48 {
                // Drop an arbitrary older entry (not the one we just inserted)
                let drop_key = cache.keys().find(|k| *k != &key).cloned();
                if let Some(k) = drop_key {
                    cache.remove(&k);
                    if let Ok(mut schema) = SCHEMA_MEMO.lock() {
                        schema.remove(&k);
                    }
                }
            }
            arc
        }
    };
    let conn = slot
        .lock()
        .map_err(|_| "conn lock poisoned".to_string())?;
    f(&conn, &key)
}

fn table_exists_cached(conn: &Connection, path_key: &str, name: &str) -> bool {
    if let Ok(mut memo) = SCHEMA_MEMO.lock() {
        let entry = memo.entry(path_key.to_string()).or_insert_with(|| SchemaMemo {
            tables: HashMap::new(),
            columns: HashMap::new(),
            labels: None,
        });
        if let Some(v) = entry.tables.get(name) {
            return *v;
        }
        let exists = table_exists(conn, name);
        entry.tables.insert(name.to_string(), exists);
        return exists;
    }
    table_exists(conn, name)
}

fn column_set_cached(conn: &Connection, path_key: &str, table: &str) -> HashSet<String> {
    if let Ok(mut memo) = SCHEMA_MEMO.lock() {
        let entry = memo.entry(path_key.to_string()).or_insert_with(|| SchemaMemo {
            tables: HashMap::new(),
            columns: HashMap::new(),
            labels: None,
        });
        if let Some(cols) = entry.columns.get(table) {
            return cols.clone();
        }
        let cols = column_set(conn, table);
        entry.columns.insert(table.to_string(), cols.clone());
        return cols;
    }
    column_set(conn, table)
}

fn module_labels_cached(conn: &Connection, path: &Path) -> (String, String) {
    let key = path.to_string_lossy().to_string();
    if let Ok(mut memo) = SCHEMA_MEMO.lock() {
        let entry = memo.entry(key).or_insert_with(|| SchemaMemo {
            tables: HashMap::new(),
            columns: HashMap::new(),
            labels: None,
        });
        if let Some(l) = &entry.labels {
            return l.clone();
        }
        let labels = module_labels(conn, path);
        entry.labels = Some(labels.clone());
        return labels;
    }
    module_labels(conn, path)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerseRow {
    pub book: String,
    pub book_number: i32,
    pub chapter: i32,
    pub verse: i32,
    pub text: String,
    pub translation: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterResult {
    pub book: String,
    pub book_number: i32,
    pub chapter: i32,
    pub translation: String,
    pub module_title: String,
    pub verses: Vec<VerseRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentaryResult {
    pub reference: String,
    pub source: String,
    pub title: String,
    pub text: String,
    pub level: String,
    pub module: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryResult {
    pub term: String,
    pub source: String,
    pub title: String,
    pub definition: String,
    pub module: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LexiconResult {
    pub term: String,
    pub source: String,
    pub title: String,
    pub definition: String,
    pub module: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentProbe {
    pub path: String,
    pub has_content: bool,
    pub level: String,
    pub note: String,
}

/// One Strong’s link resolved from an interlinear Bible (e.g. iRV 1960+).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrongHit {
    pub strong: String,
    pub spanish: String,
    pub greek: String,
    pub translit: String,
    pub source_module: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveStrongsResult {
    pub word: String,
    pub hits: Vec<StrongHit>,
    pub interlinear_path: String,
    pub interlinear_title: String,
    pub note: String,
}

/// One concordance-style hit for word cross-references.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BibleSearchHit {
    pub book_number: i32,
    pub book: String,
    pub chapter: i32,
    pub verse: i32,
    pub snippet: String,
    pub translation: String,
    pub module: String,
}

const MAX_TEXT: usize = 50_000;

static RE_BLU: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<blu>(.*?)</blu>").unwrap());
static RE_NUM: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)<num>\s*([HG]\s*\d+)\s*</num>").unwrap());
static RE_GRK: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<grk>(.*?)</grk>").unwrap());
static RE_TAGS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<[^>]+>").unwrap());
static RE_NON_WORD: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[^\p{L}\p{M}\p{N}]+").unwrap());

pub fn get_chapter(
    module_path: &str,
    book_number: i32,
    chapter: i32,
) -> Result<ChapterResult, String> {
    let path = PathBuf::from(module_path);
    with_conn(&path, |conn, path_key| {
        if !table_exists_cached(conn, path_key, "Bible") {
            return Err("Este módulo no tiene tabla Bible".into());
        }
        let (title, abbr) = module_labels_cached(conn, &path);
        let book_name = book_by_number(book_number)
            .map(|b| b.name)
            .unwrap_or_else(|| format!("Libro {book_number}"));

        let mut stmt = conn
            .prepare(
                "SELECT Book, Chapter, Verse, Scripture FROM Bible \
                 WHERE Book=?1 AND Chapter=?2 ORDER BY Verse",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(rusqlite::params![book_number, chapter], |row| {
                Ok((
                    row.get::<_, i32>(0)?,
                    row.get::<_, i32>(1)?,
                    row.get::<_, i32>(2)?,
                    row.get::<_, Value>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut verses = Vec::new();
        for r in rows {
            let (b, ch, v, scripture) = r.map_err(|e| e.to_string())?;
            let text = cell_to_text(&scripture).unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            verses.push(VerseRow {
                book: book_name.clone(),
                book_number: b,
                chapter: ch,
                verse: v,
                text,
                translation: abbr.clone(),
            });
        }
        if verses.is_empty() {
            return Err(format!(
                "Sin versículos en {book_name} {chapter} para este módulo"
            ));
        }
        Ok(ChapterResult {
            book: book_name,
            book_number,
            chapter,
            translation: abbr,
            module_title: title,
            verses,
        })
    })
}

/// Comentario filtrado por nivel (como e-Sword):
/// - verse: solo notas que cubren ese versículo (exacto o rango)
/// - chapter: solo notas de capítulo
/// - book: intro/overview del libro
/// Por defecto el front pide solo `verse` para no mezclar intros de libro
/// en versículos sin nota propia (ej. Vida Plena en Jer 26:1).
pub fn get_commentaries(
    module_path: &str,
    book_number: i32,
    chapter: i32,
    verse: i32,
    include_verse: bool,
    include_chapter: bool,
    include_book: bool,
) -> Result<Vec<CommentaryResult>, String> {
    let path = PathBuf::from(module_path);
    // If nothing selected, default to verse-only (safe isolation)
    let include_verse = include_verse || (!include_chapter && !include_book);
    with_conn(&path, |conn, path_key| {
        let (title, abbr) = module_labels_cached(conn, &path);
        let book_name = book_by_number(book_number)
            .map(|b| b.name)
            .unwrap_or_else(|| format!("Libro {book_number}"));
        let module_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut results = Vec::new();

        // 1) Verse-level only when requested
        if include_verse && table_exists_cached(conn, path_key, "VerseCommentary") {
            let cols = column_set_cached(conn, path_key, "VerseCommentary");
            let has_end = cols.contains("ChapterEnd") && cols.contains("VerseEnd");

            let sql = if has_end {
                "SELECT ChapterBegin, VerseBegin, ChapterEnd, VerseEnd, Comments \
                 FROM VerseCommentary \
                 WHERE Book=?1 \
                   AND ChapterBegin <= ?2 AND ChapterEnd >= ?2 \
                   AND NOT (ChapterBegin = ?2 AND VerseBegin > ?3) \
                   AND NOT (ChapterEnd = ?2 AND VerseEnd < ?3) \
                 ORDER BY ChapterBegin, VerseBegin \
                 LIMIT 40"
            } else {
                "SELECT ChapterBegin, VerseBegin, ChapterBegin, VerseBegin, Comments \
                 FROM VerseCommentary \
                 WHERE Book=?1 AND ChapterBegin=?2 AND VerseBegin=?3 \
                 LIMIT 40"
            };

            if let Ok(mut stmt) = conn.prepare(sql) {
                if let Ok(rows) =
                    stmt.query_map(rusqlite::params![book_number, chapter, verse], |row| {
                        Ok((
                            row.get::<_, i32>(0)?,
                            row.get::<_, i32>(1)?,
                            row.get::<_, i32>(2)?,
                            row.get::<_, i32>(3)?,
                            row.get::<_, Value>(4)?,
                        ))
                    })
                {
                    for r in rows.flatten() {
                        let (cb, vb, ce, ve, comments) = r;
                        push_comment(
                            &mut results,
                            comments,
                            &book_name,
                            cb,
                            vb,
                            ce,
                            ve,
                            &abbr,
                            &title,
                            "verse",
                            &module_name,
                        );
                    }
                }
            }

            // Also pull exact verse if range missed (different schemas)
            if let Ok(mut stmt) = conn.prepare(
                "SELECT ChapterBegin, VerseBegin, Comments FROM VerseCommentary \
                 WHERE Book=?1 AND ChapterBegin=?2 AND VerseBegin=?3 LIMIT 20",
            ) {
                if let Ok(rows) =
                    stmt.query_map(rusqlite::params![book_number, chapter, verse], |row| {
                        Ok((
                            row.get::<_, i32>(0)?,
                            row.get::<_, i32>(1)?,
                            row.get::<_, Value>(2)?,
                        ))
                    })
                {
                    for r in rows.flatten() {
                        let (cb, vb, comments) = r;
                        // avoid exact dups by reference+prefix
                        let ref_label = format!("{book_name} {cb}:{vb}");
                        if results.iter().any(|x| x.reference == ref_label && x.level == "verse")
                        {
                            continue;
                        }
                        push_comment(
                            &mut results,
                            comments,
                            &book_name,
                            cb,
                            vb,
                            cb,
                            vb,
                            &abbr,
                            &title,
                            "verse",
                            &module_name,
                        );
                    }
                }
            }
        }

        // 2) Chapter-level only when requested
        if include_chapter && table_exists_cached(conn, path_key, "ChapterCommentary") {
            let cols = column_set_cached(conn, path_key, "ChapterCommentary");
            let chapter_col = if cols.contains("Chapter") {
                "Chapter"
            } else if cols.contains("ChapterBegin") {
                "ChapterBegin"
            } else {
                ""
            };
            if !chapter_col.is_empty() {
                let sql = format!(
                    "SELECT Comments FROM ChapterCommentary WHERE Book=?1 AND [{chapter_col}]=?2 LIMIT 20"
                );
                if let Ok(mut stmt) = conn.prepare(&sql) {
                    if let Ok(rows) =
                        stmt.query_map(rusqlite::params![book_number, chapter], |row| {
                            row.get::<_, Value>(0)
                        })
                    {
                        for r in rows.flatten() {
                            if let Some(text) = usable_text(&r) {
                                results.push(CommentaryResult {
                                    reference: format!("{book_name} {chapter}"),
                                    source: abbr.clone(),
                                    title: title.clone(),
                                    text: truncate(&text, MAX_TEXT),
                                    level: "chapter".into(),
                                    module: module_name.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // 3) Book-level only when requested (not mixed into empty verses)
        if include_book && table_exists_cached(conn, path_key, "BookCommentary") {
            if let Ok(mut stmt) =
                conn.prepare("SELECT Comments FROM BookCommentary WHERE Book=?1 LIMIT 5")
            {
                if let Ok(rows) = stmt.query_map([book_number], |row| row.get::<_, Value>(0)) {
                    for r in rows.flatten() {
                        if let Some(text) = usable_text(&r) {
                            results.push(CommentaryResult {
                                reference: book_name.clone(),
                                source: abbr.clone(),
                                title: title.clone(),
                                text: truncate(&text, MAX_TEXT),
                                level: "book".into(),
                                module: module_name.clone(),
                            });
                        }
                    }
                }
            }
        }

        Ok(results)
    })
}

fn push_comment(
    results: &mut Vec<CommentaryResult>,
    comments: Value,
    book_name: &str,
    cb: i32,
    vb: i32,
    ce: i32,
    ve: i32,
    abbr: &str,
    title: &str,
    level: &str,
    module_name: &str,
) {
    if let Some(text) = usable_text(&comments) {
        let ref_label = if cb == ce && vb == ve {
            format!("{book_name} {cb}:{vb}")
        } else {
            format!("{book_name} {cb}:{vb}–{ce}:{ve}")
        };
        results.push(CommentaryResult {
            reference: ref_label,
            source: abbr.to_string(),
            title: title.to_string(),
            text: truncate(&text, MAX_TEXT),
            level: level.into(),
            module: module_name.to_string(),
        });
    }
}

fn usable_text(val: &Value) -> Option<String> {
    let text = cell_to_text(val)?;
    if text.len() < 3 {
        return None;
    }
    Some(text)
}

/// Fast EXISTS probe — respects the same level filters as the commentary view.
/// Default (verse only): ● only if that verse has a note — not merely a book intro.
pub fn probe_commentary_fast(
    module_path: &str,
    book_number: i32,
    chapter: i32,
    verse: i32,
    include_verse: bool,
    include_chapter: bool,
    include_book: bool,
) -> ContentProbe {
    let path = PathBuf::from(module_path);
    let include_verse = include_verse || (!include_chapter && !include_book);
    let res = with_conn(&path, |conn, path_key| {
        // Verse rows exist?
        if include_verse && table_exists_cached(conn, path_key, "VerseCommentary") {
            let cols = column_set_cached(conn, path_key, "VerseCommentary");
            let has_end = cols.contains("ChapterEnd") && cols.contains("VerseEnd");
            let sql = if has_end {
                "SELECT 1 FROM VerseCommentary WHERE Book=?1 \
                 AND ChapterBegin<=?2 AND ChapterEnd>=?2 \
                 AND NOT (ChapterBegin=?2 AND VerseBegin>?3) \
                 AND NOT (ChapterEnd=?2 AND VerseEnd<?3) LIMIT 1"
            } else {
                "SELECT 1 FROM VerseCommentary WHERE Book=?1 AND ChapterBegin=?2 AND VerseBegin=?3 LIMIT 1"
            };
            if let Ok(mut stmt) = conn.prepare(sql) {
                if stmt
                    .query_row(rusqlite::params![book_number, chapter, verse], |_| Ok(1i32))
                    .is_ok()
                {
                    return Ok(("verse", true));
                }
            }
        }
        if include_chapter && table_exists_cached(conn, path_key, "ChapterCommentary") {
            let cols = column_set_cached(conn, path_key, "ChapterCommentary");
            let col = if cols.contains("Chapter") {
                "Chapter"
            } else if cols.contains("ChapterBegin") {
                "ChapterBegin"
            } else {
                ""
            };
            if !col.is_empty() {
                let sql = format!(
                    "SELECT 1 FROM ChapterCommentary WHERE Book=?1 AND [{col}]=?2 LIMIT 1"
                );
                if let Ok(mut stmt) = conn.prepare(&sql) {
                    if stmt
                        .query_row(rusqlite::params![book_number, chapter], |_| Ok(1i32))
                        .is_ok()
                    {
                        return Ok(("chapter", true));
                    }
                }
            }
        }
        if include_book && table_exists_cached(conn, path_key, "BookCommentary") {
            if let Ok(mut stmt) =
                conn.prepare("SELECT 1 FROM BookCommentary WHERE Book=?1 LIMIT 1")
            {
                if stmt
                    .query_row([book_number], |_| Ok(1i32))
                    .is_ok()
                {
                    return Ok(("book", true));
                }
            }
        }
        Ok(("", false))
    });

    match res {
        Ok((level, has)) => ContentProbe {
            path: module_path.into(),
            has_content: has,
            level: level.into(),
            note: if has { "ok".into() } else { "vacío".into() },
        },
        Err(e) => ContentProbe {
            path: module_path.into(),
            has_content: false,
            level: String::new(),
            note: e,
        },
    }
}

pub fn probe_dictionary_fast(module_path: &str, term: &str) -> ContentProbe {
    let term = term.trim();
    if term.is_empty() {
        return ContentProbe {
            path: module_path.into(),
            has_content: false,
            level: String::new(),
            note: "sin-término".into(),
        };
    }
    let path = PathBuf::from(module_path);
    let res = with_conn(&path, |conn, path_key| {
        if !table_exists_cached(conn, path_key, "Dictionary") {
            return Ok(false);
        }
        // Topic / headword only — same rules as lookup (never Definition body)
        for pat in topic_patterns(term) {
            let sql = if pat.contains('%') {
                "SELECT 1 FROM Dictionary WHERE Topic LIKE ?1 LIMIT 1"
            } else {
                "SELECT 1 FROM Dictionary WHERE Topic = ?1 LIMIT 1"
            };
            if let Ok(mut stmt) = conn.prepare(sql) {
                if stmt.query_row([&pat], |_| Ok(1i32)).is_ok() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    });
    match res {
        Ok(has) => ContentProbe {
            path: module_path.into(),
            has_content: has,
            level: "dict".into(),
            note: if has { "ok".into() } else { "vacío".into() },
        },
        Err(e) => ContentProbe {
            path: module_path.into(),
            has_content: false,
            level: String::new(),
            note: e,
        },
    }
}

pub fn probe_lexicon_fast(module_path: &str, term: &str) -> ContentProbe {
    let term = term.trim();
    if term.is_empty() {
        return ContentProbe {
            path: module_path.into(),
            has_content: false,
            level: String::new(),
            note: "sin-término".into(),
        };
    }
    let path = PathBuf::from(module_path);
    let strong = normalize_strongs(term).unwrap_or_else(|| term.to_uppercase());
    let res = with_conn(&path, |conn, path_key| {
        let table = if table_exists_cached(conn, path_key, "Lexicon") {
            "Lexicon"
        } else if table_exists_cached(conn, path_key, "Dictionary") {
            "Dictionary"
        } else {
            return Ok(false);
        };
        let sql = format!("SELECT 1 FROM [{table}] WHERE Topic = ?1 OR Topic LIKE ?2 LIMIT 1");
        let like = format!("{strong}%");
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if stmt
                .query_row(rusqlite::params![strong, like], |_| Ok(1i32))
                .is_ok()
            {
                return Ok(true);
            }
        }
        Ok(false)
    });
    match res {
        Ok(has) => ContentProbe {
            path: module_path.into(),
            has_content: has,
            level: "lex".into(),
            note: if has { "ok".into() } else { "vacío".into() },
        },
        Err(e) => ContentProbe {
            path: module_path.into(),
            has_content: false,
            level: String::new(),
            note: e,
        },
    }
}

// Keep old names for lib.rs
pub fn probe_commentary(
    module_path: &str,
    book_number: i32,
    chapter: i32,
    verse: i32,
    include_verse: bool,
    include_chapter: bool,
    include_book: bool,
) -> ContentProbe {
    probe_commentary_fast(
        module_path,
        book_number,
        chapter,
        verse,
        include_verse,
        include_chapter,
        include_book,
    )
}

pub fn probe_dictionary(module_path: &str, term: &str) -> ContentProbe {
    probe_dictionary_fast(module_path, term)
}

pub fn probe_lexicon(module_path: &str, term: &str) -> ContentProbe {
    probe_lexicon_fast(module_path, term)
}

/// Concordance-style search of a Bible module for a whole word (case-insensitive).
/// Used by the cross-reference UI (word mode). Skips the optional origin verse.
pub fn search_bible_word(
    module_path: &str,
    term: &str,
    exclude_book: Option<i32>,
    exclude_chapter: Option<i32>,
    exclude_verse: Option<i32>,
    limit: i32,
) -> Result<Vec<BibleSearchHit>, String> {
    let path = PathBuf::from(module_path);
    let term = term.trim();
    if term.is_empty() {
        return Ok(vec![]);
    }
    // Guard against pathological queries (SQL LIKE + regex)
    if term.chars().count() > 64 {
        return Err("Término demasiado largo (máx. 64 caracteres)".into());
    }
    let limit = limit.clamp(1, 120) as usize;
    let term_lower = term.to_lowercase();
    let like = format!("%{term}%");

    with_conn(&path, |conn, path_key| {
        if !table_exists_cached(conn, path_key, "Bible") {
            return Err("Este módulo no tiene tabla Bible".into());
        }
        let (title, abbr) = module_labels_cached(conn, &path);
        let module_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let _ = title;

        // Prefetch candidates with LIKE (text only — blobs can't be searched usefully).
        // Cap overscan so whole-word filtering still returns up to `limit` hits.
        let scan_cap = (limit * 8).clamp(40, 800) as i32;
        let mut stmt = conn
            .prepare(
                "SELECT Book, Chapter, Verse, Scripture FROM Bible \
                 WHERE typeof(Scripture)='text' AND Scripture LIKE ?1 \
                 ORDER BY Book, Chapter, Verse \
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(rusqlite::params![like, scan_cap], |row| {
                Ok((
                    row.get::<_, i32>(0)?,
                    row.get::<_, i32>(1)?,
                    row.get::<_, i32>(2)?,
                    row.get::<_, Value>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut out = Vec::with_capacity(limit);
        for r in rows {
            let (book_n, ch, v, scripture) = r.map_err(|e| e.to_string())?;
            if let (Some(eb), Some(ec), Some(ev)) =
                (exclude_book, exclude_chapter, exclude_verse)
            {
                if book_n == eb && ch == ec && v == ev {
                    continue;
                }
            }
            let text = cell_to_text(&scripture).unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            if !text_contains_whole_word(&text, &term_lower) {
                continue;
            }
            let book_name = book_by_number(book_n)
                .map(|b| b.name)
                .unwrap_or_else(|| format!("Libro {book_n}"));
            let snippet = make_word_snippet(&text, &term_lower, 140);
            out.push(BibleSearchHit {
                book_number: book_n,
                book: book_name,
                chapter: ch,
                verse: v,
                snippet,
                translation: abbr.clone(),
                module: module_name.clone(),
            });
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    })
}

fn text_contains_whole_word(text: &str, term_lower: &str) -> bool {
    let hay: String = text
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let needle: String = term_lower
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    if needle.is_empty() {
        return false;
    }
    let hay_chars: Vec<char> = hay.chars().collect();
    let needle_chars: Vec<char> = needle.chars().collect();
    let nlen = needle_chars.len();
    if nlen > hay_chars.len() {
        return false;
    }
    'outer: for i in 0..=(hay_chars.len() - nlen) {
        for (j, nc) in needle_chars.iter().enumerate() {
            if hay_chars[i + j] != *nc {
                continue 'outer;
            }
        }
        let before_ok = i == 0 || !is_word_char(hay_chars[i - 1]);
        let after_ok = i + nlen >= hay_chars.len() || !is_word_char(hay_chars[i + nlen]);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == 'ʼ' || c == '’'
}

fn make_word_snippet(text: &str, term_lower: &str, max_chars: usize) -> String {
    let plain: String = text.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    let lower: String = plain
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let needle: String = term_lower
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let plain_chars: Vec<char> = plain.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    let needle_chars: Vec<char> = needle.chars().collect();
    let nlen = needle_chars.len();
    let mut char_idx = None;
    if nlen > 0 && nlen <= lower_chars.len() {
        'outer: for i in 0..=(lower_chars.len() - nlen) {
            for (j, nc) in needle_chars.iter().enumerate() {
                if lower_chars[i + j] != *nc {
                    continue 'outer;
                }
            }
            char_idx = Some(i);
            break;
        }
    }
    let Some(idx) = char_idx else {
        let mut s: String = plain_chars.iter().take(max_chars).collect();
        if plain_chars.len() > max_chars {
            s.push('…');
        }
        return s;
    };
    let half = max_chars.saturating_sub(nlen) / 2;
    let start = idx.saturating_sub(half);
    let end = (idx + nlen + half).min(plain_chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(plain_chars[start..end].iter());
    if end < plain_chars.len() {
        out.push('…');
    }
    out
}

pub fn search_commentaries(
    module_path: &str,
    term: &str,
    limit: i32,
) -> Result<Vec<CommentaryResult>, String> {
    let path = PathBuf::from(module_path);
    let term = term.trim();
    if term.is_empty() {
        return Ok(vec![]);
    }
    with_conn(&path, |conn, path_key| {
        let (title, abbr) = module_labels_cached(conn, &path);
        let module_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let like = format!("%{term}%");
        let mut out = Vec::new();

        if table_exists_cached(conn, path_key, "VerseCommentary") {
            // Only text columns — blobs can't be LIKE-searched meaningfully
            if let Ok(mut stmt) = conn.prepare(
                "SELECT Book, ChapterBegin, VerseBegin, Comments FROM VerseCommentary \
                 WHERE typeof(Comments)='text' AND Comments LIKE ?1 LIMIT ?2",
            ) {
                if let Ok(rows) = stmt.query_map(rusqlite::params![like, limit], |row| {
                    Ok((
                        row.get::<_, i32>(0)?,
                        row.get::<_, i32>(1)?,
                        row.get::<_, i32>(2)?,
                        row.get::<_, Value>(3)?,
                    ))
                }) {
                    for r in rows.flatten() {
                        let (b, ch, v, comments) = r;
                        if let Some(text) = usable_text_filter(&comments) {
                            let book_name = book_by_number(b)
                                .map(|x| x.name)
                                .unwrap_or_else(|| format!("Libro {b}"));
                            out.push(CommentaryResult {
                                reference: format!("{book_name} {ch}:{v}"),
                                source: abbr.clone(),
                                title: title.clone(),
                                text: truncate(&text, MAX_TEXT),
                                level: "verse".into(),
                                module: module_name.clone(),
                            });
                        }
                    }
                }
            }
        }
        Ok(out)
    })
}

/// Dictionary lookup like e-Sword entry browse / a real dictionary:
/// match **headwords (Topic)** only — never scan definition bodies.
/// Order: exact title → prefix (starts with) → optional whole-word-in-title for longer queries.
pub fn lookup_dictionary(
    module_path: &str,
    term: &str,
    limit: i32,
) -> Result<Vec<DictionaryResult>, String> {
    let path = PathBuf::from(module_path);
    let term = term.trim();
    if term.is_empty() {
        return Ok(vec![]);
    }
    let limit = limit.clamp(1, 80);
    with_conn(&path, |conn, path_key| {
        if !table_exists_cached(conn, path_key, "Dictionary") {
            return Err("Este módulo no tiene tabla Dictionary".into());
        }
        let (title, abbr) = module_labels_cached(conn, &path);
        let module_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut ranked: Vec<(i32, DictionaryResult)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let term_norm = strip_accents(&term.to_uppercase());

        // Phase A: exact Topic (several case variants — e-Sword topics are often UPPERCASE)
        for pat in dict_exact_patterns(term) {
            push_dict_rows(
                conn,
                &pat,
                false,
                limit,
                &mut ranked,
                &mut seen,
                &abbr,
                &title,
                &module_name,
                100, // exact score
                &term_norm,
            );
        }

        // Phase B: prefix — "DIOS%" finds "DIOS", "DIOS DE ISRAEL", not "AMOR DE DIOS"
        if (ranked.len() as i32) < limit {
            for pat in dict_prefix_patterns(term) {
                push_dict_rows(
                    conn,
                    &pat,
                    true,
                    limit,
                    &mut ranked,
                    &mut seen,
                    &abbr,
                    &title,
                    &module_name,
                    80,
                    &term_norm,
                );
            }
        }

        // Phase C (optional, longer terms only): Topic contains as a whole token,
        // e.g. "DE DIOS" inside "HIJO DE DIOS". Still Topic-only — never Definition.
        // Skipped for very short queries to avoid flooding ("de", "el", "luz" noise).
        if (ranked.len() as i32) < limit && term_norm.chars().count() >= 4 {
            for pat in dict_contains_patterns(term) {
                push_dict_rows(
                    conn,
                    &pat,
                    true,
                    limit,
                    &mut ranked,
                    &mut seen,
                    &abbr,
                    &title,
                    &module_name,
                    40,
                    &term_norm,
                );
            }
        }

        // Best score first, then shorter titles, then A–Z
        ranked.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.term.len().cmp(&b.1.term.len()))
                .then_with(|| a.1.term.to_lowercase().cmp(&b.1.term.to_lowercase()))
        });

        Ok(ranked
            .into_iter()
            .take(limit as usize)
            .map(|(_, r)| r)
            .collect())
    })
}

fn dict_exact_patterns(term: &str) -> Vec<String> {
    let t = term.trim();
    let up = t.to_uppercase();
    let low = t.to_lowercase();
    let mut v = vec![t.to_string(), up.clone(), low];
    let na = strip_accents(&up);
    if na != up {
        v.push(na);
    }
    // Title-case-ish: Dios
    if !t.is_empty() {
        let mut chars = t.chars();
        if let Some(f) = chars.next() {
            let titled: String = f.to_uppercase().chain(chars.flat_map(|c| c.to_lowercase())).collect();
            v.push(titled);
        }
    }
    v.dedup();
    v
}

fn dict_prefix_patterns(term: &str) -> Vec<String> {
    let t = term.trim();
    let up = t.to_uppercase();
    let mut v = vec![format!("{t}%"), format!("{up}%")];
    let na = strip_accents(&up);
    if na != up {
        v.push(format!("{na}%"));
    }
    v.dedup();
    v
}

fn dict_contains_patterns(term: &str) -> Vec<String> {
    let t = term.trim();
    let up = t.to_uppercase();
    // Leading/trailing % only for longer headword discovery inside multi-word titles
    let mut v = vec![format!("%{up}%"), format!("%{t}%")];
    let na = strip_accents(&up);
    if na != up {
        v.push(format!("%{na}%"));
    }
    v.dedup();
    v
}

fn push_dict_rows(
    conn: &Connection,
    pat: &str,
    is_like: bool,
    limit: i32,
    ranked: &mut Vec<(i32, DictionaryResult)>,
    seen: &mut HashSet<String>,
    abbr: &str,
    title: &str,
    module_name: &str,
    base_score: i32,
    term_norm: &str,
) {
    if ranked.len() as i32 >= limit * 2 {
        // gather a bit extra for ranking, then trim
        return;
    }
    let sql = if is_like {
        "SELECT Topic, Definition FROM Dictionary WHERE Topic LIKE ?1 LIMIT ?2"
    } else {
        "SELECT Topic, Definition FROM Dictionary WHERE Topic = ?1 LIMIT ?2"
    };
    let fetch_n = (limit * 3).clamp(10, 120);
    let Ok(mut stmt) = conn.prepare(sql) else {
        return;
    };
    let Ok(rows) = stmt.query_map(rusqlite::params![pat, fetch_n], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Value>(1)?))
    }) else {
        return;
    };
    for r in rows.flatten() {
        let (topic, def) = r;
        if !seen.insert(topic.clone()) {
            continue;
        }
        // For contains-phase noise: require the normalized term as a token of the title
        if base_score <= 40 {
            let topic_norm = strip_accents(&topic.to_uppercase());
            if !topic_has_token(&topic_norm, term_norm) {
                continue;
            }
        }
        let Some(definition) = usable_text_filter(&def) else {
            continue;
        };
        let score = score_dict_topic(&topic, term_norm, base_score);
        ranked.push((
            score,
            DictionaryResult {
                term: topic,
                source: abbr.to_string(),
                title: title.to_string(),
                definition: truncate(&definition, MAX_TEXT),
                module: module_name.to_string(),
            },
        ));
    }
}

fn topic_has_token(topic_norm: &str, term_norm: &str) -> bool {
    if topic_norm == term_norm {
        return true;
    }
    if topic_norm.starts_with(term_norm)
        && topic_norm
            .chars()
            .nth(term_norm.chars().count())
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true)
    {
        return true;
    }
    // Split on non-alphanumerics
    topic_norm
        .split(|c: char| !c.is_alphanumeric())
        .any(|tok| tok == term_norm)
}

fn score_dict_topic(topic: &str, term_norm: &str, base: i32) -> i32 {
    let tn = strip_accents(&topic.to_uppercase());
    if tn == term_norm {
        return base + 50;
    }
    if tn.starts_with(term_norm) {
        // closer length = better
        let extra = 20 - (tn.len().saturating_sub(term_norm.len()).min(20) as i32);
        return base + extra;
    }
    if topic_has_token(&tn, term_norm) {
        return base;
    }
    base - 10
}

pub fn list_dictionary_topics(
    module_path: &str,
    term: &str,
    limit: i32,
) -> Result<Vec<DictionaryResult>, String> {
    lookup_dictionary(module_path, term, limit)
}

/// Léxico: Strong’s exacto primero (rápido). Sin escaneo pesado de definiciones.
pub fn lookup_lexicon(
    module_path: &str,
    term: &str,
    limit: i32,
) -> Result<Vec<LexiconResult>, String> {
    let path = PathBuf::from(module_path);
    let term = term.trim();
    if term.is_empty() {
        return Ok(vec![]);
    }
    with_conn(&path, |conn, path_key| {
        let table = if table_exists_cached(conn, path_key, "Lexicon") {
            "Lexicon"
        } else if table_exists_cached(conn, path_key, "Dictionary") {
            "Dictionary"
        } else {
            return Err("Este módulo no tiene tabla Lexicon/Dictionary".into());
        };
        let (title, abbr) = module_labels_cached(conn, &path);
        let module_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Build ordered candidates for Topic
        let mut keys: Vec<String> = Vec::new();
        if let Some(s) = normalize_strongs(term) {
            keys.push(s.clone());
            // Also with leading zeros variants H01254
            if let Some(rest) = s.get(1..) {
                if let Ok(n) = rest.parse::<u32>() {
                    keys.push(format!("{}{n}", &s[..1]));
                    keys.push(format!("{}{:04}", &s[..1], n));
                    keys.push(format!("{}{:05}", &s[..1], n));
                }
            }
        }
        keys.push(term.to_string());
        keys.push(term.to_uppercase());
        keys.push(format!("{}%", term.to_uppercase()));
        if let Some(s) = normalize_strongs(term) {
            keys.push(format!("{s}%"));
        }

        for key in &keys {
            if out.len() as i32 >= limit {
                break;
            }
            let sql = if key.contains('%') {
                format!("SELECT Topic, Definition FROM [{table}] WHERE Topic LIKE ?1 LIMIT ?2")
            } else {
                format!("SELECT Topic, Definition FROM [{table}] WHERE Topic = ?1 LIMIT ?2")
            };
            if let Ok(mut stmt) = conn.prepare(&sql) {
                if let Ok(rows) = stmt.query_map(rusqlite::params![key, limit], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Value>(1)?))
                }) {
                    for r in rows.flatten() {
                        let (topic, def) = r;
                        if !seen.insert(topic.clone()) {
                            continue;
                        }
                        if let Some(definition) = usable_text_filter(&def) {
                            out.push(LexiconResult {
                                term: topic,
                                source: abbr.clone(),
                                title: title.clone(),
                                definition: truncate(&definition, MAX_TEXT),
                                module: module_name.clone(),
                            });
                        }
                    }
                }
            }
            // Exact Strong hit is enough
            if !out.is_empty() && normalize_strongs(term).is_some() {
                break;
            }
        }

        // Hebrew / Greek lemma: e-Sword often stores script as HTML entities
        // (e.g. &#x05D1;&#x05E8;&#x05D0; for ברא), not plain Unicode.
        if out.is_empty() && contains_hebrew_or_greek(term) {
            let pats = script_search_patterns(term);
            let sql = format!(
                "SELECT Topic, Definition FROM [{table}] \
                 WHERE typeof(Definition)='text' AND Definition LIKE ?1 LIMIT 12"
            );
            for pat in &pats {
                if out.len() as i32 >= limit {
                    break;
                }
                if let Ok(mut stmt) = conn.prepare(&sql) {
                    if let Ok(rows) = stmt.query_map(rusqlite::params![pat], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, Value>(1)?))
                    }) {
                        for r in rows.flatten() {
                            let (topic, def) = r;
                            if !seen.insert(topic.clone()) {
                                continue;
                            }
                            if let Some(definition) = usable_text_filter(&def) {
                                // Prefer topics that look like Strong's numbers
                                out.push(LexiconResult {
                                    term: topic,
                                    source: abbr.clone(),
                                    title: title.clone(),
                                    definition: truncate(&definition, MAX_TEXT),
                                    module: module_name.clone(),
                                });
                            }
                            if out.len() as i32 >= limit {
                                break;
                            }
                        }
                    }
                }
            }
            // Rank Strong's topics first (H#### / G####)
            out.sort_by(|a, b| {
                let sa = a.term.starts_with('H') || a.term.starts_with('G');
                let sb = b.term.starts_with('H') || b.term.starts_with('G');
                sb.cmp(&sa).then_with(|| a.term.cmp(&b.term))
            });
        }

        Ok(out)
    })
}

fn contains_hebrew_or_greek(s: &str) -> bool {
    s.chars().any(is_hebrew_or_greek_char)
}

fn is_hebrew_or_greek_char(c: char) -> bool {
    let u = c as u32;
    (0x0590..=0x05FF).contains(&u)
        || (0xFB1D..=0xFB4F).contains(&u)
        || (0x0370..=0x03FF).contains(&u)
        || (0x1F00..=0x1FFF).contains(&u)
}

/// Strip Hebrew/Greek combining marks (niqqud, accents) → consonants/base letters only.
fn strip_script_marks(s: &str) -> String {
    s.chars()
        .filter(|c| {
            let u = *c as u32;
            // Keep Hebrew letters + final forms; drop points/marks
            if (0x05D0..=0x05EA).contains(&u) || (0x05F0..=0x05F4).contains(&u) {
                return true;
            }
            // Greek base letters
            if (0x0370..=0x03FF).contains(&u) || (0x1F00..=0x1FFF).contains(&u) {
                // drop combining-ish by filtering known mark ranges later
                return !is_combining_mark(*c);
            }
            if (0xFB1D..=0xFB4F).contains(&u) {
                return true; // presentation forms — keep
            }
            false
        })
        .collect()
}

fn is_combining_mark(c: char) -> bool {
    let u = c as u32;
    (0x0591..=0x05C7).contains(&u) // Hebrew points
        || (0x0300..=0x036F).contains(&u)
}

/// Patterns so SQLite can find e-Sword HTML-entity Hebrew/Greek.
fn script_search_patterns(term: &str) -> Vec<String> {
    let raw = term.trim();
    let bare = strip_script_marks(raw);
    let mut pats = Vec::new();

    if !raw.is_empty() {
        pats.push(format!("%{raw}%"));
    }
    if !bare.is_empty() && bare != raw {
        pats.push(format!("%{bare}%"));
    }

    // HTML hex entities as stored in Chávez / Strong modules: &#x05D1;&#x05E8;&#x05D0;
    let letters: Vec<char> = bare
        .chars()
        .filter(|c| {
            let u = *c as u32;
            (0x05D0..=0x05EA).contains(&u)
                || (0x0370..=0x03FF).contains(&u)
                || (0x1F00..=0x1FFF).contains(&u)
        })
        .collect();

    if !letters.is_empty() {
        let hex_upper: String = letters
            .iter()
            .map(|c| format!("&#x{:04X};", *c as u32))
            .collect();
        let hex_lower: String = letters
            .iter()
            .map(|c| format!("&#x{:04x};", *c as u32))
            .collect();
        let dec: String = letters
            .iter()
            .map(|c| format!("&#{};", *c as u32))
            .collect();
        pats.push(format!("%{hex_upper}%"));
        pats.push(format!("%{hex_lower}%"));
        pats.push(format!("%{dec}%"));

        // Pointed forms: letter + optional marks between (SQLite LIKE)
        // e.g. ב%ר%א matches בָּרָא
        if letters.len() >= 2 && letters.len() <= 12 {
            let mut loose = String::from("%");
            for (i, c) in letters.iter().enumerate() {
                loose.push(*c);
                if i + 1 < letters.len() {
                    loose.push('%');
                }
            }
            loose.push('%');
            pats.push(loose);
        }
    }

    pats.dedup();
    pats
}

fn usable_text_filter(val: &Value) -> Option<String> {
    let text = cell_to_text(val)?;
    if text.starts_with("[Módulo cifrado") || text.starts_with("[Contenido binario") {
        // Still return so UI can explain — but mark
        return Some(text);
    }
    if text.len() < 2 {
        return None;
    }
    Some(text)
}

/// Patterns for probes / legacy callers — **Topic headwords only** (no definition body).
fn topic_patterns(term: &str) -> Vec<String> {
    let mut v = dict_exact_patterns(term);
    v.extend(dict_prefix_patterns(term));
    // Only allow contains for longer terms (probe ● for multi-word titles)
    if strip_accents(&term.to_uppercase()).chars().count() >= 4 {
        v.extend(dict_contains_patterns(term));
    }
    v.dedup();
    v
}

fn strip_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'Á' | 'À' | 'Ä' | 'Â' => 'A',
            'É' | 'È' | 'Ë' | 'Ê' => 'E',
            'Í' | 'Ì' | 'Ï' | 'Î' => 'I',
            'Ó' | 'Ò' | 'Ö' | 'Ô' => 'O',
            'Ú' | 'Ù' | 'Ü' | 'Û' => 'U',
            'Ñ' => 'N',
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

fn normalize_strongs(term: &str) -> Option<String> {
    let t = term.trim().to_uppercase().replace(' ', "");
    if t.starts_with('H') || t.starts_with('G') {
        let prefix = t.chars().next()?;
        let rest: String = t.chars().skip(1).collect();
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            let num = rest.trim_start_matches('0');
            let num = if num.is_empty() { "0" } else { num };
            return Some(format!("{prefix}{num}"));
        }
    }
    if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
        let num = t.trim_start_matches('0');
        let num = if num.is_empty() { "0" } else { num };
        return Some(format!("H{num}"));
    }
    None
}

fn column_set(conn: &Connection, table: &str) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info([{table}])")) {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) {
            for r in rows.flatten() {
                set.insert(r);
            }
        }
    }
    set
}

fn module_labels(conn: &Connection, path: &Path) -> (String, String) {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "módulo".into());

    for sql in [
        "SELECT Title, Abbreviation FROM Details LIMIT 1",
        "SELECT Description, Abbreviation FROM Details LIMIT 1",
    ] {
        if let Ok((t, a)) = conn.query_row(sql, [], |r| {
            Ok((
                r.get::<_, String>(0).unwrap_or_default(),
                r.get::<_, String>(1).unwrap_or_default(),
            ))
        }) {
            let title = if t.is_empty() { stem.clone() } else { t };
            let abbr = if a.is_empty() { stem } else { a };
            return (title, abbr);
        }
    }
    (stem.clone(), stem)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    }
}

/// Raw cell text preserving e-Sword markup tags (`<num>`, `<blu>`, `<grk>`).
fn cell_to_markup(val: &Value) -> Option<String> {
    match val {
        Value::Text(s) => {
            if s.trim().is_empty() {
                None
            } else {
                Some(s.clone())
            }
        }
        Value::Blob(b) => {
            if let Ok(s) = std::str::from_utf8(b) {
                if s.contains('<') {
                    return Some(s.to_string());
                }
            }
            // Fall back to decoder (may strip some tags if cleaned)
            super::text::decode_blob(b).filter(|s| s.contains('<') || s.contains("G") || s.contains("H"))
        }
        _ => None,
    }
}

fn normalize_lemma(s: &str) -> String {
    let stripped = RE_TAGS.replace_all(s, "");
    let decoded = html_escape::decode_html_entities(&stripped);
    let lower = decoded.to_lowercase();
    // strip combining marks (accents)
    let no_marks: String = lower
        .chars()
        .filter(|c| {
            let u = *c as u32;
            !(0x0300..=0x036F).contains(&u) && !(0x1AB0..=0x1AFF).contains(&u)
        })
        .collect();
    // rough Spanish accent fold
    no_marks
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            other => other,
        })
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn clean_inline(s: &str) -> String {
    let t = RE_TAGS.replace_all(s, "");
    let t = html_escape::decode_html_entities(&t);
    let t = RE_NON_WORD.replace_all(&t, " ");
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_greek_token(s: &str) -> String {
    // Interlinear often appends word order digits: χαίρετε1
    let t = clean_inline(s);
    t.chars()
        .filter(|c| !c.is_ascii_digit())
        .collect::<String>()
        .trim()
        .to_string()
}

fn normalize_strong_code(raw: &str) -> String {
    let t = raw.to_uppercase().replace(' ', "");
    if t.starts_with('H') || t.starts_with('G') {
        let prefix = t.chars().next().unwrap_or('G');
        let digits: String = t.chars().skip(1).filter(|c| c.is_ascii_digit()).collect();
        let num = digits.trim_start_matches('0');
        let num = if num.is_empty() { "0" } else { num };
        return format!("{prefix}{num}");
    }
    t
}

#[derive(Debug, Clone)]
struct InterlinearToken {
    spanish: String,
    strongs: Vec<String>,
    greek: String,
    translit: String,
}

/// Parse e-Sword interlinear markup into Spanish ↔ Strong’s tokens.
/// Format example:
/// `<grk>χαίρετε1</grk> chairete <num>G5463</num> VPAM2P <blu>Gozaos</blu>`
fn parse_interlinear_tokens(raw: &str) -> Vec<InterlinearToken> {
    let mut out = Vec::new();
    let mut last_end = 0usize;
    for cap in RE_BLU.captures_iter(raw) {
        let full = match cap.get(0) {
            Some(m) => m,
            None => continue,
        };
        let spanish_raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let spanish = clean_inline(spanish_raw);
        let before = &raw[last_end..full.start()];
        last_end = full.end();

        if spanish.is_empty() || spanish == "•" || spanish == "·" || spanish == "→" {
            continue;
        }

        let mut strongs: Vec<String> = RE_NUM
            .captures_iter(before)
            .filter_map(|c| c.get(1).map(|m| normalize_strong_code(m.as_str())))
            .filter(|s| s.len() >= 2)
            .collect();
        strongs.dedup();

        let greek = RE_GRK
            .captures_iter(before)
            .last()
            .and_then(|c| c.get(1).map(|m| clean_greek_token(m.as_str())))
            .unwrap_or_default();

        // Transliteration: plain Latin token just before first <num> in this segment
        let translit = {
            let cut = RE_NUM.find(before).map(|m| m.start()).unwrap_or(before.len());
            let chunk = &before[..cut];
            // strip last </grk> ... take trailing latin word
            let after_grk = chunk.rfind("</grk>").map(|i| &chunk[i + 6..]).unwrap_or(chunk);
            let cleaned = RE_TAGS.replace_all(after_grk, " ");
            cleaned
                .split_whitespace()
                .rev()
                .find(|w| w.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == 'ô' || c == 'ê' || c == 'â'))
                .unwrap_or("")
                .to_string()
        };

        if strongs.is_empty() {
            continue;
        }

        out.push(InterlinearToken {
            spanish,
            strongs,
            greek,
            translit,
        });
    }
    out
}

fn score_token_match(token_norm: &str, word_norm: &str) -> i32 {
    if token_norm.is_empty() || word_norm.is_empty() {
        return 0;
    }
    if token_norm == word_norm {
        return 100;
    }
    if token_norm.starts_with(word_norm) || word_norm.starts_with(token_norm) {
        return 80;
    }
    // common Spanish endings: gozaos / gozar
    let stem = |s: &str| -> String {
        let mut t = s.to_string();
        for end in ["aos", "áos", "aros", "aros", "eis", "éis", "ando", "iendo", "ados", "idas", "es", "os", "as", "ar", "er", "ir"] {
            let e = normalize_lemma(end);
            if t.len() > e.len() + 2 && t.ends_with(&e) {
                t.truncate(t.len() - e.len());
                break;
            }
        }
        t
    };
    let a = stem(token_norm);
    let b = stem(word_norm);
    if a.len() >= 3 && a == b {
        return 70;
    }
    if token_norm.contains(word_norm) || word_norm.contains(token_norm) {
        return 50;
    }
    0
}

/// Resolve a Spanish (or Greek) word in a verse to Strong’s numbers via interlinear Bibles.
pub fn resolve_word_strongs(
    book_number: i32,
    chapter: i32,
    verse: i32,
    word: &str,
    word_index: i32,
    interlinear_paths: &[String],
) -> Result<ResolveStrongsResult, String> {
    let word = word.trim();
    if word.is_empty() {
        return Err("No hay palabra seleccionada".into());
    }
    // Already a Strong’s code
    if let Some(s) = normalize_strongs(word) {
        return Ok(ResolveStrongsResult {
            word: word.to_string(),
            hits: vec![StrongHit {
                strong: s,
                spanish: word.to_string(),
                greek: String::new(),
                translit: String::new(),
                source_module: "direct".into(),
            }],
            interlinear_path: String::new(),
            interlinear_title: String::new(),
            note: "código Strong’s directo".into(),
        });
    }

    if interlinear_paths.is_empty() {
        return Ok(ResolveStrongsResult {
            word: word.to_string(),
            hits: vec![],
            interlinear_path: String::new(),
            interlinear_title: String::new(),
            note: "No hay Biblia interlineal en e-Sword (p. ej. iRV 1960+). Instálala para mapear español → griego/hebreo.".into(),
        });
    }

    // Prefer paths that look like Spanish interlinear RV
    let mut paths: Vec<&String> = interlinear_paths.iter().collect();
    paths.sort_by_key(|p| {
        let n = p.to_lowercase();
        let mut score = 0i32;
        if n.contains("irv") || n.contains("1960") {
            score -= 20;
        }
        if n.contains("interlineal") || n.contains("interlinear") {
            score -= 10;
        }
        if n.contains("griego") || n.contains("hebrew") || n.contains("hebreo") {
            score -= 5;
        }
        score
    });

    let word_norm = normalize_lemma(word);
    let mut last_note = String::new();

    for path_str in paths {
        let path = PathBuf::from(path_str);
        let attempt = with_conn(&path, |conn, path_key| {
            if !table_exists_cached(conn, path_key, "Bible") {
                return Err("sin tabla Bible".into());
            }
            let (title, _abbr) = module_labels_cached(conn, &path);
            let mut stmt = conn
                .prepare(
                    "SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3 LIMIT 1",
                )
                .map_err(|e| e.to_string())?;
            let val: Value = stmt
                .query_row(rusqlite::params![book_number, chapter, verse], |row| {
                    row.get(0)
                })
                .map_err(|_| format!("Sin versículo en interlineal ({title})"))?;
            let raw = cell_to_markup(&val)
                .ok_or_else(|| format!("Texto vacío en interlineal ({title})"))?;
            if !raw.to_ascii_lowercase().contains("<num>")
                && !raw.to_ascii_lowercase().contains("<blu>")
            {
                return Err(format!("No parece interlineal con Strong’s ({title})"));
            }
            let tokens = parse_interlinear_tokens(&raw);
            if tokens.is_empty() {
                return Err(format!("No se pudieron parsear tokens ({title})"));
            }

            // Score all tokens
            let mut ranked: Vec<(i32, usize)> = tokens
                .iter()
                .enumerate()
                .map(|(i, t)| (score_token_match(&normalize_lemma(&t.spanish), &word_norm), i))
                .filter(|(s, _)| *s > 0)
                .collect();
            ranked.sort_by(|a, b| b.0.cmp(&a.0));

            let mut chosen: Vec<&InterlinearToken> = Vec::new();
            if !ranked.is_empty() {
                let best = ranked[0].0;
                for (s, i) in &ranked {
                    if *s == best {
                        chosen.push(&tokens[*i]);
                    }
                }
                // If several equal matches and word_index is set, pick among them
                if chosen.len() > 1 && word_index >= 0 {
                    let idx = word_index as usize;
                    if idx < chosen.len() {
                        chosen = vec![chosen[idx]];
                    }
                }
            } else if word_index >= 0 && (word_index as usize) < tokens.len() {
                // Positional fallback: N-th Spanish gloss in interlinear
                chosen.push(&tokens[word_index as usize]);
            }

            if chosen.is_empty() {
                return Err(format!(
                    "«{word}» no aparece en el interlineal de este versículo ({title})"
                ));
            }

            let mut hits = Vec::new();
            let mut seen = HashSet::new();
            for tok in chosen {
                for s in &tok.strongs {
                    if !seen.insert(s.clone()) {
                        continue;
                    }
                    hits.push(StrongHit {
                        strong: s.clone(),
                        spanish: tok.spanish.clone(),
                        greek: tok.greek.clone(),
                        translit: tok.translit.clone(),
                        source_module: title.clone(),
                    });
                }
            }

            Ok(ResolveStrongsResult {
                word: word.to_string(),
                hits,
                interlinear_path: path_str.clone(),
                interlinear_title: title,
                note: "resuelto vía interlineal".into(),
            })
        });

        match attempt {
            Ok(res) if !res.hits.is_empty() => return Ok(res),
            Ok(res) => last_note = res.note,
            Err(e) => last_note = e,
        }
    }

    Ok(ResolveStrongsResult {
        word: word.to_string(),
        hits: vec![],
        interlinear_path: String::new(),
        interlinear_title: String::new(),
        note: if last_note.is_empty() {
            format!("No se encontró Strong’s para «{word}» en este versículo")
        } else {
            last_note
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esword::catalog::scan_modules;
    use std::time::Instant;

    #[test]
    fn search_bible_word_finds_whole_word_hits() {
        let modules = match scan_modules(None) {
            Ok(m) => m,
            Err(_) => return,
        };
        let bible = modules.iter().find(|m| {
            m.module_type == "bible"
                && !m.encrypted
                && (m.filename.to_lowercase().contains("reina")
                    || m.filename.to_lowercase().contains("rv1960")
                    || m.title.to_lowercase().contains("reina"))
        }).or_else(|| modules.iter().find(|m| m.module_type == "bible" && !m.encrypted));
        let Some(bible) = bible else {
            eprintln!("skip: no bible for word search");
            return;
        };
        let hits = match search_bible_word(&bible.path, "Dios", Some(1), Some(1), Some(1), 20) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("skip search: {e}");
                return;
            }
        };
        // Genesis 1 alone has many "Dios"; exclude only 1:1 so we should still get hits
        assert!(
            !hits.is_empty(),
            "expected word hits for 'Dios' in {}",
            bible.filename
        );
        for h in &hits {
            assert!(h.chapter >= 1);
            assert!(h.verse >= 1);
            assert!(!h.snippet.is_empty());
            // excluded origin
            assert!(!(h.book_number == 1 && h.chapter == 1 && h.verse == 1));
        }
        eprintln!(
            "search_bible_word Dios: {} hits via {}",
            hits.len(),
            bible.abbreviation
        );
    }

    #[test]
    fn get_chapter_genesis_1_and_cache_conn() {
        let modules = match scan_modules(None) {
            Ok(m) => m,
            Err(_) => return,
        };
        let bible = modules
            .iter()
            .find(|m| m.module_type == "bible" && !m.filename.to_lowercase().contains("inter"))
            .or_else(|| modules.iter().find(|m| m.module_type == "bible"));
        let Some(bible) = bible else { return };

        let t0 = Instant::now();
        let ch = match get_chapter(&bible.path, 1, 1) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("skip chapter: {e}");
                return;
            }
        };
        let first_ms = t0.elapsed().as_millis();
        assert!(!ch.verses.is_empty());
        assert_eq!(ch.book_number, 1);
        assert_eq!(ch.chapter, 1);

        let t1 = Instant::now();
        let ch2 = get_chapter(&bible.path, 1, 1).expect("second chapter load");
        let second_ms = t1.elapsed().as_millis();
        assert_eq!(ch.verses.len(), ch2.verses.len());
        eprintln!(
            "get_chapter {}: first={first_ms}ms second={second_ms}ms verses={}",
            bible.abbreviation,
            ch.verses.len()
        );
        // Second hit should reuse open connection
        assert!(second_ms <= first_ms + 200);
    }

    #[test]
    fn dictionary_matches_headwords_not_definition_body() {
        let modules = match scan_modules(None) {
            Ok(m) => m,
            Err(_) => return,
        };
        let dict = modules
            .iter()
            .find(|m| {
                if m.module_type != "dictionary" {
                    return false;
                }
                let h = format!("{} {}", m.title, m.filename).to_lowercase();
                !h.contains("strong")
                    && (h.contains("mundo")
                        || h.contains("lockward")
                        || h.contains("certeza")
                        || h.contains("holman")
                        || h.contains("easton")
                        || h.contains("ortiz")
                        || h.contains("pik"))
            })
            .or_else(|| modules.iter().find(|m| m.module_type == "dictionary"));
        let Some(dict) = dict else {
            eprintln!("skip: no dictionary module");
            return;
        };

        // "amor" as headword should return entries titled around AMOR, not random body hits
        let rows = lookup_dictionary(&dict.path, "amor", 25).expect("lookup");
        eprintln!(
            "dict {} amor → {} hits: {:?}",
            dict.abbreviation,
            rows.len(),
            rows.iter().take(8).map(|r| &r.term).collect::<Vec<_>>()
        );
        for r in &rows {
            let tn = strip_accents(&r.term.to_uppercase());
            let ok = tn == "AMOR"
                || tn.starts_with("AMOR")
                || topic_has_token(&tn, "AMOR");
            assert!(
                ok,
                "headword-only: unexpected topic «{}» for query amor",
                r.term
            );
        }

        // Very short / common particle should not flood via definition scan
        let de_rows = lookup_dictionary(&dict.path, "xyznonexistentwordqq", 20).expect("miss");
        assert!(de_rows.is_empty());
    }

    #[test]
    fn commentary_levels_isolated_for_vida_plena_jer26() {
        let modules = match scan_modules(None) {
            Ok(m) => m,
            Err(_) => return,
        };
        let vp = modules.iter().find(|m| {
            m.module_type == "commentary"
                && (m.filename.to_lowercase().contains("vidaplena")
                    || m.title.to_lowercase().contains("vida plena"))
        });
        let Some(vp) = vp else {
            eprintln!("skip: Vida Plena not installed");
            return;
        };
        // Jer = 24. Verse 1 has no verse note; book intro exists.
        let v1_verse_only = get_commentaries(&vp.path, 24, 26, 1, true, false, false)
            .expect("v1 verse");
        assert!(
            v1_verse_only.is_empty(),
            "Jer 26:1 verse-only should be empty, got {} rows",
            v1_verse_only.len()
        );
        let v1_with_book = get_commentaries(&vp.path, 24, 26, 1, false, false, true)
            .expect("v1 book");
        assert!(
            v1_with_book.iter().any(|r| r.level == "book"),
            "book filter should show book intro"
        );
        let v2 = get_commentaries(&vp.path, 24, 26, 2, true, false, false).expect("v2");
        assert!(
            v2.iter().any(|r| r.level == "verse"),
            "Jer 26:2 should have verse note"
        );
        let v8 = get_commentaries(&vp.path, 24, 26, 8, true, false, false).expect("v8");
        assert!(
            v8.iter().any(|r| r.level == "verse"),
            "Jer 26:8 should have verse note"
        );
        // Probe ● follows filters
        let probe_v1 = probe_commentary(&vp.path, 24, 26, 1, true, false, false);
        assert!(!probe_v1.has_content, "● should be off for Jer 26:1 verse-only");
        let probe_v2 = probe_commentary(&vp.path, 24, 26, 2, true, false, false);
        assert!(probe_v2.has_content, "● should be on for Jer 26:2");
    }

    #[test]
    fn resolve_gozaos_matt_5_12() {
        let modules = match scan_modules(None) {
            Ok(m) => m,
            Err(_) => return,
        };
        let paths: Vec<String> = modules
            .iter()
            .filter(|m| {
                let h = format!(
                    "{} {} {}",
                    m.title.to_lowercase(),
                    m.filename.to_lowercase(),
                    m.abbreviation.to_lowercase()
                );
                h.contains("interlineal") || h.contains("interlinear")
            })
            .map(|m| m.path.clone())
            .collect();
        if paths.is_empty() {
            eprintln!("skip: no interlinear module");
            return;
        }
        // Matthew = book 40 in e-Sword
        let res = resolve_word_strongs(40, 5, 12, "Gozaos", 0, &paths).expect("resolve");
        eprintln!("resolve Gozaos: note={} hits={:?}", res.note, res.hits.iter().map(|h| &h.strong).collect::<Vec<_>>());
        assert!(
            !res.hits.is_empty(),
            "expected Strong’s for Gozaos, note={}",
            res.note
        );
        assert!(
            res.hits.iter().any(|h| h.strong == "G5463"),
            "expected G5463, got {:?}",
            res.hits
        );
    }

    #[test]
    fn probe_many_commentaries_parallel_safe() {
        let modules = match scan_modules(None) {
            Ok(m) => m,
            Err(_) => return,
        };
        let paths: Vec<String> = modules
            .iter()
            .filter(|m| m.module_type == "commentary")
            .take(12)
            .map(|m| m.path.clone())
            .collect();
        if paths.is_empty() {
            return;
        }
        let t0 = Instant::now();
        let mut probes = Vec::new();
        // Simulate parallel chunks like lib.rs
        let handles: Vec<_> = paths
            .chunks(4)
            .map(|chunk| {
                let chunk: Vec<String> = chunk.to_vec();
                std::thread::spawn(move || {
                    chunk
                        .into_iter()
                        .map(|p| probe_commentary(&p, 1, 1, 1, true, false, false))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for h in handles {
            probes.extend(h.join().unwrap());
        }
        eprintln!(
            "probed {} commentaries in {}ms (ok={})",
            probes.len(),
            t0.elapsed().as_millis(),
            probes.iter().filter(|p| p.has_content).count()
        );
        assert_eq!(probes.len(), paths.len());
    }
}
