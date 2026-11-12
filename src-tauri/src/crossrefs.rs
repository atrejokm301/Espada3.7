//! Bible cross-references: OpenBible SQLite + TSK-style e-Sword commentaries.

use crate::books::{book_by_number, BOOKS};
use crate::modules::{load_commentary, ModuleInfo, StudyEntry};
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

/// Opt-in behaviours whose defaults depend on open product questions (plan
/// questions_for_user Q1) or that would shorten verse-precise lists. `Default` keeps today's
/// behaviour. `recommended()` is the audit plan's proposal (ISS-39 steps 3-4, ISS-42).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CrossRefOptions {
    /// ISS-42: also offer the Spanish TSK ("El Tesoro del Conocimiento Bíblico", TCB) and the
    /// Thompson reference Bible as sources.
    pub include_spanish_sources: bool,
    /// ISS-39 step 4: list verse-granular sources first (OpenBible, TSK/TSKe, TCB, then the rest),
    /// so the UI's "first non-encrypted" default becomes TSKe instead of NASB.
    pub verse_granular_first: bool,
    /// ISS-39 step 3: also apply `limit` (after dedupe) to commentary sources. When off,
    /// commentary lists are complete; TSKe has up to ~180 verse-precise refs on dense verses.
    pub limit_commentary: bool,
}

impl CrossRefOptions {
    /// The audit plan's proposed behaviour (all opt-ins on).
    // Not called inside the crate until lib.rs (WP5) exposes the opt-ins.
    #[allow(dead_code)]
    pub fn recommended() -> Self {
        Self {
            include_spanish_sources: true,
            verse_granular_first: true,
            limit_commentary: true,
        }
    }
}

/// Shown when the chosen source is an encrypted (DRM) e-Sword module (ISS-41: the
/// OpenBible database is NOT shipped in the modules folder, so don't claim it is).
const ENCRYPTED_SOURCE_NOTE: &str = "El módulo TSK disponible (p. ej. tsk.cmtx) está cifrado por e-Sword y no se puede leer. \
     Puede copiar openbible_cross_references.sqlite a la carpeta de módulos \
     o usar un TSKe.cmti sin DRM.";

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

/// `is_cross_reference_module` plus the opt-in Spanish sources (ISS-42). Uses only specific
/// keywords: a bare "referencias" would match ordinary commentaries (RVR1960x, TNM notes).
pub fn is_cross_reference_module_with(m: &ModuleInfo, opts: CrossRefOptions) -> bool {
    if is_cross_reference_module(m) {
        return true;
    }
    if !opts.include_spanish_sources || m.kind != "commentary" {
        return false;
    }
    let hay = format!("{} {}", m.title.to_lowercase(), m.filename.to_lowercase());
    hay.contains("tesoro del conocimiento")
        || hay.contains("referencia thompson")
        || m.abbreviation.trim().eq_ignore_ascii_case("tcb")
}

/// ISS-39 step 4 ranking: 0 OpenBible, 1 TSK/TSKe, 2 TCB, 3 everything else.
fn source_rank(kind: &str, m: Option<&ModuleInfo>) -> u8 {
    if kind == "openbible" {
        return 0;
    }
    let Some(m) = m else { return 3 };
    let hay = format!(
        "{} {} {}",
        m.title.to_lowercase(),
        m.abbreviation.to_lowercase(),
        m.filename.to_lowercase()
    );
    if hay.contains("tsk") || hay.contains("treasury of scripture") {
        1
    } else if hay.contains("tesoro del conocimiento") || m.abbreviation.trim().eq_ignore_ascii_case("tcb") {
        2
    } else {
        3
    }
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
    list_cross_reference_sources_with(modules, modules_dir, CrossRefOptions::default())
}

/// `list_cross_reference_sources` with opt-in detection (ISS-42) and ordering (ISS-39 step 4).
/// With `verse_granular_first` off the order is today's: OpenBible files, then modules in
/// scan order.
pub fn list_cross_reference_sources_with(
    modules: &[ModuleInfo],
    modules_dir: &str,
    opts: CrossRefOptions,
) -> Vec<CrossRefSource> {
    let mut out: Vec<(u8, CrossRefSource)> = Vec::new();
    for p in openbible_db_candidates(modules_dir) {
        out.push((
            source_rank("openbible", None),
            CrossRefSource {
                path: p.to_string_lossy().to_string(),
                title: "OpenBible · Referencias cruzadas (CC BY)".into(),
                kind: "openbible".into(),
                encrypted: false,
            },
        ));
    }
    for m in modules.iter().filter(|m| is_cross_reference_module_with(m, opts)) {
        out.push((
            source_rank("commentary", Some(m)),
            CrossRefSource {
                path: m.path.clone(),
                title: format!("{} — {}", m.abbreviation, m.title),
                kind: "commentary".into(),
                encrypted: m.encrypted,
            },
        ));
    }
    if opts.verse_granular_first {
        // Stable: equal ranks keep scan order.
        out.sort_by_key(|(rank, _)| *rank);
    }
    out.into_iter().map(|(_, s)| s).collect()
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
        // ISS-41: OpenBible stores ranges that cross a chapter end as a reversed pair
        // (Gn 11:32 – 12:5 arrives as 11:32-5). Show and navigate to the start verse.
        let v2 = if v2 < v1 { v1 } else { v2 };
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

// Attribute order can vary — extract each bible-ref span, then its data-* attrs.
// RE_SPAN depends on text.rs emitting `class="bible-ref…"` as the FIRST attribute
// (guarded by the snapshot tests).
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

/// (book, chapter, verse_start, verse_end), or (chapter_begin, verse_begin, chapter_end, verse_end)
/// for a commentary row span.
type Span = (i32, i32, i32, i32);

/// (book, chapter, verse_start, verse_end) of one rendered bible-ref span, if it resolves.
fn ref_from_span(span: &str) -> Option<Span> {
    let book_raw = RE_BOOK.captures(span).and_then(|c| c.get(1).map(|x| x.as_str().to_string()));
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
        return None;
    };
    let tb = crate::books::resolve_book_name(&book_raw)?;
    Some((tb, ch, v1, v2))
}

