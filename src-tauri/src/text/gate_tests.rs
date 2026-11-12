//! WP2 ship gates against the real module library (read-only).
//!
//! The slow gates are `#[ignore]`; run them with
//! `cargo test --release --lib text::gate_tests -- --ignored --nocapture`.
//! Every library test skips with a visible message when the library is absent.

use super::legacy;
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{Connection, OpenFlags};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(super) fn library_dir() -> Option<String> {
    let dir = std::env::var("ESPADA_MODULES_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/Downloads/for mac espada")
    });
    if std::path::Path::new(&dir).is_dir() {
        Some(dir)
    } else {
        eprintln!("SKIP: module library not found at {dir} — library test not run");
        None
    }
}

pub(super) fn open_ro(path: &str) -> Option<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
}

/// Same decoding as modules.rs `cell_to_string` (UTF-8, UTF-16 LE, Windows-1252).
pub(super) fn cell(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<String> {
    if let Ok(s) = row.get::<_, String>(idx) {
        return Ok(s);
    }
    if let Ok(bytes) = row.get::<_, Vec<u8>>(idx) {
        if let Ok(s) = std::str::from_utf8(&bytes) {
            return Ok(s.to_string());
        }
        if bytes.len() >= 2 && bytes.len() % 2 == 0 {
            let u16s: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            let slice = if u16s.first() == Some(&0xFEFF) { &u16s[1..] } else { &u16s[..] };
            return Ok(String::from_utf16_lossy(slice));
        }
        return Ok(encoding_rs::WINDOWS_1252.decode(&bytes).0.into_owned());
    }
    Ok(String::new())
}

/// What a browser shows (innerHTML): tags are skipped; entities decoded per text node.
pub(crate) fn visible(html: &str) -> String {
    let b = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut seg = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'<'
            && i + 1 < b.len()
            && (b[i + 1].is_ascii_alphabetic() || matches!(b[i + 1], b'/' | b'!' | b'?'))
        {
            out.push_str(&html_escape::decode_html_entities(&seg));
            seg.clear();
            match html[i..].find('>') {
                Some(j) => {
                    // a line break renders as a break, never glues words
                    if html[i..i + j + 1].eq_ignore_ascii_case("<br>") {
                        out.push(' ');
                    }
                    i += j + 1;
                    continue;
                }
                None => break,
            }
        }
        let ch = html[i..].chars().next().unwrap();
        seg.push(ch);
        i += ch.len_utf8();
    }
    out.push_str(&html_escape::decode_html_entities(&seg));
    let d = out.replace('\u{a0}', " ");
    d.split_whitespace().collect::<Vec<_>>().join(" ")
}

static RE_LIT_ENT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"&(#\d+|#x[0-9a-fA-F]+|[A-Za-z][A-Za-z0-9]{1,9});").unwrap());
static RE_TAGFRAG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"</?(?:span|font|p|br|div|td|tr|th|table|sup|sub|heb|grk|ref|num|a|em|strong|lat|tvm|blu|red)(?:\s+[a-z-]+\s*=|/?>|[A-ZÁÉÍÓÚ]|\s*$)|</(?:span|p|font|a|div)\s"#).unwrap()
});
static RE_ANGLE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^<>]{0,30}>").unwrap());
static RE_WORD: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\p{L}{2,}$").unwrap());

/// Letter-only tokens (≥2 letters) of the old visible text that the new visible text lacks.
fn lost_words(old_v: &str, new_v: &str) -> Vec<String> {
    let mut have: HashMap<&str, i64> = HashMap::new();
    for t in new_v.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation() || "«»“”‘’·•—–¿¡".contains(c)) {
        *have.entry(t).or_default() += 1;
    }
    let mut lost = Vec::new();
    for t in old_v.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation() || "«»“”‘’·•—–¿¡".contains(c)) {
        if !RE_WORD.is_match(t) {
            continue;
        }
        let e = have.entry(t).or_default();
        *e -= 1;
        if *e < 0 {
            lost.push(t.to_string());
        }
    }
    lost
}

