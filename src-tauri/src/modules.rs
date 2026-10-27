//! e-Sword-compatible SQLite module scanning and queries.

use crate::text::{clean_esword_text, looks_encrypted, to_plain};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

static CONN_CACHE: Lazy<Mutex<HashMap<String, Arc<Mutex<Connection>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

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
    pub kind: String, // bible | commentary | dictionary | lexicon | reference
    pub encrypted: bool,
    pub has_strongs: bool,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerseRow {
    pub verse: i32,
    pub html: String,
    pub plain: String,
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
}

fn open_ro(path: &str) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("No se pudo abrir módulo: {e}"))
}

fn with_conn<T, F>(path: &str, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection) -> Result<T, String>,
{
    {
        let cache = CONN_CACHE.lock();
        if let Some(arc) = cache.get(path) {
            let conn = arc.lock();
            return f(&conn);
        }
    }
    let conn = open_ro(path)?;
    let arc = Arc::new(Mutex::new(conn));
    {
        let mut cache = CONN_CACHE.lock();
        // Bound cache size
        if cache.len() > 48 {
            cache.clear();
        }
        cache.insert(path.to_string(), arc.clone());
    }
    let conn = arc.lock();
    f(&conn)
}

fn kind_from_ext(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "bbli" | "bblx" | "bbl" => Some("bible"),
        "cmti" | "cmtx" | "cmt" => Some("commentary"),
        "dcti" | "dctx" | "dct" => Some("dictionary"),
        "lexi" | "lexx" | "lex" => Some("lexicon"),
        "refi" | "refx" | "ref" => Some("reference"),
        _ => None,
    }
}

fn is_legacy_encrypted_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "bblx" | "cmtx" | "dctx" | "lexx" | "refx"
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

