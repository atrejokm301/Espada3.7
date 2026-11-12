//! WP3 QA tests (test-writer-qa stage): edge cases, error paths, concurrency and
//! property-style checks that the implementer's golden tests do not cover.
//!
//! - Synthetic SQLite fixtures live in the system temp dir (never in the library).
//! - Real-library checks go through `golden_tests::library()` and print a SKIP line when the
//!   library is absent. The library is only ever opened read-only.

use super::golden_tests::{
    first, irv, lib_file, library, make_dict, mods, paths_of, tmp_dir, GN_1_1, JN_1_1,
};
use super::*;

// ─── Fixtures ───

fn make_bible(path: &Path, rows: &[(i32, i32, i32, String)]) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT, Strongs BOOL);
         INSERT INTO Details VALUES ('Prueba','PRB',1,1);
         CREATE TABLE Bible (Book INT, Chapter INT, Verse INT, Scripture TEXT);
         CREATE INDEX BookChapterVerseIndex ON Bible (Book, Chapter, Verse);",
    )
    .unwrap();
    let tx = c.unchecked_transaction().unwrap();
    for (b, ch, v, t) in rows {
        tx.execute(
            "INSERT INTO Bible VALUES (?1,?2,?3,?4)",
            rusqlite::params![b, ch, v, t],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

fn make_lex(path: &Path, rows: &[(&str, &str)], indexed: bool) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT);
         INSERT INTO Details VALUES ('Léxico prueba','LXP',1);
         CREATE TABLE Lexicon (Topic TEXT, Definition TEXT);",
    )
    .unwrap();
    if indexed {
        c.execute_batch("CREATE INDEX TopicIndex ON Lexicon (Topic);").unwrap();
    }
    for (t, d) in rows {
        c.execute("INSERT INTO Lexicon VALUES (?1, ?2)", rusqlite::params![t, d])
            .unwrap();
    }
}

fn make_dict_unindexed(path: &Path, rows: &[(&str, &str)]) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT);
         INSERT INTO Details VALUES ('Sin índice','SIX',1);
         CREATE TABLE Dictionary (Topic TEXT, Definition TEXT);",
    )
    .unwrap();
    for (t, d) in rows {
        c.execute("INSERT INTO Dictionary VALUES (?1, ?2)", rusqlite::params![t, d])
            .unwrap();
    }
}

fn s(x: &str) -> String {
    x.to_string()
}

const GN_2_4_SYN: &str = r#"<heb>יְהוָה</heb>1 <num>H3068</num>:NPDSMN <blu>Jehová</blu> <heb>אֱלֹהִים</heb>2 <num>H430</num>:NPDSMN <blu>Dios</blu> <heb>אֶרֶץ</heb>3 <num>H776</num>:NCcSFA <blu>tierra</blu>"#;
/// Shape of the real iRV Gn 15:2 (Señor = H136, Jehová = H3069).
const GN_15_2_SYN: &str = r#"<heb>אֲדֹנָ</heb>4 <num>H136</num>:NCcPMS <blu>Señor</blu> <heb>יהוה</heb>6 <num>H3069</num>:NPDSMN <blu>Jehová,</blu> <heb>מַה</heb>7 <num>H4100</num>:AIc <blu>¿qué</blu>"#;
const ONLY_H136_SYN: &str = r#"<heb>אֲדֹנָי</heb>1 <num>H136</num>:NCcPMS <blu>Señor,</blu> <heb>שְׁמַע</heb>2 <num>H8085</num>:Vqv <blu>oye</blu>"#;
const MT_1_1_SYN: &str = r#"‹ <grk>Ἰησοῦ3 Ψριστοῦ4</grk> › Iêsou Christou <num>G2424</num> <num>G5547</num> NGSM NGSM <blu>Jesucristo,</blu>"#;
const JOEL_2_28_SYN: &str = r#"<heb>רוּחִי</heb>1 <num>H7307</num>:NCcSFS <blu>Espíritu</blu> <heb>בָּשָׂר</heb>2 <num>H1320</num>:NCcSMA <blu>carne</blu>"#;

/// A synthetic iRV-shaped interlinear in its own temp folder (no RVR1960+ next to it).
fn syn_interlinear(name: &str) -> String {
    let d = tmp_dir(name);
    let p = d.join("00Interlineal-iRV 1960+.bbli");
    let mut rows = vec![
        (1, 1, 1, s(GN_1_1)),
        (43, 1, 1, s(JN_1_1)),
        (1, 2, 4, s(GN_2_4_SYN)),
        (1, 15, 2, s(GN_15_2_SYN)),
        (1, 20, 4, s(ONLY_H136_SYN)),
        (40, 1, 1, s(MT_1_1_SYN)),
    ];
    // Joel / Malachi chapter shapes of the RV numbering (KJV): 20 / 32 / 21, Mal 3 = 18, 4 = 6.
    for (b, c, n) in [(29, 1, 20), (29, 2, 32), (29, 3, 21), (39, 3, 18), (39, 4, 6)] {
        for v in 1..=n {
            let t = if (b, c, v) == (29, 2, 28) { s(JOEL_2_28_SYN) } else { s("<blu>x</blu>") };
            rows.push((b, c, v, t));
        }
    }
    make_bible(&p, &rows);
    p.to_string_lossy().to_string()
}

fn at(ip: &str, word: &str, b: i32, c: i32, v: i32, occ: Option<usize>, o: &ResolveOpts) -> WordLinkResult {
    resolve_word_links_at(word, &[], &[], Some(ip), b, c, v, occ, o)
}

fn d0() -> ResolveOpts {
    ResolveOpts::default()
}

// ─── ISS-03: strict Strong parser, boundaries and round-trip property ───

#[test]
fn qa_parse_strong_code_boundaries() {
    assert_eq!(parse_strong_code("H99999").as_deref(), Some("H99999"));
    assert_eq!(parse_strong_code("G000001").as_deref(), Some("G1")); // leading zeros beyond 5 digits
    assert_eq!(parse_strong_code("H 00001").as_deref(), Some("H1"));
    for bad in [
        "H100000", "HH430", "H-430", "H\t430", "H430 G25", "H430.", "430", "G", "  ", "Ｈ430",
        "H٤٣٠", "H４３０", "Η430", /* Greek Eta */ "H+430", "0H430",
    ] {
        assert_eq!(parse_strong_code(bad), None, "{bad:?}");
    }
}

#[test]
fn qa_parse_strong_code_round_trips_every_code() {
    for n in 1..=99_999u32 {
        for p in ['H', 'G'] {
            let canon = format!("{p}{n}");
            assert_eq!(parse_strong_code(&canon).as_deref(), Some(canon.as_str()));
            let padded = format!("{}{n:05}", p.to_ascii_lowercase());
            assert_eq!(parse_strong_code(&padded).as_deref(), Some(canon.as_str()));
            // idempotent: the canonical form parses to itself
            let again = parse_strong_code(&canon).unwrap();
            assert_eq!(parse_strong_code(&again).as_deref(), Some(canon.as_str()));
        }
    }
}