#[derive(Default)]
struct DiffStats {
    rows: usize,
    changed: usize,
    cat: BTreeMap<&'static str, usize>,
    per_file: BTreeMap<(String, &'static str), usize>,
    examples: BTreeMap<&'static str, Vec<String>>,
    lost_word_rows: BTreeMap<String, usize>,
    lost_examples: Vec<String>,
}

fn window(a: &str, b: &str) -> (String, String) {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let mut i = 0;
    while i < ac.len() && i < bc.len() && ac[i] == bc[i] {
        i += 1;
    }
    let s = i.saturating_sub(50);
    (
        ac[s..(i + 70).min(ac.len())].iter().collect(),
        bc[s..(i + 70).min(bc.len())].iter().collect(),
    )
}

fn diff_row(st: &mut DiffStats, file: &str, raw: &str) {
    st.rows += 1;
    let old = legacy::clean_esword_text(raw);
    let new = super::clean_esword_text(raw);
    let vo = visible(&old);
    let vn = visible(&new);
    if vo == vn {
        return;
    }
    st.changed += 1;
    let lw = lost_words(&vo, &vn);
    if !lw.is_empty() {
        *st.lost_word_rows.entry(file.to_string()).or_default() += 1;
        if st.lost_examples.len() < 40 {
            let (wo, wn) = window(&vo, &vn);
            st.lost_examples
                .push(format!("{file} lost {:?}\n      OLD «{wo}»\n      NEW «{wn}»", &lw[..lw.len().min(6)]));
        }
    }
    let (tf_o, tf_n) = (RE_TAGFRAG.find_iter(&vo).count(), RE_TAGFRAG.find_iter(&vn).count());
    let (eo, en) = (RE_LIT_ENT.find_iter(&vo).count(), RE_LIT_ENT.find_iter(&vn).count());
    let (ao, an) = (RE_ANGLE.find_iter(&vo).count(), RE_ANGLE.find_iter(&vn).count());
    let k: &'static str = if tf_n > tf_o {
        "E_new_shows_malformed_tag_fragment"
    } else if en > eo {
        "A_new_shows_literal_entity"
    } else if an > ao {
        "B_new_shows_angle_text_old_hid"
    } else if vn.len() > vo.len() {
        "C_new_longer_other"
    } else {
        "D_new_shorter_or_changed"
    };
    *st.cat.entry(k).or_default() += 1;
    *st.per_file.entry((file.to_string(), k)).or_default() += 1;
    let e = st.examples.entry(k).or_default();
    if e.len() < 25 && !e.iter().any(|x| x.starts_with(&format!("{file}|"))) {
        let (wo, wn) = window(&vo, &vn);
        e.push(format!("{file}| OLD «{wo}»\n      NEW «{wn}»"));
    }
}

fn sources_for(ext: &str, bibles: bool) -> Vec<(&'static str, &'static str)> {
    match (ext, bibles) {
        ("bbli", true) => vec![("Bible", "Scripture")],
        ("cmti", false) => vec![
            ("VerseCommentary", "Comments"),
            ("ChapterCommentary", "Comments"),
            ("BookCommentary", "Comments"),
        ],
        ("dcti" | "lexi", false) => vec![("Dictionary", "Definition"), ("Lexicon", "Definition")],
        ("refi", false) => vec![("Reference", "Content")],
        _ => vec![],
    }
}

fn run_diff(dir: &str, bibles: bool) -> DiffStats {
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();
    let mut st = DiffStats::default();
    for f in &files {
        let ext = f.rsplit('.').next().unwrap_or("").to_lowercase();
        let srcs = sources_for(&ext, bibles);
        if srcs.is_empty() {
            continue;
        }
        let Some(conn) = open_ro(&format!("{dir}/{f}")) else { continue };
        for (t, col) in srcs {
            let Ok(mut stmt) = conn.prepare(&format!("SELECT {col} FROM \"{t}\"")) else { continue };
            let Ok(rows) = stmt.query_map([], |r| cell(r, 0)) else { continue };
            for raw in rows.flatten() {
                if !raw.is_empty() {
                    diff_row(&mut st, f, &raw);
                }
            }
        }
    }
    st
}

fn report(label: &str, st: &DiffStats) {
    println!("\n===== {label}: rows={} visibly_changed={} by_cat={:?}", st.rows, st.changed, st.cat);
    let mut pf: Vec<_> = st.per_file.iter().collect();
    pf.sort_by(|a, b| a.0 .1.cmp(b.0 .1).then(b.1.cmp(a.1)));
    for ((f, k), n) in pf.iter().take(80) {
        println!("PERFILE {k} {n} {f}");
    }
    for (k, v) in &st.examples {
        println!("=== {k}");
        for e in v {
            println!("  {e}");
        }
    }
    println!("LOST-WORD rows per file: {:?}", st.lost_word_rows);
    for e in &st.lost_examples {
        println!("  {e}");
    }
}

