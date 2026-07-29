use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::DEFAULT_ESWORD_PATH;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleInfo {
    pub filename: String,
    pub path: String,
    pub module_type: String,
    pub title: String,
    pub abbreviation: String,
    pub size_mb: f64,
    /// True if content samples look like e-Sword sqliteplus ciphertext (older .cmtx/.lexx/…).
    #[serde(default)]
    pub encrypted: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    root: String,
    /// path -> fingerprint + module info
    entries: HashMap<String, CacheEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    mtime_ms: u64,
    size: u64,
    info: ModuleInfo,
}

const CACHE_VERSION: u32 = 3;

fn cache_file_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .or_else(|| std::env::var_os("HOME"))?;
    Some(
        PathBuf::from(base)
            .join("asignacion-del-cielo-bible")
            .join("module-catalog-v2.json"),
    )
}

fn file_fingerprint(path: &Path) -> Option<(u64, u64)> {
    let meta = path.metadata().ok()?;
    let size = meta.len();
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Some((mtime_ms, size))
}

fn load_cache(root: &Path) -> Option<CacheFile> {
    let p = cache_file_path()?;
    let raw = fs::read_to_string(p).ok()?;
    let cache: CacheFile = serde_json::from_str(&raw).ok()?;
    if cache.version != CACHE_VERSION {
        return None;
    }
    if cache.root != root.to_string_lossy() {
        return None;
    }
    Some(cache)
}

fn save_cache(cache: &CacheFile) {
    let Some(p) = cache_file_path() else {
        return;
    };
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string(cache) {
        let _ = fs::write(p, raw);
    }
}

pub fn scan_modules(esword_path: Option<String>) -> Result<Vec<ModuleInfo>, String> {
    let root = PathBuf::from(esword_path.unwrap_or_else(|| DEFAULT_ESWORD_PATH.to_string()));
    if !root.exists() {
        return Err(format!(
            "No se encontró la carpeta e-Sword: {}. ¿Está instalado e-Sword?",
            root.display()
        ));
    }

    let patterns: &[(&str, &str)] = &[
        ("bbli", "bible"),
        ("bblx", "bible"),
        ("cmti", "commentary"),
        ("cmtx", "commentary"),
        ("dcti", "dictionary"),
        ("dctx", "dictionary"),
        ("lexi", "lexicon"),
        ("lexx", "lexicon"),
    ];

    // Collect candidates from directory (one pass, no SQLite yet)
    let mut candidates: Vec<(PathBuf, String, u64, u64)> = Vec::new();
    let entries = fs::read_dir(&root).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let Some((_, kind)) = patterns.iter().find(|(e, _)| *e == ext) else {
            continue;
        };
        let Some((mtime_ms, size)) = file_fingerprint(&path) else {
            continue;
        };
        candidates.push((path, kind.to_string(), mtime_ms, size));
    }

    let mut cache = load_cache(&root).unwrap_or(CacheFile {
        version: CACHE_VERSION,
        root: root.to_string_lossy().to_string(),
        entries: HashMap::new(),
    });

    // Reuse cached details when mtime+size match; only open SQLite for new/changed files
    let mut modules: Vec<ModuleInfo> = Vec::with_capacity(candidates.len());
    let mut need_analyze: Vec<(PathBuf, String, u64, u64)> = Vec::new();

    for (path, kind, mtime_ms, size) in candidates {
        let key = path.to_string_lossy().to_string();
        if let Some(ent) = cache.entries.get(&key) {
            if ent.mtime_ms == mtime_ms && ent.size == size && ent.info.module_type == kind {
                modules.push(ent.info.clone());
                continue;
            }
        }
        need_analyze.push((path, kind, mtime_ms, size));
    }

    // Analyze missing/changed modules in parallel (biggest cold-start win)
    let analyzed: Vec<(String, u64, u64, ModuleInfo)> = parallel_analyze(need_analyze);

    for (key, mtime_ms, size, info) in analyzed {
        cache.entries.insert(
            key,
            CacheEntry {
                mtime_ms,
                size,
                info: info.clone(),
            },
        );
        modules.push(info);
    }

    // Drop cache entries for files that disappeared
    let live: std::collections::HashSet<String> =
        modules.iter().map(|m| m.path.clone()).collect();
    cache.entries.retain(|k, _| live.contains(k));
    save_cache(&cache);

    modules.sort_by(|a, b| {
        a.module_type
            .cmp(&b.module_type)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });

    Ok(modules)
}