#[test]
fn qa_strong_code_forms_dedupe_and_reject() {
    assert_eq!(strong_code_forms("H10000"), ["H10000", "h10000"]);
    assert!(strong_code_forms("H0").is_empty());
    assert!(strong_code_forms("").is_empty());
    let f = strong_code_forms("g26");
    assert_eq!(&f[..3], ["G26", "G0026", "G00026"]);
}

// ─── ISS-02: resolution edge cases on a synthetic interlinear (no library) ───

#[test]
fn qa_resolve_degenerate_inputs_never_panic_and_are_empty() {
    let ip = syn_interlinear("degenerate");
    for w in ["", "   ", "¿?", "—", "«»", "123"] {
        let r = at(&ip, w, 43, 1, 1, None, &d0());
        assert!(r.strongs.is_empty(), "{w:?}: {:?}", r.strongs);
        assert!(r.match_kind.is_empty(), "{w:?}: {}", r.match_kind);
        assert!(r.matched_hit.is_none(), "{w:?}");
    }
    // missing interlinear, empty path, missing verse and out-of-range references
    let r = resolve_word_links_at("Verbo", &[], &[], None, 43, 1, 1, Some(0), &d0());
    assert!(r.strongs.is_empty() && !r.interlinear_used);
    let r = resolve_word_links_at("Verbo", &[], &[], Some(""), 43, 1, 1, None, &d0());
    assert!(r.strongs.is_empty() && !r.interlinear_used);
    let r = resolve_word_links_at("Verbo", &[], &[], Some("/nonexistent/espada/il.bbli"), 43, 1, 1, None, &d0());
    assert!(r.strongs.is_empty() && !r.interlinear_used);
    for (b, c, v) in [(43, 1, 99), (0, 0, 0), (-1, -1, -1), (99, 1, 1), (i32::MAX, i32::MAX, i32::MAX)] {
        let r = at(&ip, "Verbo", b, c, v, Some(usize::MAX), &d0());
        assert!(r.strongs.is_empty(), "{b}:{c}:{v} {:?}", r.strongs);
        assert!(r.interlinear_fallback.is_none(), "no RVR1960+ next to the fixture");
    }
}

#[test]
fn qa_occurrence_aligns_with_supplied_filler_words_and_clamps() {
    let ip = syn_interlinear("occurrence");
    // Jn 1:1 RVR1960: «En el principio era el Verbo, y el Verbo …»: the first «el» is supplied
    // (grey, no Strong), the next three are G3588.
    let el0 = at(&ip, "el", 43, 1, 1, Some(0), &d0());
    assert!(el0.strongs.is_empty(), "supplied «el» must not borrow a Strong: {:?}", el0.strongs);
    assert_eq!(el0.match_kind, "gloss", "a supplied word keeps match_kind gloss for the UI");
    for n in 1..=3 {
        assert_eq!(first(&at(&ip, "el", 43, 1, 1, Some(n), &d0())), "G3588", "el #{n}");
    }
    // None = first unit that carries a Strong
    assert_eq!(first(&at(&ip, "el", 43, 1, 1, None, &d0())), "G3588");
    // past the end → first match (here the supplied one), never a panic
    let over = at(&ip, "el", 43, 1, 1, Some(usize::MAX), &d0());
    assert_eq!(over.strongs, el0.strongs);
    for n in [0, 1, 2, 3, 1000] {
        assert_eq!(first(&at(&ip, "Verbo", 43, 1, 1, Some(n), &d0())), "G3056", "Verbo #{n}");
    }
    // «Dios» with the article demoted: content code first in every occurrence
    for n in [0, 1, 7] {
        let r = at(&ip, "Dios", 43, 1, 1, Some(n), &d0());
        assert_eq!(first(&r), "G2316", "Dios #{n}: {:?}", r.strongs);
        assert_eq!(r.matched_hit.as_ref().map(|h| h.strong.as_str()), Some("G2316"));
    }
}

#[test]
fn qa_typed_mode_vs_tap_mode_tiers() {
    let ip = syn_interlinear("typed");
    let typed = ResolveOpts { typed: true, ..d0() };
    assert_eq!(first(&at(&ip, "Verbo", 43, 1, 1, None, &typed)), "G3056");
    // T2 prefix (Verbos ↔ Verbo) only on taps
    assert_eq!(first(&at(&ip, "Verbos", 43, 1, 1, None, &d0())), "G3056");
    assert!(at(&ip, "Verbos", 43, 1, 1, None, &typed).strongs.is_empty());
    // transliteration: exact token only, taps only
    let lg = at(&ip, "logos", 43, 1, 1, None, &d0());
    assert_eq!((first(&lg), lg.match_kind.as_str()), ("G3056", "translit"));
    assert_eq!(first(&at(&ip, "Theon", 43, 1, 1, None, &d0())), "G2316");
    assert!(at(&ip, "logo", 43, 1, 1, None, &d0()).strongs.is_empty(), "no translit prefix match");
    assert!(at(&ip, "logos", 43, 1, 1, None, &typed).strongs.is_empty(), "typed: no translit without the lemma index");
    // short taps never substring-match
    for w in ["er", "di", "ve", "o"] {
        assert!(at(&ip, w, 43, 1, 1, None, &d0()).strongs.is_empty(), "{w}");
    }
}

#[test]
fn qa_script_taps_fold_accents_and_points() {
    let ip = syn_interlinear("script");
    // acute vs the iRV's grave accent, and no accents at all
    for w in ["Θεός", "θεος", "ΘΕΟΣ", "Θεὸς"] {
        let r = at(&ip, w, 43, 1, 1, None, &d0());
        assert_eq!(first(&r), "G2316", "{w}: {:?}", r.strongs);
        assert_eq!(r.match_kind, "original", "{w}");
    }
    let r = at(&ip, "λογος", 43, 1, 1, Some(2), &d0());
    assert_eq!(first(&r), "G3056");
    // unpointed Hebrew
    assert_eq!(first(&at(&ip, "אלהים", 1, 1, 1, None, &d0())), "H430");
    assert_eq!(first(&at(&ip, "בָּרָא", 1, 1, 1, None, &d0())), "H1254");
}

#[test]
fn qa_code_and_tagged_taps() {
    let ip = syn_interlinear("codes");
    let r = at(&ip, " g3056 ", 43, 1, 1, None, &d0());
    assert_eq!(r.strongs, ["G3056"]);
    assert_eq!(r.match_kind, "code");
    assert_eq!(r.matched_hit.as_ref().map(|h| h.original.as_str()), Some("λόγος"));
    assert_eq!(r.greek, ["λόγος"]);
    // a code not in the verse still resolves to itself
    let r = at(&ip, "H9999", 43, 1, 1, None, &d0());
    assert_eq!((r.strongs.clone(), r.matched_hit.is_none()), (vec![s("H9999")], true));
    // tagged: canonicalised, invalid ones dropped, duplicates removed, order kept
    let o = ResolveOpts { tagged_strongs: vec![s("h0"), s("g3056 "), s("x"), s("G3056"), s("G2532")], ..d0() };
    let r = at(&ip, "Verbo", 43, 1, 1, None, &o);
    assert_eq!(r.strongs, ["G3056", "G2532"]);
    assert_eq!(r.match_kind, "tagged");
    // all tags invalid → the normal text path
    let o = ResolveOpts { tagged_strongs: vec![s("H0"), s("")], ..d0() };
    let r = at(&ip, "Verbo", 43, 1, 1, None, &o);
    assert_eq!((first(&r), r.match_kind.as_str()), ("G3056", "gloss"));
}