fn hit_from_ref((tb, ch, v1, v2): Span) -> CrossRefHit {
    CrossRefHit {
        to_book: tb,
        to_chapter: ch,
        to_verse_start: v1,
        to_verse_end: v2,
        votes: 0,
        label: format_ref_label(tb, ch, v1, v2),
        html: format_ref_html(tb, ch, v1, v2),
    }
}

/// Parse bible-ref spans out of TSK commentary HTML into discrete hits.
fn hits_from_commentary_html(html: &str) -> Vec<CrossRefHit> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in RE_SPAN.find_iter(html) {
        let Some(key) = ref_from_span(m.as_str()) else {
            continue;
        };
        if !seen.insert(key) {
            continue;
        }
        out.push(hit_from_ref(key));
    }
    out
}

/// ISS-39 step 1. NASB-style multi-verse rows are laid out as
///   `[n] <ref>anchor</ref> footnote text`  and  `(n) <ref>Book c:v[-v2]</ref>: <ref>…</ref>; …`.
/// Returns the refs of the `(n)` paragraphs whose anchor covers book/chapter/verse. It drops
/// the anchor ref itself, every `[n]` footnote paragraph, and refs before the first marker.
/// A `(n)` marker counts as an anchor only when its ref is followed by ':'.
/// Returns None when the html has no `(n)` anchors (an unstructured row).
fn anchored_hits(html: &str, book: i32, chapter: i32, verse: i32) -> Option<Vec<CrossRefHit>> {
    static RE_TAG: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"<[^>]*>").unwrap());
    // A marker right before the span. Only whitespace (NBSP included) or a short run of
    // letters may sit in between: NASB writes `(1)  Song of <ref>Son 2:1</ref>:`.
    static RE_MARKER: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r"(?:\((\d{1,3})\)|\[(\d{1,3})\])\s*(?:\p{L}[\p{L}\s.]{0,24})?$").unwrap()
    });
    // A `(n)` anchor ref is followed by ':' (closing tags/whitespace may sit in between). This
    // keeps prose counts such as Thompson's "El creador (1) <ref>Sal 104:5</ref>." from being
    // read as anchors, which would empty those rows.
    static RE_ANCHOR_TAIL: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"^(?:\s|</[^>]*>)*:").unwrap());
    let mut any_anchor = false;
    let mut keep = false;
    let mut prev = 0;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in RE_SPAN.find_iter(html) {
        let gap = RE_TAG.replace_all(&html[prev..m.start()], "");
        prev = m.end();
        if let Some(c) = RE_MARKER.captures(&gap) {
            if c.get(1).is_none() {
                // `[n]` textual footnote paragraph.
                keep = false;
                continue;
            }
            if RE_ANCHOR_TAIL.is_match(&html[m.end()..]) {
                // `(n)` anchor: its paragraph belongs to the verse(s) the anchor names.
                any_anchor = true;
                keep = matches!(
                    ref_from_span(m.as_str()),
                    Some((tb, ch, v1, v2)) if tb == book && ch == chapter && v1 <= verse && verse <= v2.max(v1)
                );
                continue;
            }
            // `(n)` not followed by ':' is prose, and this span is an ordinary ref.
        }
        if !keep {
            continue;
        }
        if let Some(key) = ref_from_span(m.as_str()) {
            if seen.insert(key) {
                out.push(hit_from_ref(key));
            }
        }
    }
    any_anchor.then_some(out)
}

/// Row span from load_commentary's verse-scope label: "v.N", "v.A–B" or "C:V–C2:V2".
/// Returns None for any other shape; such rows are treated as single-verse (left untouched).
fn entry_span(label: &str, chapter: i32) -> Option<Span> {
    static RE_ONE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"^v\.(\d+)$").unwrap());
    static RE_VV: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"^v\.(\d+)\s*[–-]\s*(\d+)$").unwrap());
    static RE_CVCV: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r"^(\d+):(\d+)\s*[–-]\s*(\d+):(\d+)$").unwrap()
    });
    let l = label.trim();
    let n = |c: &regex::Captures, i: usize| c[i].parse::<i32>().ok();
    if let Some(c) = RE_ONE.captures(l) {
        let v = n(&c, 1)?;
        return Some((chapter, v, chapter, v));
    }
    if let Some(c) = RE_VV.captures(l) {
        return Some((chapter, n(&c, 1)?, chapter, n(&c, 2)?));
    }
    if let Some(c) = RE_CVCV.captures(l) {
        return Some((n(&c, 1)?, n(&c, 2)?, n(&c, 3)?, n(&c, 4)?));
    }
    None
}

