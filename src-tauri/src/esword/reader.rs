use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::books::{book_by_number, book_name_by_number};
use super::catalog::table_exists;
use super::bible_titles::{find_titles, Titles};
use super::sections::Titles as StudyTitles;
use super::text::cell_to_text;

/// Escape SQL `LIKE` metacharacters in user input. Paired with `ESCAPE '\'` in
/// the query. Without this, searching for `%` matched every row in the module.
fn like_escape(term: &str) -> String {
    let mut out = String::with_capacity(term.len() + 4);
    for c in term.chars() {
        if c == '%' || c == '_' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Per-module connection slots. Global map lock is only held briefly so different
/// modules can be queried in parallel (probes, parallel Bibles, etc.).
/// `u64` is a monotonic use-stamp so eviction is LRU rather than arbitrary — a
/// 48-module probe sweep used to be able to close the Bible you are reading.
static CONN_CACHE: Lazy<Mutex<HashMap<String, (u64, Arc<Mutex<Connection>>)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static CONN_TICK: AtomicU64 = AtomicU64::new(0);

const CONN_CACHE_MAX: usize = 48;

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

pub(super) fn with_conn<T>(
    path: &Path,
    f: impl FnOnce(&Connection, &str) -> Result<T, String>,
) -> Result<T, String> {
    let key = path.to_string_lossy().to_string();
    let stamp = CONN_TICK.fetch_add(1, Ordering::Relaxed);
    let slot = {
        let mut cache = CONN_CACHE
            .lock()
            .map_err(|_| "cache lock poisoned".to_string())?;
        if let Some(existing) = cache.get_mut(&key) {
            existing.0 = stamp;
            existing.1.clone()
        } else {
            let conn = open_ro_conn(path)?;
            let arc = Arc::new(Mutex::new(conn));
            cache.insert(key.clone(), (stamp, arc.clone()));
            // Bound cache size so we don't keep hundreds of open FDs forever.
            // Evict least-recently-used, never the entry we just inserted.
            while cache.len() > CONN_CACHE_MAX {
                let drop_key = cache
                    .iter()
                    .filter(|(k, _)| *k != &key)
                    .min_by_key(|(_, (t, _))| *t)
                    .map(|(k, _)| k.clone());
                match drop_key {
                    Some(k) => {
                        cache.remove(&k);
                        if let Ok(mut schema) = SCHEMA_MEMO.lock() {
                            schema.remove(&k);
                        }
                    }
                    None => break,
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

pub(super) fn table_exists_cached(conn: &Connection, path_key: &str, name: &str) -> bool {
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

pub(super) fn column_set_cached(conn: &Connection, path_key: &str, table: &str) -> HashSet<String> {
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

pub(super) fn module_labels_cached(conn: &Connection, path: &Path) -> (String, String) {
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
    /// The query that actually produced this hit. Differs from what the reader
    /// clicked when the lemma fallback fired ("aborrecen" -> "aborrecer").
    #[serde(default)]
    pub query_term: String,
    /// True when `query_term` is a guessed lemma rather than the typed word.
    #[serde(default)]
    pub fallback: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LexiconResult {
    pub term: String,
    pub source: String,
    pub title: String,
    pub definition: String,
    pub module: String,
    #[serde(default)]
    pub query_term: String,
    #[serde(default)]
    pub fallback: bool,
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

pub(super) const MAX_TEXT: usize = 50_000;

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

        let rows: Vec<(i32, i32, i32, Value)> =
            rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
        // Section titles stay in the text, marked (see bible_titles.rs). Plain-text titles are
        // read only when few verses carry them: a module that opens every verse on a new line
        // would turn its first lines into titles.
        let found: Vec<Titles> = rows
            .iter()
            .map(|(_, _, _, val)| match val {
                Value::Text(raw) => find_titles(raw, true),
                _ => Titles::default(),
            })
            .collect();
        let text_leads = rows
            .iter()
            .zip(&found)
            .filter(|((_, _, _, val), t)| matches!(val, Value::Text(raw) if t.has_text_lead(raw)))
            .count();
        let text_lines = text_leads <= 3 || text_leads * 5 <= rows.len() * 2;
        let mut verses = Vec::new();
        for ((b, ch, v, scripture), titles) in rows.into_iter().zip(found) {
            let text = match &scripture {
                Value::Text(raw) => {
                    let titles = if text_lines { titles } else { find_titles(raw, false) };
                    if titles.is_empty() {
                        cell_to_text(&scripture).unwrap_or_default()
                    } else {
                        super::text::clean_text(&titles.mark(raw))
                    }
                }
                _ => cell_to_text(&scripture).unwrap_or_default(),
            };
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
        let titles = StudyTitles::new(conn, path_key);

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
                            &titles,
                        );
                    }
                }
            }

            // Also pull exact verse if range missed (different schemas).
            // Skipped when !has_end — the query above already *is* this query,
            // so running it again only to discard every row was wasted work.
            if has_end {
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
                            if results
                                .iter()
                                .any(|x| x.reference == ref_label && x.level == "verse")
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
                                &titles,
                            );
                        }
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
                            if let Some(text) = titles.display(&r, 3) {
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
                        if let Some(text) = titles.display(&r, 3) {
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
    titles: &StudyTitles,
) {
    if let Some(text) = titles.display(&comments, 3) {
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
        for (pat, is_like) in topic_patterns(term) {
            let sql = if is_like {
                "SELECT 1 FROM Dictionary WHERE Topic LIKE ?1 ESCAPE '\\' LIMIT 1"
            } else {
                "SELECT 1 FROM Dictionary WHERE Topic = ?1 LIMIT 1"
            };
            if let Ok(mut stmt) = conn.prepare(sql) {
                if stmt.query_row([&pat], |_| Ok(1i32)).is_ok() {
                    return Ok(true);
                }
            }
        }
        // Same lemma fallback as `lookup_dictionary`, or the ● markers would
        // stay dark for exactly the inflected words the fallback exists to
        // rescue. Capped tighter: this runs once per installed dictionary.
        for cand in lemma_candidates(term).into_iter().take(MAX_PROBE_CANDIDATES) {
            for pat in dict_exact_patterns(&cand) {
                if let Ok(mut stmt) =
                    conn.prepare("SELECT 1 FROM Dictionary WHERE Topic = ?1 LIMIT 1")
                {
                    if stmt.query_row([&pat], |_| Ok(1i32)).is_ok() {
                        return Ok(true);
                    }
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
    let res = with_conn(&path, |conn, path_key| {
        let table = if table_exists_cached(conn, path_key, "Lexicon") {
            "Lexicon"
        } else if table_exists_cached(conn, path_key, "Dictionary") {
            "Dictionary"
        } else {
            return Ok(false);
        };
        // The lookup's own topic keys, in one pass over the table: these modules have no
        // index on Topic, so each separate query would be another full scan.
        let keys = lexicon_topic_keys(term);
        let test: Vec<String> = keys
            .iter()
            .enumerate()
            .map(|(i, (_, is_like))| {
                if *is_like {
                    format!("Topic LIKE ?{} ESCAPE '\\'", i + 1)
                } else {
                    format!("Topic = ?{}", i + 1)
                }
            })
            .collect();
        let sql = format!("SELECT 1 FROM [{table}] WHERE {} LIMIT 1", test.join(" OR "));
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if stmt
                .query_row(rusqlite::params_from_iter(keys.iter().map(|(k, _)| k)), |_| {
                    Ok(1i32)
                })
                .is_ok()
            {
                return Ok(true);
            }
        }
        // Word-shaped queries get the lemma fallback; a Strong's code does not,
        // since guessing lemmas from "G26" is meaningless.
        if normalize_strongs(term).is_none() {
            let exact = format!("SELECT 1 FROM [{table}] WHERE Topic = ?1 LIMIT 1");
            for cand in lemma_candidates(term).into_iter().take(MAX_PROBE_CANDIDATES) {
                for pat in dict_exact_patterns(&cand) {
                    if let Ok(mut stmt) = conn.prepare(&exact) {
                        if stmt.query_row([&pat], |_| Ok(1i32)).is_ok() {
                            return Ok(true);
                        }
                    }
                }
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
    let like = format!("%{}%", like_escape(term));

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
                 WHERE typeof(Scripture)='text' AND Scripture LIKE ?1 ESCAPE '\\' \
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
            let book_name = book_name_by_number(book_n)
                .map(str::to_string)
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
    // Unclamped LIMIT is dangerous: SQLite reads a negative LIMIT as "no limit",
    // so a bad argument would pull an entire commentary module into memory.
    let limit = limit.clamp(1, 200);
    with_conn(&path, |conn, path_key| {
        let (title, abbr) = module_labels_cached(conn, &path);
        let module_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let like = format!("%{}%", like_escape(term));
        let mut out = Vec::new();
        let titles = StudyTitles::new(conn, path_key);

        if table_exists_cached(conn, path_key, "VerseCommentary") {
            // Only text columns — blobs can't be LIKE-searched meaningfully
            if let Ok(mut stmt) = conn.prepare(
                "SELECT Book, ChapterBegin, VerseBegin, Comments FROM VerseCommentary \
                 WHERE typeof(Comments)='text' AND Comments LIKE ?1 ESCAPE '\\' LIMIT ?2",
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
                        if let Some(text) = titles.display(&comments, 2) {
                            let book_name = book_name_by_number(b)
                                .map(str::to_string)
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
/// How many guessed lemmas to try before giving up. The list is ordered by
/// confidence, so the useful ones are at the front.
const MAX_FALLBACK_CANDIDATES: usize = 14;

/// Tighter cap for the ● probes, which run once per installed module.
const MAX_PROBE_CANDIDATES: usize = 8;

pub fn lookup_dictionary(
    module_path: &str,
    term: &str,
    limit: i32,
) -> Result<Vec<DictionaryResult>, String> {
    let mut hits = lookup_dictionary_inner(module_path, term, limit, false)?;
    if !hits.is_empty() {
        for h in &mut hits {
            h.query_term = term.trim().to_string();
        }
        return Ok(hits);
    }

    // Bible text is inflected, dictionary topics are lemmas. Try the plausible
    // headwords for this word, exact-match only so a bad guess finds nothing
    // rather than something wrong.
    for cand in lemma_candidates(term)
        .into_iter()
        .take(MAX_FALLBACK_CANDIDATES)
    {
        let got = lookup_dictionary_inner(module_path, &cand, limit, true)?;
        if !got.is_empty() {
            return Ok(got
                .into_iter()
                .map(|mut r| {
                    r.query_term = cand.clone();
                    r.fallback = true;
                    r
                })
                .collect());
        }
    }
    Ok(vec![])
}

/// Exact-headword-only lookup.
///
/// Used for *guessed* terms (see [`lemma_candidates`]). A guess has to land on
/// a real headword to count — allowing the prefix and contains phases lets a
/// bad stem like "algar" match "ALGARROBA" and report a confident hit for
/// "alguno", which is worse than finding nothing.
pub fn lookup_dictionary_exact(
    module_path: &str,
    term: &str,
    limit: i32,
) -> Result<Vec<DictionaryResult>, String> {
    lookup_dictionary_inner(module_path, term, limit, true)
}

fn lookup_dictionary_inner(
    module_path: &str,
    term: &str,
    limit: i32,
    exact_only: bool,
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
        let titles = StudyTitles::new(conn, path_key);

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
                &titles,
            );
        }

        // Phase B: prefix — "DIOS%" finds "DIOS", "DIOS DE ISRAEL", not "AMOR DE DIOS"
        if !exact_only && (ranked.len() as i32) < limit {
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
                    &titles,
                );
            }
        }

        // Phase C (optional, longer terms only): Topic contains as a whole token,
        // e.g. "DE DIOS" inside "HIJO DE DIOS". Still Topic-only — never Definition.
        // Skipped for very short queries to avoid flooding ("de", "el", "luz" noise).
        if !exact_only && (ranked.len() as i32) < limit && term_norm.chars().count() >= 4 {
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
                    &titles,
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

/// Prefix patterns. The term is LIKE-escaped so a literal `%` typed by the user
/// is matched as a character instead of turning into "match everything".
fn dict_prefix_patterns(term: &str) -> Vec<String> {
    let t = like_escape(term.trim());
    let up = like_escape(&term.trim().to_uppercase());
    let mut v = vec![format!("{t}%"), format!("{up}%")];
    let na = like_escape(&strip_accents(&term.trim().to_uppercase()));
    if na != up {
        v.push(format!("{na}%"));
    }
    v.dedup();
    v
}

fn dict_contains_patterns(term: &str) -> Vec<String> {
    let t = like_escape(term.trim());
    let up = like_escape(&term.trim().to_uppercase());
    // Leading/trailing % only for longer headword discovery inside multi-word titles
    let mut v = vec![format!("%{up}%"), format!("%{t}%")];
    let na = like_escape(&strip_accents(&term.trim().to_uppercase()));
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
    titles: &StudyTitles,
) {
    if ranked.len() as i32 >= limit * 2 {
        // gather a bit extra for ranking, then trim
        return;
    }
    let sql = if is_like {
        "SELECT Topic, Definition FROM Dictionary WHERE Topic LIKE ?1 ESCAPE '\\' LIMIT ?2"
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
        let Some(definition) = titles.display(&def, 2) else {
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
                query_term: String::new(),
                fallback: false,
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

/// Léxico with the same lemma fallback as the dictionary.
///
/// A lexicon is keyed by Strong's number, but the reader may well arrive with a
/// Spanish word and no Strong's — the interlinear only resolves about half of
/// them — so an inflected Spanish query has to reach the headword too.
pub fn lookup_lexicon(
    module_path: &str,
    term: &str,
    limit: i32,
) -> Result<Vec<LexiconResult>, String> {
    let mut hits = lookup_lexicon_inner(module_path, term, limit)?;
    if !hits.is_empty() {
        for h in &mut hits {
            h.query_term = term.trim().to_string();
        }
        return Ok(hits);
    }
    // A Strong's code either exists or it does not; guessing lemmas off it is
    // meaningless, so only fall back for word-shaped queries. Nor for a Greek or Hebrew
    // word: the variants are Spanish inflections, and each one repeats the full
    // definition scan — 12 s per hover in Multiléxico, which spells Greek as entities.
    if normalize_strongs(term).is_some() || contains_hebrew_or_greek(term) {
        return Ok(vec![]);
    }
    for cand in lemma_candidates(term)
        .into_iter()
        .take(MAX_FALLBACK_CANDIDATES)
    {
        let got = lookup_lexicon_inner(module_path, &cand, limit)?;
        if !got.is_empty() {
            return Ok(got
                .into_iter()
                .map(|mut r| {
                    r.query_term = cand.clone();
                    r.fallback = true;
                    r
                })
                .collect());
        }
    }
    Ok(vec![])
}

fn lookup_lexicon_inner(
    module_path: &str,
    term: &str,
    limit: i32,
) -> Result<Vec<LexiconResult>, String> {
    let path = PathBuf::from(module_path);
    let term = term.trim();
    if term.is_empty() {
        return Ok(vec![]);
    }
    // Same guard as the dictionary: a negative LIMIT means "unlimited" in SQLite.
    let limit = limit.clamp(1, 80);
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
        let titles = StudyTitles::new(conn, path_key);

        let keys = lexicon_topic_keys(term);
        for (key, is_like) in &keys {
            if out.len() as i32 >= limit {
                break;
            }
            let sql = if *is_like {
                format!(
                    "SELECT Topic, Definition FROM [{table}] \
                     WHERE Topic LIKE ?1 ESCAPE '\\' LIMIT ?2"
                )
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
                        if let Some(definition) = titles.display(&def, 2) {
                            out.push(LexiconResult {
                                term: topic,
                                source: abbr.clone(),
                                title: title.clone(),
                                definition: truncate(&definition, MAX_TEXT),
                                module: module_name.clone(),
                                query_term: String::new(),
                                fallback: false,
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
                 WHERE typeof(Definition)='text' AND Definition LIKE ?1 ESCAPE '\\' LIMIT 12"
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
                            if let Some(definition) = titles.display(&def, 2) {
                                // Prefer topics that look like Strong's numbers
                                out.push(LexiconResult {
                                    term: topic,
                                    source: abbr.clone(),
                                    title: title.clone(),
                                    definition: truncate(&definition, MAX_TEXT),
                                    module: module_name.clone(),
                                    query_term: String::new(),
                                    fallback: false,
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

/// Topic keys a lexicon term is looked up by, in order, as `(pattern, is_like)`. A Strong's
/// code is tried the ways modules store it (G26, G0026, G00026) before the prefix match.
/// The lookup and the ● probe both use this list, so a ● always means the lookup finds it.
fn lexicon_topic_keys(term: &str) -> Vec<(String, bool)> {
    let term = term.trim();
    let strong = normalize_strongs(term);
    let mut keys: Vec<(String, bool)> = Vec::new();
    if let Some(s) = &strong {
        keys.push((s.clone(), false));
        if let Ok(n) = s[1..].parse::<u32>() {
            keys.push((format!("{}{:04}", &s[..1], n), false));
            keys.push((format!("{}{:05}", &s[..1], n), false));
        }
    }
    keys.push((term.to_string(), false));
    keys.push((term.to_uppercase(), false));
    keys.push((format!("{}%", like_escape(&term.to_uppercase())), true));
    if let Some(s) = &strong {
        keys.push((format!("{s}%"), true));
    }
    let mut seen = HashSet::new();
    keys.retain(|k| seen.insert(k.clone()));
    keys
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
        pats.push(format!("%{}%", like_escape(raw)));
    }
    if !bare.is_empty() && bare != raw {
        pats.push(format!("%{}%", like_escape(&bare)));
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

/// Patterns for probes / legacy callers — **Topic headwords only** (no definition body).
/// Returns `(pattern, is_like)`: the caller can no longer infer LIKE-ness by
/// looking for a `%`, since escaped user input may legitimately contain one.
fn topic_patterns(term: &str) -> Vec<(String, bool)> {
    let mut v: Vec<(String, bool)> = dict_exact_patterns(term)
        .into_iter()
        .map(|p| (p, false))
        .collect();
    v.extend(dict_prefix_patterns(term).into_iter().map(|p| (p, true)));
    // Only allow contains for longer terms (probe ● for multi-word titles)
    if strip_accents(&term.to_uppercase()).chars().count() >= 4 {
        v.extend(dict_contains_patterns(term).into_iter().map(|p| (p, true)));
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

/// Candidate dictionary headwords for an inflected Spanish word.
///
/// This is deliberately *not* a lemmatizer. It generates a short, ordered list
/// of plausible headwords and lets the dictionary itself be the oracle — a
/// wrong guess simply finds nothing, so precision costs nothing while recall
/// goes up. Bible text is inflected ("aborrecen", "abriendo", "alcanzarán")
/// while dictionary topics are lemmas ("aborrecer", "abrir", "alcanzar"), and
/// that mismatch is the single largest reason a clicked word finds no entry.
pub fn lemma_candidates(word: &str) -> Vec<String> {
    let w = word.trim().to_lowercase();
    let n = w.chars().count();
    if n < 3 {
        return vec![];
    }
    let chars: Vec<char> = w.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if s.chars().count() >= 3 && !out.contains(&s) {
            out.push(s);
        }
    };

    // Accents are dropped in some headword sets and kept in others.
    let bare = strip_accents(&w.to_uppercase()).to_lowercase();
    if bare != w {
        push(bare.clone());
    }

    // Plurals: luces -> luz, mujeres -> mujer, hijos -> hijo
    if w.ends_with("ces") && n > 4 {
        push(format!("{}z", chars[..n - 3].iter().collect::<String>()));
    }
    if w.ends_with("es") && n > 4 {
        push(chars[..n - 2].iter().collect::<String>());
    }
    if w.ends_with('s') && n > 3 {
        push(chars[..n - 1].iter().collect::<String>());
    }

    // High-precision endings first: participles and gerunds map cleanly back to
    // an infinitive, so these are tried before any generic stemming.
    const SHAPED: &[(&str, &[&str])] = &[
        ("ando", &["ar"]),
        ("iendo", &["er", "ir"]),
        ("yendo", &["er", "ir"]),
        ("ados", &["ar"]),
        ("adas", &["ar"]),
        ("ado", &["ar"]),
        ("ada", &["ar"]),
        ("idos", &["er", "ir"]),
        ("idas", &["er", "ir"]),
        ("ido", &["er", "ir"]),
        ("ida", &["er", "ir"]),
    ];
    for (suffix, endings) in SHAPED {
        if let Some(stem) = w.strip_suffix(suffix) {
            if stem.chars().count() >= 2 {
                for e in *endings {
                    push(format!("{stem}{e}"));
                }
            }
        }
    }

    // Ordering matters: callers cap how many candidates they try, so the
    // high-confidence ones have to come first. Peeling one character off
    // "alcanzarán" yields the junk stem "alcanzará"; peeling two yields the
    // answer. So sweep every cut for stems that are *already* infinitives
    // before generating any stem+ending combinations.
    let stems: Vec<String> = (1..=5usize)
        .take_while(|cut| n > cut + 2)
        .flat_map(|cut| {
            let raw: String = chars[..n - cut].iter().collect();
            let bare = strip_accents(&raw.to_uppercase()).to_lowercase();
            if bare == raw {
                vec![raw]
            } else {
                vec![raw, bare]
            }
        })
        .collect();

    for stem in &stems {
        if stem.ends_with("ar") || stem.ends_with("er") || stem.ends_with("ir") {
            push(stem.clone());
        }
    }

    // Then restore an infinitive from the stem: acercó -> acerc + ar. Five
    // characters is enough to reach "abr" in "abriendo". Aggressive on
    // purpose — callers match these exactly, so a nonsense stem finds nothing
    // instead of matching something wrong.
    //
    // Consonant-final stems go first: Spanish verb stems end in a consonant
    // (alegr-, aborrec-, abr-), so a vowel-final stem like "alegrao" only ever
    // yields junk. Letting it queue ahead pushed the real answer past the
    // caller's candidate cap.
    let is_consonant_final = |s: &String| {
        s.chars()
            .last()
            .map(|c| !matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'á' | 'é' | 'í' | 'ó' | 'ú'))
            .unwrap_or(false)
    };
    let ordered: Vec<&String> = stems
        .iter()
        .filter(|s| is_consonant_final(s))
        .chain(stems.iter().filter(|s| !is_consonant_final(s)))
        .collect();

    for stem in ordered {
        // Spelling shifts that preserve the sound: busqué -> buscar,
        // pagué -> pagar, empecé -> empezar.
        let fixed = if let Some(p) = stem.strip_suffix("qu") {
            format!("{p}c")
        } else if let Some(p) = stem.strip_suffix("gu") {
            format!("{p}g")
        } else if let Some(p) = stem.strip_suffix('c') {
            format!("{p}z")
        } else {
            stem.clone()
        };
        for ending in ["ar", "er", "ir"] {
            push(format!("{stem}{ending}"));
            if &fixed != stem {
                push(format!("{fixed}{ending}"));
            }
        }
    }

    // Adjective / participle gender: santos -> santo -> santa
    if w.ends_with('o') && n > 3 {
        push(format!("{}a", chars[..n - 1].iter().collect::<String>()));
    }
    out
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

pub(super) fn truncate(s: &str, max: usize) -> String {
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

        // dedup() only drops *adjacent* duplicates; interlinear segments repeat
        // the same code non-consecutively (e.g. G846 … G2532 … G846).
        let mut seen_codes = HashSet::new();
        let strongs: Vec<String> = RE_NUM
            .captures_iter(before)
            .filter_map(|c| c.get(1).map(|m| normalize_strong_code(m.as_str())))
            .filter(|s| s.len() >= 2)
            .filter(|s| seen_codes.insert(s.clone()))
            .collect();

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
                // Positional fallback: N-th Spanish gloss in the interlinear.
                // Only accept it when the gloss is at least plausibly the same
                // word — a raw positional guess hands back a confident but wrong
                // Strong's number, which is worse than reporting "not found".
                let cand = &tokens[word_index as usize];
                let cand_norm = normalize_lemma(&cand.spanish);
                let shared = cand_norm
                    .chars()
                    .zip(word_norm.chars())
                    .take_while(|(a, b)| a == b)
                    .count();
                if shared >= 3 {
                    chosen.push(cand);
                }
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

// ---------------------------------------------------------------------------
// Reference works (.refi/.refx) and devotionals (.devi/.devx)
//
// Both are single-table modules, unlike the verse-addressed ones above:
//   Reference(Chapter TEXT, Content)   — named sections: "PORTADA", "PROLOGO"
//   Devotional(Month, Day, Devotion)   — one entry per calendar day
// ---------------------------------------------------------------------------

/// One entry in a reference work's table of contents.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceEntry {
    /// SQLite rowid — stable handle, since section titles repeat and can be blank.
    pub index: i64,
    pub chapter: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceResult {
    pub index: i64,
    pub chapter: String,
    pub source: String,
    pub title: String,
    pub text: String,
    pub module: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevotionalResult {
    pub month: i32,
    pub day: i32,
    pub source: String,
    pub title: String,
    pub text: String,
    pub module: String,
}

fn module_file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Table of contents for a reference module.
pub fn list_reference_chapters(module_path: &str) -> Result<Vec<ReferenceEntry>, String> {
    let path = PathBuf::from(module_path);
    with_conn(&path, |conn, path_key| {
        if !table_exists_cached(conn, path_key, "Reference") {
            return Err("Este módulo no tiene tabla Reference".into());
        }
        let mut stmt = conn
            .prepare("SELECT rowid, [Chapter] FROM [Reference] ORDER BY rowid")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                let idx: i64 = row.get(0)?;
                let raw: Value = row.get(1)?;
                Ok((idx, raw))
            })
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for r in rows.flatten() {
            let (idx, raw) = r;
            // Section titles carry the same entity/nbsp noise as the body text.
            let title = cell_to_text(&raw).unwrap_or_default();
            let title = title.trim().to_string();
            out.push(ReferenceEntry {
                index: idx,
                chapter: if title.is_empty() {
                    format!("Sección {idx}")
                } else {
                    title
                },
            });
        }
        Ok(out)
    })
}

/// One section of a reference module, addressed by the rowid from
/// [`list_reference_chapters`].
pub fn get_reference_chapter(
    module_path: &str,
    index: i64,
) -> Result<Option<ReferenceResult>, String> {
    let path = PathBuf::from(module_path);
    with_conn(&path, |conn, path_key| {
        if !table_exists_cached(conn, path_key, "Reference") {
            return Err("Este módulo no tiene tabla Reference".into());
        }
        let (title, abbr) = module_labels_cached(conn, &path);
        let module = module_file_name(&path);

        let mut stmt = conn
            .prepare("SELECT [Chapter], [Content] FROM [Reference] WHERE rowid = ?1 LIMIT 1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map([index], |row| {
                let ch: Value = row.get(0)?;
                let body: Value = row.get(1)?;
                Ok((ch, body))
            })
            .map_err(|e| e.to_string())?;

        let Some(Ok((ch, body))) = rows.next() else {
            return Ok(None);
        };
        let Some(text) = cell_to_text(&body) else {
            return Ok(None);
        };
        let chapter = cell_to_text(&ch).unwrap_or_default().trim().to_string();
        Ok(Some(ReferenceResult {
            index,
            chapter: if chapter.is_empty() {
                format!("Sección {index}")
            } else {
                chapter
            },
            source: abbr,
            title,
            text,
            module,
        }))
    })
}

/// Which (month, day) pairs a devotional actually covers.
pub fn list_devotional_days(module_path: &str) -> Result<Vec<(i32, i32)>, String> {
    let path = PathBuf::from(module_path);
    with_conn(&path, |conn, path_key| {
        if !table_exists_cached(conn, path_key, "Devotional") {
            return Err("Este módulo no tiene tabla Devotional".into());
        }
        let mut stmt = conn
            .prepare("SELECT [Month], [Day] FROM [Devotional] ORDER BY [Month], [Day]")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?)))
            .map_err(|e| e.to_string())?;
        Ok(rows.flatten().collect())
    })
}

/// The devotional entry for a calendar day.
pub fn get_devotional(
    module_path: &str,
    month: i32,
    day: i32,
) -> Result<Option<DevotionalResult>, String> {
    let path = PathBuf::from(module_path);
    with_conn(&path, |conn, path_key| {
        if !table_exists_cached(conn, path_key, "Devotional") {
            return Err("Este módulo no tiene tabla Devotional".into());
        }
        let (title, abbr) = module_labels_cached(conn, &path);
        let module = module_file_name(&path);

        let mut stmt = conn
            .prepare(
                "SELECT [Devotion] FROM [Devotional] \
                 WHERE [Month] = ?1 AND [Day] = ?2 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map([month, day], |row| row.get::<_, Value>(0))
            .map_err(|e| e.to_string())?;

        let Some(Ok(body)) = rows.next() else {
            return Ok(None);
        };
        let Some(text) = cell_to_text(&body) else {
            return Ok(None);
        };
        Ok(Some(DevotionalResult {
            month,
            day,
            source: abbr,
            title,
            text,
            module,
        }))
    })
}

#[cfg(test)]
mod lemma_tests {
    use super::*;

    fn first_n(word: &str, n: usize) -> Vec<String> {
        lemma_candidates(word).into_iter().take(n).collect()
    }

    #[test]
    fn inflected_verbs_reach_their_infinitive_early() {
        // Each of these must appear inside the probe cap, not just somewhere in
        // the list — the ● sweep only tries the first few.
        let cases = [
            ("aborrecen", "aborrecer"),
            ("aborrecerás", "aborrecer"),
            ("abriendo", "abrir"),
            ("acercó", "acercar"),
            ("alcanzarán", "alcanzar"),
            ("alegraos", "alegrar"),
            ("amaron", "amar"),
            ("andamos", "andar"),
            ("bendecid", "bendecir"),
            ("clamamos", "clamar"),
        ];
        for (inflected, lemma) in cases {
            let got = first_n(inflected, MAX_PROBE_CANDIDATES);
            assert!(
                got.iter().any(|c| c == lemma),
                "{inflected} -> {lemma} not in first {MAX_PROBE_CANDIDATES}: {got:?}"
            );
        }
    }

    #[test]
    fn plurals_and_gender_reduce() {
        for (word, want) in [
            ("ajenos", "ajeno"),
            ("antiguos", "antiguo"),
            ("celestiales", "celestial"),
            ("luces", "luz"),
        ] {
            let got = lemma_candidates(word);
            assert!(got.contains(&want.to_string()), "{word} -> {want}: {got:?}");
        }
    }

    #[test]
    fn candidates_stay_bounded_and_sane() {
        for word in ["aborrecerías", "constantinopla", "a", "de", "yo"] {
            let got = lemma_candidates(word);
            assert!(got.len() <= 60, "{word} produced {} candidates", got.len());
            for c in &got {
                assert!(c.chars().count() >= 3, "{word} produced short {c:?}");
                assert!(!c.contains(' '), "{word} produced spaced {c:?}");
            }
        }
        // Too short to inflect meaningfully.
        assert!(lemma_candidates("de").is_empty());
    }

    #[test]
    fn strongs_codes_never_take_the_lemma_path() {
        // Guessing lemmas off "G26" is meaningless; lookup must not try.
        assert!(normalize_strongs("G26").is_some());
        assert!(normalize_strongs("H1254").is_some());
        assert!(normalize_strongs("aborrecen").is_none());
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::*;
    use crate::esword::scan_modules;

    fn dictionaries() -> Vec<String> {
        scan_modules(None)
            .map(|all| {
                all.into_iter()
                    .filter(|m| m.module_type == "dictionary" && !m.encrypted)
                    .map(|m| m.path)
                    .take(30)
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn fallback_is_reported_and_direct_hits_are_not() {
        let dicts = dictionaries();
        if dicts.is_empty() {
            eprintln!("skip: library not installed");
            return;
        }
        // A direct hit must never be labelled a fallback.
        let mut checked_direct = false;
        for d in &dicts {
            if let Ok(rows) = lookup_dictionary(d, "Dios", 3) {
                if let Some(r) = rows.first() {
                    assert!(!r.fallback, "direct hit marked as fallback in {d}");
                    assert_eq!(r.query_term, "Dios");
                    checked_direct = true;
                    break;
                }
            }
        }
        assert!(checked_direct, "no dictionary had an entry for Dios");

        // An inflected word that resolves must say which lemma it used.
        let mut checked_fallback = false;
        for d in &dicts {
            if let Ok(rows) = lookup_dictionary(d, "aborrecen", 3) {
                if let Some(r) = rows.first() {
                    assert!(r.fallback, "expected fallback flag for aborrecen in {d}");
                    assert_eq!(r.query_term, "aborrecer");
                    assert_ne!(r.query_term, "aborrecen");
                    checked_fallback = true;
                    break;
                }
            }
        }
        assert!(checked_fallback, "aborrecen resolved in no dictionary");
    }

    #[test]
    fn nonsense_words_stay_empty() {
        // The fallback must not manufacture hits out of junk.
        for d in dictionaries().iter().take(15) {
            for junk in ["zzzznotaword", "qwertyuiop", "xyzzyxx"] {
                let rows = lookup_dictionary(d, junk, 3).unwrap_or_default();
                assert!(rows.is_empty(), "{junk} matched in {d}: {:?}", rows[0].term);
                assert!(!probe_dictionary(d, junk).has_content);
            }
        }
    }
}

#[cfg(test)]
mod sync_audit {
    //! Measures how often clicking a word in the Bible actually lands somewhere
    //! in the dictionaries and lexicons. Run with:
    //!   cargo test --lib -- --ignored sync_audit --nocapture
    use super::*;
    use crate::esword::scan_modules;

    fn modules_of(kind: &str) -> Vec<crate::esword::ModuleInfo> {
        scan_modules(None)
            .map(|all| {
                all.into_iter()
                    .filter(|m| m.module_type == kind && !m.encrypted)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn pick(mods: &[crate::esword::ModuleInfo], needles: &[&str]) -> Option<String> {
        for needle in needles {
            if let Some(m) = mods
                .iter()
                .find(|m| m.filename.to_lowercase().contains(needle))
            {
                return Some(m.path.clone());
            }
        }
        mods.first().map(|m| m.path.clone())
    }

    /// Words as a reader would click them: letters only, no Strong's numbers.
    fn words_of(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = String::new();
        for c in text.chars() {
            if c.is_alphabetic() {
                cur.push(c);
            } else {
                if cur.chars().count() >= 3 {
                    out.push(cur.to_lowercase());
                }
                cur.clear();
            }
        }
        if cur.chars().count() >= 3 {
            out.push(cur.to_lowercase());
        }
        out
    }

    #[test]
    #[ignore = "audit: needs the installed e-Sword library"]
    fn dictionary_hit_rate_for_real_bible_words() {
        let bibles = modules_of("bible");
        let dicts = modules_of("dictionary");
        if bibles.is_empty() || dicts.is_empty() {
            eprintln!("skip: library not installed");
            return;
        }
        let bible = pick(&bibles, &["rvr1960", "rv1960", "lbla"]).expect("bible");
        // A spread of narrative, poetry, law, gospel and epistle.
        let passages: &[(i32, i32)] = &[(1, 1), (2, 20), (19, 23), (23, 55), (40, 5), (43, 3), (45, 8)];

        let mut vocab: Vec<String> = Vec::new();
        for (book, chapter) in passages {
            if let Ok(res) = get_chapter(&bible, *book, *chapter) {
                for v in res.verses {
                    vocab.extend(words_of(&v.text));
                }
            }
        }
        vocab.sort();
        vocab.dedup();
        assert!(!vocab.is_empty(), "no vocabulary extracted");

        let probe: Vec<String> = dicts
            .iter()
            .take(6)
            .map(|m| m.path.clone())
            .collect();

        let mut hit = 0usize;
        let mut misses: Vec<String> = Vec::new();
        for word in &vocab {
            let found = probe.iter().any(|d| {
                lookup_dictionary(d, word, 3)
                    .map(|r| !r.is_empty())
                    .unwrap_or(false)
            });
            if found {
                hit += 1;
            } else {
                misses.push(word.clone());
            }
        }
        eprintln!(
            "\nvocabulary: {} unique words over {} passages, {} dictionaries probed",
            vocab.len(),
            passages.len(),
            probe.len()
        );
        eprintln!(
            "dictionary hit: {hit}/{}  ({:.1}%)   miss: {}",
            vocab.len(),
            100.0 * hit as f64 / vocab.len() as f64,
            misses.len()
        );
        eprintln!("\nfirst 40 misses:");
        for m in misses.iter().take(40) {
            eprint!("{m}  ");
        }
        eprintln!();

        // Q1: is this a coverage problem? Widen to many more dictionaries.
        let wide: Vec<String> = dicts.iter().take(30).map(|m| m.path.clone()).collect();
        let mut wide_recover = 0usize;
        let mut still: Vec<String> = Vec::new();
        for word in &misses {
            let found = wide.iter().any(|d| {
                lookup_dictionary(d, word, 3)
                    .map(|r| !r.is_empty())
                    .unwrap_or(false)
            });
            if found {
                wide_recover += 1;
            } else {
                still.push(word.clone());
            }
        }
        eprintln!(
            "\nQ1 widen 6 -> 30 dictionaries: recovers {wide_recover} of {} misses ({} still missing)",
            misses.len(),
            still.len()
        );

        // Q2: is this a word-form problem? Try morphological candidates.
        // Narrower dictionary set here — candidates multiply the query count.
        let wide: Vec<String> = wide.into_iter().take(10).collect();
        let mut lemma_recover = 0usize;
        let mut examples: Vec<(String, String)> = Vec::new();
        let mut hopeless: Vec<String> = Vec::new();
        for word in &still {
            let mut got: Option<String> = None;
            for cand in lemma_candidates(word) {
                if wide.iter().any(|d| {
                    lookup_dictionary_exact(d, &cand, 3)
                        .map(|r| !r.is_empty())
                        .unwrap_or(false)
                }) {
                    got = Some(cand);
                    break;
                }
            }
            match got {
                Some(c) => {
                    lemma_recover += 1;
                    if examples.len() < 25 {
                        examples.push((word.clone(), c));
                    }
                }
                None => hopeless.push(word.clone()),
            }
        }
        eprintln!(
            "Q2 lemma candidates: recovers {lemma_recover} of the remaining {} ({} truly absent)",
            still.len(),
            hopeless.len()
        );
        eprintln!("\n  recovered examples:");
        for (w, c) in &examples {
            eprintln!("    {w:<16} -> {c}");
        }
        eprintln!("\n  still absent (first 30):");
        for w in hopeless.iter().take(30) {
            eprint!("{w}  ");
        }
        eprintln!();

        let final_hit = hit + wide_recover + lemma_recover;
        eprintln!(
            "\nPROJECTED hit rate: {final_hit}/{} ({:.1}%)  — from {:.1}%",
            vocab.len(),
            100.0 * final_hit as f64 / vocab.len() as f64,
            100.0 * hit as f64 / vocab.len() as f64
        );
    }

    /// Same vocabulary, measured through the shipping `lookup_dictionary`
    /// (fallback included) plus the cost of a full ● sweep.
    #[test]
    #[ignore = "audit: needs the installed e-Sword library"]
    fn hit_rate_and_probe_cost_after_fallback() {
        use std::time::Instant;
        let bibles = modules_of("bible");
        let dicts = modules_of("dictionary");
        if bibles.is_empty() || dicts.is_empty() {
            eprintln!("skip: library not installed");
            return;
        }
        let bible = pick(&bibles, &["rvr1960", "rv1960", "lbla"]).expect("bible");
        let passages: &[(i32, i32)] = &[(1, 1), (2, 20), (19, 23), (23, 55), (40, 5), (43, 3), (45, 8)];
        let mut vocab: Vec<String> = Vec::new();
        for (book, chapter) in passages {
            if let Ok(res) = get_chapter(&bible, *book, *chapter) {
                for v in res.verses {
                    vocab.extend(words_of(&v.text));
                }
            }
        }
        vocab.sort();
        vocab.dedup();

        for n in [6usize, 30] {
            let probe: Vec<String> = dicts.iter().take(n).map(|m| m.path.clone()).collect();
            let t = Instant::now();
            let mut hit = 0usize;
            let mut via_fallback = 0usize;
            for word in &vocab {
                let mut found = false;
                let mut fb = false;
                for d in &probe {
                    if let Ok(rows) = lookup_dictionary(d, word, 3) {
                        if let Some(first) = rows.first() {
                            found = true;
                            fb = first.fallback;
                            break;
                        }
                    }
                }
                if found {
                    hit += 1;
                    if fb {
                        via_fallback += 1;
                    }
                }
            }
            eprintln!(
                "{n:>3} dictionaries: hit {hit}/{} ({:.1}%)  of which {via_fallback} via lemma fallback  [{} ms]",
                vocab.len(),
                100.0 * hit as f64 / vocab.len() as f64,
                t.elapsed().as_millis()
            );
        }

        // Cost of one click: the ● sweep across every installed dictionary,
        // for a word that only the fallback can resolve.
        let all: Vec<String> = dicts.iter().map(|m| m.path.clone()).collect();
        for word in ["dios", "aborrecen", "abriendo", "alcanzarán", "zzzznotaword"] {
            // Old behaviour: exact/prefix/contains on the literal word only.
            let before = all
                .iter()
                .filter(|d| {
                    lookup_dictionary_inner(d, word, 1, false)
                        .map(|r| !r.is_empty())
                        .unwrap_or(false)
                })
                .count();
            let t = Instant::now();
            let after = all
                .iter()
                .filter(|d| probe_dictionary(d, word).has_content)
                .count();
            eprintln!(
                "● sweep {:>13}: {before:>3} -> {after:>3} of {} dictionaries  [{} ms]",
                word,
                all.len(),
                t.elapsed().as_millis()
            );
        }
    }

    #[test]
    #[ignore = "audit: needs the installed e-Sword library"]
    fn strongs_resolution_rate() {
        let bibles = modules_of("bible");
        if bibles.is_empty() {
            eprintln!("skip: library not installed");
            return;
        }
        let bible = pick(&bibles, &["rvr1960", "rv1960"]).expect("bible");
        let interlinears: Vec<String> = bibles
            .iter()
            .filter(|m| {
                let n = m.filename.to_lowercase();
                n.contains("interlineal") || n.contains("irv") || n.contains("int")
            })
            .map(|m| m.path.clone())
            .collect();
        eprintln!("\ninterlinear modules found: {}", interlinears.len());
        for p in interlinears.iter().take(5) {
            eprintln!("  {p}");
        }
        if interlinears.is_empty() {
            eprintln!("no interlinear -> lexicon can never be reached from a Spanish word");
            return;
        }

        let Ok(res) = get_chapter(&bible, 43, 3) else {
            eprintln!("skip: could not read John 3");
            return;
        };
        let mut total = 0usize;
        let mut resolved = 0usize;
        for v in res.verses.iter().take(20) {
            for (i, w) in words_of(&v.text).iter().enumerate() {
                total += 1;
                let got = resolve_word_strongs(43, 3, v.verse, w, i as i32, &interlinears);
                if got.map(|r| !r.hits.is_empty()).unwrap_or(false) {
                    resolved += 1;
                }
            }
        }
        eprintln!(
            "Strong's resolved for {resolved}/{total} words in John 3 ({:.1}%)",
            100.0 * resolved as f64 / total.max(1) as f64
        );
    }
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    use crate::esword::scan_modules;

    fn modules_of(kind: &str) -> Vec<String> {
        match scan_modules(None) {
            Ok(all) => all
                .into_iter()
                .filter(|m| m.module_type == kind && !m.encrypted)
                .map(|m| m.path)
                .collect(),
            Err(_) => vec![],
        }
    }

    #[test]
    fn reference_modules_are_catalogued_and_readable() {
        let paths = modules_of("reference");
        if paths.is_empty() {
            eprintln!("skip: no reference modules installed");
            return;
        }
        eprintln!("reference modules: {}", paths.len());

        let mut read = 0usize;
        for path in paths.iter().take(25) {
            let sections = match list_reference_chapters(path) {
                Ok(s) => s,
                Err(e) => panic!("{path}: {e}"),
            };
            assert!(!sections.is_empty(), "{path}: empty table of contents");
            for entry in &sections {
                assert!(
                    !entry.chapter.trim().is_empty(),
                    "{path}: blank section title at {}",
                    entry.index
                );
            }
            // Every section the contents list advertises must actually open.
            for entry in sections.iter().take(3) {
                let got = get_reference_chapter(path, entry.index)
                    .unwrap_or_else(|e| panic!("{path} #{}: {e}", entry.index));
                if let Some(res) = got {
                    assert_eq!(res.index, entry.index);
                    read += 1;
                }
            }
        }
        assert!(read > 0, "no reference section produced readable text");
        eprintln!("reference sections read: {read}");
    }

    #[test]
    fn devotionals_cover_the_calendar() {
        let paths = modules_of("devotional");
        if paths.is_empty() {
            eprintln!("skip: no devotional modules installed");
            return;
        }
        eprintln!("devotional modules: {}", paths.len());

        for path in &paths {
            let days = match list_devotional_days(path) {
                Ok(d) => d,
                Err(e) => panic!("{path}: {e}"),
            };
            assert!(!days.is_empty(), "{path}: no devotional days");
            for (m, d) in &days {
                assert!((1..=12).contains(m), "{path}: month {m} out of range");
                assert!((1..=31).contains(d), "{path}: day {d} out of range");
            }
            // January 1st is the one entry every devotional in the library has.
            let jan1 = get_devotional(path, 1, 1).unwrap_or_else(|e| panic!("{path}: {e}"));
            let jan1 = jan1.unwrap_or_else(|| panic!("{path}: no entry for 1 Jan"));
            assert!(!jan1.text.trim().is_empty(), "{path}: empty 1 Jan text");
        }
    }

    #[test]
    fn missing_entries_are_none_not_errors() {
        let refs = modules_of("reference");
        if let Some(path) = refs.first() {
            // rowid 0 never exists in SQLite.
            assert!(matches!(get_reference_chapter(path, 0), Ok(None)));
        }
        let devs = modules_of("devotional");
        if let Some(path) = devs.first() {
            assert!(matches!(get_devotional(path, 13, 1), Ok(None)));
        }
    }

    #[test]
    fn wrong_module_type_reports_clearly() {
        let bibles = modules_of("bible");
        if let Some(path) = bibles.first() {
            let err = list_reference_chapters(path).unwrap_err();
            assert!(err.contains("Reference"), "unexpected error: {err}");
            let err = list_devotional_days(path).unwrap_err();
            assert!(err.contains("Devotional"), "unexpected error: {err}");
        }
    }
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
        // Try candidates until one actually yields Genesis 1. Picking a single
        // module and bailing on error made this test pass without ever
        // exercising the connection cache it exists to check.
        let mut picked = None;
        for m in modules
            .iter()
            .filter(|m| m.module_type == "bible" && !m.encrypted)
            .filter(|m| !m.filename.to_lowercase().contains("inter"))
        {
            let t0 = Instant::now();
            if let Ok(c) = get_chapter(&m.path, 1, 1) {
                if !c.verses.is_empty() {
                    picked = Some((m, c, t0.elapsed().as_millis()));
                    break;
                }
            }
        }
        let Some((bible, ch, first_ms)) = picked else {
            eprintln!("skip: no bible module returned Génesis 1");
            return;
        };
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
    fn like_metacharacters_are_escaped() {
        assert_eq!(like_escape("amor"), "amor");
        assert_eq!(like_escape("100%"), r"100\%");
        assert_eq!(like_escape("a_b"), r"a\_b");
        assert_eq!(like_escape(r"a\b"), r"a\\b");
    }

    #[test]
    fn wildcard_query_does_not_match_every_headword() {
        let modules = match scan_modules(None) {
            Ok(m) => m,
            Err(_) => return,
        };
        let Some(dict) = modules.iter().find(|m| m.module_type == "dictionary") else {
            eprintln!("skip: no dictionary module");
            return;
        };
        // "%" was interpolated straight into LIKE, so it matched every row.
        let rows = lookup_dictionary(&dict.path, "%", 25).expect("lookup");
        eprintln!("dict {} '%' → {} hits", dict.abbreviation, rows.len());
        for r in &rows {
            assert!(
                r.term.contains('%'),
                "'%' must be a literal, not a wildcard — got topic «{}»",
                r.term
            );
        }
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