#[test]
fn qa_resolve_wrapper_equals_at_with_defaults() {
    let ip = syn_interlinear("wrapper");
    for (w, b, c, v) in [("Verbo", 43, 1, 1), ("el", 43, 1, 1), ("principio", 1, 1, 1), ("cielos", 1, 1, 1), ("nada", 1, 1, 1)] {
        let a = resolve_word_links(w, &[], &[], Some(&ip), b, c, v);
        let z = resolve_word_links_at(w, &[], &[], Some(&ip), b, c, v, None, &d0());
        assert_eq!((a.strongs, a.match_kind), (z.strongs, z.match_kind), "{w}");
    }
}

// ─── ISS-37: divine-name aliases on synthetic verses ───

#[test]
fn qa_divine_alias_preference_order() {
    let ip = syn_interlinear("alias");
    let r = |w: &str, c: i32, v: i32| at(&ip, w, 1, c, v, None, &d0());
    let x = r("SEÑOR", 2, 4);
    assert_eq!((first(&x), x.match_kind.as_str()), ("H3068", "alias"));
    assert_eq!(first(&r("Yahvé", 2, 4)), "H3068");
    assert_eq!(first(&r("YAHVEH", 2, 4)), "H3068");
    assert_eq!(first(&r("Señor", 2, 4)), "H3068", "«Señor» with no Señor gloss → YHWH");
    // Gn 15:2 shape: «Oh Señor DIOS» — Señor is the Adonai gloss, DIOS → H3069
    let sn = r("Señor", 15, 2);
    assert_eq!((first(&sn), sn.match_kind.as_str()), ("H136", "gloss"));
    assert_eq!(first(&r("señor", 15, 2)), "H136");
    assert_eq!(first(&r("DIOS", 15, 2)), "H3069");
    assert_eq!(first(&r("SEÑOR", 15, 2)), "H3069", "no H3068 → H3069");
    // DIOS (caps) without H3069 falls back to the «Dios» gloss
    let d = r("DIOS", 2, 4);
    assert_eq!((first(&d), d.match_kind.as_str()), ("H430", "gloss"));
    // only Adonai in the verse
    assert_eq!(first(&r("Adonai", 20, 4)), "H136");
    assert_eq!(first(&r("SEÑOR", 20, 4)), "H136");
    // Di-s (any case) → Elohim, never YHWH
    assert_eq!(first(&r("Di-s", 2, 4)), "H430");
    assert_eq!(first(&r("DI-S", 2, 4)), "H430");
    // no divine unit at all → nothing (not a random code)
    assert!(r("Yahvé", 1, 1).strongs.is_empty());
}

// ─── ISS-30: Greek capital fix only for the iRV file ───

#[test]
fn qa_greek_caps_fix_only_for_irv_named_files() {
    let ip = syn_interlinear("caps");
    let r = at(&ip, "Jesucristo", 40, 1, 1, None, &d0());
    assert_eq!(r.strongs[..2], ["G2424", "G5547"]);
    assert!(r.matched_hit.unwrap().original.contains("Χριστοῦ"));
    let d = tmp_dir("caps-other");
    let other = d.join("Otro interlineal.bbli");
    make_bible(&other, &[(40, 1, 1, s(MT_1_1_SYN))]);
    let r = at(other.to_str().unwrap(), "Jesucristo", 40, 1, 1, None, &d0());
    assert!(r.matched_hit.unwrap().original.contains("Ψριστοῦ"), "non-iRV text is left as stored");
}

// ─── ISS-35: versification remap end to end on synthetic Bibles ───

#[test]
fn qa_versification_remap_end_to_end_and_degenerate_paths() {
    let ip = syn_interlinear("remap");
    let d = tmp_dir("remap-bible");
    let cj = d.join("cj.bbli");
    let mut rows = Vec::new();
    for (b, c, n) in [(29, 1, 20), (29, 2, 27), (29, 3, 5), (29, 4, 21), (39, 3, 24)] {
        for v in 1..=n {
            rows.push((b, c, v, s("x")));
        }
    }
    make_bible(&cj, &rows);
    let cjs = cj.to_string_lossy().to_string();
    let m = versification_remap(&cjs, &ip);
    assert!(m.joel && m.malachi && !m.kings1 && !m.numbers, "{m:?}");
    let o = ResolveOpts { bible_path: Some(cjs.clone()), ..d0() };
    let r = at(&ip, "Espíritu", 29, 3, 1, None, &o);
    assert_eq!(first(&r), "H7307", "{:?}", r.strongs);
    assert_eq!(r.interlinear_verse, Some([29, 2, 28]));
    // without bible_path the tap reads Joel 3:1 of the interlinear (filler) → nothing
    assert!(at(&ip, "Espíritu", 29, 3, 1, None, &d0()).strongs.is_empty());
    // identity: no interlinear_verse field
    assert_eq!(at(&ip, "Verbo", 43, 1, 1, None, &o).interlinear_verse, None);
    // degenerate Bible paths are identity and never panic
    for bp in ["", "/nonexistent/espada/b.bbli", ip.as_str()] {
        let o = ResolveOpts { bible_path: Some(bp.to_string()), ..d0() };
        let r = at(&ip, "Espíritu", 29, 2, 28, None, &o);
        assert_eq!(first(&r), "H7307", "{bp:?}");
        assert_eq!(r.interlinear_verse, None, "{bp:?}");
    }
    assert!(versification_remap("/nonexistent/espada/b.bbli", &ip).is_identity());
}

#[test]
fn qa_remap_boundaries() {
    let all = VersificationRemap { joel: true, malachi: true, kings1: true, numbers: true };
    assert_eq!(all.map(29, 3, 5), (29, 2, 32));
    assert_eq!(all.map(29, 4, 21), (29, 3, 21));
    assert_eq!(all.map(29, 2, 27), (29, 2, 27));
    assert_eq!(all.map(39, 3, 24), (39, 4, 6));
    assert_eq!(all.map(11, 5, 14), (11, 4, 34));
    assert_eq!(all.map(11, 5, 32), (11, 5, 18));
    assert_eq!(all.map(4, 17, 15), (4, 16, 50));
    assert_eq!(all.map(4, 17, 28), (4, 17, 13));
    // other books never move
    for (b, c, v) in [(1, 3, 1), (28, 3, 1), (30, 4, 1), (12, 5, 1), (5, 17, 1), (43, 3, 16)] {
        assert_eq!(all.map(b, c, v), (b, c, v));
    }
    // a near-miss signature does not trigger (Joel ch 3 with 21 verses = RV shape)
    let rv = |b: i32, c: i32| match (b, c) {
        (29, 1) => 20,
        (29, 2) => 32,
        (29, 3) => 21,
        (29, 4) => 21,
        _ => 0,
    };
    assert!(!detect_remap(&rv, &rv).joel);
    // every identity map is the identity on a grid of references
    let id = VersificationRemap::default();
    for b in 1..=66 {
        for c in [1, 3, 4, 5, 16, 17] {
            for v in [1, 15, 19, 30] {
                assert_eq!(id.map(b, c, v), (b, c, v));
            }
        }
    }
}