/// Gate (ISS-05/ISS-24): study rows. Malformed-fragment rows ≤ 2, new literal-entity rows 0.
#[test]
#[ignore]
fn gate_study_rows_visible_text_diff() {
    let Some(dir) = library_dir() else { return };
    let st = run_diff(&dir, false);
    report("STUDY", &st);
    let frag = st.cat.get("E_new_shows_malformed_tag_fragment").copied().unwrap_or(0);
    let ent = st.cat.get("A_new_shows_literal_entity").copied().unwrap_or(0);
    println!("GATE study: rows={} malformed_fragment_rows={frag} (≤2) literal_entity_rows={ent} (=0)", st.rows);
    assert!(frag <= 2, "malformed fragment rows {frag}");
    assert_eq!(ent, 0, "new literal-entity rows");
}

/// Gate: Bible rows. RV1960 and iRV verse html visibly unchanged; CEVALLOS loses no words.
#[test]
#[ignore]
fn gate_bible_rows_visible_text_diff() {
    let Some(dir) = library_dir() else { return };
    let st = run_diff(&dir, true);
    report("BIBLE", &st);
    let changed_in = |name: &str| -> usize {
        st.per_file.iter().filter(|((f, _), _)| f == name).map(|(_, n)| *n).sum()
    };
    let rv = changed_in("00RVR1960.bbli");
    let irv = changed_in("00Interlineal-iRV 1960+.bbli");
    // Changes other than the intended ISS-25 underscore fix ('3Jn_1:15' → '3Jn 1:15').
    let mut unintended = 0usize;
    for f in ["00RVR1960.bbli", "00Interlineal-iRV 1960+.bbli"] {
        let conn = open_ro(&format!("{dir}/{f}")).expect("open");
        let mut stmt = conn.prepare("SELECT Book, Chapter, Verse, Scripture FROM Bible").unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i32>(0)?, r.get::<_, i32>(1)?, r.get::<_, i32>(2)?, cell(r, 3)?)))
            .unwrap();
        for (b, c, v, raw) in rows.flatten() {
            let vo = visible(&legacy::clean_esword_text(&raw));
            let vn = visible(&super::clean_esword_text(&raw));
            if vo != vn {
                let normalized = super::convert_underscore_refs(&vo);
                let kind = if normalized == vn { "ISS-25 underscore (intended)" } else { "UNINTENDED" };
                if normalized != vn {
                    unintended += 1;
                }
                let (wo, wn) = window(&vo, &vn);
                println!("  {f} {b}:{c}:{v} [{kind}]\n      OLD «{wo}»\n      NEW «{wn}»");
            }
        }
    }
    let cev_lost = st.lost_word_rows.get("00A-CEVALLOS NT-INT+.bbli").copied().unwrap_or(0);
    let ent = st.cat.get("A_new_shows_literal_entity").copied().unwrap_or(0);
    println!("GATE bible: rows={} RV1960_changed={rv} iRV_changed={irv} RV1960+iRV_unintended={unintended} CEVALLOS_rows_losing_words={cev_lost} literal_entity_rows={ent}", st.rows);
    assert_eq!(unintended, 0, "RV1960/iRV visible text changed beyond ISS-25");
    assert_eq!(cev_lost, 0, "CEVALLOS rows losing words");
    assert_eq!(ent, 0, "new literal-entity rows");
}

// ------------------------------------------------------------------ cross-references

/// Replica of crossrefs.rs `hits_from_commentary_html` (private there): returns targets.
fn ref_targets(html: &str) -> Vec<(i32, i32, i32, i32)> {
    static RE_SPAN: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(?is)<span\s+class="bible-ref[^"]*"[^>]*>.*?</span>"#).unwrap());
    static RE_BOOK: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)data-book="([^"]+)""#).unwrap());
    static RE_CH: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)data-chapter="(\d+)""#).unwrap());
    static RE_V1: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)data-verse="(\d+)""#).unwrap());
    static RE_V2: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)data-verse-end="(\d+)""#).unwrap());
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for m in RE_SPAN.find_iter(html) {
        let span = m.as_str();
        let book = RE_BOOK.captures(span).map(|c| c[1].to_string());
        let ch = RE_CH.captures(span).and_then(|c| c[1].parse().ok());
        let v1 = RE_V1.captures(span).and_then(|c| c[1].parse().ok());
        let v2 = RE_V2.captures(span).and_then(|c| c[1].parse().ok()).or(v1);
        let (Some(book), Some(ch), Some(v1), Some(v2)) = (book, ch, v1, v2) else { continue };
        let Some(tb) = crate::books::resolve_book_name(&book) else { continue };
        if seen.insert((tb, ch, v1, v2)) {
            out.push((tb, ch, v1, v2));
        }
    }
    out
}

