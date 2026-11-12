//! Frozen cross-reference snapshot gate (WP6).
//!
//! WP2 (text.rs) and WP3 (modules.rs) use this as a regression gate. Both feed the
//! cross-reference panel: `load_commentary` → `clean_esword_text` → the `bible-ref`
//! span shape → `hits_from_commentary_html`.
//!
//! Fixtures (all in `src/crossrefs/`):
//! - `crossref_snapshot.jsonl`: the auditors' artifact, written verbatim by the audit probe
//!   from the ORIGINAL code (git HEAD 6748155, before WP1/WP6). Never edit it.
//!   It is the provenance baseline.
//! - `tricky_refs_snapshot.txt`: same provenance. Covers tricky `<ref>` shapes pushed through
//!   `clean_esword_text` → `hits_from_commentary_html`.
//! - `crossref_snapshot_expected.jsonl`: the exact expected output of the CURRENT code
//!   (WP1 aliases + WP6 fixes). It is the gate.
//!
//! How the gate is enforced:
//! 1. `snapshot_matches_expected` (needs the library): today's output equals the expected file,
//!    including labels, targets, order and the format_ref_html hash.
//! 2. `expected_differs_from_audit_only_by_intended_deltas` (no library needed): every
//!    difference between the expected file and the auditors' frozen file is one of the
//!    documented, intended deltas (WP1 aliases, ISS-39, ISS-41).
//! 3. `tricky_refs_hits_match_frozen` (no library needed): hits for the tricky shapes equal
//!    the frozen file, except the two WP1 alias lines.
//!
//! To intentionally change the expected output, regenerate it, then review the diff:
//!   ESPADA_REGEN_CROSSREF_SNAPSHOT=1 cargo test --lib regenerate_crossref_snapshot -- --ignored
//! This writes ONLY `src/crossrefs/crossref_snapshot_expected.jsonl`. It opens the module
//! library read-only and never writes to it.

use super::*;
use crate::modules::scan_modules;
use crate::text::clean_esword_text;
use std::collections::{BTreeMap, HashSet};

/// The module library: `ESPADA_MODULES_DIR`, else `$HOME/Downloads/for mac espada`.
static LIB: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("ESPADA_MODULES_DIR").unwrap_or_else(|_| {
        format!("{}/Downloads/for mac espada", std::env::var("HOME").unwrap_or_default())
    })
});
const AUDIT_FROZEN: &str = include_str!("crossref_snapshot.jsonl");
const EXPECTED: &str = include_str!("crossref_snapshot_expected.jsonl");
const TRICKY_FROZEN: &str = include_str!("tricky_refs_snapshot.txt");

const SRC_NASB: &str = "NASB — NASB Footnotes and Cross-References";
const SRC_TSKE: &str = "TSKe — The Treasury of Scripture Knowledge, Enhanced";
const SRC_TCB: &str = "[undetected] 02TCB.cmti";
const SRC_THOMPSON: &str = "[undetected] 02THOMPSON.cmti";
const SRC_OPENBIBLE: &str = "[repo data/, not in modules dir] OpenBible";

/// Books whose e-Sword tokens WP1 (ISS-06) taught `resolve_book_name`. A target that is new
/// relative to the auditors' frozen file is legitimate only if it points into one of these.
/// Judges, Ruth, Ecclesiastes, Song, Hosea, Obadiah, Zephaniah, Haggai, Luke, John, Acts,
/// 1-2 Thessalonians, James, Revelation.
const WP1_ALIAS_BOOKS: &[i32] = &[7, 8, 21, 22, 28, 31, 36, 37, 42, 43, 44, 52, 53, 59, 66];

/// The first 20 verses of the audit probe's `verses40()`, in its order.
fn snapshot_verses() -> Vec<(i32, i32, i32)> {
    vec![
        (1, 1, 1), (19, 23, 1), (23, 53, 5), (43, 3, 16), (45, 8, 28), (66, 22, 21),
        (1, 50, 26), (2, 1, 1), (2, 40, 38), (19, 1, 1), (19, 150, 6), (23, 1, 1),
        (23, 66, 24), (39, 1, 1), (39, 4, 6), (40, 1, 1), (40, 28, 20), (42, 1, 1),
        (42, 24, 53), (44, 1, 1),
    ]
}

fn library_present() -> bool {
    std::path::Path::new(LIB.as_str()).is_dir()
}

fn openbible_repo_db() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../data/openbible_cross_references.sqlite").to_string()
}