// ─── ISS-04 / ISS-10: exact Strong lookups on synthetic lexicons ───

#[test]
fn qa_lexicon_exact_codes_on_indexed_and_unindexed_tables() {
    let d = tmp_dir("lexexact");
    let rows: &[(&str, &str)] = &[
        ("G26", ""),                  // empty first row must be skipped
        ("G0026", "<p>amor</p>"),
        ("H0430", "<p>Elohim</p>"),
        ("H4300", "<p>vecino</p>"),
        ("H4301", "<p>vecino 2</p>"),
        ("G250", "<p>vecino G</p>"),
        ("g25", "<p>agapao</p>"),
        ("Hijo de Cus (Gn. 10:7)", "<p>prosa</p>"),
    ];
    for indexed in [true, false] {
        let p = d.join(format!("lx{indexed}.lexi"));
        make_lex(&p, rows, indexed);
        let ps = p.to_str().unwrap();
        let t = |q: &str| lexicon_lookup(ps, q).unwrap().map(|e| e.topic);
        assert_eq!(t("G26").as_deref(), Some("G0026"), "indexed={indexed}");
        assert_eq!(t("H430").as_deref(), Some("H0430"), "indexed={indexed}");
        assert_eq!(t("h 430").as_deref(), Some("H0430"), "indexed={indexed}");
        assert_eq!(t("G25").as_deref(), Some("g25"), "lowercase topic, indexed={indexed}");
        for miss in ["H43", "H4", "G2", "G5", "H1", "H99999"] {
            assert_eq!(t(miss), None, "{miss} indexed={indexed}");
            assert!(!probe_lexicon_has(ps, miss), "{miss}");
        }
        assert!(probe_lexicon_has(ps, "H430"));
        assert_eq!(t(""), None);
        assert_eq!(t("   "), None);
    }
}

#[test]
fn qa_lexicon_errors_are_spanish_and_not_panics() {
    let d = tmp_dir("lexerr");
    let garbage = d.join("roto.lexi");
    fs::write(&garbage, b"esto no es una base de datos SQLite, solo texto.").unwrap();
    let gs = garbage.to_str().unwrap();
    let e = lexicon_lookup(gs, "G26").unwrap_err();
    assert_eq!(e, "El archivo no es un módulo válido");
    assert!(!probe_lexicon_has(gs, "G26"));
    let missing = lexicon_lookup("/nonexistent/espada/x.lexi", "G26").unwrap_err();
    assert!(missing.starts_with("El módulo ya no está"), "{missing}");
}

// ─── ISS-07/08/14/32: dictionary ranking, index path == scan path ───

const DICT_ROWS: &[(&str, &str)] = &[
    ("JEHOVÁ", "<p>Nombre divino.</p>"),
    ("Moises", "<p>Legislador.</p>"),
    ("papa", "<p>Tubérculo.</p>"),
    ("papá", "<p>Padre.</p>"),
    ("El", "<p>Dios cananeo.</p>"),
    ("Él", "<p>Pronombre.</p>"),
    ("FE", "<p>Confianza.</p>"),
    ("FEBE", "<p>Diaconisa.</p>"),
    ("Jefe", "<p>Líder.</p>"),
    ("VIEJO, A", "<p>Anciano.</p>"),
    ("HAYA (n)", "<p>Árbol.</p>"),
    ("HAYA (v.)", "<p>Verbo haber.</p>"),
    ("Pan\t", "<p>Alimento.</p>"),
    ("PANAL", "<p>Miel.</p>"),
    ("REINO DE DIOS", "<p>Reinado.</p>"),
    ("REPUDIAR", "<p>Divorcio.</p>"),
    ("PECADO", "<p>Falta.</p>"),
    ("Luz", "<p>Claridad.</p>"),
    ("AGUA", "<p>Líquido.</p>"),
    ("AGUAS DE CELO", "<p>Prueba.</p>"),
    ("Acción de Gracias", "<p>Gratitud.</p>"),
    ("Gracia", "<p>Favor.</p>"),
    ("Espíritu, Espíritu Santo\t", "<p>Tercera persona.</p>"),
    ("Espíritus Malos", "<p>Demonios.</p>"),
    ("AARÓN_1", "<p>Sacerdote.</p>"),
    ("AARÓN_2", "<p>Otro Aarón.</p>"),
    ("sujeto", "<p>Persona.</p>"),
    ("Éfeso", "<p>Ciudad.</p>"),
    ("Melodioso", "<p>Junk.</p>"),
    ("vacío", ""),
    ("vacío", "<p>   </p>"),
    ("100%", "<p>Porcentaje.</p>"),
    (" ISAÍAS", "<p>Profeta (topic stored with a leading space, as in DiccBibl1).</p>"),
];

const DICT_QUERIES: &[&str] = &[
    "Jehová", "jehova", "JEHOVA", "Moisés", "Moises", "papa", "papá", "él", "El", "fe", "viejo",
    "Haya", "pan", "reino", "repudia", "pecados", "luces", "aguas", "gracia", "Espíritu", "Aarón",
    "sujetó", "efeso", "Isaias", "Isaías", "Dios", "vacío", "100%", "%", "_", "a_ua", "", "  ",
];

fn topics(es: &[DictEntry]) -> Vec<(String, String)> {
    es.iter().map(|e| (e.topic.clone(), e.match_kind.clone())).collect()
}

#[test]
fn qa_dictionary_index_path_equals_scan_path() {
    let d = tmp_dir("dicteq");
    let ix = d.join("ix.dcti");
    let sc = d.join("sc.dcti");
    make_dict(&ix, DICT_ROWS);
    make_dict_unindexed(&sc, DICT_ROWS);
    let (ixs, scs) = (ix.to_str().unwrap(), sc.to_str().unwrap());
    for search in [false, true] {
        for q in DICT_QUERIES {
            let o = DictLookupOpts { search };
            let a = topics(&dictionary_lookup_ranked(ixs, q, 50, o).unwrap());
            let b = topics(&dictionary_lookup_ranked(scs, q, 50, o).unwrap());
            assert_eq!(a, b, "query {q:?} search={search}");
            assert_eq!(probe_dictionary_has(ixs, q), probe_dictionary_has(scs, q), "probe {q:?}");
        }
    }
}