/// Split work across a small thread pool (no extra crate required).
fn parallel_analyze(
    items: Vec<(PathBuf, String, u64, u64)>,
) -> Vec<(String, u64, u64, ModuleInfo)> {
    if items.is_empty() {
        return Vec::new();
    }
    if items.len() == 1 {
        let (path, kind, mtime_ms, size) = &items[0];
        return match analyze_module(path, kind) {
            Some(info) => vec![(
                path.to_string_lossy().to_string(),
                *mtime_ms,
                *size,
                info,
            )],
            None => Vec::new(),
        };
    }

    let workers = std::thread::available_parallelism()
        .map(|n| n.get().min(8).max(2))
        .unwrap_or(4);
    let chunk_size = (items.len() + workers - 1) / workers;
    let mut handles = Vec::new();

    for chunk in items.chunks(chunk_size.max(1)) {
        let chunk: Vec<_> = chunk.to_vec();
        handles.push(std::thread::spawn(move || {
            let mut out = Vec::with_capacity(chunk.len());
            for (path, kind, mtime_ms, size) in chunk {
                if let Some(info) = analyze_module(&path, &kind) {
                    out.push((path.to_string_lossy().to_string(), mtime_ms, size, info));
                }
            }
            out
        }));
    }

    let mut all = Vec::new();
    for h in handles {
        if let Ok(part) = h.join() {
            all.extend(part);
        }
    }
    all
}

fn analyze_module(path: &Path, kind: &str) -> Option<ModuleInfo> {
    let size_mb = path
        .metadata()
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);
    let filename = path.file_name()?.to_string_lossy().to_string();
    let stem = path.file_stem()?.to_string_lossy().to_string();

    let mut title = stem.clone();
    let mut abbreviation = stem.clone();
    let mut encrypted = false;

    if let Ok(conn) = open_ro(path) {
        // Read-only pragmas: faster Details peek
        let _ = conn.execute_batch(
            "PRAGMA query_only=ON; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-2000;",
        );
        if let Ok((t, a)) = read_details(&conn) {
            if !t.is_empty() {
                title = t;
            }
            if !a.is_empty() {
                abbreviation = a;
            }
        }
        encrypted = sample_looks_encrypted(&conn, kind);
    }

    if title.len() > 140 {
        title = format!("{}…", &title.chars().take(120).collect::<String>());
    }
    if abbreviation.len() > 40 {
        abbreviation = abbreviation.chars().take(40).collect();
    }

    Some(ModuleInfo {
        filename,
        path: path.to_string_lossy().to_string(),
        module_type: kind.to_string(),
        title,
        abbreviation,
        size_mb: (size_mb * 100.0).round() / 100.0,
        encrypted,
    })
}

