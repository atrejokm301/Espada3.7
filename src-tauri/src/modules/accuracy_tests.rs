//! Accuracy gates for word taps (word → Strong), run on the real library.
//!
//! Scenario: a reader of a PLAIN Bible (00RVR1960, 00LBLA) taps a word. The truth is the
//! word's own tag in the Strong-tagged edition of the same text (00RVR1960+, 00LBLA+), taken
//! position by position on the verses whose words are identical in both editions. Each tap
//! sends what app.v7.js sends: the word, its occurrence among same-text words of the verse,
//! the Bible path and the iRV 1960+ interlinear — never the truth's tags.
//!
//! Both paths are measured: with the tagged twin (default) and the interlinear alone
//! (`skip_twin`). Slow and library-bound, so `#[ignore]`:
//!   cargo test --release --lib accuracy_ -- --ignored --nocapture
//! Env: `ACC_STEP=n` (every n-th verse), `ACC_OUT=/dir` (miss lists, one TSV per run).
//!
//! Independent check of the truth itself (2026-11-11): where the old interlinear answer and
//! RVR1960+ disagree and Webster 1833 (KJV Strong's) or LBLA+ backs only one side, RVR1960+
//! wins 5 146 to 5.

use super::golden_tests::library;
use super::*;
use std::io::Write as _;

/// Spanish function words: taps on these are reported apart from content words.
const FUNCTION_WORDS: &[&str] = &[
    "a", "al", "ante", "aquel", "aquella", "aquellas", "aquello", "aquellos", "como", "con",
    "contra", "cual", "cuales", "cuando", "de", "del", "desde", "donde", "e", "el", "ella",
    "ellas", "ello", "ellos", "en", "entre", "esa", "esas", "ese", "eso", "esos", "esta", "estas",
    "este", "esto", "estos", "hacia", "hasta", "la", "las", "le", "les", "lo", "los", "me", "mi",
    "mis", "mas", "ni", "no", "nos", "nosotros", "o", "os", "para", "pero", "por", "porque",
    "pues", "que", "quien", "quienes", "se", "si", "sin", "sino", "so", "sobre", "su", "sus", "te",
    "ti", "tu", "tus", "u", "un", "una", "unas", "uno", "unos", "vosotros", "y", "ya", "yo",
];

#[derive(Default, Debug, Clone, Copy)]
struct Tally {
    correct: usize,
    partial: usize,
    wrong: usize,
    none: usize,
}
impl Tally {
    fn n(&self) -> usize {
        self.correct + self.partial + self.wrong + self.none
    }
    fn pct(&self) -> f64 {
        100.0 * self.correct as f64 / self.n().max(1) as f64
    }
    fn line(&self, label: &str) -> String {
        let n = self.n().max(1) as f64;
        format!(
            "{label:<40} n={:>6}  correct {:>5.1}%  partial {:>4.1}%  wrong {:>4.1}%  none {:>4.1}%",
            self.n(),
            100.0 * self.correct as f64 / n,
            100.0 * self.partial as f64 / n,
            100.0 * self.wrong as f64 / n,
            100.0 * self.none as f64 / n
        )
    }
    fn add(&mut self, got: &[String], truth: &str) {
        if got.first().map(String::as_str) == Some(truth) {
            self.correct += 1
        } else if got.iter().any(|c| c == truth) {
            self.partial += 1
        } else if got.is_empty() {
            self.none += 1
        } else {
            self.wrong += 1
        }
    }
}

fn chapters(path: &str) -> Vec<(i32, i32)> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut st = conn
        .prepare("SELECT DISTINCT Book, Chapter FROM Bible WHERE Book BETWEEN 1 AND 66 ORDER BY Book, Chapter")
        .unwrap();
    let rows = st.query_map([], |r| Ok((r.get::<_, i32>(0)?, r.get::<_, i32>(1)?))).unwrap();
    rows.flatten().collect()
}

struct Run {
    content: Tally,
    content_ot: Tally,
    content_nt: Tally,
    repeated: Tally,
    function: Tally,
    verses_used: usize,
    verses_skipped: usize,
}