#[test]
fn qa_dictionary_ranking_expectations() {
    let d = tmp_dir("dictrank");
    let p = d.join("r.dcti");
    make_dict(&p, DICT_ROWS);
    let ps = p.to_str().unwrap();
    let q = |w: &str| topics(&dictionary_lookup(ps, w, 50).unwrap());
    let qs = |w: &str| topics(&dictionary_lookup_ranked(ps, w, 50, DictLookupOpts { search: true }).unwrap());
    let has = |v: &[(String, String)], t: &str| v.iter().any(|x| x.0 == t);
    assert_eq!(q("jehova")[0], (s("JEHOVÁ"), s("exact")));
    assert_eq!(q("JEHOVA")[0].0, "JEHOVÁ");
    assert_eq!(q("Moisés")[0], (s("Moises"), s("exact")));
    let papa = q("papa");
    assert_eq!(papa[0].0, "papa");
    assert!(!has(&papa, "papá"), "accent-insensitive hit shown although an exact one exists: {papa:?}");
    assert_eq!(q("papá")[0].0, "papá");
    assert_eq!(q("él").iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["Él"]);
    assert_eq!(q("El")[0].0, "El");
    assert_eq!(q("fe"), [(s("FE"), s("exact"))]);
    assert_eq!(q("efeso")[0].0, "Éfeso");
    // padded accented headword found by the unaccented query too (ISS-07, DiccBibl1)
    assert_eq!(q("Isaías")[0], (s(" ISAÍAS"), s("exact")));
    assert_eq!(q("Isaias")[0], (s(" ISAÍAS"), s("exact")));
    assert!(!has(&q("sujetó"), "sujeto"));
    assert_eq!(q("luces")[0], (s("Luz"), s("lemma")));
    assert_eq!(q("pecados")[0], (s("PECADO"), s("lemma")));
    let aguas = q("aguas");
    let ia = aguas.iter().position(|x| x.0 == "AGUA").expect("AGUA");
    let ic = aguas.iter().position(|x| x.0 == "AGUAS DE CELO").unwrap_or(usize::MAX);
    assert!(ia < ic, "{aguas:?}");
    let aaron = q("Aarón");
    assert_eq!(aaron.len(), 2, "{aaron:?}");
    assert!(aaron.iter().all(|x| x.0 == "AARÓN"));
    assert!(q("Dios").is_empty(), "no mid-word or phrase junk for a tap");
    assert!(has(&qs("Dios"), "REINO DE DIOS"));
    assert!(!has(&qs("odio"), "Melodioso"), "contains tier is whole-token only");
    let gr = qs("gracia");
    assert_eq!(gr[0], (s("Gracia"), s("exact")));
    assert!(gr.iter().any(|x| x.0 == "Acción de Gracias" && x.1 == "contains"), "{gr:?}");
    assert!(q("vacío").is_empty(), "empty and whitespace-only definitions are skipped");
    assert!(!probe_dictionary_has(ps, "vacío"));
    // LIKE wildcards are literal
    assert!(qs("a_ua").is_empty());
    assert_eq!(qs("100%").iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["100%"]);
    assert!(q("%").is_empty() && q("_").is_empty());
    assert!(q("").is_empty() && q("   ").is_empty());
    // the tap/probe path never returns verbform/contains as full entries
    let rep = q("repudia");
    assert!(rep.iter().all(|x| x.1 == "verbform"), "{rep:?}");
    assert!(!probe_dictionary_has(ps, "repudia"));
}

#[test]
fn qa_huge_entry_is_truncated_through_the_lookup() {
    let d = tmp_dir("huge");
    let p = d.join("h.dcti");
    let big = "<p>línea con acentos áéíóú y ñ</p>".repeat(20_000); // ≈ 720 KB
    make_dict(&p, &[("Grande", big.as_str())]);
    let e = dictionary_lookup(p.to_str().unwrap(), "grande", 5).unwrap();
    assert_eq!(e.len(), 1);
    assert!(e[0].truncated);
    assert!(e[0].html.len() <= MAX_ENTRY_HTML + 64, "{}", e[0].html.len());
    assert!(e[0].html.contains("texto truncado"));
    assert!(e[0].plain.contains("texto truncado"));
}

#[test]
fn qa_truncate_multibyte_without_block_boundary() {
    // no <p>/<br>/newline anywhere: the cut must land on a char boundary
    for unit in ["á", "ñ", "€", "𝄞", "ab"] {
        let html = unit.repeat(MAX_ENTRY_HTML / unit.len() + 7);
        let (h, t) = truncate_entry_html(html.clone());
        assert!(t, "{unit}");
        assert!(h.starts_with(unit) && h.len() <= MAX_ENTRY_HTML + 64);
    }
    let exact = "a".repeat(MAX_ENTRY_HTML);
    assert_eq!(truncate_entry_html(exact.clone()), (exact, false));
    assert!(truncate_entry_html("a".repeat(MAX_ENTRY_HTML + 1)).1);
}

#[test]
fn qa_headword_variant_edge_cases() {
    assert!(headword_variants("").is_empty());
    assert!(headword_variants("   ").is_empty());
    let el = headword_variants("ÉL");
    assert!(!el.iter().any(|v| v == "EL" || v == "El" || v == "el"), "{el:?}");
    let mas = headword_variants("Más");
    assert!(!mas.iter().any(|v| v == "Mas"), "{mas:?}");
    let n = headword_variants("Ñandú");
    assert!(n.iter().any(|v| v == "Ñandu"), "{n:?}");
    assert!(!n.iter().any(|v| v.contains('N') && !v.contains('Ñ')), "ñ is a letter, never stripped: {n:?}");
    // NFC: decomposed ñ and ú compose
    assert_eq!(headword_variants("N\u{303}andu\u{301}"), n);
    assert!(lemma_candidates("").is_empty());
    for w in ["s", "es", "ces", "luz", "ñus", "años"] {
        let _ = lemma_candidates(w); // no slicing panics on short or multibyte words
    }
    assert!(lemma_candidates("años").contains(&s("año")));
}

// ─── ISS-12 / ISS-26: legacy schemas and broken files ───