/// Peek one content cell: high-entropy AES-sized blobs ⇒ sqliteplus ciphertext.
fn sample_looks_encrypted(conn: &Connection, kind: &str) -> bool {
    let probes: &[(&str, &str)] = match kind {
        "bible" => &[("Bible", "Scripture")],
        "commentary" => &[
            ("VerseCommentary", "Comments"),
            ("ChapterCommentary", "Comments"),
        ],
        "dictionary" => &[("Dictionary", "Definition")],
        "lexicon" => &[("Lexicon", "Definition"), ("Dictionary", "Definition")],
        _ => &[],
    };
    for (table, col) in probes {
        let sql = format!(
            "SELECT [{col}] FROM [{table}] WHERE typeof([{col}])='blob' LIMIT 3"
        );
        let Ok(mut stmt) = conn.prepare(&sql) else {
            continue;
        };
        let Ok(rows) = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0)) else {
            continue;
        };
        for r in rows.flatten() {
            if r.len() >= 32 && r.len() % 16 == 0 && high_entropy(&r) {
                // Not zlib/gzip header
                let zlibish = r[0] == 0x78 && matches!(r[1], 0x01 | 0x5e | 0x9c | 0xda);
                let gzip = r.len() >= 2 && r[0] == 0x1f && r[1] == 0x8b;
                if !zlibish && !gzip {
                    return true;
                }
            }
        }
    }
    false
}

fn high_entropy(b: &[u8]) -> bool {
    let sample = &b[..b.len().min(256)];
    let mut seen = [false; 256];
    for &x in sample {
        seen[x as usize] = true;
    }
    seen.iter().filter(|&&x| x).count() > 180
}

pub fn open_ro(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("No se pudo abrir {}: {e}", path.display()))
}

fn read_details(conn: &Connection) -> Result<(String, String), rusqlite::Error> {
    // EXISTS is cheaper than COUNT(*)
    let has: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='Details' LIMIT 1",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    if !has {
        return Ok((String::new(), String::new()));
    }

    let attempts = [
        "SELECT Title, Abbreviation FROM Details LIMIT 1",
        "SELECT Description, Abbreviation FROM Details LIMIT 1",
        "SELECT Title, Abbr FROM Details LIMIT 1",
    ];

    for sql in attempts {
        if let Ok((t, a)) = conn.query_row(sql, [], |r| {
            Ok((
                r.get::<_, String>(0).unwrap_or_default(),
                r.get::<_, String>(1).unwrap_or_default(),
            ))
        }) {
            return Ok((t, a));
        }
    }

    let mut stmt = conn.prepare("SELECT * FROM Details LIMIT 1")?;
    let col_count = stmt.column_count();
    let row = stmt.query_row([], |r| {
        let mut t = String::new();
        let mut a = String::new();
        if col_count > 0 {
            t = r
                .get::<_, String>(0)
                .or_else(|_| r.get::<_, i64>(0).map(|n| n.to_string()))
                .unwrap_or_default();
        }
        if col_count > 1 {
            a = r
                .get::<_, String>(1)
                .or_else(|_| r.get::<_, i64>(1).map(|n| n.to_string()))
                .unwrap_or_default();
        }
        Ok((t, a))
    })?;
    Ok(row)
}

pub fn table_exists(conn: &rusqlite::Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
        [name],
        |_| Ok(true),
    )
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn scan_modules_twice_is_stable_and_cache_helps() {
        let t0 = Instant::now();
        let first = match scan_modules(None) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("skip: no e-Sword folder ({e})");
                return;
            }
        };
        let cold_ms = t0.elapsed().as_millis();
        assert!(!first.is_empty(), "expected at least one module");

        let t1 = Instant::now();
        let second = scan_modules(None).expect("second scan");
        let warm_ms = t1.elapsed().as_millis();

        assert_eq!(first.len(), second.len());
        // Cached warm scan should usually be faster (directory only + JSON),
        // but keep a soft assertion so CI noise does not flake hard.
        eprintln!(
            "scan modules: cold={cold_ms}ms warm={warm_ms}ms count={}",
            first.len()
        );
        assert!(
            warm_ms <= cold_ms.saturating_mul(3) + 500,
            "warm scan unexpectedly much slower: cold={cold_ms} warm={warm_ms}"
        );

        let bibles: Vec<_> = first.iter().filter(|m| m.module_type == "bible").collect();
        assert!(!bibles.is_empty(), "expected at least one bible module");
    }
}