/// Taps every tagged word of `reading` (verses identical to `truth`) and scores the answers.
fn measure(reading: &str, truth: &str, il: &str, skip_twin: bool, miss_out: Option<&str>) -> Run {
    let step: usize = std::env::var("ACC_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
    let mut out = miss_out.map(|p| std::fs::File::create(p).unwrap());
    if let Some(f) = out.as_mut() {
        writeln!(f, "ref\tword\tocc\ttruth\tgot\tkind\ttwin\tclass").unwrap();
    }
    let opts = ResolveOpts { bible_path: Some(reading.to_string()), skip_twin, ..ResolveOpts::default() };
    let mut run = Run {
        content: Tally::default(),
        content_ot: Tally::default(),
        content_nt: Tally::default(),
        repeated: Tally::default(),
        function: Tally::default(),
        verses_used: 0,
        verses_skipped: 0,
    };
    let mut verse_no = 0usize;
    for (b, c) in chapters(truth) {
        let (Ok(tc), Ok(rc)) = (load_bible_chapter(truth, b, c), load_bible_chapter(reading, b, c)) else {
            continue;
        };
        let rmap: HashMap<i32, &VerseRow> = rc.verses.iter().map(|v| (v.verse, v)).collect();
        for tv in &tc.verses {
            verse_no += 1;
            if verse_no % step != 0 {
                continue;
            }
            let Some(rv) = rmap.get(&tv.verse) else { continue };
            let tt = verse_word_tokens(&tv.html);
            let rt = verse_word_tokens(&rv.html);
            // Clean truth only: the same words in the same order in both editions.
            if tt.len() != rt.len() || tt.iter().zip(&rt).any(|(a, b)| a.key != b.key) {
                run.verses_skipped += 1;
                continue;
            }
            run.verses_used += 1;
            let mut seen: HashMap<&str, usize> = HashMap::new();
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for t in &rt {
                *counts.entry(t.key.as_str()).or_default() += 1;
            }
            for (k, t) in rt.iter().enumerate() {
                let occ = {
                    let e = seen.entry(t.key.as_str()).or_default();
                    *e += 1;
                    *e - 1
                };
                let Some(truth_code) = primary_first(tt[k].strongs.clone()).into_iter().next() else {
                    continue;
                };
                let r = resolve_word_links_at(&t.text, &[], &[], Some(il), b, c, tv.verse, Some(occ), &opts);
                let class = if FUNCTION_WORDS.contains(&t.key.as_str()) { "function" } else { "content" };
                if class == "content" {
                    run.content.add(&r.strongs, &truth_code);
                    if b <= 39 {
                        run.content_ot.add(&r.strongs, &truth_code);
                    } else {
                        run.content_nt.add(&r.strongs, &truth_code);
                    }
                    if counts[t.key.as_str()] > 1 {
                        run.repeated.add(&r.strongs, &truth_code);
                    }
                } else {
                    run.function.add(&r.strongs, &truth_code);
                }
                if r.strongs.first() != Some(&truth_code) {
                    if let Some(f) = out.as_mut() {
                        writeln!(
                            f,
                            "{b}:{c}:{}\t{}\t{occ}\t{}\t{}\t{}\t{}\t{class}",
                            tv.verse,
                            t.text,
                            tt[k].strongs.join(" "),
                            r.strongs.join(" "),
                            r.match_kind,
                            r.tagged_twin.as_deref().unwrap_or("")
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
    run
}

fn report(title: &str, r: &Run) {
    eprintln!(
        "── {title}  (verses scored {}, skipped as not word-identical {})",
        r.verses_used, r.verses_skipped
    );
    eprintln!("{}", r.content.line("content words"));
    eprintln!("{}", r.content_ot.line("  Old Testament"));
    eprintln!("{}", r.content_nt.line("  New Testament"));
    eprintln!("{}", r.repeated.line("content words repeated in verse"));
    eprintln!("{}", r.function.line("function words"));
}

fn lib_paths(names: &[&str]) -> Option<Vec<String>> {
    let lib = library()?;
    let v: Vec<_> = names.iter().map(|n| lib.join(n)).collect();
    if let Some(m) = v.iter().find(|p| !p.is_file()) {
        eprintln!("SKIP: {} missing", m.display());
        return None;
    }
    Some(v.iter().map(|p| p.to_string_lossy().to_string()).collect())
}

#[test]
fn verse_word_tokens_reads_own_tags_and_skips_codes() {
    let html = r#"<span class="word" data-strong="G3588 G2316">Dios</span> al <span class="morph">VAAI3S</span> mundo <sup class="fn">[a]</sup>, <span class="strong-ref" data-strong="G25">G25</span> amó 3 H430"#;
    let t = verse_word_tokens(html);
    let words: Vec<_> = t.iter().map(|x| x.text.as_str()).collect();
    assert_eq!(words, ["Dios", "al", "mundo", "amó"]);
    assert_eq!(t[0].strongs, ["G3588", "G2316"]);
    assert!(t[1].strongs.is_empty());
    assert_eq!(t[3].key, "amo");
}

#[test]
#[ignore = "slow: real library, every tagged word of RVR1960+ through the plain RVR1960"]
fn accuracy_tap_plain_rvr1960() {
    let Some(p) = lib_paths(&["00RVR1960.bbli", "00RVR1960+.bbli", "00Interlineal-iRV 1960+.bbli"]) else {
        return;
    };
    let dir = std::env::var("ACC_OUT").ok();
    let file = |n: &str| dir.as_ref().map(|d| format!("{d}/{n}"));
    let t0 = std::time::Instant::now();
    let twin = measure(&p[0], &p[1], &p[2], false, file("rvr1960-twin.tsv").as_deref());
    let il = measure(&p[0], &p[1], &p[2], true, file("rvr1960-interlinear.tsv").as_deref());
    eprintln!("ACCURACY plain RVR1960 taps vs RVR1960+ own tags ({:.1}s)", t0.elapsed().as_secs_f64());
    report("interlinear only (before)", &il);
    report("with tagged twin (now)", &twin);
    assert!(twin.content.n() > 100_000, "too few taps: {}", twin.content.n());
    assert!(twin.content.pct() >= il.content.pct(), "the twin made content taps worse");
}

#[test]
#[ignore = "slow: real library, every tagged word of LBLA+ through the plain LBLA"]
fn accuracy_tap_plain_lbla() {
    let Some(p) = lib_paths(&["00LBLA.bbli", "00LBLA+.bbli", "00Interlineal-iRV 1960+.bbli"]) else {
        return;
    };
    let dir = std::env::var("ACC_OUT").ok();
    let file = |n: &str| dir.as_ref().map(|d| format!("{d}/{n}"));
    let t0 = std::time::Instant::now();
    let twin = measure(&p[0], &p[1], &p[2], false, file("lbla-twin.tsv").as_deref());
    let il = measure(&p[0], &p[1], &p[2], true, file("lbla-interlinear.tsv").as_deref());
    eprintln!("ACCURACY plain LBLA taps vs LBLA+ own tags ({:.1}s)", t0.elapsed().as_secs_f64());
    report("interlinear only (before)", &il);
    report("with tagged twin (now)", &twin);
    assert!(twin.content.pct() >= il.content.pct(), "the twin made content taps worse");
}

#[test]
#[ignore = "slow: real library, twin detection for every .bbli"]
fn report_tagged_twins() {
    let Some(lib) = library() else { return };
    let mut paths: Vec<_> = std::fs::read_dir(&lib)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("bbli"))
        .collect();
    paths.sort();
    let (mut with, mut slowest) = (0usize, (0.0f64, String::new()));
    for p in &paths {
        let ps = p.to_string_lossy().to_string();
        let t = std::time::Instant::now();
        let twin = tagged_twin(&ps);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        if ms > slowest.0 {
            slowest = (ms, ps.clone());
        }
        if let Some(tw) = twin.filter(|tw| *tw != ps) {
            with += 1;
            let name = |x: &str| std::path::Path::new(x).file_name().unwrap().to_string_lossy().to_string();
            eprintln!("TWIN {:<70} → {}  ({ms:.0} ms)", name(&ps), name(&tw));
        }
    }
    eprintln!("{with} of {} Bibles have a tagged twin; slowest first detection {:.0} ms ({})", paths.len(), slowest.0, slowest.1);
}

/// `palabra <num>G25</num> (<num>G5656</num>)` layout (RV2017+, RV1602P+): each word followed
/// by a `<num>` run gets those codes; parenthesised `<num>` (Robinson tense codes) are not tags.
fn num_after_word_truth(raw: &str) -> Vec<(String, Vec<String>)> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?s)\(\s*<num>[^<]*</num>\s*\)|<num>([^<]*)</num>|<[^>]+>|([\p{L}\p{M}\p{N}]+(?:['’][\p{L}\p{M}\p{N}]+)*)").unwrap()
    });
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut last_word_open = false;
    for c in RE.captures_iter(raw) {
        if let Some(code) = c.get(1) {
            if let (true, Some(w)) = (last_word_open, out.last_mut()) {
                if let Some(s) = parse_strong_code(code.as_str()) {
                    w.1.push(s);
                }
            }
        } else if let Some(w) = c.get(2) {
            if w.as_str().chars().any(|ch| ch.is_alphabetic()) {
                out.push((loose_key(w.as_str()), Vec::new()));
                last_word_open = true;
            }
        } else if !c[0].starts_with('(') {
            // other markup between a word and its codes keeps the word open
        }
    }
    out
}

#[test]
#[ignore = "slow: RV2017 words through its RVR1960+ twin vs RV2017+'s own codes"]
fn accuracy_tap_rv2017_cross_revision_twin() {
    let Some(p) = lib_paths(&["00RV2017+.bbli", "00Interlineal-iRV 1960+.bbli"]) else { return };
    let (reading, il) = (&p[0], &p[1]);
    eprintln!("twin of RV2017+: {:?}", tagged_twin(reading));
    let mut res = [Tally::default(), Tally::default()];
    let (mut used, mut skipped) = (0, 0);
    for (b, c) in chapters(reading) {
        let Ok(ch) = load_bible_chapter(reading, b, c) else { continue };
        for vr in &ch.verses {
            let toks = verse_word_tokens(&vr.html);
            let truth = num_after_word_truth(&load_raw_verse(reading, b, c, vr.verse).unwrap_or_default());
            if toks.len() != truth.len() || toks.iter().zip(&truth).any(|(t, u)| t.key != u.0) {
                skipped += 1;
                continue;
            }
            used += 1;
            let mut seen: HashMap<&str, usize> = HashMap::new();
            for (k, t) in toks.iter().enumerate() {
                let occ = {
                    let e = seen.entry(t.key.as_str()).or_default();
                    *e += 1;
                    *e - 1
                };
                if FUNCTION_WORDS.contains(&t.key.as_str()) {
                    continue;
                }
                let Some(tc) = primary_first(truth[k].1.clone()).into_iter().next() else { continue };
                for (i, skip) in [false, true].into_iter().enumerate() {
                    let o = ResolveOpts { bible_path: Some(reading.clone()), skip_twin: skip, ..ResolveOpts::default() };
                    let r = resolve_word_links_at(&t.text, &[], &[], Some(il), b, c, vr.verse, Some(occ), &o);
                    res[i].add(&r.strongs, &tc);
                }
            }
        }
    }
    eprintln!("RV2017 content taps vs RV2017+ own codes (verses {used}, skipped {skipped}):");
    eprintln!("{}", res[1].line("interlinear only"));
    eprintln!("{}", res[0].line("RVR1960+ twin"));
}

// ─── Unit tests (synthetic modules, no library) ───

fn mk_bible(path: &std::path::Path, abbr: &str, rows: &[(i32, i32, i32, String)]) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(&format!(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT, Strongs BOOL);
         INSERT INTO Details VALUES ('{abbr}','{abbr}',1,1);
         CREATE TABLE Bible (Book INT, Chapter INT, Verse INT, Scripture TEXT);
         CREATE INDEX BookChapterVerseIndex ON Bible (Book, Chapter, Verse);"
    ))
    .unwrap();
    let tx = c.unchecked_transaction().unwrap();
    for (b, ch, v, t) in rows {
        tx.execute("INSERT INTO Bible VALUES (?1,?2,?3,?4)", rusqlite::params![b, ch, v, t]).unwrap();
    }
    tx.commit().unwrap();
}

const PLAIN: &str = "En el principio creó Dios los cielos y la tierra.";
const TAGGED: &str = "En el principio <tvm>N</tvm> <num>H7225</num> creó <tvm>V</tvm> <num>H1254</num> \
                      Dios <tvm>N</tvm> <num>H430</num> los cielos <tvm>N</tvm> <num>H8064</num> \
                      y la tierra <tvm>N</tvm> <num>H776</num>.";
/// Gn 2:5 has «tierra» twice with different words behind it.
const PLAIN_2: &str = "no había hombre para que labrase la tierra, ni planta en la tierra";
const TAGGED_2: &str = "no había hombre <tvm>N</tvm> <num>H120</num> para que labrase <tvm>V</tvm> <num>H5647</num> \
                        la tierra <tvm>N</tvm> <num>H127</num>, ni planta <tvm>N</tvm> <num>H7880</num> en la tierra <tvm>N</tvm> <num>H776</num>";

/// Plain + tagged twin + an unrelated tagged translation + an NT-only tagged edition.
fn twin_library(name: &str) -> (String, String) {
    let d = super::golden_tests::tmp_dir(name);
    let refs: Vec<(i32, i32, i32)> = LAYOUT_SAMPLE.iter().chain(TWIN_PROBES.iter()).copied().collect();
    let rows = |text: &str| -> Vec<(i32, i32, i32, String)> {
        let mut v: Vec<_> = refs.iter().map(|&(b, c, vv)| (b, c, vv, text.to_string())).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut plain = rows(PLAIN);
    plain.push((1, 2, 5, PLAIN_2.into()));
    plain.push((1, 2, 6, "subía de la tierra un vapor".into()));
    let mut tagged = rows(TAGGED);
    tagged.push((1, 2, 5, TAGGED_2.into()));
    tagged.push((1, 2, 6, "Pero subía <tvm>V</tvm> <num>H5927</num> de la tierra <tvm>N</tvm> <num>H776</num> un vapor <tvm>N</tvm> <num>H108</num>, el cual regaba toda la faz de la tierra".into()));
    let other = rows("Al comienzo <tvm>N</tvm> <num>H7225</num> hizo <tvm>V</tvm> <num>H6213</num> el Señor <tvm>N</tvm> <num>H3068</num> todo <tvm>N</tvm> <num>H3605</num> cuanto existe <tvm>V</tvm> <num>H1961</num>.");
    let nt_only: Vec<_> = rows(TAGGED).into_iter().filter(|r| r.0 >= 40).collect();
    let (p, t) = (d.join("prueba.bbli"), d.join("prueba+.bbli"));
    mk_bible(&p, "PRB", &plain);
    mk_bible(&t, "PRB+", &tagged);
    mk_bible(&d.join("otra+.bbli"), "OTR+", &other);
    mk_bible(&d.join("0prueba-NT+.bbli"), "PNT+", &nt_only);
    (p.to_string_lossy().to_string(), t.to_string_lossy().to_string())
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn primary_first_puts_content_before_article_and_object_marker() {
    assert_eq!(primary_first(s(&["G3588", "G2316"])), s(&["G2316", "G3588"]));
    assert_eq!(primary_first(s(&["H853", "H5315"])), s(&["H5315", "H853"]));
    assert_eq!(primary_first(s(&["G3588"])), s(&["G3588"]), "alone: kept");
    assert_eq!(primary_first(s(&["G3588", "H853"])), s(&["G3588", "H853"]), "all secondary: order kept");
    assert_eq!(primary_first(s(&["H7225", "H1254"])), s(&["H7225", "H1254"]), "nothing to move");
    assert_eq!(primary_first(vec![]), Vec::<String>::new());
}

#[test]
fn lcs_pairs_aligns_in_order_and_skips_insertions() {
    assert_eq!(lcs_pairs(&["a", "b", "c", "d"], &["a", "c", "d", "e"]), vec![(0, 0), (2, 1), (3, 2)]);
    assert_eq!(lcs_pairs(&["x", "y"], &["z"]), vec![]);
    assert!(lcs_pairs(&[], &["a"]).is_empty());
    // A repeated word keeps its own position.
    assert_eq!(lcs_pairs(&["la", "tierra", "y", "la", "tierra"], &["la", "tierra", "y", "la", "tierra"]).len(), 5);
}

#[test]
fn tagged_twin_found_for_the_same_text_only() {
    let (plain, tagged) = twin_library("twin-detect");
    assert_eq!(tagged_twin(&plain).as_deref(), Some(tagged.as_str()), "same text → the tagged edition");
    assert_eq!(tagged_twin(&tagged).as_deref(), Some(tagged.as_str()), "a tagged Bible is its own twin");
    let d = std::path::Path::new(&plain).parent().unwrap();
    // Unrelated text with its own tags is never a twin.
    let lone = d.join("sola.bbli");
    let refs: Vec<_> = TWIN_PROBES.iter().map(|&(b, c, v)| (b, c, v, "Palabras que no se parecen en nada".to_string())).collect();
    mk_bible(&lone, "SOL", &refs);
    assert_eq!(tagged_twin(&lone.to_string_lossy()), None);
}

#[test]
fn twin_answers_untagged_taps_with_own_tags_by_occurrence() {
    let (plain, _) = twin_library("twin-tap");
    let o = ResolveOpts { bible_path: Some(plain.clone()), ..ResolveOpts::default() };
    let tap = |w: &str, b, c, v, occ| resolve_word_links_at(w, &[], &[], None, b, c, v, occ, &o);
    let r = tap("creó", 1, 1, 1, Some(0));
    assert_eq!(r.strongs, s(&["H1254"]));
    assert_eq!(r.match_kind, "tagged");
    assert_eq!(r.tagged_twin.as_deref(), Some("PRB+"));
    // «tierra» twice: each occurrence keeps its own word.
    assert_eq!(tap("tierra", 1, 2, 5, Some(0)).strongs, s(&["H127"]));
    assert_eq!(tap("tierra", 1, 2, 5, Some(1)).strongs, s(&["H776"]));
    // Past the end: the first one (never nothing), like the interlinear path.
    assert_eq!(tap("tierra", 1, 2, 5, Some(9)).strongs, s(&["H127"]));
    // Untagged in the twin («la») and a word the twin lacks: no twin answer.
    assert!(tap("la", 1, 1, 1, Some(0)).tagged_twin.is_none());
    assert!(tap("Adán", 1, 1, 1, Some(0)).strongs.is_empty());
    // Gn 2:6 wording differs a lot (overlap < 0.6): the twin is not trusted for that verse.
    let r6 = tap("tierra", 1, 2, 6, Some(0));
    assert!(r6.tagged_twin.is_none(), "{:?}", r6.strongs);
}

#[test]
fn twin_not_used_for_codes_typed_queries_or_when_skipped() {
    let (plain, _) = twin_library("twin-skip");
    let base = ResolveOpts { bible_path: Some(plain.clone()), ..ResolveOpts::default() };
    let typed = ResolveOpts { typed: true, ..base.clone() };
    let skip = ResolveOpts { skip_twin: true, ..base.clone() };
    let tap = |w: &str, o: &ResolveOpts| resolve_word_links_at(w, &[], &[], None, 1, 1, 1, Some(0), o);
    assert_eq!(tap("H430", &base).strongs, s(&["H430"]), "a Strong code stays a code");
    assert!(tap("creó", &typed).tagged_twin.is_none(), "typed search is not a tap");
    assert!(tap("creó", &skip).strongs.is_empty(), "skip_twin: interlinear only (none here)");
    // The word's own tags from the UI still win over the twin.
    let own = ResolveOpts { tagged_strongs: s(&["H9999"]), ..base.clone() };
    assert_eq!(tap("creó", &own).strongs, s(&["H9999"]));
    assert!(tap("creó", &own).tagged_twin.is_none());
}

#[test]
fn ui_tags_are_ordered_content_first() {
    let o = ResolveOpts { tagged_strongs: s(&["G3588", "G2316"]), ..ResolveOpts::default() };
    let r = resolve_word_links_at("Dios", &[], &[], None, 43, 3, 16, Some(0), &o);
    assert_eq!(r.strongs, s(&["G2316", "G3588"]));
}

/// Taps on real verses (word, book, chapter, verse, occurrence) used by the dictionary probe.
pub(crate) const DICT_PROBE_TAPS: &[(&str, i32, i32, i32)] = &[
    ("amó", 43, 3, 16), ("mundo", 43, 3, 16), ("unigénito", 43, 3, 16), ("cree", 43, 3, 16),
    ("pierda", 43, 3, 16), ("vida", 43, 3, 16), ("eterna", 43, 3, 16), ("creó", 1, 1, 1),
    ("cielos", 1, 1, 1), ("tierra", 1, 1, 1), ("aguas", 1, 1, 2), ("Espíritu", 1, 1, 2),
    ("luz", 1, 1, 3), ("tinieblas", 1, 1, 2), ("bendijo", 1, 1, 22), ("imagen", 1, 1, 26),
    ("señoree", 1, 1, 26), ("Jehová", 1, 2, 4), ("hombre", 1, 2, 7), ("polvo", 1, 2, 7),
    ("aliento", 1, 2, 7), ("huerto", 1, 2, 8), ("árbol", 1, 2, 9), ("serpiente", 1, 3, 1),
    ("pecado", 45, 6, 23), ("muerte", 45, 6, 23), ("dádiva", 45, 6, 23), ("justificados", 45, 5, 1),
    ("fe", 45, 5, 1), ("paz", 45, 5, 1), ("gracia", 45, 5, 2), ("gloria", 45, 5, 2),
    ("tribulaciones", 45, 5, 3), ("esperanza", 45, 5, 4), ("ama", 45, 8, 28), ("propósito", 45, 8, 28),
    ("llamados", 45, 8, 28), ("predestinó", 45, 8, 29), ("glorificó", 45, 8, 30), ("pastor", 19, 23, 1),
    ("faltará", 19, 23, 1), ("pastos", 19, 23, 2), ("confortará", 19, 23, 3), ("valle", 19, 23, 4),
    ("sombra", 19, 23, 4), ("vara", 19, 23, 4), ("cayado", 19, 23, 4), ("herido", 23, 53, 5),
    ("rebeliones", 23, 53, 5), ("molido", 23, 53, 5), ("pecados", 23, 53, 5), ("castigo", 23, 53, 5),
    ("llaga", 23, 53, 5), ("pobres", 40, 5, 3), ("reino", 40, 5, 3), ("bienaventurados", 40, 5, 3),
    ("bautizado", 44, 2, 38), ("arrepentíos", 44, 2, 38), ("perdón", 44, 2, 38), ("don", 44, 2, 38),
    ("ángeles", 40, 25, 31), ("trono", 40, 25, 31), ("ovejas", 40, 25, 33), ("cabritos", 40, 25, 33),
    ("Abraham", 1, 12, 1), ("tierra", 1, 12, 1), ("parentela", 1, 12, 1), ("Moisés", 2, 3, 4),
    ("zarza", 2, 3, 2), ("Jerusalén", 44, 1, 8), ("testigos", 44, 1, 8), ("sacerdote", 58, 4, 14),
];

#[test]
#[ignore = "probe: prints what the dictionary pane shows for real taps"]
fn probe_dictionary_taps() {
    let Some(p) = lib_paths(&[
        "00RVR1960.bbli",
        "00Interlineal-iRV 1960+.bbli",
        "Nelson.dcti",
        "03 Nuevo Diccionario Biblico Vila Escuain.dcti",
        "03 Nuevo Diccionario Biblico Certeza.dcti",
        "03 Diccionario Biblico Mundo Hispano.dcti",
        "03STRONGes.lexi",
    ]) else {
        return;
    };
    let dicts = p[2..6].to_vec();
    let lexes = p[6..].to_vec();
    let o = ResolveOpts { bible_path: Some(p[0].clone()), ..ResolveOpts::default() };
    let (mut empty, mut total) = (0, 0);
    for &(w, b, c, v) in DICT_PROBE_TAPS {
        let r = resolve_word_links_at(w, &dicts, &lexes, Some(&p[1]), b, c, v, Some(0), &o);
        total += 1;
        let shown: Vec<String> = r
            .dictionary
            .iter()
            .filter(|e| !matches!(e.match_kind.as_str(), "verbform" | "contains" | "phrase"))
            .map(|e| format!("{}:{}[{}]", e.module_abbr, e.topic.trim(), e.match_kind))
            .collect();
        if shown.is_empty() {
            empty += 1;
        }
        eprintln!("DICT {w:<16} {:<12} → {}", r.strongs.first().cloned().unwrap_or_default(), shown.join(" · "));
    }
    eprintln!("taps with no dictionary entry shown: {empty} of {total}");
}

#[test]
#[ignore = "probe: dictionary coverage with/without the Strong bridge on sampled verses"]
fn probe_dictionary_bridge_coverage() {
    let Some(p) = lib_paths(&[
        "00RVR1960.bbli",
        "00Interlineal-iRV 1960+.bbli",
        "Nelson.dcti",
        "03 Nuevo Diccionario Biblico Vila Escuain.dcti",
        "03 Nuevo Diccionario Biblico Certeza.dcti",
        "03 Diccionario Biblico Mundo Hispano.dcti",
        "03STRONGes.lexi",
    ]) else {
        return;
    };
    let step: usize = std::env::var("ACC_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(40);
    let dicts = p[2..6].to_vec();
    let lexes = p[6..].to_vec();
    let o = ResolveOpts { bible_path: Some(p[0].clone()), ..ResolveOpts::default() };
    let shown = |r: &WordLinkResult| r.dictionary.iter().filter(|e| !matches!(e.match_kind.as_str(), "verbform" | "contains" | "phrase")).count();
    let (mut taps, mut before_any, mut after_any) = (0usize, 0usize, 0usize);
    let mut bridged: std::collections::BTreeMap<String, String> = Default::default();
    let mut verse_no = 0usize;
    for (b, c) in chapters(&p[0]) {
        let Ok(ch) = load_bible_chapter(&p[0], b, c) else { continue };
        for vr in &ch.verses {
            verse_no += 1;
            if verse_no % step != 0 {
                continue;
            }
            let mut seen: HashMap<String, usize> = HashMap::new();
            for t in verse_word_tokens(&vr.html) {
                let occ = {
                    let e = seen.entry(t.key.clone()).or_default();
                    *e += 1;
                    *e - 1
                };
                if FUNCTION_WORDS.contains(&t.key.as_str()) || t.key.chars().count() < 3 {
                    continue;
                }
                taps += 1;
                let r0 = resolve_word_links_at(&t.text, &dicts, &[], Some(&p[1]), b, c, vr.verse, Some(occ), &o);
                let r1 = resolve_word_links_at(&t.text, &dicts, &lexes, Some(&p[1]), b, c, vr.verse, Some(occ), &o);
                before_any += (shown(&r0) > 0) as usize;
                after_any += (shown(&r1) > 0) as usize;
                let br: Vec<String> = r1.dictionary.iter().filter(|e| e.match_kind == "strong").map(|e| e.topic.trim().to_string()).collect();
                if !br.is_empty() {
                    let mut uniq: Vec<String> = Vec::new();
                    for x in br {
                        if !uniq.iter().any(|u| u.eq_ignore_ascii_case(&x)) {
                            uniq.push(x);
                        }
                    }
                    bridged.entry(format!("{} {}", t.key, r1.strongs.first().cloned().unwrap_or_default())).or_insert(uniq.join(" | "));
                }
            }
        }
    }
    eprintln!("content taps {taps}: with ≥1 dictionary entry shown — before {before_any} ({:.1}%), with bridge {after_any} ({:.1}%)",
        100.0 * before_any as f64 / taps.max(1) as f64, 100.0 * after_any as f64 / taps.max(1) as f64);
    eprintln!("distinct bridged word/Strong pairs: {}", bridged.len());
    if let Ok(path) = std::env::var("ACC_OUT") {
        let mut f = std::fs::File::create(format!("{path}/bridge-pairs.tsv")).unwrap();
        for (k, v) in &bridged {
            writeln!(f, "{k}\t{v}").unwrap();
        }
    }
}

// ─── Strong bridge (dictionary): synthetic lexicon + dictionary ───

fn mk_topic_db(path: &std::path::Path, table: &str, rows: &[(String, String)]) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(&format!(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT);
         INSERT INTO Details VALUES ('Prueba','PRB',1);
         CREATE TABLE {table} (Topic TEXT, Definition TEXT);
         CREATE INDEX TopicIndex ON {table} (Topic);"
    ))
    .unwrap();
    let tx = c.unchecked_transaction().unwrap();
    for (t, d) in rows {
        tx.execute(&format!("INSERT INTO {table} VALUES (?1,?2)"), rusqlite::params![t, d]).unwrap();
    }
    tx.commit().unwrap();
}

/// STRONGes-like lexicon (with 1000 filler entries so it counts as a gloss source) and a
/// dictionary with the concept articles, a homonym (ESTANQUE), a name (JEHÚ) and qualified
/// subtopics the bridge must not use.
fn bridge_library(name: &str) -> (String, String) {
    let d = super::golden_tests::tmp_dir(name);
    let mut lex: Vec<(String, String)> = vec![
        ("G25".into(), "ἀγαπάω agapao ; tal vez de ἄγαν ; amar (en sentido social o moral): — amado, amada, amar, amor. Comp. G5368 .".into()),
        ("G26".into(), "ἀγάπη agápe ; de G25 ; amor , i.e. afecto: — ágape, amado, amor.".into()),
        ("H1288".into(), "&mdash; בָּרַךְ barak ; raíz prim.; arrodillarse: — arrodillarse, bendecir, bienaventurado, dichoso.".into()),
        ("H1293".into(), "&mdash; בְּרָכָה beraká ; de H1288 ; bendición : — bendecir, bendición, don, estanque, generoso.".into()),
        ("H3068".into(), "&mdash; יְהֹוָה Yejová ; de H1961 ; (el) auto-Existente o Eterno: — Jehová, Dios.".into()),
        ("H3058".into(), "&mdash; יֵהוּא Yeju ; de H3068 y H1931 ; Jehová (es) Él: — Jehú.".into()),
        ("H5288".into(), "&mdash; נַעַר náar ; de H5287 ; muchacho: — criado, joven, muchacho.".into()),
        ("H5291".into(), "&mdash; נַעֲרָה naará ; fem. de H5288 ; muchacha: — criada, joven, muchacha.".into()),
        ("G4309".into(), "προορίζω proorízo ; de G4253 y G3724 ; limitar de antemano: — determinar antes, predestinar.".into()),
        ("G3340".into(), "μετανοέω metanoéo ; de G3326 y G3539 ; pensar diferente: — arrepentirse.".into()),
        ("G3341".into(), "μετάνοια metánoia ; de G3340 , (subj.) compunción: — arrepentimiento, arrepentirse.".into()),
        ("G5547".into(), "Χριστός Jristós ; de G5548 ; ungido, i.e. el Mesías: — Cristo.".into()),
        ("G5546".into(), "Χριστιανός Jristianós ; de G5547 ; cristiano: — cristiano.".into()),
        ("H3123".into(), "&mdash; יוֹנָה yoná ; prob. de la misma que H3196 ; paloma : — paloma, palomino.".into()),
        ("G622".into(), "ἀπόλλυμι apóllumi ; de G575 y la base de G3639 ; destruir plenamente: — destruir, perder, perecer.".into()),
        ("G684".into(), "ἀπώλεια apóleia ; de un der. presunto de G622 ; ruina o pérdida: — destrucción, perdición.".into()),
        ("G599".into(), "ἀποθνήσκω apothnésko ; de G575 y G2348 ; morir: — matar, morir, mortal, muerto, perecer.".into()),
        ("G4100".into(), "πιστεύω pisteúo ; de G4102 ; tener fe: — confiar, creer, creyente.".into()),
    ];
    for i in 0..1000 {
        // letters only: a gloss with digits is not a gloss
        let tag: String = format!("{i:03}").chars().map(|c| (b'a' + c.to_digit(10).unwrap() as u8) as char).collect();
        lex.push((format!("G{}", 6000 + i), format!("palabra ; de G1 ; relleno: — relleno{tag}.")));
    }
    let dict: Vec<(String, String)> = [
        ("AMOR", "El amor de Dios…"),
        ("ESTANQUE", "Depósito de agua…"),
        ("BENDICIÓN", "Favor de Dios…"),
        ("JEHÚ", "Rey de Israel…"),
        ("CRIADA", "Sierva…"),
        ("PREDESTINACIÓN", "Determinación previa…"),
        ("ARREPENTIMIENTO", "Cambio de mente…"),
        ("PALOMA, ESTIÉRCOL DE", "Alimento en el sitio de Samaria…"),
        ("Amor (Fiesta de)", "Ágape de la iglesia…"),
        ("MUNDO", "La tierra habitada…"),
        ("CRISTIANO", "Nombre dado en Antioquía…"),
        ("MUCHACHA", "Joven…"),
        ("PERDICIÓN", "Destrucción eterna…"),
        ("MUERTO", "El que ha muerto…"),
        ("MORTAL", "Sujeto a la muerte…"),
        ("CREER", "Dar crédito…"),
    ]
    .iter()
    .map(|(t, d)| (t.to_string(), d.to_string()))
    .collect();
    let (lp, dp) = (d.join("strong-es.lexi"), d.join("dicc.dcti"));
    mk_topic_db(&lp, "Lexicon", &lex);
    mk_topic_db(&dp, "Dictionary", &dict);
    (lp.to_string_lossy().to_string(), dp.to_string_lossy().to_string())
}

fn bridged(word: &str, code: &str, lex: &str, dict: &str) -> Vec<(String, String)> {
    let o = ResolveOpts { tagged_strongs: s(&[code]), ..ResolveOpts::default() };
    let r = resolve_word_links_at(word, &[dict.to_string()], &[lex.to_string()], None, 1, 1, 1, Some(0), &o);
    r.dictionary.iter().map(|e| (e.topic.trim().to_string(), e.match_kind.clone())).collect()
}

#[test]
fn gloss_tail_roots_and_nominalizations_parse_strong_es() {
    let p = "ἀγαπάω agapao ; tal vez de ἄγαν (mucho) ; amar (en sentido moral): — amado, amada, amar, amor. Comp. G5368 .";
    assert_eq!(parse_gloss_tail(p), s(&["amado", "amada", "amar", "amor"]));
    let (g, proper) = parse_gloss_tail_names("Yeju ; de H3068 y H1931 ; Jehová (es) Él: — Jehú.");
    assert!(g.is_empty() && proper, "a capitalised gloss is a name, never a concept");
    assert_eq!(parse_gloss_tail("x ; y: — (recibir) honra, dar gloria, glorificar."), s(&["honra", "glorificar"]), "parentheses off, phrases out");
    assert!(parse_gloss_tail("sin lista de glosas").is_empty());
    assert_eq!(parse_gloss_roots("ἀγάπη agápe ; de G25 ; amor, i.e. afecto"), s(&["G25"]));
    assert_eq!(parse_gloss_roots("μετάνοια ; de G3326 y G3539 ; pensar"), s(&["G3326"]), "the second «y» code is not a «de» match");
    assert_eq!(gloss_nominalizations("predestinar"), s(&["predestinación"]));
    assert_eq!(gloss_nominalizations("glorificar"), s(&["glorificación"]));
    assert_eq!(gloss_nominalizations("bautizar"), s(&["bautismo"]));
    assert_eq!(gloss_nominalizations("arrepentirse"), s(&["arrepentimiento"]));
    assert!(gloss_nominalizations("ser").is_empty(), "too short");
}

#[test]
fn synonym_list_headwords_only() {
    assert!(synonym_list_headword("AMOR", "amor"));
    assert!(synonym_list_headword("LLAMAR, LLAMAMIENTO", "llamar"));
    assert!(synonym_list_headword("Santidad, Santo, Santos", "santidad"));
    assert!(!synonym_list_headword("PALOMA, ESTIÉRCOL DE", "paloma"), "inverted phrase = another article");
    assert!(!synonym_list_headword("Ofrenda (Iglesias Paulinas)", "ofrenda"), "qualified subtopic");
    assert!(!synonym_list_headword("DEDICACIÓN, FIESTA DE LA", "dedicación"));
}

#[test]
fn bridge_reaches_the_concept_article_for_verbs() {
    let (lex, dict) = bridge_library("bridge-verbs");
    assert_eq!(bridged("amó", "G25", &lex, &dict), vec![("AMOR".to_string(), "strong".to_string())]);
    assert_eq!(bridged("predestinó", "G4309", &lex, &dict), vec![("PREDESTINACIÓN".to_string(), "strong".to_string())], "noun of a verb gloss");
    assert_eq!(bridged("arrepentíos", "G3340", &lex, &dict), vec![("ARREPENTIMIENTO".to_string(), "strong".to_string())], "derived code's gloss");
}

#[test]
fn bridge_never_uses_other_senses_names_gender_or_qualified_topics() {
    let (lex, dict) = bridge_library("bridge-neg");
    let b = bridged("bendijo", "H1288", &lex, &dict);
    assert_eq!(b, vec![("BENDICIÓN".to_string(), "strong".to_string())], "never ESTANQUE (another sense of beraká)");
    assert!(bridged("Jehová", "H3068", &lex, &dict).is_empty(), "a name: never JEHÚ (derived name)");
    assert!(bridged("muchacho", "H5288", &lex, &dict).is_empty(), "never MUCHACHA (other gender)");
    assert!(bridged("Cristo", "G5547", &lex, &dict).is_empty(), "a name's derived word (cristiano) is not its article");
    assert!(bridged("palominos", "H3123", &lex, &dict).is_empty(), "only «PALOMA, ESTIÉRCOL DE» exists: another article");
    // A word with its own article keeps it, unlabelled, and gets nothing bridged.
    assert_eq!(bridged("mundo", "G2889", &lex, &dict), vec![("MUNDO".to_string(), "exact".to_string())]);
    // No Strong code → no bridge.
    let r = resolve_word_links_at("amó", &[dict.clone()], &[lex.clone()], None, 1, 1, 1, Some(0), &ResolveOpts::default());
    assert!(r.dictionary.is_empty());
    // No gloss source among the lexicons → no bridge.
    assert!(bridged("amó", "G25", &dict, &dict).is_empty());
}

#[test]
fn diccionario_view_lookup_puts_bridged_articles_first_only_without_a_full_match() {
    let (lex, dict) = bridge_library("bridge-view");
    let l = [lex.clone()];
    let got = dictionary_lookup_bridged(&dict, "amó", 40, &s(&["G25"]), &l).unwrap();
    assert_eq!(got.first().map(|e| (e.topic.as_str(), e.match_kind.as_str())), Some(("AMOR", "strong")));
    let plain = dictionary_lookup_bridged(&dict, "mundo", 40, &s(&["G2889"]), &l).unwrap();
    assert!(plain.iter().all(|e| e.match_kind != "strong"), "a full match: list unchanged");
    assert_eq!(
        dictionary_lookup_bridged(&dict, "amó", 40, &[], &l).unwrap().len(),
        dictionary_lookup(&dict, "amó", 40).unwrap().len(),
        "no codes: same as the plain lookup"
    );
}

#[test]
#[ignore = "probe: cost of the Strong bridge on the real library"]
fn probe_bridge_cost() {
    let Some(lib) = library() else { return };
    let mut lexes: Vec<String> = std::fs::read_dir(&lib).unwrap().flatten().map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("lexi"))
        .map(|p| p.to_string_lossy().to_string()).collect();
    lexes.sort();
    let dicts: Vec<String> = ["Nelson.dcti", "03 Nuevo Diccionario Biblico Vila Escuain.dcti", "03 Nuevo Diccionario Biblico Certeza.dcti", "03 Diccionario Biblico Mundo Hispano.dcti"]
        .iter().map(|n| lib.join(n).to_string_lossy().to_string()).collect();
    let t = std::time::Instant::now();
    let c = strong_bridge_candidates("creó", &s(&["H1254"]), &lexes);
    eprintln!("first bridge call ({} lexicons, builds the gloss index): {:.0} ms → {:?}", lexes.len(), t.elapsed().as_secs_f64() * 1e3, c.iter().map(|x| &x.gloss).collect::<Vec<_>>());
    let o = ResolveOpts { tagged_strongs: s(&["H1254"]), ..ResolveOpts::default() };
    let t = std::time::Instant::now();
    for _ in 0..50 {
        let _ = resolve_word_links_at("creó", &dicts, &lexes, None, 1, 1, 1, Some(0), &o);
    }
    let with = t.elapsed().as_secs_f64() * 1e3 / 50.0;
    let t = std::time::Instant::now();
    for _ in 0..50 {
        let _ = resolve_word_links_at("creó", &dicts, &[], None, 1, 1, 1, Some(0), &o);
    }
    let without = t.elapsed().as_secs_f64() * 1e3 / 50.0;
    eprintln!("resolve «creó» with 4 dictionaries: {with:.2} ms per tap with all lexicons (bridge + lexicon pass), {without:.2} ms with none");
}

#[test]
fn twin_skips_huge_verses_instead_of_building_a_huge_lcs_table() {
    let (plain, tagged) = twin_library("twin-huge");
    // Gn 50:26 as a 600-word "verse" in both editions (tagged words at both ends).
    let words: Vec<String> = (0..600).map(|i| format!("palabra{}", (b'a' + (i % 26) as u8) as char)).collect();
    let plain_v = format!("inicio {} final", words.join(" "));
    let tagged_v = format!("inicio <tvm>N</tvm> <num>H1</num> {} final <tvm>N</tvm> <num>H2</num>", words.join(" "));
    for (p, v) in [(&plain, plain_v), (&tagged, tagged_v)] {
        let c = Connection::open(p).unwrap();
        c.execute("INSERT INTO Bible VALUES (1,50,26,?1)", rusqlite::params![v]).unwrap();
    }
    let o = ResolveOpts { bible_path: Some(plain.clone()), ..ResolveOpts::default() };
    let r = resolve_word_links_at("inicio", &[], &[], None, 1, 50, 26, Some(0), &o);
    assert!(r.tagged_twin.is_none(), "602 × 602 words: over the LCS bound, no twin answer");
    // A normal verse of the same Bibles still uses the twin.
    assert!(resolve_word_links_at("creó", &[], &[], None, 1, 1, 1, Some(0), &o).tagged_twin.is_some());
}

#[test]
fn bridge_reaches_articles_from_stem_changing_and_irregular_verbs() {
    let (lex, dict) = bridge_library("bridge-irregular");
    let only = |w: &str, code: &str| bridged(w, code, &lex, &dict);
    // pierda → perder (e→ie) → PERDICIÓN (G684 «de … G622»); nothing shared with «pierda» itself.
    assert_eq!(only("pierda", "G622"), vec![("PERDICIÓN".to_string(), "strong".to_string())]);
    assert_eq!(only("perezcan", "G622"), Vec::<(String, String)>::new(), "a form of perecer: no perecer-noun here");
    // murió → morir (o→u) → MUERTO through «muer-»; never MORTAL («mor» is under 4 letters).
    assert_eq!(only("murió", "G599"), vec![("MUERTO".to_string(), "strong".to_string())]);
    // creyó → creer (i→y) → CREER, the infinitive itself is the article.
    assert_eq!(only("creyó", "G4100"), vec![("CREER".to_string(), "strong".to_string())]);
    // The verb must be in the word's own family: «pierda» under G25 (amar) finds nothing.
    assert!(only("pierda", "G25").is_empty());
}

#[test]
#[ignore = "probe: topic index on real dictionaries"]
fn probe_dictionary_index() {
    let Some(lib) = library() else { return };
    let mut dcti: Vec<_> = std::fs::read_dir(&lib).unwrap().flatten().map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("dcti")).collect();
    dcti.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)));
    for p in dcti.iter().take(3) {
        let ps = p.to_string_lossy().to_string();
        let t = std::time::Instant::now();
        let s = dictionary_index(&ps, Some("amor"), None, 2, 3);
        eprintln!("{:>8.1} ms  build+window  {:?}  total={:?}", t.elapsed().as_secs_f64() * 1e3, p.file_name().unwrap(), s.as_ref().map(|x| x.total));
    }
    let nelson = lib.join("Nelson.dcti").to_string_lossy().to_string();
    for q in ["pierda", "Amor", "año", "nube", "oveja", "Abraham", "zarza"] {
        let s = dictionary_index(&nelson, Some(q), None, 3, 4).unwrap();
        let rows: Vec<String> = s.rows.iter().map(|r| if Some(r.pos) == s.focus { format!("[{}]", r.topic) } else { r.topic.clone() }).collect();
        eprintln!("NELSON «{q}» exact={} focus={:?}/{}: {}", s.exact, s.focus, s.total, rows.join(" · "));
    }
    let t = std::time::Instant::now();
    for _ in 0..100 {
        let _ = dictionary_index(&nelson, Some("pierda"), None, 40, 80);
    }
    eprintln!("cached window of 120 rows: {:.3} ms", t.elapsed().as_secs_f64() * 1e3 / 100.0);
}