#[test]
fn qa_legacy_commentary_tables_and_description_title() {
    let d = tmp_dir("legacycmt");
    let p = d.join("Viejo.cmtx");
    {
        let c = Connection::open(&p).unwrap();
        c.execute_batch(
            "CREATE TABLE Details (Description TEXT, Abbreviation TEXT);
             INSERT INTO Details VALUES ('  Comentario   antiguo ', 'CAN');
             CREATE TABLE Verses (Book INT, ChapterBegin INT, VerseBegin INT, ChapterEnd INT, VerseEnd INT, Comments TEXT);
             CREATE TABLE Chapters (Book INT, Chapter INT, Comments TEXT);
             CREATE TABLE Books (Book INT, Comments TEXT);
             INSERT INTO Verses VALUES (43, 3, 16, 0, 0, 'Vino de noche');
             INSERT INTO Verses VALUES (61, 1, 20, 'º', 21, 'rango con TEXT');
             INSERT INTO Verses VALUES (43, 2, 30, 3, 5, 'cruza capítulos');
             INSERT INTO Chapters VALUES (43, 3, 'capítulo tres');
             INSERT INTO Books VALUES (43, 'libro de Juan');",
        )
        .unwrap();
    }
    let ps = p.to_str().unwrap();
    let e = load_commentary(ps, 43, 3, 16, true, true, true).unwrap();
    let labels: Vec<&str> = e.iter().map(|x| x.label.as_str()).collect();
    assert_eq!(labels, ["v.16", "Cap. 3", "Libro"], "{labels:?}");
    let cee = load_commentary(ps, 61, 1, 21, true, false, false).unwrap();
    assert_eq!(cee.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), ["v.20–21"]);
    let cross = load_commentary(ps, 43, 3, 2, true, false, false).unwrap();
    assert_eq!(cross.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), ["2:30–3:5"]);
    assert!(load_commentary(ps, 43, 3, 6, true, false, false).unwrap().is_empty());
    for (b, c, v) in [(43, 3, 16), (61, 1, 21), (43, 3, 2), (43, 3, 6), (43, 4, 1)] {
        assert_eq!(
            probe_commentary_has(ps, b, c, v, true, false, false),
            !load_commentary(ps, b, c, v, true, false, false).unwrap().is_empty(),
            "{b}:{c}:{v}"
        );
    }
    let mods = scan_modules(d.to_str().unwrap()).unwrap();
    let m = mods.iter().find(|m| m.filename == "Viejo.cmtx").unwrap();
    assert_eq!(m.title, "Comentario antiguo");
    assert!(!m.encrypted);
}

#[test]
fn qa_wrong_kind_and_broken_files_give_spanish_errors() {
    let d = tmp_dir("broken");
    // a dictionary opened as a commentary
    let dict = d.join("dic.dcti");
    make_dict(&dict, &[("x", "y")]);
    let e = load_commentary(dict.to_str().unwrap(), 43, 3, 16, true, false, false).unwrap_err();
    assert_eq!(e, "Formato de módulo no compatible");
    assert!(!probe_commentary_has(dict.to_str().unwrap(), 43, 3, 16, true, false, false));
    // a Bible opened as a dictionary
    let bible = d.join("b.bbli");
    make_bible(&bible, &[(1, 1, 1, s("En el principio"))]);
    let e = dictionary_lookup(bible.to_str().unwrap(), "principio", 3).unwrap_err();
    assert!(e.starts_with("Este módulo no tiene"), "{e}");
    // garbage bytes and an empty file
    let garbage = d.join("roto.dcti");
    fs::write(&garbage, vec![0x42u8; 4096]).unwrap();
    let empty = d.join("vacio.cmti");
    fs::write(&empty, b"").unwrap();
    for (p, cmt) in [(&garbage, false), (&empty, true)] {
        let ps = p.to_str().unwrap();
        let e = if cmt {
            load_commentary(ps, 1, 1, 1, true, false, false).unwrap_err()
        } else {
            dictionary_lookup(ps, "Dios", 3).unwrap_err()
        };
        if cmt {
            // an empty file is a valid empty SQLite database: it simply has no tables
            assert_eq!(e, "Formato de módulo no compatible", "{ps}");
        } else {
            assert_eq!(e, "El archivo no es un módulo válido", "{ps}");
        }
        assert!(!probe_dictionary_has(ps, "Dios"));
        assert!(!probe_commentary_has(ps, 1, 1, 1, true, true, true));
    }
    // scanning a folder with broken files never fails and never marks them readable modules
    let mods = scan_modules(d.to_str().unwrap()).unwrap();
    assert_eq!(mods.len(), 4, "{:?}", mods.iter().map(|m| &m.filename).collect::<Vec<_>>());
}

// ─── ISS-28: connection cache under concurrent use ───