/// Turn the verse-scope commentary rows matched for book/chapter/verse into panel hits.
/// The result does not depend on row order (ISS-39 step 2):
/// - single-verse rows (TSKe, TCB verse rows) contribute all their refs, as before;
/// - structured multi-verse rows (NASB chapter rows) contribute only the paragraphs anchored
///   on this verse (ISS-39 step 1);
/// - unstructured multi-verse rows (TCB chapter outlines, Thompson book intros) are used only
///   when nothing verse-precise exists;
/// - the raw-HTML block appears only when there are no hits at all.
fn collect_commentary_hits(entries: &[StudyEntry], book: i32, chapter: i32, verse: i32) -> Vec<CrossRefHit> {
    // Stable order: precise rows first (input order kept), then multi-verse rows by span.
    let mut order: Vec<(u8, Span, usize)> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| match entry_span(&e.label, chapter) {
            Some(sp) if (sp.0, sp.1) != (sp.2, sp.3) => (1, sp, i),
            _ => (0, (0, 0, 0, 0), i),
        })
        .collect();
    order.sort_by_key(|(multi, sp, _)| (*multi, *sp));

    let mut precise = Vec::new();
    let mut loose = Vec::new();
    let mut raw_candidate: Option<&StudyEntry> = None;
    for &(multi, _, i) in &order {
        let e = &entries[i];
        if multi == 1 {
            if let Some(h) = anchored_hits(&e.html, book, chapter, verse) {
                precise.extend(h);
                continue;
            }
            loose.extend(hits_from_commentary_html(&e.html));
        } else {
            precise.extend(hits_from_commentary_html(&e.html));
        }
        if raw_candidate.is_none() && !e.html.trim().is_empty() {
            raw_candidate = Some(e);
        }
    }

    let dedupe = |v: Vec<CrossRefHit>| {
        let mut seen = std::collections::HashSet::new();
        v.into_iter()
            .filter(|h| seen.insert((h.to_book, h.to_chapter, h.to_verse_start, h.to_verse_end)))
            .collect::<Vec<_>>()
    };
    let hits = dedupe(precise);
    if !hits.is_empty() {
        return hits;
    }
    let hits = dedupe(loose);
    if !hits.is_empty() {
        return hits;
    }
    // No parseable refs anywhere: surface one raw HTML block (the frontend shows it as-is).
    raw_candidate
        .map(|e| {
            vec![CrossRefHit {
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
            }]
        })
        .unwrap_or_default()
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
    Ok(collect_commentary_hits(&entries, book, chapter, verse))
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
    load_cross_references_with(
        modules,
        modules_dir,
        book,
        chapter,
        verse,
        preferred_path,
        limit,
        CrossRefOptions::default(),
    )
}