// ─── Topic index (e-Sword style) ───

fn index_library(name: &str) -> String {
    let d = super::golden_tests::tmp_dir(name);
    let rows: Vec<(String, String)> = [
        ("Ñu", "Antílope…"), ("ÁRBOL", "Planta…"), ("abeja", "Insecto…"), ("Amor", "Afecto…"),
        ("AMÓN", "Hijo de Lot…"), ("Anzuelo", "Gancho…"), ("Año", "Tiempo…"), ("  ¡Aleluya!", "Alabad…"),
        ("Nube", "Vapor…"), ("Oveja", "Animal…"), ("Zarza", "Arbusto…"), ("Vacío", ""),
    ]
    .iter()
    .map(|(t, d)| (t.to_string(), d.to_string()))
    .collect();
    let p = d.join("indice.dcti");
    mk_topic_db(&p, "Dictionary", &rows);
    p.to_string_lossy().to_string()
}

#[test]
fn index_key_is_spanish_dictionary_order() {
    let mut words = vec!["Ñu", "nube", "Oveja", "año", "anzuelo", "ÁRBOL", "abeja", "¡Aleluya!", "Amor", "AMÓN"];
    words.sort_by_key(|w| index_key(w));
    assert_eq!(words, ["abeja", "¡Aleluya!", "AMÓN", "Amor", "anzuelo", "año", "ÁRBOL", "nube", "Ñu", "Oveja"]);
    assert_eq!(index_key("  AMÓN "), index_key("amon"), "case, accents and outer spaces off");
    // Numbered topics (Thompson chain, notes) in natural order, before letters.
    let mut nums = vec!["10", "Aarón", "2", "1000", "1", "100", "01. Alfa", "02. Beta", "Gn 1:10", "Gn 1:9"];
    nums.sort_by_key(|w| index_key(w));
    assert_eq!(nums, ["1", "01. Alfa", "2", "02. Beta", "10", "100", "1000", "Aarón", "Gn 1:9", "Gn 1:10"]);
}