/// Gate: crossref hits from the CR modules, old cleaner vs new cleaner, row by row.
#[test]
#[ignore]
fn gate_crossref_hits_old_equals_new() {
    let Some(dir) = library_dir() else { return };
    let mut total_old = 0usize;
    let mut total_new = 0usize;
    let mut diff_rows = 0usize;
    for f in ["tske_v12.cmti", "nasb.cmti", "02TCB.cmti", "02THOMPSON.cmti"] {
        let Some(conn) = open_ro(&format!("{dir}/{f}")) else {
            println!("skip {f}: not found");
            continue;
        };
        let mut stmt = conn.prepare("SELECT Comments FROM VerseCommentary").unwrap();
        let rows: Vec<String> = stmt.query_map([], |r| cell(r, 0)).unwrap().flatten().collect();
        let (mut o_n, mut n_n, mut d) = (0usize, 0usize, 0usize);
        let mut shown = 0;
        for raw in &rows {
            let o = ref_targets(&legacy::clean_esword_text(raw));
            let n = ref_targets(&super::clean_esword_text(raw));
            o_n += o.len();
            n_n += n.len();
            if o != n {
                d += 1;
                if shown < 5 {
                    shown += 1;
                    let os: BTreeSet<_> = o.iter().collect();
                    let ns: BTreeSet<_> = n.iter().collect();
                    println!(
                        "  {f} diff: only_old={:?} only_new={:?}",
                        os.difference(&ns).take(5).collect::<Vec<_>>(),
                        ns.difference(&os).take(5).collect::<Vec<_>>()
                    );
                }
            }
        }
        println!("{f}: rows={} hits_old={o_n} hits_new={n_n} rows_differing={d}", rows.len());
        total_old += o_n;
        total_new += n_n;
        diff_rows += d;
    }
    println!("GATE crossrefs: hits_old={total_old} hits_new={total_new} rows_differing={diff_rows}");
    assert_eq!(total_old, total_new);
    assert_eq!(diff_rows, 0);
}

/// Gate: compare against a crossref snapshot given in ESPADA_CROSSREF_SNAPSHOT
/// (labels/targets/html hash for NASB, TSKe; targets for TCB/THOMPSON).
/// Only meaningful against a snapshot taken from the same crossrefs.rs/books.rs.
/// The live regression gate is crossrefs::snapshot_tests (crossref_snapshot_expected.jsonl);
/// this one skips unless the env var is set.
#[test]
#[ignore]
fn gate_crossref_frozen_snapshot() {
    use std::hash::{Hash, Hasher};
    let Some(dir) = library_dir() else { return };
    let Ok(snap) = std::env::var("ESPADA_CROSSREF_SNAPSHOT") else {
        eprintln!("SKIP: set ESPADA_CROSSREF_SNAPSHOT to run; crossrefs::snapshot_tests is the live gate");
        return;
    };
    let Ok(text) = std::fs::read_to_string(&snap) else {
        eprintln!("SKIP: crossref snapshot not found at {snap}");
        return;
    };
    let mods = crate::modules::scan_modules(&dir).expect("scan");
    let sources = crate::crossrefs::list_cross_reference_sources(&mods, &dir);
    let (mut checked, mut mismatched) = (0usize, 0usize);
    for line in text.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let title = v["source"].as_str().unwrap();
        let from: Vec<i32> = v["from"].as_array().unwrap().iter().map(|x| x.as_i64().unwrap() as i32).collect();
        let want_targets: Vec<Vec<i64>> = v["targets"]
            .as_array()
            .map(|a| a.iter().map(|t| t.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect()).collect())
            .unwrap_or_default();
        let (got_targets, got_labels, got_hash): (Vec<Vec<i64>>, Option<Vec<String>>, Option<String>) =
            if let Some(f) = title.strip_prefix("[undetected] ") {
                let entries = crate::modules::load_commentary(&format!("{dir}/{f}"), from[0], from[1], from[2], true, false, false).unwrap();
                let mut seen = BTreeSet::new();
                let mut t = Vec::new();
                for e in &entries {
                    for h in ref_targets(&e.html) {
                        if seen.insert(h) {
                            t.push(vec![h.0 as i64, h.1 as i64, h.2 as i64, h.3 as i64]);
                        }
                    }
                }
                (t, None, None)
            } else if title.starts_with("[repo") {
                continue;
            } else {
                let Some(src) = sources.iter().find(|s| s.title == title) else {
                    println!("source {title} not found");
                    mismatched += 1;
                    continue;
                };
                let p = crate::crossrefs::load_cross_references(&mods, &dir, from[0], from[1], from[2], Some(&src.path), 80).unwrap();
                let t = p.entries.iter().map(|e| vec![e.to_book as i64, e.to_chapter as i64, e.to_verse_start as i64, e.to_verse_end as i64]).collect();
                let l = p.entries.iter().map(|e| e.label.clone()).collect();
                let mut h = std::collections::hash_map::DefaultHasher::new();
                p.entries.iter().map(|e| e.html.as_str()).collect::<Vec<_>>().join("\u{1}").hash(&mut h);
                (t, Some(l), Some(format!("{:016x}", h.finish())))
            };
        checked += 1;
        let mut ok = got_targets.iter().filter(|t| t[0] > 0).cloned().collect::<Vec<_>>()
            == want_targets.iter().filter(|t| t[0] > 0).cloned().collect::<Vec<_>>();
        if let Some(l) = got_labels {
            let want: Vec<String> = v["labels"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
            ok &= l == want;
        }
        if let Some(h) = got_hash {
            ok &= v["html_hash"].as_str() == Some(h.as_str());
        }
        if !ok {
            mismatched += 1;
            println!("MISMATCH {title} {from:?}");
        }
    }
    println!("GATE crossref snapshot: checked={checked} mismatched={mismatched}");
    assert_eq!(mismatched, 0);
}