fn read_details(conn: &Connection) -> (String, String, i32, bool) {
    let mut title = String::new();
    let mut abbr = String::new();
    let mut version = 0i32;
    let mut strongs = false;

    let _ = conn.query_row("SELECT Title FROM Details LIMIT 1", [], |r| {
        title = r.get::<_, String>(0).unwrap_or_default();
        Ok(())
    });
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

/// e-Sword dictionaries use `Dictionary`; lexicons use `Lexicon` (same Topic/Definition shape).
fn topic_table(conn: &Connection, prefer: &str) -> Option<&'static str> {
    let order = if prefer == "lexicon" {
        ["Lexicon", "Dictionary"]
    } else {
        ["Dictionary", "Lexicon"]
    };
    for t in order {
        if table_exists(conn, t) {
            return Some(t);
        }
    }
    None
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
    let samples: &[&str] = match kind {
        "bible" => &["SELECT Scripture FROM Bible LIMIT 1"],
        "commentary" => &[
            "SELECT Comments FROM VerseCommentary LIMIT 1",
            "SELECT Comments FROM ChapterCommentary LIMIT 1",
            "SELECT Comments FROM BookCommentary LIMIT 1",
        ],
        "dictionary" | "lexicon" => &[
            "SELECT Definition FROM Lexicon LIMIT 1",
            "SELECT Definition FROM Dictionary LIMIT 1",
        ],
        "reference" => &["SELECT Content FROM Reference LIMIT 1"],
        _ => return false,
    };
    for sql in samples {
        if let Ok(sample) = conn.query_row(*sql, [], |r| cell_to_string(r, 0)) {
            if !sample.is_empty() {
                return looks_encrypted(&sample);
            }
        }
    }
    false
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
                return Ok(c.modules.clone());
            }
        }
    }

    let mut modules = Vec::new();
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
        let mut encrypted = is_legacy_encrypted_ext(&ext);

        // Skip bad/unreadable files without aborting the whole scan
        match open_ro(&path_str) {
            Ok(conn) => {
                let (t, a, v, s) = read_details(&conn);
                if !t.is_empty() {
                    title = t;
                }
                if !a.is_empty() {
                    abbreviation = a;
                }
                version = v;
                has_strongs = s || kind == "lexicon";
                if !encrypted {
                    encrypted = sample_encrypted(&conn, kind);
                }
            }
            Err(_) => {
                encrypted = true;
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

pub fn load_bible_chapter(
    module_path: &str,
    book: i32,
    chapter: i32,
) -> Result<ChapterPayload, String> {
    with_conn(module_path, |conn| {
        let (title, abbr, _, _) = read_details(conn);
        let mut stmt = conn
            .prepare(
                "SELECT Verse, Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 ORDER BY Verse",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![book, chapter], |row| {
                let verse: i32 = row.get(0)?;
                let scripture = cell_to_string(row, 1)?;
                Ok((verse, scripture))
            })
            .map_err(|e| e.to_string())?;

        let mut verses = Vec::new();
        for r in rows.flatten() {
            let (verse, scripture) = r;
            if looks_encrypted(&scripture) {
                return Err(
                    "Este módulo parece cifrado (🔒). Use un módulo .bbli moderno sin DRM."
                        .into(),
                );
            }
            let html = clean_esword_text(&scripture);
            let plain = to_plain(&html);
            verses.push(VerseRow { verse, html, plain });
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
    with_conn(module_path, |conn| {
        let scripture: String = conn
            .query_row(
                "SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3",
                rusqlite::params![book, chapter, verse],
                |row| cell_to_string(row, 0),
            )
            .map_err(|_| "Versículo no encontrado".to_string())?;
        let html = clean_esword_text(&scripture);
        Ok(to_plain(&html))
    })
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
    with_conn(module_path, |conn| {
        let mut out = Vec::new();

        if include_verse {
            // Match rows where chapterBegin..chapterEnd covers chapter and verse range covers verse
            let mut stmt = conn
                .prepare(
                    "SELECT ChapterBegin, VerseBegin, ChapterEnd, VerseEnd, Comments
                     FROM VerseCommentary WHERE Book=?1",
                )
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(rusqlite::params![book], |row| {
                    Ok((
                        row.get::<_, i32>(0).unwrap_or(0),
                        row.get::<_, i32>(1).unwrap_or(0),
                        row.get::<_, Option<i32>>(2).ok().flatten().unwrap_or(0),
                        row.get::<_, Option<i32>>(3).ok().flatten().unwrap_or(0),
                        cell_to_string(row, 4).unwrap_or_default(),
                    ))
                })
                .map_err(|e| e.to_string())?;

            for r in rows.flatten() {
                let (cb, vb, ce, ve, comments) = r;
                if comments.trim().is_empty() {
                    continue;
                }
                let end_ch = if ce == 0 { cb } else { ce };
                let end_v = if ve == 0 { vb } else { ve };
                let after_start = chapter > cb || (chapter == cb && verse >= vb);
                let before_end = chapter < end_ch || (chapter == end_ch && verse <= end_v);
                // Also single-verse rows with zero ends
                let single = ce == 0 && ve == 0 && cb == chapter && vb == verse;
                if single || (after_start && before_end) {
                    if looks_encrypted(&comments) {
                        return Err("Módulo de comentario cifrado (🔒).".into());
                    }
                    let html = clean_esword_text(&comments);
                    let label = if cb == end_ch && vb == end_v {
                        format!("v.{vb}")
                    } else if cb == end_ch {
                        format!("v.{vb}–{end_v}")
                    } else {
                        format!("{cb}:{vb}–{end_ch}:{end_v}")
                    };
                    out.push(StudyEntry {
                        scope: "verse".into(),
                        label,
                        plain: to_plain(&html),
                        html,
                    });
                }
            }
        }

        if include_chapter {
            if let Ok(comments) = conn.query_row(
                "SELECT Comments FROM ChapterCommentary WHERE Book=?1 AND Chapter=?2",
                rusqlite::params![book, chapter],
                |row| cell_to_string(row, 0),
            ) {
                if !comments.trim().is_empty() {
                    let html = clean_esword_text(&comments);
                    out.push(StudyEntry {
                        scope: "chapter".into(),
                        label: format!("Cap. {chapter}"),
                        plain: to_plain(&html),
                        html,
                    });
                }
            }
        }

        if include_book {
            if let Ok(comments) = conn.query_row(
                "SELECT Comments FROM BookCommentary WHERE Book=?1",
                rusqlite::params![book],
                |row| cell_to_string(row, 0),
            ) {
                if !comments.trim().is_empty() {
                    let html = clean_esword_text(&comments);
                    out.push(StudyEntry {
                        scope: "book".into(),
                        label: "Libro".into(),
                        plain: to_plain(&html),
                        html,
                    });
                }
            }
        }

        Ok(out)
    })
}

fn lookup_in_topic_table(
    conn: &Connection,
    table: &str,
    query: &str,
    limit: i32,
) -> Result<Vec<DictEntry>, String> {
    // Only headword/topic — never full-text scan of Definition body
    let sql = format!(
        "SELECT Topic, Definition FROM \"{table}\"
         WHERE Topic = ?1 COLLATE NOCASE
            OR Topic LIKE ?2
            OR Topic LIKE ?3
         ORDER BY
           CASE
             WHEN Topic = ?1 COLLATE NOCASE THEN 0
             WHEN Topic LIKE ?2 THEN 1
             ELSE 2
           END,
           length(Topic),
           Topic
         LIMIT ?4"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let prefix = format!("{query}%");
    let contains = format!("%{query}%");
    let rows = stmt
        .query_map(rusqlite::params![query, prefix, contains, limit], |row| {
            Ok((cell_to_string(row, 0)?, cell_to_string(row, 1)?))
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for r in rows.flatten() {
        let (topic, def) = r;
        if def.is_empty() {
            continue;
        }
        if looks_encrypted(&def) {
            return Err("Módulo cifrado (🔒). Use un .lexi/.dcti moderno sin DRM.".into());
        }
        let html = clean_esword_text(&def);
        out.push(DictEntry {
            topic,
            plain: to_plain(&html),
            html,
        });
    }
    Ok(out)
}

pub fn dictionary_lookup(module_path: &str, query: &str, limit: i32) -> Result<Vec<DictEntry>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    with_conn(module_path, |conn| {
        let table = topic_table(conn, "dictionary")
            .ok_or_else(|| "Este módulo no tiene tabla Dictionary/Lexicon.".to_string())?;
        lookup_in_topic_table(conn, table, q, limit)
    })
}

pub fn lexicon_lookup(module_path: &str, strong: &str) -> Result<Option<DictEntry>, String> {
    let raw = strong.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    with_conn(module_path, |conn| {
        // Prefer Lexicon table (.lexi / .lexx), fall back to Dictionary (some Strong packs as .dcti)
        let table = topic_table(conn, "lexicon")
            .ok_or_else(|| "Este módulo no tiene tabla Lexicon/Dictionary.".to_string())?;

        // Exact Strong variants first
        for c in strong_candidates(raw) {
            let sql = format!(
                "SELECT Topic, Definition FROM \"{table}\" WHERE Topic = ?1 COLLATE NOCASE LIMIT 1"
            );
            if let Ok((topic, def)) = conn.query_row(&sql, rusqlite::params![c], |row| {
                Ok((cell_to_string(row, 0)?, cell_to_string(row, 1)?))
            }) {
                if looks_encrypted(&def) {
                    return Err("Léxico cifrado (🔒).".into());
                }
                let html = clean_esword_text(&def);
                return Ok(Some(DictEntry {
                    topic,
                    plain: to_plain(&html),
                    html,
                }));
            }
        }

        // Headword / partial topic (Vine Spanish lemmas, etc.) — not definition body
        let mut entries = lookup_in_topic_table(conn, table, raw, 8)?;
        // Prefer exact-ish Strong-looking hit if query is Strong-like
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
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(rusqlite::params![limit], |row| {
                    Ok((cell_to_string(row, 0)?, cell_to_string(row, 1)?))
                })
                .map_err(|e| e.to_string())?;
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
                });
            }
            return Ok(out);
        }
        let mut stmt = conn
            .prepare(
                "SELECT Chapter, Content FROM Reference
                 WHERE Chapter = ?1 COLLATE NOCASE
                    OR Chapter LIKE ?2
                    OR Chapter LIKE ?3
                 ORDER BY
                   CASE WHEN Chapter = ?1 COLLATE NOCASE THEN 0
                        WHEN Chapter LIKE ?2 THEN 1
                        ELSE 2 END,
                   Chapter
                 LIMIT ?4",
            )
            .map_err(|e| e.to_string())?;
        let prefix = format!("{q}%");
        let contains = format!("%{q}%");
        let rows = stmt
            .query_map(rusqlite::params![q, prefix, contains, limit], |row| {
                Ok((cell_to_string(row, 0)?, cell_to_string(row, 1)?))
            })
            .map_err(|e| e.to_string())?;
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
            });
        }
        Ok(out)
    })
}