/// Stable 64-bit FNV-1a. DefaultHasher is not guaranteed stable across Rust releases.
fn fnv1a(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

#[derive(Debug, Clone, PartialEq)]
struct Rec {
    source: String,
    from: (i32, i32, i32),
    error: Option<String>,
    labels: Vec<String>,
    targets: Vec<[i32; 4]>,
    /// FNV-1a over the html of the NAVIGABLE hits only. Synthetic raw blocks carry module text
    /// that WP2 may legitimately change; they are gated by their label and count.
    nav_html_hash: String,
    raw_blocks: usize,
}

fn rec_from_entries(source: &str, from: (i32, i32, i32), r: Result<Vec<CrossRefHit>, String>) -> Rec {
    match r {
        Ok(es) => {
            let nav: Vec<&str> = es
                .iter()
                .filter(|e| e.to_book > 0 && e.to_chapter > 0)
                .map(|e| e.html.as_str())
                .collect();
            Rec {
                source: source.into(),
                from,
                error: None,
                labels: es.iter().map(|e| e.label.clone()).collect(),
                targets: es
                    .iter()
                    .map(|e| [e.to_book, e.to_chapter, e.to_verse_start, e.to_verse_end])
                    .collect(),
                nav_html_hash: fnv1a(&nav.join("\u{1}")),
                raw_blocks: es.iter().filter(|e| e.to_book <= 0 || e.to_chapter <= 0).count(),
            }
        }
        Err(e) => Rec {
            source: source.into(),
            from,
            error: Some(e),
            labels: vec![],
            targets: vec![],
            nav_html_hash: String::new(),
            raw_blocks: 0,
        },
    }
}

fn rec_to_json(r: &Rec) -> serde_json::Value {
    if let Some(e) = &r.error {
        return serde_json::json!({"source": r.source, "from": [r.from.0, r.from.1, r.from.2], "error": e});
    }
    serde_json::json!({
        "source": r.source,
        "from": [r.from.0, r.from.1, r.from.2],
        "count": r.labels.len(),
        "labels": r.labels,
        "targets": r.targets,
        "nav_html_hash": r.nav_html_hash,
        "raw_blocks": r.raw_blocks,
    })
}

fn parse_line(line: &str) -> Rec {
    let v: serde_json::Value = serde_json::from_str(line).expect("snapshot line is JSON");
    let from = v["from"].as_array().expect("from");
    let from = (
        from[0].as_i64().unwrap() as i32,
        from[1].as_i64().unwrap() as i32,
        from[2].as_i64().unwrap() as i32,
    );
    let labels: Vec<String> = v["labels"]
        .as_array()
        .map(|a| a.iter().map(|x| x.as_str().unwrap().to_string()).collect())
        .unwrap_or_default();
    let targets: Vec<[i32; 4]> = v["targets"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|t| {
                    let t = t.as_array().unwrap();
                    [
                        t[0].as_i64().unwrap() as i32,
                        t[1].as_i64().unwrap() as i32,
                        t[2].as_i64().unwrap() as i32,
                        t[3].as_i64().unwrap() as i32,
                    ]
                })
                .collect()
        })
        .unwrap_or_default();
    Rec {
        source: v["source"].as_str().unwrap().to_string(),
        from,
        error: v["error"].as_str().map(str::to_string),
        raw_blocks: v["raw_blocks"]
            .as_u64()
            .map(|n| n as usize)
            .unwrap_or_else(|| targets.iter().filter(|t| t[0] <= 0 || t[1] <= 0).count()),
        nav_html_hash: v["nav_html_hash"].as_str().unwrap_or("").to_string(),
        labels,
        targets,
    }
}

fn parse_file(s: &str) -> BTreeMap<(String, (i32, i32, i32)), Rec> {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(parse_line)
        .map(|r| ((r.source.clone(), r.from), r))
        .collect()
}

/// Produce today's records for every (source, verse) in the snapshot. The calls mirror the
/// audit probe: NASB and TSKe go through the public `load_cross_references` with the UI's
/// limit of 80, TCB and Thompson through `load_from_commentary_module`, and OpenBible through
/// `load_openbible` on the repo's data/ copy. Returns None when the library is absent.
fn generate_current() -> Option<Vec<Rec>> {
    if !library_present() {
        return None;
    }
    let mods = scan_modules(&LIB).expect("scan library");
    let nasb = format!("{}/nasb.cmti", *LIB);
    let tske = format!("{}/tske_v12.cmti", *LIB);
    let tcb = format!("{}/02TCB.cmti", *LIB);
    let thompson = format!("{}/02THOMPSON.cmti", *LIB);
    let ob = openbible_repo_db();
    let have_ob = std::path::Path::new(&ob).is_file();
    let mut out = Vec::new();
    for (source, path) in [
        (SRC_NASB, &nasb),
        (SRC_TSKE, &tske),
        (SRC_TCB, &tcb),
        (SRC_THOMPSON, &thompson),
        (SRC_OPENBIBLE, &ob),
    ] {
        if source == SRC_OPENBIBLE && !have_ob {
            eprintln!("skip OpenBible snapshot rows: {ob} is absent");
            continue;
        }
        for (b, c, v) in snapshot_verses() {
            let r = match source {
                SRC_NASB | SRC_TSKE => {
                    load_cross_references(&mods, &LIB, b, c, v, Some(path), 80).map(|p| {
                        assert_eq!(&p.source_path, path, "preferred source not honoured");
                        p.entries
                    })
                }
                SRC_OPENBIBLE => load_openbible(path, b, c, v, 80),
                _ => load_from_commentary_module(path, b, c, v),
            };
            out.push(rec_from_entries(source, (b, c, v), r));
        }
    }
    Some(out)
}

#[test]
#[ignore = "writes the expected fixture; run explicitly with ESPADA_REGEN_CROSSREF_SNAPSHOT=1"]
fn regenerate_crossref_snapshot_expected() {
    if std::env::var("ESPADA_REGEN_CROSSREF_SNAPSHOT").ok().as_deref() != Some("1") {
        eprintln!("skip: set ESPADA_REGEN_CROSSREF_SNAPSHOT=1 to regenerate the expected snapshot");
        return;
    }
    let Some(recs) = generate_current() else {
        eprintln!("skip: module library not found at {}", *LIB);
        return;
    };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/crossrefs/crossref_snapshot_expected.jsonl");
    let mut s = String::new();
    for r in &recs {
        s.push_str(&rec_to_json(r).to_string());
        s.push('\n');
    }
    std::fs::write(path, s).expect("write expected snapshot");
    eprintln!("wrote {} records to {path}", recs.len());
}