#[test]
fn dictionary_index_windows_focus_and_exact() {
    let p = index_library("index-window");
    let all = dictionary_index(&p, None, None, 0, 100).unwrap();
    assert_eq!(all.total, 12);
    let topics: Vec<&str> = all.rows.iter().map(|r| r.topic.as_str()).collect();
    assert_eq!(&topics[..4], ["abeja", "¡Aleluya!", "AMÓN", "Amor"], "leading «¡» and spaces ignored for order");
    assert_eq!(&topics[8..], ["Ñu", "Oveja", "Vacío", "Zarza"], "ñ between n and o");
    // Exact (case and accents aside): focus on that topic.
    let s = dictionary_index(&p, Some("amon"), None, 1, 2).unwrap();
    assert!(s.exact);
    assert_eq!(s.rows.iter().map(|r| r.topic.as_str()).collect::<Vec<_>>(), ["¡Aleluya!", "AMÓN", "Amor"]);
    assert_eq!(s.focus, Some(2));
    assert_eq!(s.start, 1);
    // Not a topic: focus is where it would go (before «Anzuelo»), not exact.
    let s = dictionary_index(&p, Some("amos"), None, 1, 1).unwrap();
    assert!(!s.exact);
    assert_eq!(s.rows[s.rows.iter().position(|r| Some(r.pos) == s.focus).unwrap()].topic, "Anzuelo");
    // Past the last topic: focus = total, window clamped.
    let s = dictionary_index(&p, Some("zzz"), None, 2, 5).unwrap();
    assert_eq!((s.focus, s.exact, s.start, s.rows.len()), (Some(12), false, 10, 2));
    // Anchor (Anteriores / Siguientes) wins over the query for the window, focus still reported.
    let s = dictionary_index(&p, Some("amor"), Some(9), 0, 3).unwrap();
    assert_eq!((s.start, s.rows.len(), s.focus), (9, 3, Some(3)));
    // Huge windows are bounded; no query → from the start.
    let s = dictionary_index(&p, None, None, 50_000, 50_000).unwrap();
    assert_eq!((s.start, s.rows.len(), s.focus), (0, 12, None));
}