pub fn probe_commentary_has(
    module_path: &str,
    book: i32,
    chapter: i32,
    verse: i32,
    include_verse: bool,
    include_chapter: bool,
    include_book: bool,
) -> bool {
    load_commentary(
        module_path,
        book,
        chapter,
        verse,
        include_verse,
        include_chapter,
        include_book,
    )
    .map(|v| !v.is_empty())
    .unwrap_or(false)
}

pub fn probe_dictionary_has(module_path: &str, topic: &str) -> bool {
    if topic.trim().is_empty() {
        return false;
    }
    dictionary_lookup(module_path, topic, 1)
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

pub fn probe_lexicon_has(module_path: &str, strong: &str) -> bool {
    lexicon_lookup(module_path, strong)
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
        .map_err(|e| e.to_string())
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
    CONN_CACHE.lock().clear();
    *MODULE_LIST_CACHE.lock() = None;
}

// ─── Word interconnection (Spanish ↔ Strong ↔ dict ↔ lexicon) ───

static RE_STRONG_ONLY: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[HG]\d+$").unwrap());
/// Tag markers for sequential interlinear walk (iRV / e-Sword style).
static RE_IL_TAG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?is)<(?:num)\s*>\s*([HG]\s*\d+)\s*</num>|<blu>(.*?)</blu>|<heb>(.*?)</heb>|<grk>(.*?)</grk>",
    )
    .unwrap()
});

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedEntry {
    pub module_path: String,
    pub module_abbr: String,
    pub topic: String,
    pub html: String,
    pub plain: String,
}

