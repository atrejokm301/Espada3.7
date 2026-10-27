//! Bible cross-references: OpenBible SQLite + TSK-style e-Sword commentaries.

use crate::books::{book_by_number, BOOKS};
use crate::modules::{load_commentary, ModuleInfo};
use crate::text::looks_encrypted;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossRefSource {
    pub path: String,
    pub title: String,
    pub kind: String, // openbible | commentary
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossRefHit {
    pub to_book: i32,
    pub to_chapter: i32,
    pub to_verse_start: i32,
    pub to_verse_end: i32,
    pub votes: i32,
    pub label: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossRefPayload {
    pub source: String,
    pub source_path: String,
    pub source_kind: String,
    pub from_label: String,
    pub entries: Vec<CrossRefHit>,
    pub note: Option<String>,
}

/// Heuristic: commentaries that are cross-reference tools (TSK, Torrey, …).
pub fn is_cross_reference_module(m: &ModuleInfo) -> bool {
    if m.kind != "commentary" {
        return false;
    }
    let hay = format!(
        "{} {} {}",
        m.title.to_lowercase(),
        m.abbreviation.to_lowercase(),
        m.filename.to_lowercase()
    );
    const KEYS: &[&str] = &[
        "tsk",
        "tske",
        "torrey",
        "cross ref",
        "crossref",
        "cross-ref",
        "cross_reference",
        "crossreferences",
        "x-ref",
        "xref",
        "referencias cruzadas",
        "referencia cruzada",
        "parallel passage",
        "pasajes paralelos",
        "treasury of scripture",
        "treasury of scriptural",
    ];
    KEYS.iter().any(|k| hay.contains(k))
}

fn openbible_db_candidates(modules_dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(modules_dir);
    let mut out = Vec::new();
    let preferred = [
        "openbible_cross_references.sqlite",
        "openbible_cross_references.db",
        "cross_references.sqlite",
        "cross_references.db",
    ];
    for name in preferred {
        let p = dir.join(name);
        if p.is_file() {
            out.push(p);
        }
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !name.ends_with(".sqlite") && !name.ends_with(".db") {
                continue;
            }
            if name.contains("cross") || name.contains("openbible") || name.contains("xref") {
                if !out.iter().any(|x| x == &p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Discover CR data sources in the modules folder (OpenBible DB + TSK-like modules).
pub fn list_cross_reference_sources(modules: &[ModuleInfo], modules_dir: &str) -> Vec<CrossRefSource> {
    let mut out = Vec::new();
    for p in openbible_db_candidates(modules_dir) {
        out.push(CrossRefSource {
            path: p.to_string_lossy().to_string(),
            title: "OpenBible · Referencias cruzadas (CC BY)".into(),
            kind: "openbible".into(),
            encrypted: false,
        });
    }
    for m in modules.iter().filter(|m| is_cross_reference_module(m)) {
        out.push(CrossRefSource {
            path: m.path.clone(),
            title: format!("{} — {}", m.abbreviation, m.title),
            kind: "commentary".into(),
            encrypted: m.encrypted,
        });
    }
    out
}

fn format_ref_label(book: i32, chapter: i32, v1: i32, v2: i32) -> String {
    let name = book_by_number(book)
        .map(|b| b.abbr)
        .unwrap_or("?");
    if v1 == v2 {
        format!("{name} {chapter}:{v1}")
    } else {
        format!("{name} {chapter}:{v1}-{v2}")
    }
}

fn format_ref_html(book: i32, chapter: i32, v1: i32, v2: i32) -> String {
    let name = book_by_number(book)
        .map(|b| b.name)
        .unwrap_or("?");
    let abbr = book_by_number(book)
        .map(|b| b.abbr)
        .unwrap_or("?");
    let label = if v1 == v2 {
        format!("{abbr} {chapter}:{v1}")
    } else {
        format!("{abbr} {chapter}:{v1}-{v2}")
    };
    // data-book uses Spanish name so existing resolveRefElement / resolve_book_ref works
    format!(
        r#"<span class="bible-ref" data-book="{name}" data-chapter="{chapter}" data-verse="{v1}" data-verse-end="{v2}" title="Ver pasaje">{label}</span>"#
    )
}

fn load_openbible(
    db_path: &str,
    book: i32,
    chapter: i32,
    verse: i32,
    limit: i32,
) -> Result<Vec<CrossRefHit>, String> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("No se pudo abrir OpenBible: {e}"))?;

    // Ensure schema looks right
    let ok: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cross_references'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if ok == 0 {
        return Err("El archivo SQLite no tiene la tabla cross_references.".into());
    }

    let lim = limit.clamp(1, 200);
    let mut stmt = conn
        .prepare(
            "SELECT to_book, to_chapter, to_verse_start, to_verse_end, votes
             FROM cross_references
             WHERE from_book=?1 AND from_chapter=?2 AND from_verse=?3
             ORDER BY votes DESC
             LIMIT ?4",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![book, chapter, verse, lim], |row| {
            Ok((
                row.get::<_, i32>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i32>(4).unwrap_or(0),
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for r in rows.flatten() {
        let (tb, tc, v1, v2, votes) = r;
        if !(1..=66).contains(&tb) {
            continue;
        }
        out.push(CrossRefHit {
            to_book: tb,
            to_chapter: tc,
            to_verse_start: v1,
            to_verse_end: v2,
            votes,
            label: format_ref_label(tb, tc, v1, v2),
            html: format_ref_html(tb, tc, v1, v2),
        });
    }
    Ok(out)
}

/// Parse bible-ref spans out of TSK commentary HTML into discrete hits.
fn hits_from_commentary_html(html: &str) -> Vec<CrossRefHit> {
    // Attribute order can vary — extract each bible-ref span, then its data-* attrs
    static RE_SPAN: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r#"(?is)<span\s+class="bible-ref[^"]*"[^>]*>.*?</span>"#).unwrap()
    });
    static RE_BOOK: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r#"(?i)data-book="([^"]+)""#).unwrap());
    static RE_CH: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r#"(?i)data-chapter="(\d+)""#).unwrap());
    static RE_V1: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r#"(?i)data-verse="(\d+)""#).unwrap());
    static RE_V2: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r#"(?i)data-verse-end="(\d+)""#).unwrap());

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for m in RE_SPAN.find_iter(html) {
        let span = m.as_str();
        let book_raw = RE_BOOK
            .captures(span)
            .and_then(|c| c.get(1).map(|x| x.as_str().to_string()));
        let ch = RE_CH
            .captures(span)
            .and_then(|c| c.get(1).and_then(|x| x.as_str().parse().ok()));
        let v1 = RE_V1
            .captures(span)
            .and_then(|c| c.get(1).and_then(|x| x.as_str().parse().ok()));
        let v2 = RE_V2
            .captures(span)
            .and_then(|c| c.get(1).and_then(|x| x.as_str().parse().ok()))
            .or(v1);
        let (Some(book_raw), Some(ch), Some(v1), Some(v2)) = (book_raw, ch, v1, v2) else {
            continue;
        };
        let Some(tb) = crate::books::resolve_book_name(&book_raw) else {
            continue;
        };
        let key = (tb, ch, v1, v2);
        if !seen.insert(key) {
            continue;
        }
        out.push(CrossRefHit {
            to_book: tb,
            to_chapter: ch,
            to_verse_start: v1,
            to_verse_end: v2,
            votes: 0,
            label: format_ref_label(tb, ch, v1, v2),
            html: format_ref_html(tb, ch, v1, v2),
        });
    }
    out
}

fn load_from_commentary_module(
    path: &str,
    book: i32,
    chapter: i32,
    verse: i32,
) -> Result<Vec<CrossRefHit>, String> {
    let entries = load_commentary(path, book, chapter, verse, true, false, false)?;
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    // Detect encrypted blob that slipped through
    for e in &entries {
        if looks_encrypted(&e.plain) || looks_encrypted(&e.html) {
            return Err(
                "Este módulo CR está cifrado (🔒). Use un .cmti sin DRM o el archivo OpenBible SQLite."
                    .into(),
            );
        }
    }
    let mut hits = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for e in entries {
        for h in hits_from_commentary_html(&e.html) {
            let key = (h.to_book, h.to_chapter, h.to_verse_start, h.to_verse_end);
            if seen.insert(key) {
                hits.push(h);
            }
        }
        // If no parseable <ref> spans, still surface the raw HTML as one "entry"
        // via a synthetic hit with label from entry (frontend can show HTML block).
        if hits.is_empty() && !e.html.trim().is_empty() {
            hits.push(CrossRefHit {
                to_book: 0,
                to_chapter: 0,
                to_verse_start: 0,
                to_verse_end: 0,
                votes: 0,
                label: if e.label.is_empty() {
                    "Referencias".into()
                } else {
                    e.label.clone()
                },
                html: e.html.clone(),
            });
        }
    }
    Ok(hits)
}

pub fn load_cross_references(
    modules: &[ModuleInfo],
    modules_dir: &str,
    book: i32,
    chapter: i32,
    verse: i32,
    preferred_path: Option<&str>,
    limit: i32,
) -> Result<CrossRefPayload, String> {
    let sources = list_cross_reference_sources(modules, modules_dir);
    if sources.is_empty() {
        return Ok(CrossRefPayload {
            source: String::new(),
            source_path: String::new(),
            source_kind: String::new(),
            from_label: format_ref_label(book, chapter, verse, verse),
            entries: vec![],
            note: Some(
                "No hay datos de referencias cruzadas. Coloque openbible_cross_references.sqlite \
                 en la carpeta de módulos, o importe un módulo TSK/TSKe en formato .cmti (no .cmtx cifrado)."
                    .into(),
            ),
        });
    }

    // Pick source: preferred if valid, else first non-encrypted, else first
    let chosen = preferred_path
        .and_then(|p| sources.iter().find(|s| s.path == p))
        .or_else(|| sources.iter().find(|s| !s.encrypted))
        .or_else(|| sources.first())
        .cloned()
        .ok_or_else(|| "Sin fuente CR".to_string())?;

    if chosen.encrypted {
        return Ok(CrossRefPayload {
            source: chosen.title,
            source_path: chosen.path,
            source_kind: chosen.kind,
            from_label: format_ref_label(book, chapter, verse, verse),
            entries: vec![],
            note: Some(
                "El módulo TSK disponible (p. ej. tsk.cmtx) está cifrado por e-Sword y no se puede leer. \
                 Use openbible_cross_references.sqlite (ya generado en la carpeta de módulos) \
                 o un TSKe.cmti sin DRM."
                    .into(),
            ),
        });
    }

    let entries = match chosen.kind.as_str() {
        "openbible" => load_openbible(&chosen.path, book, chapter, verse, limit)?,
        "commentary" => load_from_commentary_module(&chosen.path, book, chapter, verse)?,
        _ => Vec::new(),
    };

    let note = if entries.is_empty() {
        Some(format!(
            "Sin referencias para {} en «{}».",
            format_ref_label(book, chapter, verse, verse),
            chosen.title
        ))
    } else {
        None
    };

    // Silence unused BOOKS warning if any — book_by_number uses it
    let _ = BOOKS.len();

    Ok(CrossRefPayload {
        source: chosen.title,
        source_path: chosen.path,
        source_kind: chosen.kind,
        from_label: format_ref_label(book, chapter, verse, verse),
        entries,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openbible_john_3_16_has_hits() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/openbible_cross_references.sqlite"
        );
        if !std::path::Path::new(path).is_file() {
            eprintln!("skip: no openbible db at {path}");
            return;
        }
        let hits = load_openbible(path, 43, 3, 16, 10).expect("load");
        assert!(!hits.is_empty(), "expected CR hits for John 3:16");
        // Top voted should include Romans 5:8 in typical OpenBible data
        assert!(hits.iter().any(|h| h.to_book == 45 && h.to_chapter == 5));
        assert!(hits[0].votes >= hits.last().unwrap().votes);
    }

    #[test]
    fn cr_module_heuristic() {
        let tsk = ModuleInfo {
            path: "/tmp/tsk.cmti".into(),
            filename: "tsk.cmti".into(),
            title: "Treasury of Scripture Knowledge".into(),
            abbreviation: "TSK".into(),
            kind: "commentary".into(),
            encrypted: false,
            has_strongs: false,
            version: 4,
        };
        let plain = ModuleInfo {
            path: "/tmp/mh.cmti".into(),
            filename: "matthew_henry.cmti".into(),
            title: "Matthew Henry".into(),
            abbreviation: "MH".into(),
            kind: "commentary".into(),
            encrypted: false,
            has_strongs: false,
            version: 4,
        };
        assert!(is_cross_reference_module(&tsk));
        assert!(!is_cross_reference_module(&plain));
    }
}