#[test]
fn qa_conn_cache_is_correct_under_concurrent_lookups() {
    let d = tmp_dir("concurrent");
    let paths: Vec<String> = (0..12)
        .map(|i| {
            let p = d.join(format!("c{i}.dcti"));
            let def = format!("<p>contenido número {i}</p>");
            make_dict(&p, &[("clave", def.as_str())]);
            p.to_string_lossy().to_string()
        })
        .collect();
    let paths = Arc::new(paths);
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let paths = paths.clone();
            std::thread::spawn(move || {
                for round in 0..40 {
                    let i = (t * 7 + round) % paths.len();
                    let e = dictionary_lookup(&paths[i], "clave", 1).unwrap();
                    assert!(e[0].plain.contains(&format!("número {i}")), "thread {t} got {:?}", e[0].plain);
                    assert!(probe_dictionary_has(&paths[(i + 1) % paths.len()], "clave"));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("worker panicked");
    }
    assert!(conn_cache_len() <= CONN_CACHE_CAP);
}

// ─── ISS-43: lemma index on a synthetic STRONGes ───

#[test]
fn qa_lemma_index_synthetic_build_lookup_and_rebuild() {
    let d = tmp_dir("lemma");
    let p = d.join("04STRONGes.lexi");
    let rows: Vec<(&str, &str)> = vec![
        ("G26", r#"<p><grk>ἀγάπη</grk> <span style="font-weight:bold;">agápe</span>; amor</p>"#),
        ("G25", r#"<p>→ <grk>ἀγαπάω</grk> <span style="font-weight:bold;">agapáo</span></p>"#),
        ("H7965", r#"<p><heb>שָׁלוֹם</heb> <span style="font-weight:bold;">shalóm</span></p>"#),
        ("H7999", r#"<p><heb>שָׁלוֹם</heb> <span style="font-weight:bold;">shalóm</span></p>"#), // homograph
        ("Nota", r#"<p><grk>λόγος</grk></p>"#), // not a code → never indexed
    ];
    make_lex(&p, &rows, true);
    let ps = vec![p.to_string_lossy().to_string(), s("/x/04AHLB.lexi")];
    assert_eq!(lemma_index_lookup(&ps, "ἀγάπη", false), ["G26"]);
    assert_eq!(lemma_index_lookup(&ps, "αγαπη", false), ["G26"]);
    assert_eq!(lemma_index_lookup(&ps, "ΑΓΑΠΗ", false), ["G26"]);
    assert_eq!(lemma_index_lookup(&ps, "agape", true), ["G26"]);
    assert!(lemma_index_lookup(&ps, "agape", false).is_empty(), "no translit for taps");
    assert_eq!(lemma_index_lookup(&ps, "שלום", false), ["H7965", "H7999"]);
    assert!(lemma_index_lookup(&ps, "λόγος", false).is_empty(), "non-code topics are not indexed");
    assert!(lemma_index_lookup(&ps, "", true).is_empty());
    assert!(lemma_index_lookup(&ps, "  ", true).is_empty());
    assert!(lemma_index_lookup(&[s("/x/04AHLB.lexi")], "ἀγάπη", true).is_empty(), "no STRONGes → empty");
    // resolve uses it for an out-of-verse Greek tap and reports it
    let ip = syn_interlinear("lemma-il");
    let r = resolve_word_links_at("ἀγάπη", &[], &ps, Some(&ip), 1, 1, 1, None, &d0());
    assert_eq!((r.strongs.clone(), r.lemma_index_used), (vec![s("G26")], true));
    // replacing the file rebuilds the index (file identity)
    let tmp = d.join("new.lexi");
    make_lex(&tmp, &[("G3056", r#"<p><grk>λόγος</grk> <span style="font-weight:bold;">lógos</span> palabra</p>"#)], true);
    fs::rename(&tmp, &p).unwrap();
    assert_eq!(lemma_index_lookup(&ps, "λογος", false), ["G3056"]);
    assert!(lemma_index_lookup(&ps, "ἀγάπη", false).is_empty());
}

// ─── Own-tag detection on degenerate files ───

#[test]
fn qa_own_tag_detection_degenerate_files() {
    assert!(!bible_has_own_tags("/nonexistent/espada/x.bbli"));
    assert!(!bible_has_own_tags(""));
    let d = tmp_dir("owntag");
    let dict = d.join("d.dcti");
    make_dict(&dict, &[("x", "y")]);
    assert!(!bible_has_own_tags(dict.to_str().unwrap()));
    assert!(find_interlinear_path(&[]).is_none());
}

// ─── Real-library property checks (cheap) ───

/// UI-like tokenizer: words of letters (and the apostrophe), with the occurrence index of
/// each normalized token in the verse.
fn tokens_with_occurrence(plain: &str) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    plain
        .split(|c: char| !(c.is_alphabetic() || c == '\''))
        .filter(|t| !t.is_empty())
        .map(|t| {
            let n = normalize_word(t);
            let k = counts.entry(n).or_default();
            *k += 1;
            (t.to_string(), *k - 1)
        })
        .collect()
}

#[test]
fn qa_golden_property_resolve_invariants_on_rvr1960_tokens() {
    let (Some(ip), Some(bible)) = (irv(), lib_file("00RVR1960.bbli")) else { return };
    let verses: Vec<(i32, i32, i32)> = [
        (1, 1, 1), (1, 1, 2), (1, 1, 3), (1, 2, 7), (1, 3, 15), (1, 15, 2), (2, 20, 2), (5, 6, 5),
        (19, 23, 1), (19, 23, 4), (23, 53, 5), (29, 2, 28), (40, 5, 3), (40, 6, 9), (43, 1, 1),
        (43, 1, 14), (43, 3, 16), (45, 8, 28), (46, 13, 4), (66, 22, 21),
    ]
    .to_vec();
    let (mut taps, mut with_code) = (0usize, 0usize);
    let mut bad = Vec::new();
    for (b, c, v) in verses {
        let plain = load_verse_text(&bible, b, c, v).unwrap();
        for (tok, occ) in tokens_with_occurrence(&plain) {
            taps += 1;
            let r = resolve_word_links_at(&tok, &[], &[], Some(&ip), b, c, v, Some(occ), &d0());
            let ctx = format!("{tok}#{occ}@{b}:{c}:{v} → {:?} ({})", r.strongs, r.match_kind);
            let mut seen = HashSet::new();
            for code in &r.strongs {
                if parse_strong_code(code).as_deref() != Some(code.as_str()) {
                    bad.push(format!("non-canonical code: {ctx}"));
                }
                if !seen.insert(code) {
                    bad.push(format!("duplicate code: {ctx}"));
                }
            }
            if !r.strongs.is_empty() {
                with_code += 1;
                if r.match_kind.is_empty() {
                    bad.push(format!("codes without match_kind: {ctx}"));
                }
            }
            if r.match_kind == "gloss" && !r.strongs.is_empty() {
                if r.matched_hit.as_ref().map(|h| &h.strong) != r.strongs.first() {
                    bad.push(format!("matched_hit is not strongs[0]: {ctx}"));
                }
            }
            // determinism and the clamp rule: past-the-end == occurrence 0
            let again = resolve_word_links_at(&tok, &[], &[], Some(&ip), b, c, v, Some(occ), &d0());
            if (again.strongs.clone(), again.match_kind.clone()) != (r.strongs.clone(), r.match_kind.clone()) {
                bad.push(format!("non-deterministic: {ctx}"));
            }
            let z = resolve_word_links_at(&tok, &[], &[], Some(&ip), b, c, v, Some(0), &d0());
            let big = resolve_word_links_at(&tok, &[], &[], Some(&ip), b, c, v, Some(10_000), &d0());
            if z.strongs != big.strongs {
                bad.push(format!("clamp differs from occurrence 0: {ctx} vs {:?}", big.strongs));
            }
        }
    }
    eprintln!("PROBE qa resolve property: {taps} RVR1960 taps, {with_code} with a Strong ({:.1}%)", with_code as f64 * 100.0 / taps.max(1) as f64);
    assert!(bad.is_empty(), "{:#?}", &bad[..bad.len().min(25)]);
    assert!(with_code * 100 >= taps * 60, "coverage collapsed: {with_code}/{taps}");
}

#[test]
fn qa_golden_property_lexicon_lookups_are_exact_everywhere() {
    let Some(m) = mods() else { return };
    let lexs = paths_of(&m, "lexicon");
    let codes = [
        "H1", "H2", "H85", "H305", "H430", "H3068", "H7225", "H8064", "H8674", "G1", "G5", "G25",
        "G26", "G230", "G2316", "G3056", "G5207", "G5624", "H99999", "G99999",
    ];
    let (mut checked, mut found) = (0, 0);
    let mut bad = Vec::new();
    for l in &lexs {
        for c in codes {
            checked += 1;
            match lexicon_lookup(l, c) {
                Ok(Some(e)) => {
                    found += 1;
                    if parse_strong_code(&e.topic).as_deref() != Some(c) {
                        bad.push(format!("{l}: {c} → topic {:?}", e.topic));
                    }
                    if e.html.trim().is_empty() {
                        bad.push(format!("{l}: {c} → empty html"));
                    }
                    if !probe_lexicon_has(l, c) {
                        bad.push(format!("{l}: {c} probe false but lookup found {:?}", e.topic));
                    }
                }
                Ok(None) => {
                    if probe_lexicon_has(l, c) {
                        bad.push(format!("{l}: {c} probe true but lookup None"));
                    }
                }
                Err(_) => {}
            }
        }
    }
    eprintln!("PROBE qa lexicon exactness: {checked} lookups over {} lexicons, {found} found", lexs.len());
    assert!(bad.is_empty(), "{:#?}", &bad[..bad.len().min(25)]);
    assert!(found > 0);
}

#[test]
fn qa_golden_property_dictionary_entry_invariants() {
    let Some(m) = mods() else { return };
    let dicts = paths_of(&m, "dictionary");
    let words = ["Dios", "fe", "Jehová", "pecados", "pan", "aguas", "él", "Moises", "de", "Aarón"];
    let kinds = ["exact", "headword", "lemma", "verbform", "contains"];
    let mut bad = Vec::new();
    let mut n = 0;
    for d in &dicts {
        for w in words {
            let Ok(es) = dictionary_lookup(d, w, 10) else { continue };
            if es.iter().any(|e| e.match_kind == "exact") && es[0].match_kind != "exact" {
                bad.push(format!("{d}: {w} exact entry not first: {:?}", topics(&es)));
            }
            for e in &es {
                n += 1;
                if e.html.trim().is_empty() {
                    bad.push(format!("{d}: {w} empty html for {:?}", e.topic));
                }
                if !kinds.contains(&e.match_kind.as_str()) {
                    bad.push(format!("{d}: {w} bad kind {:?}", e.match_kind));
                }
                if display_topic(&e.topic) != e.topic {
                    bad.push(format!("{d}: {w} homograph suffix shown {:?}", e.topic));
                }
                if e.truncated != e.html.contains("texto truncado") && e.truncated {
                    bad.push(format!("{d}: {w} truncated without marker"));
                }
                if e.html.len() > MAX_ENTRY_HTML + 64 {
                    bad.push(format!("{d}: {w} html {} bytes", e.html.len()));
                }
            }
        }
    }
    eprintln!("PROBE qa dictionary invariants: {n} entries over {} dictionaries", dicts.len());
    assert!(bad.is_empty(), "{:#?}", &bad[..bad.len().min(25)]);
}

/// ISS-07: an unaccented ASCII query finds the accented headword in every dictionary that has
/// it (the accent-adding range must not stop at its row limit in large dictionaries).
#[test]
fn qa_golden_property_unaccented_queries_find_accented_headwords() {
    let Some(m) = mods() else { return };
    let dicts = paths_of(&m, "dictionary");
    let words = ["Moisés", "Jehová", "Jerusalén", "Josué", "Faraón", "Salomón", "Isaías", "Jesús", "Simeón", "Gedeón"];
    let mut misses = Vec::new();
    let mut total = 0;
    for d in &dicts {
        for w in words {
            let Ok(es) = dictionary_lookup(d, w, 5) else { continue };
            let Some(t) = es.iter().find(|e| e.match_kind == "exact" && strip_accents_es(&e.topic) != e.topic) else {
                continue;
            };
            total += 1;
            let plain = strip_accents_es(w);
            let got = dictionary_lookup(d, &plain, 10).unwrap_or_default();
            if !got.iter().any(|e| e.topic == t.topic && e.match_kind == "exact") {
                misses.push(format!("{d}: {plain} misses {:?} (got {:?})", t.topic, topics(&got)));
            }
        }
    }
    eprintln!("PROBE qa unaccented → accented headwords: {total} pairs, {} misses", misses.len());
    assert!(misses.is_empty(), "{:#?}", &misses[..misses.len().min(25)]);
}

#[test]
fn qa_golden_property_tagged_html_codes_are_canonical() {
    let Some(lib) = library() else { return };
    let re = Regex::new(r#"data-strong="([^"]*)""#).unwrap();
    for f in ["00RVR1960+.bbli", "10LBLA+.bbli", "00LBLA+.bbli"] {
        let p = lib.join(f).to_string_lossy().to_string();
        let mut n = 0;
        for (b, c) in [(1, 1), (1, 12), (19, 23), (43, 3), (66, 22)] {
            let ch = load_bible_chapter(&p, b, c).unwrap();
            for v in &ch.verses {
                for cap in re.captures_iter(&v.html) {
                    for code in cap[1].split_whitespace() {
                        n += 1;
                        assert_eq!(parse_strong_code(code).as_deref(), Some(code), "{f} {b}:{c}:{} {code}", v.verse);
                    }
                }
            }
        }
        eprintln!("PROBE qa tagged html {f}: {n} codes, all canonical");
        assert!(n > 100, "{f}: {n}");
    }
}

#[test]
fn qa_golden_resolve_dictionary_side_is_full_matches_only() {
    let (Some(m), Some(ip)) = (mods(), irv()) else { return };
    let dicts = paths_of(&m, "dictionary");
    let lex = paths_of(&m, "lexicon");
    for (w, b, c, v) in [("repudia", 24, 3, 8), ("Dios", 1, 1, 1), ("fe", 58, 11, 1), ("aguas", 1, 1, 2)] {
        let r = resolve_word_links_at(w, &dicts, &lex, Some(&ip), b, c, v, None, &d0());
        for e in &r.dictionary {
            assert!(matches!(e.match_kind.as_str(), "exact" | "headword" | "lemma"), "{w}: {} {} {}", e.module_abbr, e.topic, e.match_kind);
            assert!(!e.html.trim().is_empty());
        }
        for e in &r.lexicon {
            if !r.strongs.is_empty() {
                assert!(r.strongs.contains(&parse_strong_code(&e.topic).unwrap_or_default()), "{w}: lexicon topic {} not in {:?}", e.topic, r.strongs);
            }
        }
        for c in &r.related {
            assert!(parse_strong_code(c).is_some(), "{c}");
        }
    }
}

// ─── Real-library goldens the plan lists but the implementer did not assert ───

#[test]
fn qa_golden_iss37_lbla_senor_dios_gn_15_2() {
    let (Some(lib), Some(ip)) = (library(), irv()) else { return };
    let o = ResolveOpts { bible_path: Some(lib.join("00LBLA.bbli").to_string_lossy().to_string()), ..d0() };
    let r = |w: &str| resolve_word_links_at(w, &[], &[], Some(&ip), 1, 15, 2, None, &o);
    assert_eq!(first(&r("DIOS")), "H3069", "{:?}", r("DIOS").strongs);
    assert_eq!(first(&r("Señor")), "H136");
    // LBLA Gn 2:4 «el SEÑOR Dios»: SEÑOR → H3068, Dios → H430
    let g = |w: &str| resolve_word_links_at(w, &[], &[], Some(&ip), 1, 2, 4, None, &o);
    assert_eq!(first(&g("SEÑOR")), "H3068");
    assert_eq!(first(&g("Dios")), "H430");
}

#[test]
fn qa_golden_iss08_and_iss07_listed_headwords() {
    if library().is_none() {
        return;
    }
    let has = |name: &str, q: &str, topic: &str| {
        let Some(p) = lib_file(name) else { return };
        let es = dictionary_lookup(&p, q, 30).unwrap();
        assert!(es.iter().any(|e| e.topic == topic), "{name} {q} → {:?}", topics(&es));
    };
    let nc = "00_NC-STRONG-E_Nueva_Concordancia_Strong_Exhaustiva.dcti";
    has(nc, "Haya", "HAYA (n)");
    has(nc, "Haya", "HAYA (v.)");
    has("02 Concordancia y diccionario de Pedro Ortiz.dcti", "fuera", "Fuera de sí, estar");
    has("Cyclopedia A-G.dcti", "Esaú", "Esau");
}

#[test]
fn qa_golden_iss35_nacar_colunga_is_identity_in_joel() {
    let (Some(lib), Some(ip)) = (library(), irv()) else { return };
    let o = ResolveOpts { bible_path: Some(lib.join("biblia_nacar-colunga.bbli").to_string_lossy().to_string()), ..d0() };
    let r = resolve_word_links_at("Espíritu", &[], &[], Some(&ip), 29, 2, 28, None, &o);
    assert_eq!(r.interlinear_verse, None);
    assert_eq!(first(&r), "H7307");
}