#[test]
fn dictionary_entry_by_rowid_opens_exactly_that_topic() {
    let p = index_library("index-entry");
    let s = dictionary_index(&p, Some("Año"), None, 0, 1).unwrap();
    let e = dictionary_entry_by_rowid(&p, s.rows[0].rowid).unwrap().expect("article");
    assert_eq!(e.topic, "Año");
    assert!(e.plain.contains("Tiempo"));
    // An empty article is «no article», a missing row too.
    let v = dictionary_index(&p, Some("vacio"), None, 0, 1).unwrap();
    assert!(dictionary_entry_by_rowid(&p, v.rows[0].rowid).unwrap().is_none());
    assert!(dictionary_entry_by_rowid(&p, 999_999).unwrap().is_none());
    // Not a dictionary: an error, never a panic.
    let (plain, _) = twin_library("index-not-dict");
    assert!(dictionary_index(&plain, Some("x"), None, 1, 1).is_err());
}

#[test]
#[ignore = "probe: the topic index on every dictionary module of the library"]
fn probe_index_every_dictionary() {
    let Some(lib) = library() else { return };
    let mods = scan_modules(&lib.to_string_lossy()).unwrap();
    let dicts: Vec<&ModuleInfo> = mods.iter().filter(|m| m.kind == "dictionary").collect();
    let (mut ok, mut empty, mut err, mut enc) = (0, 0, 0, 0);
    let mut odd_total = 0usize;
    for m in &dicts {
        let name = std::path::Path::new(&m.path).file_name().unwrap().to_string_lossy().to_string();
        if m.encrypted {
            enc += 1;
            eprintln!("LOCKED   {name}");
            continue;
        }
        match dictionary_index(&m.path, Some("amor"), None, 0, 1) {
            Err(e) => {
                err += 1;
                eprintln!("ERROR    {name}: {e}");
            }
            Ok(s) if s.total == 0 => {
                empty += 1;
                eprintln!("EMPTY    {name}");
            }
            Ok(s) => {
                ok += 1;
                // Click the row at the focus: does an article open?
                let opened = s.rows.first().map(|r| dictionary_entry_by_rowid(&m.path, r.rowid));
                let article = match &opened {
                    Some(Ok(Some(_))) => "article ok".to_string(),
                    Some(Ok(None)) => "empty article".to_string(),
                    Some(Err(e)) => format!("article ERROR {e}"),
                    None => "no row".to_string(),
                };
                // Odd topics: very long (article text in the topic) or no letters.
                let all = dictionary_index(&m.path, None, None, 0, 1_000_000).unwrap();
                let mut odd = 0usize;
                let mut i = 0u32;
                while i < all.total {
                    let w = dictionary_index(&m.path, None, Some(i), 0, 1000).unwrap();
                    odd += w.rows.iter().filter(|r| r.topic.chars().count() > 80 || !r.topic.chars().any(char::is_alphabetic)).count();
                    i += 1000;
                }
                odd_total += odd;
                eprintln!("OK       {name}: {} temas · «amor» {} · {article}{}", s.total, if s.exact { "exacto" } else { "marca" }, if odd > 0 { format!(" · {odd} temas raros") } else { String::new() });
            }
        }
    }
    eprintln!("dictionaries {}: index ok {ok}, empty {empty}, error {err}, locked {enc}; odd topics {odd_total}", dicts.len());
}

#[test]
#[ignore = "probe: natural order on real numbered dictionaries"]
fn probe_index_numbered() {
    let Some(lib) = library() else { return };
    for (f, q) in [("03THOMPSON.dcti", "100"), ("02 Concordancia y diccionario de Pedro Ortiz.dcti", "12"), ("BTX2.dcti", "7")] {
        let p = lib.join(f).to_string_lossy().to_string();
        let head = dictionary_index(&p, None, None, 0, 12).unwrap();
        let s = dictionary_index(&p, Some(q), None, 2, 3).unwrap();
        eprintln!("{f}\n   first: {}\n   «{q}» exact={}: {}", head.rows.iter().map(|r| r.topic.as_str()).collect::<Vec<_>>().join(" · "), s.exact,
            s.rows.iter().map(|r| if Some(r.pos) == s.focus { format!("[{}]", r.topic) } else { r.topic.clone() }).collect::<Vec<_>>().join(" · "));
    }
}