/// The audit's tricky `<ref>` shapes: hits must equal the old cleaner's, except the
/// cross-chapter ref, which ISS-25 now links to its start verse.
#[test]
fn tricky_refs_hits_match_old_cleaner() {
    for (r, expect_change) in [
        ("<ref>2Ki 18:13&mdash;19</ref>", false),
        ("<ref>G&eacute;n 1:1</ref>", false),
        ("<ref>Joh&nbsp;3:16</ref>", false),
        ("<ref>&#71;&#101;&#110; 1:1</ref>", false),
        ("<ref>Rom 5:8 &amp; 6:1</ref>", false),
        ("<ref><b>Mt</b> 5:3</ref>", false),
        ("<ref>Jn \"3:16\" x</ref>", false),
        ("<ref>Luk 2:14</ref>", false),
        ("<ref>Jua 3:16</ref>", false),
        ("<ref>Joh 3:16-4:2</ref>", true),
        ("<ref>Psa 23:1-6</ref>", false),
        ("<ref>Ap 22:21</ref>", false),
        ("&lt;ref&gt;Jn 3:16&lt;/ref&gt;", false),
    ] {
        let old = ref_targets(&legacy::clean_esword_text(r));
        let new = ref_targets(&super::clean_esword_text(r));
        if expect_change {
            assert!(old.is_empty(), "{r}: {old:?}");
            assert_eq!(new, vec![(43, 3, 16, 16)], "{r}");
        } else {
            assert_eq!(old, new, "{r}");
        }
    }
}

/// Dev aid: print the first visible-text diffs that lose words for one module.
/// `ESPADA_GATE_FILE="pch.cmti" cargo test --release --lib text::gate_tests::debug_file_lost_words -- --ignored --nocapture`
#[test]
#[ignore]
fn debug_file_lost_words() {
    let Some(dir) = library_dir() else { return };
    let Ok(f) = std::env::var("ESPADA_GATE_FILE") else {
        eprintln!("SKIP: set ESPADA_GATE_FILE");
        return;
    };
    let ext = f.rsplit('.').next().unwrap_or("").to_lowercase();
    let mut srcs = sources_for(&ext, true);
    srcs.extend(sources_for(&ext, false));
    let conn = open_ro(&format!("{dir}/{f}")).expect("open");
    let mut shown = 0;
    for (t, col) in srcs {
        let Ok(mut stmt) = conn.prepare(&format!("SELECT {col} FROM \"{t}\"")) else { continue };
        let rows: Vec<String> = stmt.query_map([], |r| cell(r, 0)).unwrap().flatten().collect();
        for raw in rows {
            let vo = visible(&legacy::clean_esword_text(&raw));
            let vn = visible(&super::clean_esword_text(&raw));
            let lw = lost_words(&vo, &vn);
            if lw.is_empty() {
                continue;
            }
            let (wo, wn) = window(&vo, &vn);
            println!("lost {:?}\n  OLD «{wo}»\n  NEW «{wn}»", &lw[..lw.len().min(8)]);
            if let Some(w) = lw.first() {
                if let Some(i) = raw.find(w.as_str()).or_else(|| raw.find(&w[..w.len().min(4)])) {
                    let a = raw[..i].char_indices().rev().nth(120).map(|x| x.0).unwrap_or(0);
                    println!("  RAW «{}»", raw[a..].chars().take(300).collect::<String>());
                }
            }
            shown += 1;
            if shown >= 4 {
                return;
            }
        }
    }
}