/// One interlinear “column”: original (H/G script) + Spanish + Strong (+ translit).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterlinearWordHit {
    pub spanish: String,
    pub strong: String,
    /// Hebrew or Greek lemma (script), digits/arrows stripped.
    pub original: String,
    /// Latin transliteration when present (e.g. `Houtô`, `êgapêsen`).
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
}

fn strip_tags_quick(s: &str) -> String {
    let re = Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(s, "").to_string()
}

/// Normalize Spanish/Greek/Hebrew headwords for comparison.
pub fn normalize_word(s: &str) -> String {
    let s = s.trim();
    let s = strip_tags_quick(s);
    let s = s.trim_matches(|c: char| {
        c.is_ascii_punctuation()
            || matches!(c, '«' | '»' | '“' | '”' | '‘' | '’' | '…' | '—')
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
            c if c.is_alphanumeric() || c == '\'' => out.push(c),
            _ => {}
        }
    }
    out
}

fn words_match(a: &str, b: &str) -> bool {
    let na = normalize_word(a);
    let nb = normalize_word(b);
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    na == nb || na.starts_with(&nb) || nb.starts_with(&na) || na.contains(&nb) || nb.contains(&na)
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

fn normalize_strong_code(raw: &str) -> String {
    let s = raw.trim().to_uppercase().replace(' ', "");
    if s.len() < 2 {
        return s;
    }
    let letter = s.chars().next().unwrap();
    if letter != 'H' && letter != 'G' {
        return s;
    }
    let digits: String = s.chars().skip(1).filter(|c| c.is_ascii_digit()).collect();
    let trimmed = digits.trim_start_matches('0');
    let core = if trimmed.is_empty() { "0" } else { trimmed };
    format!("{letter}{core}")
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

/// Parse e-Sword interlinear verse into Spanish ↔ Strong ↔ original (H/G) hits.
///
/// NT iRV style:
///   `<grk>ἠγάπησεν3</grk> êgapêsen <num>G25</num> VAAI3S <blu>amó</blu>`
/// OT iRV style:
///   `<heb>רֵאשִׁית</heb>2 <num>H7225</num>:NCcSFC <blu>principio</blu>`
pub fn parse_interlinear_tokens(raw: &str) -> Vec<InterlinearWordHit> {
    let mut out: Vec<InterlinearWordHit> = Vec::new();
    if raw.is_empty() {
        return out;
    }

    // Sequential state machine over tags; also capture bare translit between grk and num.
    let mut pending_original = String::new();
    let mut pending_script = String::new();
    let mut pending_translit = String::new();
    let mut pending_strongs: Vec<String> = Vec::new();
    let mut last_end = 0usize;

    for cap in RE_IL_TAG.captures_iter(raw) {
        let m = cap.get(0).unwrap();
        // Bare text between previous tag and this one may be translit (NT)
        let between = raw.get(last_end..m.start()).unwrap_or("").trim();
        if !between.is_empty() && pending_original.is_empty() == false {
            // First latin-looking token after original is translit
            for tok in between.split_whitespace() {
                let clean = tok.trim_matches(|c: char| c.is_ascii_punctuation() || c == ':' || c == '‹' || c == '›');
                if is_plausible_translit(clean) && pending_translit.is_empty() {
                    pending_translit = clean.to_string();
                    break;
                }
            }
        }
        last_end = m.end();

        if let Some(num) = cap.get(1) {
            let code = normalize_strong_code(num.as_str());
            if RE_STRONG_ONLY.is_match(&code) && !pending_strongs.iter().any(|s| s == &code) {
                pending_strongs.push(code);
            }
            continue;
        }
        if let Some(blu) = cap.get(2) {
            let spanish_raw = strip_tags_quick(blu.as_str());
            let spanish = spanish_raw
                .trim()
                .trim_matches(|c: char| c.is_ascii_punctuation() || matches!(c, '«' | '»' | '“' | '”'))
                .to_string();
            if is_plausible_spanish_gloss(&spanish) && !pending_strongs.is_empty() {
                // One hit per strong (multi-strong phrases like ὁ Θεὸς → G3588 G2316)
                for (i, strong) in pending_strongs.iter().enumerate() {
                    out.push(InterlinearWordHit {
                        spanish: spanish.clone(),
                        strong: strong.clone(),
                        original: if i == 0 {
                            pending_original.clone()
                        } else {
                            // keep original only on first strong of a multi-num group
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
            } else if is_plausible_spanish_gloss(&spanish) && !pending_original.is_empty() {
                // Spanish with original but no Strong yet — still useful
                out.push(InterlinearWordHit {
                    spanish,
                    strong: String::new(),
                    original: pending_original.clone(),
                    translit: pending_translit.clone(),
                    script: pending_script.clone(),
                });
            }
            // Reset cluster after Spanish gloss (end of unit)
            pending_original.clear();
            pending_script.clear();
            pending_translit.clear();
            pending_strongs.clear();
            continue;
        }
        if let Some(heb) = cap.get(3) {
            let o = clean_original_script(heb.as_str());
            if !o.is_empty() {
                // New original starts a new cluster if we already had strongs without spanish
                if !pending_strongs.is_empty() && !pending_original.is_empty() {
                    // flush incomplete? skip — wait for blu
                }
                if pending_original.is_empty() {
                    pending_original = o;
                } else {
                    // multi-word original (e.g. ὁ Θεὸς)
                    pending_original.push(' ');
                    pending_original.push_str(&o);
                }
                pending_script = "hebrew".into();
            }
            continue;
        }
        if let Some(grk) = cap.get(4) {
            let o = clean_original_script(grk.as_str());
            if !o.is_empty() {
                if pending_original.is_empty() {
                    pending_original = o;
                } else {
                    pending_original.push(' ');
                    pending_original.push_str(&o);
                }
                pending_script = "greek".into();
            }
            continue;
        }
    }

    // Deduplicate identical (spanish, strong, original)
    let mut seen = std::collections::HashSet::new();
    out.retain(|h| {
        let key = format!("{}|{}|{}", h.spanish, h.strong, h.original);
        seen.insert(key)
    });
    out
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
    with_conn(path, |conn| {
        let (_, a, _, _) = read_details(conn);
        Ok(if a.is_empty() {
            Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string()
        } else {
            a
        })
    })
    .unwrap_or_else(|_| "?".into())
}

/// Full interconnect: Spanish/Greek/Hebrew/Strong → dictionary + lexicon hits.
pub fn resolve_word_links(
    word: &str,
    dict_paths: &[String],
    lex_paths: &[String],
    interlinear_path: Option<&str>,
    book: i32,
    chapter: i32,
    verse: i32,
) -> WordLinkResult {
    let word = word.trim();
    let normalized = normalize_word(word);
    let mut strongs: Vec<String> = Vec::new();
    let mut hebrew: Vec<String> = Vec::new();
    let mut greek: Vec<String> = Vec::new();
    let mut interlinear_used = false;
    let mut interlinear_hits: Vec<InterlinearWordHit> = Vec::new();
    let mut matched_hit: Option<InterlinearWordHit> = None;

    // Direct Strong code typed/clicked
    let upper = normalize_strong_code(word);
    if RE_STRONG_ONLY.is_match(&upper) {
        strongs.push(upper.clone());
    }

    // Hebrew / Greek script detection on the clicked token itself
    if word.chars().any(|c| matches!(c, '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}')) {
        hebrew.push(word.to_string());
    }
    if word
        .chars()
        .any(|c| matches!(c, '\u{0370}'..='\u{03FF}' | '\u{1F00}'..='\u{1FFF}'))
    {
        greek.push(word.to_string());
    }

    // Interlinear map for current verse (full H/G + Spanish + Strong)
    if let Some(ip) = interlinear_path {
        if let Ok(raw) = with_conn(ip, |conn| {
            conn.query_row(
                "SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3",
                rusqlite::params![book, chapter, verse],
                |row| cell_to_string(row, 0),
            )
            .map_err(|e| e.to_string())
        }) {
            interlinear_used = true;
            interlinear_hits = parse_interlinear_tokens(&raw);

            // Match clicked Spanish / original / Strong against verse map
            for h in &interlinear_hits {
                let match_es = !h.spanish.is_empty()
                    && (words_match(&h.spanish, word) || words_match(&h.spanish, &normalized));
                let match_orig = !h.original.is_empty()
                    && (words_match(&h.original, word)
                        || h.original.contains(word)
                        || word.contains(&h.original));
                let match_strong = !h.strong.is_empty()
                    && normalize_strong_code(&h.strong) == normalize_strong_code(word);
                let match_tr = !h.translit.is_empty() && words_match(&h.translit, word);

                if match_es || match_orig || match_strong || match_tr {
                    if !h.strong.is_empty() && !strongs.iter().any(|s| s == &h.strong) {
                        strongs.push(h.strong.clone());
                    }
                    if !h.original.is_empty() {
                        match h.script.as_str() {
                            "hebrew" => {
                                if !hebrew.iter().any(|x| x == &h.original) {
                                    hebrew.push(h.original.clone());
                                }
                            }
                            "greek" => {
                                if !greek.iter().any(|x| x == &h.original) {
                                    greek.push(h.original.clone());
                                }
                            }
                            _ => {
                                let k = script_kind(&h.original);
                                if k == "hebrew" && !hebrew.iter().any(|x| x == &h.original) {
                                    hebrew.push(h.original.clone());
                                } else if k == "greek" && !greek.iter().any(|x| x == &h.original) {
                                    greek.push(h.original.clone());
                                }
                            }
                        }
                    }
                    if matched_hit.is_none() {
                        matched_hit = Some(h.clone());
                    }
                }
            }

            // If user typed/clicked a Strong, fill original/spanish from first matching hit
            if matched_hit.is_none() && RE_STRONG_ONLY.is_match(&upper) {
                if let Some(h) = interlinear_hits
                    .iter()
                    .find(|h| normalize_strong_code(&h.strong) == upper)
                {
                    matched_hit = Some(h.clone());
                    if !h.original.is_empty() {
                        if h.script == "hebrew" || script_kind(&h.original) == "hebrew" {
                            hebrew.push(h.original.clone());
                        } else if h.script == "greek" || script_kind(&h.original) == "greek" {
                            greek.push(h.original.clone());
                        }
                    }
                }
            }
        }
    }

    let mut dictionary = Vec::new();
    let mut lexicon = Vec::new();

    // Dictionary: Spanish headword (prefer matched Spanish gloss from interlinear)
    let dict_head = matched_hit
        .as_ref()
        .map(|h| h.spanish.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(word);
    let dict_queries = {
        let mut q = vec![dict_head.to_string(), word.to_string()];
        q.push(dict_head.to_uppercase());
        let mut chars = dict_head.chars();
        if let Some(f) = chars.next() {
            let titled = f.to_uppercase().collect::<String>() + chars.as_str();
            q.push(titled);
        }
        q.sort();
        q.dedup();
        q
    };

    for path in dict_paths.iter().take(12) {
        if dictionary.len() >= 10 {
            break;
        }
        for q in &dict_queries {
            if let Ok(entries) = dictionary_lookup(path, q, 6) {
                let abbr = module_abbr(path);
                for e in entries {
                    if dictionary
                        .iter()
                        .any(|x: &LinkedEntry| x.module_path == *path && x.topic == e.topic)
                    {
                        continue;
                    }
                    dictionary.push(LinkedEntry {
                        module_path: path.clone(),
                        module_abbr: abbr.clone(),
                        topic: e.topic,
                        html: e.html,
                        plain: e.plain,
                    });
                    if dictionary.len() >= 10 {
                        break;
                    }
                }
            }
            if dictionary.len() >= 10 {
                break;
            }
        }
    }

    // Lexicon by Strong numbers (matched first)
    for code in &strongs {
        for path in lex_paths.iter().take(10) {
            if let Ok(Some(e)) = lexicon_lookup(path, code) {
                let abbr = module_abbr(path);
                if !lexicon
                    .iter()
                    .any(|x: &LinkedEntry| x.module_path == *path && x.topic == e.topic)
                {
                    lexicon.push(LinkedEntry {
                        module_path: path.clone(),
                        module_abbr: abbr,
                        topic: e.topic,
                        html: e.html,
                        plain: e.plain,
                    });
                }
                break; // one hit per strong from first module; still try other strongs
            }
        }
    }

    // Lexicon by Spanish/Hebrew/Greek lemma (Vine etc. store Spanish topics)
    if lexicon.len() < 8 {
        let lemma_queries: Vec<String> = {
            let mut v = vec![word.to_string()];
            if let Some(h) = &matched_hit {
                if !h.spanish.is_empty() {
                    v.push(h.spanish.clone());
                }
                if !h.original.is_empty() {
                    v.push(h.original.clone());
                }
            }
            v.extend(hebrew.iter().cloned());
            v.extend(greek.iter().cloned());
            v.sort();
            v.dedup();
            v
        };
        for path in lex_paths.iter().take(10) {
            for lemma in &lemma_queries {
                if let Ok(entries) = {
                    with_conn(path, |conn| {
                        let table = topic_table(conn, "lexicon").unwrap_or("Lexicon");
                        lookup_in_topic_table(conn, table, lemma, 4)
                    })
                } {
                    let abbr = module_abbr(path);
                    for e in entries {
                        if lexicon
                            .iter()
                            .any(|x| x.module_path == *path && x.topic == e.topic)
                        {
                            continue;
                        }
                        let t = normalize_strong_code(&e.topic);
                        if RE_STRONG_ONLY.is_match(&t) && !strongs.iter().any(|s| s == &t) {
                            strongs.push(t);
                        }
                        lexicon.push(LinkedEntry {
                            module_path: path.clone(),
                            module_abbr: abbr.clone(),
                            topic: e.topic,
                            html: e.html,
                            plain: e.plain,
                        });
                        if lexicon.len() >= 10 {
                            break;
                        }
                    }
                }
                if lexicon.len() >= 10 {
                    break;
                }
            }
            if lexicon.len() >= 10 {
                break;
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
    }
}

/// Find a preferred interlinear Bible path from scanned modules.
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
    scored.into_iter().next().map(|(_, p)| p)
}

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
        let p = "/Users/amed301/Downloads/for mac espada/01 Diccionario strong.lexi";
        if !std::path::Path::new(p).exists() {
            return;
        }
        let e = lexicon_lookup(p, "G26").expect("lookup");
        assert!(e.is_some(), "G26 should exist in Strong lexi");
        let e = e.unwrap();
        assert_eq!(e.topic.to_uppercase(), "G26");
        assert!(!e.plain.is_empty());
    }

    #[test]
    fn interlinear_parses_hebrew_spanish_strong() {
        let raw = r#"<heb>יִרְמְיָהוּ</heb>2 <num>H3414</num>:NPHSMN <blu>Jeremías</blu> <heb>בֶּן</heb>3 <num>H1121</num> <blu>hijo</blu>"#;
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
        let p = "/Users/amed301/Downloads/for mac espada/00Interlineal-iRV 1960+.bbli";
        if !std::path::Path::new(p).exists() {
            return;
        }
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