/// `load_cross_references` with opt-in source detection and ordering, and an opt-in
/// commentary limit (see `CrossRefOptions`). The verse-precise filtering of multi-verse rows
/// (ISS-39 steps 1-2) applies in both.
#[allow(clippy::too_many_arguments)]
pub fn load_cross_references_with(
    modules: &[ModuleInfo],
    modules_dir: &str,
    book: i32,
    chapter: i32,
    verse: i32,
    preferred_path: Option<&str>,
    limit: i32,
    opts: CrossRefOptions,
) -> Result<CrossRefPayload, String> {
    let sources = list_cross_reference_sources_with(modules, modules_dir, opts);
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
            note: Some(ENCRYPTED_SOURCE_NOTE.into()),
        });
    }

    let entries = match chosen.kind.as_str() {
        "openbible" => load_openbible(&chosen.path, book, chapter, verse, limit)?,
        "commentary" => {
            let mut hits = load_from_commentary_module(&chosen.path, book, chapter, verse)?;
            if opts.limit_commentary {
                // After dedupe (collect_commentary_hits already deduped), same clamp as OpenBible.
                hits.truncate(limit.clamp(1, 200) as usize);
            }
            hits
        }
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
mod snapshot_tests;

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

    // ---- WP6 regression tests (synthetic, no library needed) ----

    fn cmt(filename: &str, title: &str, abbr: &str, encrypted: bool) -> ModuleInfo {
        ModuleInfo {
            path: format!("/nonexistent-espada-test/{filename}"),
            filename: filename.into(),
            title: title.into(),
            abbreviation: abbr.into(),
            kind: "commentary".into(),
            encrypted,
            has_strongs: false,
            version: 4,
        }
    }

    fn entry(label: &str, raw: &str) -> StudyEntry {
        let html = crate::text::clean_esword_text(raw);
        StudyEntry {
            scope: "verse".into(),
            label: label.into(),
            plain: html.clone(),
            html,
        }
    }

    fn targets(h: &[CrossRefHit]) -> Vec<(i32, i32, i32, i32)> {
        h.iter()
            .map(|e| (e.to_book, e.to_chapter, e.to_verse_start, e.to_verse_end))
            .collect()
    }

    /// Shape of a real NASB chapter row (nasb.cmti, John 3), trimmed.
    const NASB_LIKE: &str = r#"<p><span style="font-weight:bold;">FOOTNOTES</span></p><p><span style="color:#4D80B3;">[1]</span>  <ref>Joh 3:16</ref> Or believes in Him <ref>Joh 5:24</ref></p><p><span style="font-weight:bold;">CROSS-REFERENCES</span></p><p><span style="color:#804DB3;text-decoration:underline ;">(1)</span>  <ref>Joh 3:1</ref>: <ref>Joh 7:50</ref>; 19:39</p><p><span style="color:#804DB3;text-decoration:underline ;">(3)</span>  <ref>Joh 3:2</ref>: <ref>Mat 23:7</ref>; <ref>Joh 3:26</ref></p><p><span style="color:#804DB3;text-decoration:underline ;">(23)</span>  <ref>Joh 3:16</ref>: <ref>Rom 5:8</ref>; <ref>Eph 2:4</ref></p><p><span style="color:#804DB3;text-decoration:underline ;">(24)</span>  <ref>Joh 3:16-17</ref>: <ref>Rom 8:32</ref>; <ref>1Jn 4:9</ref></p><p><span style="color:#804DB3;text-decoration:underline ;">(27)</span>  <ref>Joh 3:17</ref>: <ref>Luk 19:10</ref>; <ref>Rom 5:8</ref></p>"#;

    #[test]
    fn multi_verse_rows_keep_only_paragraphs_anchored_on_the_verse() {
        let e = vec![entry("v.1–36", NASB_LIKE)];
        let h = collect_commentary_hits(&e, 43, 3, 16);
        assert_eq!(
            targets(&h),
            vec![(45, 5, 8, 8), (49, 2, 4, 4), (45, 8, 32, 32), (62, 4, 9, 9)],
            "only the (23) and (24) paragraphs, anchor and [1] footnote dropped"
        );
        // An anchor range covers every verse in it; v.17 gets (24) and (27).
        let h17 = collect_commentary_hits(&e, 43, 3, 17);
        assert_eq!(
            targets(&h17),
            vec![(45, 8, 32, 32), (62, 4, 9, 9), (42, 19, 10, 10), (45, 5, 8, 8)]
        );
        // A verse with no anchor paragraph: nothing, and no raw chapter block.
        assert!(collect_commentary_hits(&e, 43, 3, 30).is_empty());
        // Anchors must name the requested book and chapter.
        assert!(collect_commentary_hits(&e, 43, 4, 16).is_empty());
    }

    #[test]
    fn anchors_with_a_book_name_prefix_are_recognised() {
        // nasb.cmti, Song 2 (real shape): the anchor reads "Song of <ref>Son 2:1</ref>:".
        let raw = r#"<p><span style="color:#4D80B3;">[1]</span>  Song of <ref>Son 2:1</ref> BRIDE</p><p><span style="color:#804DB3;text-decoration:underline ;">(1)</span>  Song of <ref>Son 2:1</ref>: <ref>Isa 35:1</ref></p><p><span style="color:#804DB3;text-decoration:underline ;">(4)</span>  Song of <ref>Son 2:2</ref>: <ref>Son 1:9</ref></p>"#;
        let e = vec![entry("v.1–17", raw)];
        assert_eq!(targets(&collect_commentary_hits(&e, 22, 2, 1)), vec![(23, 35, 1, 1)]);
        assert_eq!(targets(&collect_commentary_hits(&e, 22, 2, 2)), vec![(22, 1, 9, 9)]);
    }

    #[test]
    fn single_verse_rows_are_untouched() {
        // The same html in a single-verse row keeps every ref, exactly as before WP6.
        let e = vec![entry("v.16", NASB_LIKE)];
        assert_eq!(
            targets(&collect_commentary_hits(&e, 43, 3, 16)),
            targets(&hits_from_commentary_html(&e[0].html))
        );
        // Unknown label shapes count as single-verse (never filtered).
        let e = vec![entry("Cap. 3", NASB_LIKE)];
        assert_eq!(
            targets(&collect_commentary_hits(&e, 43, 3, 16)),
            targets(&hits_from_commentary_html(&e[0].html))
        );
    }

    #[test]
    fn row_order_does_not_change_the_result() {
        // TCB-like: a chapter-outline row, a verse row with refs, and a text-only verse row.
        let outline = entry("v.1–36", "Nicodemo, <ref>Jua 3:1-13</ref>. Amor, <ref>Jua 3:16</ref>.");
        let verse = entry("v.16", "porque de tal amó. <ref>Luc 2:14</ref>; <ref>Rom 5:8</ref>");
        let text = entry("v.16", "Solo texto, sin referencias.");
        let a = collect_commentary_hits(&[outline.clone(), verse.clone(), text.clone()], 43, 3, 16);
        let b = collect_commentary_hits(&[text.clone(), verse.clone(), outline.clone()], 43, 3, 16);
        let c = collect_commentary_hits(&[verse.clone(), outline.clone(), text.clone()], 43, 3, 16);
        assert_eq!(targets(&a), vec![(42, 2, 14, 14), (45, 5, 8, 8)]);
        assert_eq!(targets(&a), targets(&b));
        assert_eq!(targets(&a), targets(&c));
        assert!(a.iter().all(|h| h.to_book > 0), "no raw block next to real hits");
    }

    #[test]
    fn unstructured_multi_verse_rows_are_only_a_fallback() {
        let outline = entry("v.1–36", "Nicodemo, <ref>Jua 3:1-13</ref>. Amor, <ref>Jua 3:16</ref>.");
        let text = entry("v.16", "Solo texto, sin referencias.");
        // With no verse-precise refs, the outline's refs are used (today's behaviour).
        let h = collect_commentary_hits(&[text.clone(), outline.clone()], 43, 3, 16);
        assert_eq!(targets(&h), vec![(43, 3, 1, 13), (43, 3, 16, 16)]);
        // Nothing parseable anywhere: one raw block, from the verse row first.
        let h = collect_commentary_hits(&[entry("v.1–36", "Introducción."), text.clone()], 43, 3, 16);
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].to_book, h[0].label.as_str()), (0, "v.16"));
        assert!(h[0].html.contains("Solo texto"));
        // Empty rows never produce a block.
        assert!(collect_commentary_hits(&[], 43, 3, 16).is_empty());
    }

    #[test]
    fn entry_span_parses_load_commentary_labels() {
        assert_eq!(entry_span("v.16", 3), Some((3, 16, 3, 16)));
        assert_eq!(entry_span("v.1–36", 3), Some((3, 1, 3, 36)));
        assert_eq!(entry_span("v.1-36", 3), Some((3, 1, 3, 36)));
        assert_eq!(entry_span("2:23–3:5", 3), Some((2, 23, 3, 5)));
        assert_eq!(entry_span("Cap. 3", 3), None);
        assert_eq!(entry_span("", 3), None);
    }

    #[test]
    fn hits_from_commentary_html_shape_is_unchanged() {
        // Refactor guard on literal span html (independent of text.rs): dedupe, unresolvable
        // books and raw spans skipped, order kept, a missing data-verse-end means v1.
        let span = |b: &str, c: i32, v1: i32, tail: &str| {
            format!(r#"<span class="bible-ref" data-book="{b}" data-chapter="{c}" data-verse="{v1}"{tail} title="Ver pasaje">x</span>"#)
        };
        let html = [
            span("Rom", 5, 8, r#" data-verse-end="8""#),
            span("Xyz", 1, 1, r#" data-verse-end="1""#),
            span("Rom", 5, 8, r#" data-verse-end="8""#),
            r#"<span class="bible-ref bible-ref-raw" data-raw="Joh 3:16, 18" title="Ver pasaje">Joh 3:16, 18</span>"#.to_string(),
            // WP2/ISS-25 cross-chapter shape: no data-verse-end, extra attrs after it.
            span("Joh", 3, 16, r#" data-chapter-end="4" data-verse-end2="2""#),
            span("Gén", 1, 1, r#" data-verse-end="2""#),
        ]
        .join(" ");
        let h = hits_from_commentary_html(&html);
        assert_eq!(targets(&h), vec![(45, 5, 8, 8), (43, 3, 16, 16), (1, 1, 1, 2)]);
        assert_eq!(h[2].label, "Gn 1:1-2");
        assert!(h[2].html.starts_with(r#"<span class="bible-ref" data-book="#));
    }

    #[test]
    fn openbible_reversed_range_renders_the_start_verse() {
        let dir = std::env::temp_dir().join(format!("espada-wp6-ob-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("openbible_cross_references.sqlite");
        let _ = std::fs::remove_file(&db);
        {
            let c = Connection::open(&db).unwrap();
            c.execute_batch(
                "CREATE TABLE cross_references (from_book INT, from_chapter INT, from_verse INT,
                   to_book INT, to_chapter INT, to_verse_start INT, to_verse_end INT, votes INT);
                 INSERT INTO cross_references VALUES (1,11,31, 1,11,32,5, 9);
                 INSERT INTO cross_references VALUES (1,11,31, 1,12,1,3, 5);
                 INSERT INTO cross_references VALUES (1,11,31, 2,3,4,4, 1);",
            )
            .unwrap();
        }
        let h = load_openbible(db.to_str().unwrap(), 1, 11, 31, 80).unwrap();
        assert_eq!(h[0].label, "Gn 11:32");
        assert_eq!((h[0].to_verse_start, h[0].to_verse_end), (32, 32));
        assert!(h[0].html.contains(r#"data-verse="32" data-verse-end="32""#));
        assert_eq!(h[1].label, "Gn 12:1-3", "ordinary ranges unchanged");
        assert_eq!(h[2].label, "Éx 3:4");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn encrypted_source_note_does_not_claim_a_bundled_database() {
        assert!(!ENCRYPTED_SOURCE_NOTE.contains("ya generado"));
        assert!(ENCRYPTED_SOURCE_NOTE.contains("Puede copiar openbible_cross_references.sqlite a la carpeta de módulos"));
        let mods = vec![cmt("tsk.cmtx", "Treasury of Scripture Knowledge", "TSK", true)];
        let p = load_cross_references(&mods, "/nonexistent-espada-test", 43, 3, 16, None, 80).unwrap();
        assert!(p.entries.is_empty());
        assert_eq!(p.note.as_deref(), Some(ENCRYPTED_SOURCE_NOTE));
    }

    #[test]
    fn spanish_sources_are_opt_in_and_specific() {
        let tcb = cmt("02TCB.cmti", "El Tesoro del Conocimiento Biblico", "TCB", false);
        let thompson = cmt("02THOMPSON.cmti", "BIBLIA DE REFERENCIA THOMPSON\n", "Thompson", false);
        let abbr_only = cmt("x.cmti", "Concordancia", "tcb", false);
        let rvr = cmt("01RVR1960x.cmti", "Reina Valera Revision 1960 con títulos, notas y referencias", "RVR1960x", false);
        let tnm = cmt("tnm.cmti", "Traducción del Nuevo Mundo (1987) - referencias y notas", "TNM", false);
        let on = CrossRefOptions { include_spanish_sources: true, ..Default::default() };
        for m in [&tcb, &thompson, &abbr_only] {
            assert!(!is_cross_reference_module(m), "{} must stay off by default", m.filename);
            assert!(is_cross_reference_module_with(m, on), "{} must be offered when opted in", m.filename);
        }
        for m in [&rvr, &tnm] {
            assert!(!is_cross_reference_module_with(m, on), "{} is not a cross-reference tool", m.filename);
        }
        let mut bible = tcb.clone();
        bible.kind = "bible".into();
        assert!(!is_cross_reference_module_with(&bible, on));
    }

    #[test]
    fn verse_granular_ordering_is_opt_in() {
        let mods = vec![
            cmt("02TCB.cmti", "El Tesoro del Conocimiento Biblico", "TCB", false),
            cmt("nasb.cmti", "NASB Footnotes and Cross-References", "NASB", false),
            cmt("torrey.cmti", "Torrey's Topical Textbook", "Torrey", false),
            cmt("tske_v12.cmti", "The Treasury of Scripture Knowledge, Enhanced", "TSKe", false),
        ];
        let files = |v: Vec<CrossRefSource>| -> Vec<String> {
            v.into_iter().map(|s| s.path.rsplit('/').next().unwrap().to_string()).collect()
        };
        // Default: today's scan order, Spanish TCB not offered.
        assert_eq!(
            files(list_cross_reference_sources(&mods, "/nonexistent-espada-test")),
            ["nasb.cmti", "torrey.cmti", "tske_v12.cmti"]
        );
        assert_eq!(
            files(list_cross_reference_sources_with(&mods, "/nonexistent-espada-test", CrossRefOptions::recommended())),
            ["tske_v12.cmti", "02TCB.cmti", "nasb.cmti", "torrey.cmti"]
        );
        // OpenBible ranks first when present.
        let dir = std::env::temp_dir().join(format!("espada-wp6-rank-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("openbible_cross_references.sqlite"), b"").unwrap();
        let v = list_cross_reference_sources_with(&mods, dir.to_str().unwrap(), CrossRefOptions::recommended());
        assert_eq!(v[0].kind, "openbible");
        assert_eq!(files(v)[1], "tske_v12.cmti");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- WP6 QA stage: edge cases and end-to-end paths (synthetic, no library needed) ----

    /// A throwaway directory unique to this process and test (the module connection cache is
    /// keyed by path, so paths are never reused across tests).
    fn qa_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("espada-wp6-qa-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write a minimal e-Sword commentary (.cmti) with the given VerseCommentary rows, in the
    /// given insertion order, and return a readable ModuleInfo for it.
    fn synthetic_cmti(dir: &Path, filename: &str, title: &str, abbr: &str, rows: &[(i32, i32, i32, i32, i32, &str)]) -> ModuleInfo {
        let path = dir.join(filename);
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE VerseCommentary (Book INT, ChapterBegin INT, VerseBegin INT, ChapterEnd INT, VerseEnd INT, Comments TEXT);",
        )
        .unwrap();
        for (b, cb, vb, ce, ve, text) in rows {
            c.execute(
                "INSERT INTO VerseCommentary VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![b, cb, vb, ce, ve, text],
            )
            .unwrap();
        }
        ModuleInfo {
            path: path.to_string_lossy().to_string(),
            filename: filename.into(),
            title: title.into(),
            abbreviation: abbr.into(),
            kind: "commentary".into(),
            encrypted: false,
            has_strongs: false,
            version: 4,
        }
    }

    #[test]
    fn nasb_like_module_end_to_end_lists_only_the_tapped_verse() {
        let dir = qa_dir("e2e-nasb");
        let nasb = synthetic_cmti(&dir, "nasb.cmti", "NASB Footnotes and Cross-References", "NASB", &[(43, 3, 1, 3, 36, NASB_LIKE)]);
        let tske = synthetic_cmti(
            &dir,
            "tske_v12.cmti",
            "The Treasury of Scripture Knowledge, Enhanced",
            "TSKe",
            &[(43, 3, 16, 3, 16, "<ref>Rom 5:8</ref>; <ref>1Jn 4:9</ref>; <ref>Joh 1:14</ref>")],
        );
        let mods = vec![nasb.clone(), tske.clone()];
        let dir_s = dir.to_str().unwrap();
        // Default: NASB is still the first source (plan question Q1) but is now verse-precise.
        let p = load_cross_references(&mods, dir_s, 43, 3, 16, None, 80).unwrap();
        assert_eq!(p.source_path, nasb.path);
        assert_eq!(p.from_label, "Jn 3:16");
        assert_eq!(targets(&p.entries), vec![(45, 5, 8, 8), (49, 2, 4, 4), (45, 8, 32, 32), (62, 4, 9, 9)]);
        assert!(p.note.is_none());
        // A verse with no anchor paragraph: empty list plus the Spanish note, never a raw block.
        let p = load_cross_references(&mods, dir_s, 43, 3, 30, None, 80).unwrap();
        assert!(p.entries.is_empty());
        assert_eq!(p.note.as_deref(), Some("Sin referencias para Jn 3:30 en «NASB — NASB Footnotes and Cross-References»."));
        // Opt-in ordering makes TSKe the default without any UI change.
        let rec = load_cross_references_with(&mods, dir_s, 43, 3, 16, None, 80, CrossRefOptions::recommended()).unwrap();
        assert_eq!(rec.source_path, tske.path);
        assert_eq!(targets(&rec.entries), vec![(45, 5, 8, 8), (62, 4, 9, 9), (43, 1, 14, 14)]);
        // A saved NASB choice is still honoured under the opt-in ordering.
        let saved = load_cross_references_with(&mods, dir_s, 43, 3, 16, Some(&nasb.path), 80, CrossRefOptions::recommended()).unwrap();
        assert_eq!(saved.source_path, nasb.path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commentary_limit_is_opt_in_clamped_and_applied_after_dedupe() {
        let dir = qa_dir("limit");
        // 5 refs with two duplicates: 3 distinct targets after dedupe.
        let m = synthetic_cmti(
            &dir,
            "tske_v12.cmti",
            "The Treasury of Scripture Knowledge, Enhanced",
            "TSKe",
            &[(43, 3, 16, 3, 16, "<ref>Rom 5:8</ref>; <ref>Rom 5:8</ref>; <ref>1Jn 4:9</ref>; <ref>Rom 5:8</ref>; <ref>Joh 1:14</ref>")],
        );
        let mods = vec![m];
        let d = dir.to_str().unwrap();
        let all = vec![(45, 5, 8, 8), (62, 4, 9, 9), (43, 1, 14, 14)];
        let on = CrossRefOptions { limit_commentary: true, ..Default::default() };
        let run = |limit: i32, opts: CrossRefOptions| {
            targets(&load_cross_references_with(&mods, d, 43, 3, 16, None, limit, opts).unwrap().entries)
        };
        // Off (default): the commentary list is complete whatever the limit.
        assert_eq!(run(1, CrossRefOptions::default()), all);
        assert_eq!(targets(&load_cross_references(&mods, d, 43, 3, 16, None, 1).unwrap().entries), all);
        // On: a prefix of the deduped list, with the OpenBible clamp (1..=200).
        assert_eq!(run(2, on), all[..2].to_vec(), "limit counts distinct targets, not raw refs");
        assert_eq!(run(3, on), all);
        assert_eq!(run(0, on), all[..1].to_vec(), "limit 0 clamps to 1");
        assert_eq!(run(-7, on), all[..1].to_vec(), "negative limit clamps to 1");
        assert_eq!(run(10_000, on), all);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sql_row_order_does_not_change_the_panel() {
        // ISS-39 step 2 end to end: the same rows inserted in different orders give the same payload.
        let outline = "Nicodemo, <ref>Jua 3:1-13</ref>. Amor, <ref>Jua 3:16</ref>.";
        let verse = "porque de tal amó. <ref>Luc 2:14</ref>; <ref>Rom 5:8</ref>";
        let text = "Solo texto, sin referencias.";
        let orders: [[(i32, i32, i32, i32, i32, &str); 3]; 3] = [
            [(43, 3, 1, 3, 36, outline), (43, 3, 16, 3, 16, verse), (43, 3, 16, 0, 0, text)],
            [(43, 3, 16, 0, 0, text), (43, 3, 16, 3, 16, verse), (43, 3, 1, 3, 36, outline)],
            [(43, 3, 16, 3, 16, verse), (43, 3, 1, 3, 36, outline), (43, 3, 16, 0, 0, text)],
        ];
        let dir = qa_dir("sql-order");
        let mut seen = Vec::new();
        for (i, rows) in orders.iter().enumerate() {
            let m = synthetic_cmti(&dir, &format!("tcb{i}.cmti"), "El Tesoro del Conocimiento Biblico", "TCB", rows);
            let p = load_from_commentary_module(&m.path, 43, 3, 16).unwrap();
            seen.push(p.iter().map(|h| (h.label.clone(), h.html.clone())).collect::<Vec<_>>());
        }
        assert_eq!(seen[0].iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["Lc 2:14", "Ro 5:8"]);
        assert_eq!(seen[0], seen[1]);
        assert_eq!(seen[0], seen[2]);
        // Verse 2: only the outline matches, so its refs are the fallback (today's behaviour).
        let m = synthetic_cmti(&dir, "tcb-outline.cmti", "El Tesoro del Conocimiento Biblico", "TCB", &orders[1]);
        assert_eq!(targets(&load_from_commentary_module(&m.path, 43, 3, 2).unwrap()), vec![(43, 3, 1, 13), (43, 3, 16, 16)]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn anchor_paragraphs_with_unresolvable_or_foreign_anchors_are_dropped() {
        // A kept paragraph must not leak into the next one when that one's anchor can't be
        // resolved, names another chapter, or is a [n] footnote.
        let p = |marker: &str, anchor: &str, refs: &str| {
            format!(r#"<p><span style="color:#804DB3;">{marker}</span>  <ref>{anchor}</ref>: {refs}</p>"#)
        };
        let raw = [
            p("(1)", "Joh 3:16", "<ref>Rom 5:8</ref>"),
            p("(2)", "Xyz 3:16", "<ref>Gen 1:1</ref>"),
            p("(3)", "Joh 3:16", "<ref>Eph 2:4</ref>"),
            p("(4)", "Joh 4:16", "<ref>Exo 3:14</ref>"),
            p("(5)", "Joh 3:16", "<ref>1Jn 4:9</ref>"),
            p("[6]", "Joh 3:16", "Or only begotten <ref>Joh 1:18</ref>"),
            p("(7)", "Mat 3:16", "<ref>Mar 1:10</ref>"),
        ]
        .concat();
        let e = vec![entry("v.1–36", &raw)];
        assert_eq!(
            targets(&collect_commentary_hits(&e, 43, 3, 16)),
            vec![(45, 5, 8, 8), (49, 2, 4, 4), (62, 4, 9, 9)]
        );
    }

    #[test]
    fn prose_counts_in_parentheses_are_not_anchors() {
        // 02THOMPSON.cmti Ps 102:25-26 (real shape, trimmed): "(1)" is a chain count followed by
        // a ref and '.', not an NASB anchor. The row must stay unstructured so its refs are the
        // fallback, instead of being emptied because Sal 104:5 does not cover Ps 102:25.
        let raw = r#"<span style="color:#0000FF;">951</span> El creador (1) <span style="color:#008000;text-decoration:underline;"><ref>Sal 104:5</ref></span>.</p> <span style="color:#0000FF;">796</span> Los cielos (2), <span style="color:#008000;text-decoration:underline;"><ref>Isa 34:4</ref></span>.</p>"#;
        let e = vec![entry("v.25–26", raw)];
        assert!(anchored_hits(&e[0].html, 19, 102, 25).is_none());
        assert_eq!(targets(&collect_commentary_hits(&e, 19, 102, 25)), vec![(19, 104, 5, 5), (23, 34, 4, 4)]);
        // The same marker with the NASB ':' is an anchor.
        let nasb = "<p>(1)  <ref>Psa 102:25</ref>: <ref>Heb 1:10</ref></p>";
        let e = vec![entry("v.1–28", nasb)];
        assert_eq!(targets(&collect_commentary_hits(&e, 19, 102, 25)), vec![(58, 1, 10, 10)]);
        assert!(collect_commentary_hits(&e, 19, 102, 26).is_empty());
    }

    #[test]
    fn anchors_separated_by_nbsp_entities_are_recognised() {
        // e-Sword exports sometimes write the gap after the marker as &nbsp; entities.
        let raw = "<p>(12)&nbsp;&nbsp;<ref>Joh 3:16</ref>: <ref>Rom 5:8</ref></p><p>(13)&nbsp;<ref>Joh 3:17</ref>: <ref>Luk 19:10</ref></p>";
        let e = vec![entry("v.1–36", raw)];
        assert_eq!(targets(&collect_commentary_hits(&e, 43, 3, 16)), vec![(45, 5, 8, 8)]);
        assert_eq!(targets(&collect_commentary_hits(&e, 43, 3, 17)), vec![(42, 19, 10, 10)]);
    }

    #[test]
    fn cross_chapter_multi_verse_rows_are_filtered_too() {
        // load_commentary labels a row spanning chapters "C:V–C2:V2"; it is multi-verse, so the
        // anchor filter applies in both chapters it covers.
        let raw = "<p>(1)  <ref>Joh 3:36</ref>: <ref>Rom 1:1</ref></p><p>(2)  <ref>Joh 4:1</ref>: <ref>Gen 1:1</ref></p>";
        let e = vec![entry("3:36–4:2", raw)];
        assert_eq!(targets(&collect_commentary_hits(&e, 43, 3, 36)), vec![(45, 1, 1, 1)]);
        assert_eq!(targets(&collect_commentary_hits(&e, 43, 4, 1)), vec![(1, 1, 1, 1)]);
        assert!(collect_commentary_hits(&e, 43, 4, 2).is_empty());
    }

    #[test]
    fn no_sources_returns_the_spanish_setup_note() {
        let plain = cmt("mh.cmti", "Matthew Henry", "MH", false);
        for mods in [vec![], vec![plain]] {
            let p = load_cross_references(&mods, "/nonexistent-espada-test", 43, 3, 16, None, 80).unwrap();
            assert!(p.entries.is_empty());
            assert_eq!((p.source.as_str(), p.from_label.as_str()), ("", "Jn 3:16"));
            let note = p.note.unwrap();
            assert!(note.starts_with("No hay datos de referencias cruzadas."), "{note}");
            assert!(!note.contains("ya generado"));
        }
    }

    #[test]
    fn preferred_encrypted_source_is_honoured_and_otherwise_skipped() {
        let dir = qa_dir("enc-choice");
        let locked = cmt("tsk.cmtx", "Treasury of Scripture Knowledge", "TSK", true);
        let open = synthetic_cmti(&dir, "tske_v12.cmti", "The Treasury of Scripture Knowledge, Enhanced", "TSKe", &[(43, 3, 16, 3, 16, "<ref>Rom 5:8</ref>")]);
        let mods = vec![locked.clone(), open.clone()];
        let d = dir.to_str().unwrap();
        // No preference: the first NON-encrypted source wins although the locked one is listed first.
        let p = load_cross_references(&mods, d, 43, 3, 16, None, 80).unwrap();
        assert_eq!(p.source_path, open.path);
        assert_eq!(targets(&p.entries), vec![(45, 5, 8, 8)]);
        // Explicitly chosen locked source: no entries, the corrected note.
        let p = load_cross_references(&mods, d, 43, 3, 16, Some(&locked.path), 80).unwrap();
        assert!(p.entries.is_empty());
        assert_eq!(p.note.as_deref(), Some(ENCRYPTED_SOURCE_NOTE));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn openbible_zero_end_and_limit_clamp() {
        let dir = qa_dir("ob-clamp");
        let db = dir.join("openbible_cross_references.sqlite");
        {
            let c = Connection::open(&db).unwrap();
            c.execute_batch(
                "CREATE TABLE cross_references (from_book INT, from_chapter INT, from_verse INT,
                   to_book INT, to_chapter INT, to_verse_start INT, to_verse_end INT, votes INT);
                 INSERT INTO cross_references VALUES (43,3,16, 45,5,8,0, 50);
                 INSERT INTO cross_references VALUES (43,3,16, 62,4,9,10, 40);
                 INSERT INTO cross_references VALUES (43,3,16, 99,1,1,1, 30);
                 INSERT INTO cross_references VALUES (43,3,16, 1,1,1,1, 20);",
            )
            .unwrap();
        }
        let path = db.to_str().unwrap();
        let h = load_openbible(path, 43, 3, 16, 80).unwrap();
        // A zero end verse is a single verse; out-of-canon books are skipped; votes order kept.
        assert_eq!(targets(&h), vec![(45, 5, 8, 8), (62, 4, 9, 10), (1, 1, 1, 1)]);
        assert_eq!(h[0].label, "Ro 5:8");
        assert!(h.iter().all(|x| x.to_verse_end >= x.to_verse_start));
        assert_eq!(load_openbible(path, 43, 3, 16, 0).unwrap().len(), 1, "limit 0 clamps to 1");
        assert_eq!(load_openbible(path, 43, 3, 16, -3).unwrap().len(), 1);
        // A file without the table is a Spanish error, not a panic.
        let bad = dir.join("cross_references.db");
        Connection::open(&bad).unwrap().execute_batch("CREATE TABLE x (a INT);").unwrap();
        let err = load_openbible(bad.to_str().unwrap(), 43, 3, 16, 80).unwrap_err();
        assert!(err.contains("cross_references"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spanish_source_keywords_edge_cases() {
        let on = CrossRefOptions { include_spanish_sources: true, ..Default::default() };
        // Case-insensitive titles and a padded abbreviation are recognised.
        assert!(is_cross_reference_module_with(&cmt("a.cmti", "EL TESORO DEL CONOCIMIENTO BÍBLICO", "X", false), on));
        assert!(is_cross_reference_module_with(&cmt("b.cmti", "Algo", " TCB ", false), on));
        // The abbreviation must be exactly TCB; the filename alone never counts.
        for m in [cmt("c.cmti", "Algo", "TCB2", false), cmt("02TCB.cmti", "Algo", "XYZ", false), cmt("d.cmti", "Comentario con referencias", "CR", false)] {
            assert!(!is_cross_reference_module_with(&m, on), "{} / {}", m.filename, m.abbreviation);
        }
        // With the default options, _with is exactly is_cross_reference_module.
        for m in [
            cmt("tsk.cmti", "Treasury of Scripture Knowledge", "TSK", false),
            cmt("02TCB.cmti", "El Tesoro del Conocimiento Biblico", "TCB", false),
            cmt("mh.cmti", "Matthew Henry", "MH", false),
        ] {
            assert_eq!(is_cross_reference_module_with(&m, CrossRefOptions::default()), is_cross_reference_module(&m));
        }
        // An encrypted TCB is listed (flagged) when opted in, but never chosen over a readable source.
        let mods = vec![cmt("02TCB.cmtx", "El Tesoro del Conocimiento Biblico", "TCB", true)];
        let v = list_cross_reference_sources_with(&mods, "/nonexistent-espada-test", on);
        assert_eq!(v.len(), 1);
        assert!(v[0].encrypted);
    }
}