/// GATE 1: today's output equals the expected snapshot.
#[test]
fn snapshot_matches_expected() {
    let Some(recs) = generate_current() else {
        eprintln!("skip snapshot_matches_expected: module library not found at {}", *LIB);
        return;
    };
    let expected = parse_file(EXPECTED);
    assert_eq!(expected.len(), 100, "expected snapshot must hold 5 sources x 20 verses");
    let mut diffs = Vec::new();
    let mut seen = 0;
    for r in &recs {
        seen += 1;
        let key = (r.source.clone(), r.from);
        let Some(e) = expected.get(&key) else {
            diffs.push(format!("{key:?}: missing from the expected snapshot"));
            continue;
        };
        if e != r {
            let es: HashSet<_> = e.targets.iter().collect();
            let rs: HashSet<_> = r.targets.iter().collect();
            diffs.push(format!(
                "{} {:?}: count {} -> {}; removed {:?}; added {:?}; same set but reordered: {}; hash {} -> {}; err {:?} -> {:?}",
                r.source,
                r.from,
                e.labels.len(),
                r.labels.len(),
                e.targets.iter().filter(|t| !rs.contains(t)).take(8).collect::<Vec<_>>(),
                r.targets.iter().filter(|t| !es.contains(t)).take(8).collect::<Vec<_>>(),
                es == rs,
                e.nav_html_hash,
                r.nav_html_hash,
                e.error,
                r.error
            ));
        }
    }
    eprintln!("crossref snapshot: compared {seen} records, {} differ", diffs.len());
    assert!(
        diffs.is_empty(),
        "cross-reference snapshot changed ({} records). If intended, regenerate and review:\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}

/// True if `small`'s elements appear in `big` in the same relative order.
fn is_subsequence(small: &[[i32; 4]], big: &[[i32; 4]]) -> bool {
    let mut it = big.iter();
    small.iter().all(|s| it.any(|b| b == s))
}

/// GATE 2 (fixture-only): every change from the auditors' frozen output is intended.
#[test]
fn expected_differs_from_audit_only_by_intended_deltas() {
    let audit = parse_file(AUDIT_FROZEN);
    let expected = parse_file(EXPECTED);
    assert_eq!(audit.len(), 100);
    assert_eq!(expected.len(), 100);
    let mut problems = Vec::new();
    for (key, a) in &audit {
        let Some(e) = expected.get(key) else {
            problems.push(format!("{key:?} missing from expected"));
            continue;
        };
        let (src, from) = (key.0.as_str(), key.1);
        assert!(a.error.is_none() && e.error.is_none(), "{key:?} errored");
        let a_nav: Vec<[i32; 4]> = a.targets.iter().copied().filter(|t| t[0] > 0).collect();
        let e_nav: Vec<[i32; 4]> = e.targets.iter().copied().filter(|t| t[0] > 0).collect();
        let a_set: HashSet<[i32; 4]> = a_nav.iter().copied().collect();
        // Rule W1 (all sources): a target missing from the frozen file must point into a book
        // that only WP1's aliases made resolvable.
        for t in &e_nav {
            if !a_set.contains(t) && !WP1_ALIAS_BOOKS.contains(&t[0]) && src != SRC_OPENBIBLE {
                problems.push(format!("{src} {from:?}: new target {t:?} is not explained by WP1"));
            }
        }
        match src {
            // TSKe rows are single-verse: nothing may be lost or reordered, only WP1 additions.
            SRC_TSKE => {
                if !is_subsequence(&a_nav, &e_nav) {
                    problems.push(format!("{src} {from:?}: frozen targets lost or reordered"));
                }
                if e.raw_blocks != a.raw_blocks {
                    problems.push(format!("{src} {from:?}: raw blocks {} -> {}", a.raw_blocks, e.raw_blocks));
                }
            }
            // OpenBible: identical, except reversed ranges become the single start verse (ISS-41).
            SRC_OPENBIBLE => {
                let norm: Vec<[i32; 4]> = a_nav
                    .iter()
                    .map(|t| if t[3] < t[2] { [t[0], t[1], t[2], t[2]] } else { *t })
                    .collect();
                if norm != e_nav {
                    problems.push(format!("{src} {from:?}: OpenBible output changed beyond ISS-41"));
                }
            }
            _ => {}
        }
        intended_iss39_rules(src, from, a, e, &a_nav, &e_nav, &mut problems);
    }
    assert!(problems.is_empty(), "unintended snapshot deltas:\n{}", problems.join("\n"));
}

/// ISS-39 rules for the multi-verse-row sources.
fn intended_iss39_rules(
    src: &str,
    from: (i32, i32, i32),
    a: &Rec,
    e: &Rec,
    _a_nav: &[[i32; 4]],
    e_nav: &[[i32; 4]],
    problems: &mut Vec<String>,
) {
    let has = |t: [i32; 4]| e_nav.contains(&t);
    match src {
        SRC_NASB => {
            // Step 1: the anchor ref itself (the tapped verse) is no longer listed.
            if has([from.0, from.1, from.2, from.2]) {
                problems.push(format!("{src} {from:?}: still lists its own anchor"));
            }
            if e.labels.len() > 80 || e.labels.len() > a.labels.len() {
                problems.push(format!("{src} {from:?}: {} entries (frozen {})", e.labels.len(), a.labels.len()));
            }
            if e.raw_blocks != 0 {
                problems.push(format!("{src} {from:?}: NASB raw block"));
            }
            if from == (43, 3, 16) {
                for want in [[45, 5, 8, 8], [62, 4, 9, 9]] {
                    if !has(want) {
                        problems.push(format!("NASB Jn 3:16 lost {want:?}"));
                    }
                }
                for bad in [[43, 7, 50, 50], [40, 23, 7, 7], [43, 3, 15, 15], [43, 3, 1, 1]] {
                    if has(bad) {
                        problems.push(format!("NASB Jn 3:16 still lists other-verse ref {bad:?}"));
                    }
                }
            }
        }
        SRC_TCB | SRC_THOMPSON => {
            // Step 2: a raw block never sits next to real hits.
            if !e_nav.is_empty() && e.raw_blocks != 0 {
                problems.push(format!("{src} {from:?}: raw block mixed with hits"));
            }
            if src == SRC_TCB && from == (43, 3, 16) {
                if !has([42, 2, 14, 14]) || !e_nav.iter().any(|t| t[0] == 43) {
                    problems.push("TCB Jn 3:16 lost Lc 2:14 or its John refs".into());
                }
                // Chapter-outline ranges (Jua 3:1-13, 3:18-21, 3:23-36) are gone.
                if e_nav.iter().any(|t| t[0] == 43 && t[1] == 3 && t[2] != t[3]) {
                    problems.push("TCB Jn 3:16 still lists chapter-outline ranges".into());
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------
// Golden tests against the real library (skip when it is absent).
// ---------------------------------------------------------------------------------------

/// The 44 verses of the audit probe (`verses40()`).
fn verses44() -> Vec<(i32, i32, i32)> {
    let mut v = snapshot_verses();
    v.extend([
        (44, 28, 31), (58, 1, 1), (58, 13, 25), (66, 1, 1), (31, 1, 1), (31, 1, 21), (57, 1, 1),
        (57, 1, 25), (63, 1, 1), (64, 1, 14), (65, 1, 1), (65, 1, 25), (29, 2, 28),
        (29, 2, 32), (29, 3, 1), (39, 4, 1), (39, 3, 18), (42, 2, 14), (7, 13, 5),
        (8, 1, 16), (21, 1, 2), (22, 2, 1), (36, 3, 17), (59, 1, 5),
    ]);
    assert_eq!(v.len(), 44);
    v
}

fn targets_of(p: &CrossRefPayload) -> Vec<[i32; 4]> {
    p.entries
        .iter()
        .map(|e| [e.to_book, e.to_chapter, e.to_verse_start, e.to_verse_end])
        .collect()
}

#[test]
fn golden_nasb_lists_only_the_tapped_verse() {
    if !library_present() {
        eprintln!("skip golden_nasb_lists_only_the_tapped_verse: module library not found at {}", *LIB);
        return;
    }
    let mods = scan_modules(&LIB).unwrap();
    let nasb = format!("{}/nasb.cmti", *LIB);
    let jn = load_cross_references(&mods, &LIB, 43, 3, 16, Some(&nasb), 80).unwrap();
    let t = targets_of(&jn);
    assert!(t.contains(&[45, 5, 8, 8]) && t.contains(&[62, 4, 9, 9]), "NASB Jn 3:16: {t:?}");
    assert!(!t.contains(&[43, 7, 50, 50]) && !t.contains(&[40, 23, 7, 7]), "NASB Jn 3:16: {t:?}");
    // Ro 8:28: the two `(2) Rom 8:28:` paragraphs hold exactly these 8 linked refs; the
    // `[3]` footnote is dropped.
    let ro = load_cross_references(&mods, &LIB, 45, 8, 28, Some(&nasb), 80).unwrap();
    let mut got: Vec<String> = ro.entries.iter().map(|e| e.label.clone()).collect();
    got.sort();
    let mut want: Vec<String> = ["Ro 8:32", "Ro 8:30", "1Co 1:9", "Gá 1:6", "Ef 1:11", "2Ts 2:14", "He 9:15", "1P 2:9"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    want.sort();
    assert_eq!(got.len(), 8, "NASB Ro 8:28: {got:?}");
    assert_eq!(
        ro.entries.iter().filter(|e| e.to_book == 45 && e.to_chapter == 8 && e.to_verse_start == 28).count(),
        0
    );
    // Labels come from BOOKS abbreviations; compare by target to stay independent of them.
    let tro = targets_of(&ro);
    for w in [[45, 8, 32, 32], [45, 8, 30, 30], [46, 1, 9, 9], [48, 1, 6, 6], [49, 1, 11, 11], [53, 2, 14, 14], [58, 9, 15, 15], [60, 2, 9, 9]] {
        assert!(tro.contains(&w), "NASB Ro 8:28 missing {w:?}: {got:?} (want {want:?})");
    }
    // Lk 24:53 has no anchor paragraph in NASB: an honest empty list with the Spanish note.
    let lk = load_cross_references(&mods, &LIB, 42, 24, 53, Some(&nasb), 80).unwrap();
    assert!(lk.entries.is_empty(), "NASB Lk 24:53: {:?}", targets_of(&lk));
    assert!(lk.note.as_deref().unwrap_or("").starts_with("Sin referencias para"));
    // Never more than 80 entries over the probe's 44 verses, even without the opt-in limit.
    for (b, c, v) in verses44() {
        let p = load_cross_references(&mods, &LIB, b, c, v, Some(&nasb), 80).unwrap();
        assert!(p.entries.len() <= 80, "NASB {b}/{c}/{v}: {} entries", p.entries.len());
        assert!(
            !p.entries.iter().any(|e| e.to_book == b && e.to_chapter == c && e.to_verse_start == v && e.to_verse_end == v),
            "NASB {b}/{c}/{v} lists its own anchor"
        );
    }
}

#[test]
fn golden_tske_and_tcb() {
    if !library_present() {
        eprintln!("skip golden_tske_and_tcb: module library not found at {}", *LIB);
        return;
    }
    let mods = scan_modules(&LIB).unwrap();
    let tske = format!("{}/tske_v12.cmti", *LIB);
    // TSKe is untouched by WP6. After WP1, Luk 2:14 resolves and leads (84 entries); before
    // WP1 the list was 80 entries starting with Ro 5:8.
    let jn = load_cross_references(&mods, &LIB, 43, 3, 16, Some(&tske), 80).unwrap();
    let labels: Vec<&str> = jn.entries.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels.len(), 84, "TSKe Jn 3:16: {labels:?}");
    assert_eq!(&labels[..2], &["Lc 2:14", "Ro 5:8"]);
    // The commentary limit is opt-in: complete by default, capped with limit_commentary.
    let mt = load_cross_references(&mods, &LIB, 40, 28, 20, Some(&tske), 80).unwrap();
    assert!(mt.entries.len() > 80, "TSKe Mt 28:20 should be complete by default");
    let opts = CrossRefOptions { limit_commentary: true, ..Default::default() };
    let mt80 = load_cross_references_with(&mods, &LIB, 40, 28, 20, Some(&tske), 80, opts).unwrap();
    assert_eq!(mt80.entries.len(), 80);
    assert_eq!(targets_of(&mt80)[..], targets_of(&mt)[..80], "limit keeps the first 80 after dedupe");

    // TCB (opt-in source): Jn 3:16 from its verse row only, no chapter-outline ranges.
    let tcb = format!("{}/02TCB.cmti", *LIB);
    let rec = recommended_payload(&mods, &tcb, 43, 3, 16);
    let t = targets_of(&rec);
    assert!(t.contains(&[42, 2, 14, 14]) && t.contains(&[43, 1, 14, 14]), "TCB Jn 3:16: {t:?}");
    assert!(!t.iter().any(|x| x[0] == 43 && x[1] == 3 && x[2] != x[3]), "TCB Jn 3:16: {t:?}");
    assert!(!t.iter().any(|x| x[0] == 0), "no raw block next to hits");
}

fn recommended_payload(mods: &[ModuleInfo], path: &str, b: i32, c: i32, v: i32) -> CrossRefPayload {
    let p = load_cross_references_with(mods, &LIB, b, c, v, Some(path), 80, CrossRefOptions::recommended()).unwrap();
    assert_eq!(p.source_path, path, "recommended options must offer {path}");
    p
}

#[test]
fn golden_source_list_default_and_opt_in() {
    if !library_present() {
        eprintln!("skip golden_source_list_default_and_opt_in: module library not found at {}", *LIB);
        return;
    }
    let mods = scan_modules(&LIB).unwrap();
    let names = |v: &[CrossRefSource]| -> Vec<String> {
        v.iter().map(|s| s.path.rsplit('/').next().unwrap().to_string()).collect()
    };
    // Default (today's behaviour, pending plan question 1): NASB then TSKe in scan order.
    let def = list_cross_reference_sources(&mods, &LIB);
    assert_eq!(names(&def), ["nasb.cmti", "tske_v12.cmti"], "default source list changed");
    let payload = load_cross_references(&mods, &LIB, 43, 3, 16, None, 80).unwrap();
    assert!(payload.source_path.ends_with("nasb.cmti"), "default source changed");
    // Opt-in: verse-granular first, Spanish TCB/Thompson offered, ordinary notes never.
    let rec = list_cross_reference_sources_with(&mods, &LIB, CrossRefOptions::recommended());
    let n = names(&rec);
    let pos = |f: &str| n.iter().position(|x| x == f);
    assert!(pos("tske_v12.cmti") < pos("02TCB.cmti") && pos("02TCB.cmti") < pos("nasb.cmti"), "{n:?}");
    assert!(pos("02THOMPSON.cmti").is_some(), "{n:?}");
    for not in ["01RVR1960x.cmti", "02RVR1960x.cmti", "tnm_traduccin_del_nuevo_mundo_1987.cmti"] {
        assert!(pos(not).is_none(), "{not} must not be a cross-reference source: {n:?}");
    }
    let payload = load_cross_references_with(&mods, &LIB, 43, 3, 16, None, 80, CrossRefOptions::recommended()).unwrap();
    assert!(payload.source_path.ends_with("tske_v12.cmti"), "recommended default should be TSKe");
    // A stale saved path falls back to the first readable source.
    let stale = load_cross_references(&mods, &LIB, 43, 3, 16, Some("/nope/missing.cmti"), 80).unwrap();
    assert!(stale.source_path.ends_with("nasb.cmti"));
}

/// Every NASB chapter row is recognised as anchor-structured, so no chapter ever falls back to
/// the chapter-wide list. Also counts `(n)` paragraphs whose anchor can't be resolved.
#[test]
fn golden_every_nasb_chapter_row_is_structured() {
    if !library_present() {
        eprintln!("skip golden_every_nasb_chapter_row_is_structured: module library not found at {}", *LIB);
        return;
    }
    let nasb = format!("{}/nasb.cmti", *LIB);
    let mut rows = 0;
    let mut unstructured = Vec::new();
    for b in 1..=66 {
        let chapters = crate::books::book_by_number(b).map(|x| x.chapters).unwrap_or(0);
        for c in 1..=chapters {
            let entries = crate::modules::load_commentary(&nasb, b, c, 1, true, false, false).unwrap();
            for e in entries.iter().filter(|e| e.label != "v.1") {
                rows += 1;
                if anchored_hits(&e.html, b, c, 1).is_none() {
                    unstructured.push(format!("{b}/{c} {}", e.label));
                }
            }
        }
    }
    eprintln!("GATE NASB structure: {rows} multi-verse rows, {} without (n) anchors: {:?}", unstructured.len(), &unstructured[..unstructured.len().min(10)]);
    assert!(rows >= 1180, "expected ~1,189 NASB chapter rows, got {rows}");
    assert!(unstructured.is_empty(), "NASB rows not recognised as structured: {unstructured:?}");
}

/// Panel quality and latency over the probe's 44 verses (the WP6 ship gate). Prints the same
/// counters as the audit probe. Asserts TSKe validity, no self-references, and no raw blocks
/// mixed with hits.
#[test]
fn golden_panel_quality_44_verses() {
    if !library_present() {
        eprintln!("skip golden_panel_quality_44_verses: module library not found at {}", *LIB);
        return;
    }
    let mods = scan_modules(&LIB).unwrap();
    let rvr = format!("{}/00RVR1960.bbli", *LIB);
    let mut vc: std::collections::HashMap<(i32, i32), i32> = Default::default();
    for (title, file) in [("NASB", "nasb.cmti"), ("TSKe", "tske_v12.cmti"), ("TCB", "02TCB.cmti"), ("Thompson", "02THOMPSON.cmti")] {
        let path = format!("{}/{file}", *LIB);
        let (mut entries, mut valid, mut nav, mut selfr, mut same_ch, mut synth, mut with) = (0, 0, 0, 0, 0, 0, 0);
        let mut exact_self = 0;
        let mut total_ms = 0.0f64;
        let mut max_ms = 0.0f64;
        for (b, c, v) in verses44() {
            let t0 = std::time::Instant::now();
            let p = load_cross_references_with(&mods, &LIB, b, c, v, Some(&path), 80, CrossRefOptions { include_spanish_sources: true, ..Default::default() }).unwrap();
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            total_ms += ms;
            max_ms = max_ms.max(ms);
            assert_eq!(p.source_path, path);
            if !p.entries.is_empty() {
                with += 1;
            }
            let has_nav = p.entries.iter().any(|e| e.to_book > 0);
            for e in &p.entries {
                entries += 1;
                if e.to_book <= 0 {
                    synth += 1;
                    assert!(!has_nav, "{title} {b}/{c}/{v}: raw block mixed with hits");
                    continue;
                }
                nav += 1;
                let n = *vc
                    .entry((e.to_book, e.to_chapter))
                    .or_insert_with(|| crate::modules::verse_count(&rvr, e.to_book, e.to_chapter).unwrap_or(0));
                if n > 0 && e.to_verse_start >= 1 && e.to_verse_end >= e.to_verse_start && e.to_verse_end <= n {
                    valid += 1;
                }
                if e.to_book == b && e.to_chapter == c {
                    if e.to_verse_start <= v && v <= e.to_verse_end {
                        selfr += 1;
                        if e.to_verse_start == v && e.to_verse_end == v {
                            exact_self += 1;
                        }
                        eprintln!("  self-ref {title} {b}/{c}/{v} -> {}", e.label);
                    } else {
                        same_ch += 1;
                    }
                }
            }
        }
        eprintln!(
            "GATE {title}: verses_with_entries {with}/44 entries {entries} navigable {nav} raw {synth} valid {valid}/{nav} self-refs {selfr} (exact {exact_self}) same-chapter-other {same_ch} latency avg {:.2} ms max {:.2} ms",
            total_ms / 44.0,
            max_ms
        );
        // Plan must_not_break: panel latency <= 5 ms per verse. Only meaningful in an optimised
        // build (debug is ~10x slower); run with `cargo test --release --lib golden_panel_quality`.
        // Measured 2026-11-01 (release): NASB avg 1.4 ms, TSKe avg 1.75 ms; the max (7-9 ms) is the
        // first call, which opens the module connection.
        if !cfg!(debug_assertions) {
            assert!(total_ms / 44.0 <= 5.0, "{title}: average panel latency {:.2} ms > 5 ms", total_ms / 44.0);
        }
        if title == "TSKe" {
            assert_eq!(valid, nav, "TSKe targets must stay 100% valid");
            assert_eq!(selfr, 0, "TSKe has no self-references");
        }
        if title == "NASB" {
            // The anchor itself is dropped. NASB's own range notes inside the verse's paragraph
            // (Mt 1:1 "(2) Mat 1:1: Mat 1:1-6: Luk 3:32-34") are legitimate and stay.
            assert_eq!(exact_self, 0, "NASB anchors are dropped");
        }
    }
}

/// Parse the tricky-refs fixture into (input, html, hits) triples.
fn parse_tricky(s: &str) -> Vec<(String, String, Vec<String>)> {
    let lines: Vec<&str> = s.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 2 < lines.len() {
        let input = lines[i].to_string();
        let html = lines[i + 1].trim_start().strip_prefix("html: ").expect("html line").to_string();
        let hits_json = lines[i + 2].trim_start().strip_prefix("hits: ").expect("hits line");
        let hits: Vec<String> = serde_json::from_str(hits_json).expect("hits are a JSON array");
        out.push((input, html, hits));
        i += 3;
    }
    out
}

/// GATE 3 (no library needed): tricky `<ref>` shapes keep their hits. The only intended
/// deltas are the WP1 aliases (Luk, Jua) and WP2's ISS-25 cross-chapter parse. The plan has
/// text.rs render `c:v-c2:v2` with data-chapter-end/data-verse-end2 and no data-verse-end, so
/// crossrefs reads the start verse; before WP2 the ref was a raw span with no hit. The
/// rendered html is not compared byte-for-byte, because WP2 may legitimately change escaping,
/// but a parsed ref must keep `class="bible-ref"` as its FIRST attribute (crossrefs' RE_SPAN
/// depends on that).
#[test]
fn tricky_refs_hits_match_frozen() {
    let frozen = parse_tricky(TRICKY_FROZEN);
    assert_eq!(frozen.len(), 13, "tricky fixture has 13 cases");
    let wp1: BTreeMap<&str, Vec<String>> = [
        ("<ref>Luk 2:14</ref>", vec!["Lc 2:14@42/2/14-14".to_string()]),
        ("<ref>Jua 3:16</ref>", vec!["Jn 3:16@43/3/16-16".to_string()]),
    ]
    .into_iter()
    .collect();
    let mut problems = Vec::new();
    for (input, frozen_html, frozen_hits) in &frozen {
        let html = clean_esword_text(input);
        let hits: Vec<String> = hits_from_commentary_html(&html)
            .iter()
            .map(|x| format!("{}@{}/{}/{}-{}", x.label, x.to_book, x.to_chapter, x.to_verse_start, x.to_verse_end))
            .collect();
        let want = wp1.get(input.as_str()).unwrap_or(frozen_hits);
        let iss25 = input == "<ref>Joh 3:16-4:2</ref>" && hits == ["Jn 3:16@43/3/16-16"];
        if &hits != want && !iss25 {
            problems.push(format!("{input}: hits {hits:?}, want {want:?}"));
        }
        if frozen_html.starts_with("<span class=\"bible-ref\"") && !html.starts_with("<span class=\"bible-ref\"") {
            problems.push(format!("{input}: bible-ref span lost its class-first shape: {html}"));
        }
    }
    assert!(problems.is_empty(), "tricky refs regressed:\n{}", problems.join("\n"));
}

// ---------------------------------------------------------------------------------------
// Property checks against the real library (QA stage; skip when it is absent).
// ---------------------------------------------------------------------------------------

/// Every VerseCommentary row of `file`, read straight from SQLite (read-only), as
/// (book, chapter_begin, verse_begin, chapter_end, verse_end, raw Comments).
fn raw_rows(file: &str) -> Vec<(i32, i32, i32, i32, i32, String)> {
    let path = format!("{}/{file}", *LIB);
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .expect("open module read-only");
    let mut stmt = conn
        .prepare("SELECT Book, ChapterBegin, VerseBegin, ChapterEnd, VerseEnd, Comments FROM VerseCommentary")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i32>(0).unwrap_or(0),
                r.get::<_, i32>(1).unwrap_or(0),
                r.get::<_, i32>(2).unwrap_or(0),
                r.get::<_, Option<i32>>(3).ok().flatten().unwrap_or(0),
                r.get::<_, Option<i32>>(4).ok().flatten().unwrap_or(0),
                r.get::<_, Option<String>>(5).ok().flatten().unwrap_or_default(),
            ))
        })
        .unwrap();
    rows.flatten().collect()
}

/// Independent oracle for ISS-39 step 1. Instead of anchored_hits' marker state machine, split
/// the RAW NASB row into `<p>` paragraphs. A paragraph whose text starts with `(n)` is a
/// cross-reference paragraph; its first `<ref>` is the anchor. The paragraph's other refs belong
/// to every verse the anchor covers. Over all 1,189 NASB chapters and every anchored verse,
/// anchored_hits must equal this oracle exactly (same targets, same order), and must never
/// invent a target that the unfiltered row does not contain. By default every 7th chapter is
/// checked (~1/7 of the 31,819 verses, a few seconds in a debug build); set
/// ESPADA_FULL_CROSSREF_ORACLE=1 to check all of them (~90 s debug, 0 mismatches on 2026-11-01).
#[test]
fn golden_nasb_anchor_filter_matches_paragraph_oracle() {
    if !library_present() {
        eprintln!("skip golden_nasb_anchor_filter_matches_paragraph_oracle: module library not found at {}", *LIB);
        return;
    }
    let re_tag = regex::Regex::new(r"<[^>]*>").unwrap();
    let re_marker = regex::Regex::new(r"^[\s\u{a0}]*\(\d{1,3}\)").unwrap();
    let re_first_ref = regex::Regex::new(r"(?s)<ref>(.*?)</ref>").unwrap();
    let re_anchor = regex::Regex::new(r"^(.+?)\s+(\d+):(\d+)(?:\s*[-–]\s*(\d+))?$").unwrap();
    let (mut chapters, mut verses_checked, mut mismatches, mut anchors_unparsed) = (0, 0, Vec::new(), 0);
    let t0 = std::time::Instant::now();
    let full = std::env::var("ESPADA_FULL_CROSSREF_ORACLE").ok().as_deref() == Some("1");
    for (i, (b, c, _vb, _ce, _ve, raw)) in raw_rows("nasb.cmti").into_iter().enumerate() {
        if !full && i % 7 != 0 {
            continue;
        }
        chapters += 1;
        // (anchor verses covered, targets of the rest of the paragraph)
        let mut paras: Vec<(i32, i32, Vec<[i32; 4]>)> = Vec::new();
        for p in raw.split("<p>") {
            if !re_marker.is_match(&re_tag.replace_all(p, "")) {
                continue;
            }
            let Some(m) = re_first_ref.captures(p) else { continue };
            let whole = m.get(0).unwrap();
            let rest = format!("{}{}", &p[..whole.start()], &p[whole.end()..]);
            let ts: Vec<[i32; 4]> = hits_from_commentary_html(&clean_esword_text(&rest))
                .iter()
                .map(|h| [h.to_book, h.to_chapter, h.to_verse_start, h.to_verse_end])
                .collect();
            let anchor = m[1].trim().to_string();
            let Some(a) = re_anchor.captures(&anchor) else {
                anchors_unparsed += 1;
                continue;
            };
            let (ab, ac) = (crate::books::resolve_book_name(&a[1]), a[2].parse::<i32>().unwrap());
            let v1: i32 = a[3].parse().unwrap();
            let v2: i32 = a.get(4).map(|x| x.as_str().parse().unwrap()).unwrap_or(v1);
            if ab == Some(b) && ac == c {
                paras.push((v1, v2.max(v1), ts));
            }
        }
        let html = clean_esword_text(&raw);
        let all: HashSet<[i32; 4]> = hits_from_commentary_html(&html)
            .iter()
            .map(|h| [h.to_book, h.to_chapter, h.to_verse_start, h.to_verse_end])
            .collect();
        let max_v = paras.iter().map(|p| p.1).max().unwrap_or(0) + 1;
        for v in 1..=max_v {
            verses_checked += 1;
            let mut seen = HashSet::new();
            let want: Vec<[i32; 4]> = paras
                .iter()
                .filter(|p| p.0 <= v && v <= p.1)
                .flat_map(|p| p.2.iter().copied())
                .filter(|t| seen.insert(*t))
                .collect();
            let got: Vec<[i32; 4]> = anchored_hits(&html, b, c, v)
                .expect("NASB rows are structured")
                .iter()
                .map(|h| [h.to_book, h.to_chapter, h.to_verse_start, h.to_verse_end])
                .collect();
            assert!(got.iter().all(|t| all.contains(t)), "{b}/{c}/{v}: anchored_hits invented a target");
            if got != want {
                mismatches.push(format!("{b}/{c}/{v}: got {got:?} want {want:?}"));
            }
        }
    }
    eprintln!(
        "GATE NASB oracle: {chapters} chapters, {verses_checked} verses, {} mismatches, {anchors_unparsed} anchors the oracle could not parse, {:.1}s",
        mismatches.len(),
        t0.elapsed().as_secs_f64()
    );
    assert!(chapters >= if full { 1180 } else { 165 }, "only {chapters} NASB chapters checked");
    assert!(mismatches.is_empty(), "{} verses differ from the paragraph oracle:\n{}", mismatches.len(), mismatches[..mismatches.len().min(20)].join("\n"));
}

/// TCB chapter outlines and Thompson verse groups / book intros are NOT anchor-structured, so
/// ISS-39 step 1 must never parse them as NASB-style rows (that would silently empty them).
/// Thompson writes "Los cielos (1), <ref>…" (a chain count, not an anchor marker).
#[test]
fn golden_non_nasb_multi_verse_rows_are_never_anchor_parsed() {
    if !library_present() {
        eprintln!("skip golden_non_nasb_multi_verse_rows_are_never_anchor_parsed: module library not found at {}", *LIB);
        return;
    }
    for file in ["02TCB.cmti", "02THOMPSON.cmti", "tske_v12.cmti"] {
        let mut multi = 0;
        let mut parsed = Vec::new();
        for (b, cb, vb, ce, ve, raw) in raw_rows(file) {
            let (ce, ve) = (if ce == 0 { cb } else { ce }, if ve == 0 { vb } else { ve });
            if (cb, vb) == (ce, ve) || raw.trim().is_empty() {
                continue;
            }
            multi += 1;
            if anchored_hits(&clean_esword_text(&raw), b, cb, vb).is_some() {
                parsed.push(format!("{b}/{cb}:{vb}-{ce}:{ve}"));
            }
        }
        eprintln!("GATE {file}: {multi} multi-verse rows, {} anchor-parsed", parsed.len());
        assert!(parsed.is_empty(), "{file}: rows wrongly treated as NASB-structured: {:?}", &parsed[..parsed.len().min(10)]);
        if file == "tske_v12.cmti" {
            assert_eq!(multi, 0, "TSKe is single-verse only; WP6 relies on that to leave it untouched");
        } else {
            assert!(multi > 1000, "{file}: expected >1000 multi-verse rows, got {multi}");
        }
    }
}

/// ISS-39 step 2 on real data: for every source and every probe verse, the rows load_commentary
/// returns give the same panel in SQL order and reversed. Which entries appear, and whether a raw
/// block appears, never depends on row order. The entry ORDER is fully order-independent when no
/// two matched rows share a verse span. Rows with the same span keep SQL (rowid) order. In 02TCB
/// the outline of a one-chapter book (Abd, Flm, 2Jn, 3Jn, Jud) is stored as a 1:1 row beside
/// the 1:1 verse row, so those verses list the same refs in rowid order.
#[test]
fn golden_panel_is_independent_of_row_order() {
    if !library_present() {
        eprintln!("skip golden_panel_is_independent_of_row_order: module library not found at {}", *LIB);
        return;
    }
    let (mut multi_row_verses, mut same_span_verses) = (0, Vec::new());
    for file in ["nasb.cmti", "tske_v12.cmti", "02TCB.cmti", "02THOMPSON.cmti"] {
        let path = format!("{}/{file}", *LIB);
        for (b, c, v) in verses44() {
            let rows = crate::modules::load_commentary(&path, b, c, v, true, false, false).unwrap();
            if rows.len() > 1 {
                multi_row_verses += 1;
            }
            let key = |h: Vec<CrossRefHit>| h.into_iter().map(|e| (e.label, e.html)).collect::<Vec<_>>();
            let fwd = key(collect_commentary_hits(&rows, b, c, v));
            let mut rev_rows = rows.clone();
            rev_rows.reverse();
            let rev = key(collect_commentary_hits(&rev_rows, b, c, v));
            let labels: HashSet<&str> = rows.iter().map(|r| r.label.as_str()).collect();
            if labels.len() == rows.len() {
                assert_eq!(fwd, rev, "{file} {b}/{c}/{v}: row order changes the panel");
            } else {
                same_span_verses.push(format!("{file} {b}/{c}/{v}"));
                let (mut f, mut r) = (fwd.clone(), rev.clone());
                f.sort();
                r.sort();
                assert_eq!(f, r, "{file} {b}/{c}/{v}: row order changes which entries are listed");
            }
        }
    }
    eprintln!("GATE row order: {multi_row_verses} multi-row verses; same-span rows (set-compared) at {same_span_verses:?}");
    // The property is only meaningful if some verses really match several rows (TCB/Thompson).
    assert!(multi_row_verses >= 40, "only {multi_row_verses} verses matched more than one row");
}

/// ISS-39 step 3 on real data: with limit_commentary, TSKe Mt 28:20 (181 refs) is exactly the
/// first clamp(limit, 1, 200) entries of the complete list; without it the list is complete.
#[test]
fn golden_commentary_limit_is_a_prefix_for_every_limit() {
    if !library_present() {
        eprintln!("skip golden_commentary_limit_is_a_prefix_for_every_limit: module library not found at {}", *LIB);
        return;
    }
    let mods = scan_modules(&LIB).unwrap();
    let tske = format!("{}/tske_v12.cmti", *LIB);
    let full = targets_of(&load_cross_references(&mods, &LIB, 40, 28, 20, Some(&tske), 80).unwrap());
    assert!(full.len() > 80 && full.len() < 200, "TSKe Mt 28:20 has {} entries", full.len());
    let on = CrossRefOptions { limit_commentary: true, ..Default::default() };
    for limit in [-1, 0, 1, 2, 79, 80, 81, 200, 201, 5000] {
        let got = targets_of(&load_cross_references_with(&mods, &LIB, 40, 28, 20, Some(&tske), limit, on).unwrap());
        let n = (limit.clamp(1, 200) as usize).min(full.len());
        assert_eq!(got, full[..n].to_vec(), "limit {limit}");
        let def = targets_of(&load_cross_references_with(&mods, &LIB, 40, 28, 20, Some(&tske), limit, CrossRefOptions::default()).unwrap());
        assert_eq!(def, full, "limit {limit} must not truncate commentary lists by default");
    }
}

/// ISS-41 on the real OpenBible data (repo data/ copy, read-only): for every source verse that
/// has a reversed range, no hit is reversed, the label shows the single start verse, and the
/// normalisation never creates a duplicate target.
#[test]
fn golden_openbible_reversed_ranges_across_the_database() {
    let ob = openbible_repo_db();
    if !std::path::Path::new(&ob).is_file() {
        eprintln!("skip golden_openbible_reversed_ranges_across_the_database: {ob} is absent");
        return;
    }
    let conn = Connection::open_with_flags(&ob, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT from_book, from_chapter, from_verse FROM cross_references
             WHERE to_verse_end < to_verse_start AND to_book BETWEEN 1 AND 66",
        )
        .unwrap();
    let froms: Vec<(i32, i32, i32)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .flatten()
        .collect();
    assert!(froms.len() > 400, "expected ~546 reversed rows, found {} source verses", froms.len());
    let mut fixed = 0;
    for (b, c, v) in froms {
        let hits = load_openbible(&ob, b, c, v, 200).unwrap();
        let mut seen = HashSet::new();
        for h in &hits {
            assert!(h.to_verse_end >= h.to_verse_start, "{b}/{c}/{v}: reversed hit {}", h.label);
            assert!(seen.insert((h.to_book, h.to_chapter, h.to_verse_start, h.to_verse_end)), "{b}/{c}/{v}: duplicate {}", h.label);
            let tail = h.label.rsplit(':').next().unwrap();
            if let Some((a, z)) = tail.split_once('-') {
                assert!(z.parse::<i32>().unwrap() > a.parse::<i32>().unwrap(), "{b}/{c}/{v}: label {}", h.label);
            } else if h.to_verse_end == h.to_verse_start {
                fixed += 1;
            }
        }
    }
    assert!(fixed > 0);
}
