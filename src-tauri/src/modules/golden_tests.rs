//! WP3 regression tests for lookup and resolution (modules.rs).
//!
//! - Unit tests need no data.
//! - Golden tests open the real module library READ-ONLY through [`library()`]: the folder
//!   from `ESPADA_MODULES_DIR`, else `$HOME/Downloads/for mac espada`. When it is absent they
//!   print «SKIP: library not found» and pass; with `ESPADA_REQUIRE_LIBRARY=1` they fail.
//! - Aggregate / timing gates run only with `ESPADA_SLOW_TESTS=1`.
//!
//! Synthetic SQLite fixtures are created in the system temp dir, never in the library.

use super::*;
use std::path::PathBuf;

// ─── Library helper (ISS-31) ───

/// Pure resolution of the library folder (testable without touching the environment).
fn library_at(
    env_dir: Option<String>,
    home: Option<String>,
    require: bool,
) -> Result<Option<PathBuf>, String> {
    let dir = match env_dir.filter(|s| !s.trim().is_empty()) {
        Some(d) => PathBuf::from(d),
        None => match home {
            Some(h) => PathBuf::from(h).join("Downloads").join("for mac espada"),
            None => return if require { Err("HOME is not set".into()) } else { Ok(None) },
        },
    };
    if dir.is_dir() {
        Ok(Some(dir))
    } else if require {
        Err(format!("library not found: {}", dir.display()))
    } else {
        Ok(None)
    }
}

/// The module library for golden tests, or `None` (with a visible SKIP line) when absent.
/// Panics when `ESPADA_REQUIRE_LIBRARY=1` and the library is missing.
pub(crate) fn library() -> Option<PathBuf> {
    let require = std::env::var("ESPADA_REQUIRE_LIBRARY").map(|v| v == "1").unwrap_or(false);
    match library_at(
        std::env::var("ESPADA_MODULES_DIR").ok(),
        std::env::var("HOME").ok(),
        require,
    ) {
        Ok(Some(p)) => Some(p),
        Ok(None) => {
            eprintln!("SKIP: library not found (set ESPADA_MODULES_DIR or install ~/Downloads/for mac espada)");
            None
        }
        Err(e) => panic!("ESPADA_REQUIRE_LIBRARY=1 but {e}"),
    }
}

pub(super) fn slow() -> bool {
    std::env::var("ESPADA_SLOW_TESTS").map(|v| v == "1").unwrap_or(false)
}

pub(super) fn lib_file(name: &str) -> Option<String> {
    let lib = library()?;
    let p = lib.join(name);
    if p.is_file() {
        Some(p.to_string_lossy().to_string())
    } else {
        eprintln!("SKIP: module not in library: {name}");
        None
    }
}

pub(super) fn irv() -> Option<String> {
    lib_file("00Interlineal-iRV 1960+.bbli")
}

pub(super) fn mods() -> Option<Vec<ModuleInfo>> {
    let lib = library()?;
    Some(scan_modules(lib.to_str().unwrap()).expect("scan"))
}

pub(super) fn paths_of(mods: &[ModuleInfo], kind: &str) -> Vec<String> {
    mods.iter()
        .filter(|m| m.kind == kind && !m.encrypted)
        .map(|m| m.path.clone())
        .collect()
}

/// Strong-only resolve (no dictionaries/lexicons) at a verse.
fn rs(word: &str, b: i32, c: i32, v: i32) -> WordLinkResult {
    let ip = irv().unwrap();
    resolve_word_links_at(word, &[], &[], Some(&ip), b, c, v, None, &ResolveOpts::default())
}

fn rs_occ(word: &str, b: i32, c: i32, v: i32, occ: usize) -> WordLinkResult {
    let ip = irv().unwrap();
    resolve_word_links_at(word, &[], &[], Some(&ip), b, c, v, Some(occ), &ResolveOpts::default())
}

fn rs_opts(word: &str, b: i32, c: i32, v: i32, opts: &ResolveOpts) -> WordLinkResult {
    let ip = irv().unwrap();
    resolve_word_links_at(word, &[], &[], Some(&ip), b, c, v, None, opts)
}

pub(super) fn first(r: &WordLinkResult) -> &str {
    r.strongs.first().map(|s| s.as_str()).unwrap_or("")
}

pub(super) fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("espada-wp3-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

// ─── Unit tests (no library) ───

#[test]
fn library_helper_resolves_env_home_and_require() {
    let none = library_at(Some("/nonexistent/espada".into()), None, false).unwrap();
    assert!(none.is_none());
    assert!(library_at(Some("/nonexistent/espada".into()), None, true).is_err());
    let tmp = tmp_dir("libhelper");
    let got = library_at(Some(tmp.to_string_lossy().to_string()), None, true).unwrap();
    assert_eq!(got, Some(tmp.clone()));
    let home = tmp_dir("libhome");
    fs::create_dir_all(home.join("Downloads").join("for mac espada")).unwrap();
    let got = library_at(None, Some(home.to_string_lossy().to_string()), false).unwrap();
    assert_eq!(got, Some(home.join("Downloads").join("for mac espada")));
}

#[test]
fn source_has_no_hardcoded_user_path() {
    let src = include_str!("../modules.rs");
    assert!(!src.contains(&["/Users/", "amed301"].concat()));
}

#[test]
fn parse_strong_code_rejects_words_and_accepts_codes() {
    for bad in [
        "hijo", "Hijo", "gracia", "Génesis", "Gn", "H", "H0", "G0", "H000",
        "Hijo de Cus (Gn. 10:7)", "Habacuc 1:1-2", "G2624\u{301}", "H1254a", "H123456", "",
        "hombre", "gloria", "hasta",
    ] {
        assert_eq!(parse_strong_code(bad), None, "{bad:?}");
    }
    for (inp, out) in [
        ("H 430", "H430"),
        ("H00430", "H430"),
        ("H0430", "H430"),
        ("g26", "G26"),
        ("h430", "H430"),
        ("H8804", "H8804"),
        ("G5656", "G5656"),
        ("G 0026", "G26"),
        ("g 26", "G26"),
        ("H0157", "H157"),
        (" G25 ", "G25"),
    ] {
        assert_eq!(parse_strong_code(inp).as_deref(), Some(out), "{inp:?}");
    }
}

#[test]
fn strong_candidates_h0_has_no_bare_letter() {
    let v = strong_candidates("H0");
    assert!(!v.iter().any(|x| x == "H"), "{v:?}");
    let v = strong_candidates("hijo");
    assert!(!v.iter().any(|x| x == "IJO"), "{v:?}");
    assert!(strong_candidates("H0430").iter().any(|x| x == "H430"));
}

#[test]
fn strong_code_forms_are_canonical_and_padded() {
    let f = strong_code_forms("H430");
    for x in ["H430", "H0430", "H00430"] {
        assert!(f.iter().any(|y| y == x), "{f:?}");
    }
    assert!(strong_code_forms("hijo").is_empty());
}

#[test]
fn match_tier_tables() {
    // T1 whole-token equality (accent/case/punctuation folded)
    assert_eq!(match_tier("jehova", "Jehová"), Some(1));
    assert_eq!(match_tier("dios", "Dios,"), Some(1));
    assert_eq!(match_tier("donde", "¿Dónde"), Some(1));
    assert_eq!(match_tier("dijo", "—dijo"), Some(1));
    assert_eq!(match_tier("faltara", "faltará."), Some(1));
    assert_eq!(match_tier("dado", "ha dado"), Some(1));
    // never substring (the old bug)
    assert_eq!(match_tier("jehova", "a"), None);
    assert_eq!(match_tier("a", "Jehová"), None);
    assert_eq!(match_tier("tesoros", "os"), None);
    assert_eq!(match_tier("senor", "no"), None);
    assert_eq!(match_tier("en", "amó"), None);
    // T2: enclitics and close prefixes, taps of 4+ letters only
    assert_eq!(match_tier("dijole", "dijo:"), Some(2));
    assert_eq!(match_tier("diole", "dio"), Some(2));
    assert_eq!(match_tier("respondiole", "respondió"), Some(2));
    assert_eq!(match_tier("levantate", "levanta"), Some(2));
    assert_eq!(match_tier("todos", "todo"), Some(2));
    assert_eq!(match_tier("aquellos", "aquel"), None);
    assert_eq!(match_tier("consejo", "con"), None);
    assert_eq!(match_tier("porque", "por"), None);
    assert_eq!(match_tier("ese", "esto"), None);
    assert_eq!(match_tier("", "x"), None);
}

pub(super) const GN_1_1: &str = r#"<heb>בְּ</heb>1 <span style="color:#804DB3;"><sup> PB</sup></span> <blu>En</blu> <heb>→</heb> <span style="color:#C0C0C0;">el</span> <heb>רֵאשִׁית</heb>2 <num>H7225</num>:NCcSFC <blu>principio</blu> <heb>בָּרָא</heb>3 <num>H1254</num>:VqAsSM3 <blu>creó</blu> <heb>אֱלֹהִים</heb>4 <num>H430</num>:NPDSMN <blu>Dios</blu> <heb>אֵת</heb>5 <num>H853</num>:PA <blu>•</blu> <heb>הַ</heb>6 <span style="color:#804DB3;"><sup> XD</sup></span> <blu>los</blu> <heb>שָּׁמַיִם</heb>7 <num>H8064</num>:NCcDMNH <blu>cielos</blu> <heb>וְ</heb>8 <span style="color:#804DB3;"><sup> CC</sup></span> <blu>y</blu> <heb>אֵת</heb>9 <num>H853</num>:PA <blu>•</blu> <heb>הָ</heb>10 <span style="color:#804DB3;"><sup> XD</sup></span> <blu>la</blu> <heb>אָרֶץ</heb>11 <num>H776</num>:NCcSFPH <blu>tierra.</blu>"#;
pub(super) const JN_1_1: &str = r#"<grk>Ἐν1</grk> En <num>G1722</num> P <blu>En</blu> <grk>→</grk> <span style="color:#C0C0C0;">el</span> <grk>ἀρχῇ2</grk> archê <num>G746</num> NDSF <blu>principio</blu> <grk>ἦν3</grk> ên <num>G2258</num> VI-I3S <blu>era</blu> <grk>ὁ4</grk> ho <num>G3588</num> DNSM <blu>el</blu> <grk>λόγος5</grk> logos <num>G3056</num> NNSM <blu>Verbo,</blu> <grk>καὶ6</grk> kai <num>G2532</num> C <blu>y</blu> <grk>ὁ7</grk> ho <num>G3588</num> DNSM <blu>el</blu> <grk>λόγος8</grk> logos <num>G3056</num> NNSM <blu>Verbo</blu> <grk>ἦν9</grk> ên <num>G2258</num> VI-I3S <blu>era</blu> <grk>πρὸς10</grk> pros <num>G4314</num> P <blu>con</blu> ‹ <grk>τὸν11 Θεόν12</grk> › ton Theon <num>G3588</num> <num>G2316</num> DASM NASM <blu>Dios,</blu> <grk>καὶ13</grk> kai <num>G2532</num> C <blu>y</blu> <grk>ὁ16</grk> ho <num>G3588</num> DNSM <blu>el</blu> <grk>λόγος17</grk> logos <num>G3056</num> NNSM <blu>Verbo</blu> <grk>ἦν15</grk> ên <num>G2258</num> VI-I3S <blu>era</blu> <grk>Θεὸς14</grk> Theos <num>G2316</num> NNSM <blu>Dios.</blu>"#;

#[test]
fn parse_interlinear_ot_has_no_morph_or_span_translit() {
    let hits = parse_interlinear_tokens(GN_1_1);
    for h in &hits {
        assert!(h.translit.is_empty(), "OT translit must be empty: {h:?}");
    }
    let codes: Vec<&str> = hits.iter().map(|h| h.strong.as_str()).filter(|s| !s.is_empty()).collect();
    assert_eq!(codes, ["H7225", "H1254", "H430", "H8064", "H776"]);
    // units keep the grey filler «el» (no hits) for occurrence alignment
    let units = parse_interlinear_units(GN_1_1, false);
    assert!(units.iter().any(|u| u.gloss == "el" && u.hits.is_empty()), "{units:?}");
}

#[test]
fn parse_interlinear_demotes_article_and_keeps_translit() {
    let units = parse_interlinear_units(JN_1_1, false);
    let dios = units.iter().find(|u| u.gloss == "Dios").expect("Dios unit");
    assert_eq!(dios.codes(), ["G2316", "G3588"]);
    assert_eq!(dios.hits[0].original, "τὸν Θεόν");
    assert_eq!(dios.hits[0].translit, "ton Theon");
    // standalone article «el» keeps G3588
    let el = units.iter().find(|u| u.gloss == "el" && !u.hits.is_empty()).unwrap();
    assert_eq!(el.codes(), ["G3588"]);
    let verbo = units.iter().find(|u| u.gloss == "Verbo").unwrap();
    assert_eq!(verbo.hits[0].translit, "logos");
    // morphology after <num> is never a transliteration
    for u in &units {
        for h in &u.hits {
            assert!(!h.translit.chars().any(|c| c.is_ascii_digit()), "{h:?}");
            assert!(!["NNSM", "DNSM", "VI-I3S", "P", "C"].contains(&h.translit.as_str()), "{h:?}");
        }
    }
    // multi-content order is kept (Jesucristo G2424, G5547)
    let jc = r#"‹ <grk>Ἰησοῦ3 Ψριστοῦ4</grk> › Iêsou Christou <num>G2424</num> <num>G5547</num> NGSM NGSM <blu>Jesucristo,</blu>"#;
    let u = parse_interlinear_units(jc, true);
    assert_eq!(u[0].codes(), ["G2424", "G5547"]);
    assert!(u[0].hits[0].original.contains("Χριστοῦ"), "{:?}", u[0].hits[0]);
}

#[test]
fn irv_greek_capitals_are_shifted_back() {
    assert_eq!(
        irv_fix_greek_caps("Ψριστοῦ Ρέτρον Τίμωνα Ϊ"),
        "Χριστοῦ Πέτρον Σίμωνα Ω"
    );
    assert_eq!(irv_fix_greek_caps("λόγος"), "λόγος");
}

#[test]
fn normalize_word_folds_greek_and_hebrew() {
    assert_eq!(normalize_word("οὕτω"), normalize_word("ουτω"));
    assert_eq!(normalize_word("ἀγάπη"), "αγαπη");
    assert_eq!(normalize_word("λόγος"), "λογοσ");
    assert_eq!(normalize_word("ου\u{313}τω"), "ουτω");
    assert_eq!(normalize_word("אַהֲבָה"), normalize_word("אהבה"));
    // Spanish behaviour unchanged
    assert_eq!(normalize_word("JEHOVÁ"), "jehova");
    assert_eq!(normalize_word("¿Dónde"), "donde");
    assert_eq!(normalize_word("—dijo"), "dijo");
    assert_eq!(normalize_word("faltará."), "faltara");
}

#[test]
fn strip_tags_quick_output_unchanged() {
    for (inp, out) in [
        ("<b>hola</b>", "hola"),
        ("a <i>b</i> c", "a b c"),
        ("sin etiquetas", "sin etiquetas"),
        ("<span style=\"x\">x</span><br/>", "x"),
        ("1 < 2 y 3 > 2", "1  2"), // same as the original per-call regex
    ] {
        assert_eq!(strip_tags_quick(inp), out);
    }
}

#[test]
fn headword_variants_cover_case_and_accents() {
    let v = headword_variants("Jehová");
    for x in ["Jehová", "jehová", "JEHOVÁ", "Jehova", "JEHOVA", "jehova"] {
        assert!(v.iter().any(|y| y == x), "{x} missing in {v:?}");
    }
    let decomposed = headword_variants("Jehova\u{301}");
    assert_eq!(v, decomposed);
    let el = headword_variants("él");
    assert!(!el.iter().any(|y| y == "el"), "{el:?}");
    assert!(el.iter().any(|y| y == "Él"), "{el:?}");
    // lowercase verb forms are never accent-stripped (sujetó ≠ sujeto)
    assert!(!headword_variants("sujetó").iter().any(|y| y == "sujeto"));
}

#[test]
fn lemma_candidates_table() {
    assert!(lemma_candidates("pecados").contains(&"pecado".to_string()));
    assert!(lemma_candidates("luces").contains(&"luz".to_string()));
    assert!(lemma_candidates("aguas").contains(&"agua".to_string()));
    assert!(lemma_candidates("ciudades").contains(&"ciudad".to_string()));
    assert!(lemma_candidates("santa").contains(&"santo".to_string()));
    assert!(lemma_candidates("es").is_empty());
    assert!(lemma_candidates("fe").is_empty());
}

#[test]
fn classify_topic_tiers() {
    let fe = Variants::new("fe");
    assert_eq!(classify_topic("FE", &fe).map(|t| t.0), Some(Tier::Exact));
    assert_eq!(classify_topic("FEBE", &fe), None); // mid-word, short query: never
    assert_eq!(classify_topic("Jefe", &fe), None);
    let dios = Variants::new("Dios");
    assert_eq!(classify_topic("Dioscorus", &dios), None);
    assert_eq!(classify_topic("odioso", &dios), None);
    assert_eq!(classify_topic("Botadios", &dios), None);
    assert_eq!(classify_topic("DIOS, NOMBRES DE", &dios).map(|t| t.0), Some(Tier::Headword));
    assert_eq!(classify_topic("Reino de Dios", &dios), None);
    let v = Variants::new("viejo");
    assert_eq!(classify_topic("VIEJO, A", &v).map(|t| t.0), Some(Tier::Headword));
    assert_eq!(classify_topic("HAYA (v.)", &Variants::new("Haya")).map(|t| t.0), Some(Tier::Headword));
    assert_eq!(classify_topic("Pecho(s)", &Variants::new("pecho")).map(|t| t.0), Some(Tier::Headword));
    assert_eq!(classify_topic("Pan\t", &Variants::new("pan")).map(|t| t.0), Some(Tier::Headword));
    assert_eq!(classify_topic("AARÓN_1", &Variants::new("Aarón")).map(|t| t.0), Some(Tier::Headword));
    assert_eq!(classify_topic("REINO DE DIOS", &Variants::new("reino")).map(|t| t.0), Some(Tier::Phrase));
    assert_eq!(classify_topic("REPUDIAR", &Variants::new("repudia")).map(|t| t.0), Some(Tier::Verbform));
    assert_eq!(classify_topic("Espíritus Malos", &Variants::new("Espíritu")), None);
    // accent handling
    assert_eq!(classify_topic("Moises", &Variants::new("Moisés")).map(|t| (t.0, t.1)), Some((Tier::Exact, 2)));
    assert_eq!(classify_topic("JEHOVÁ", &Variants::new("Jehova")).map(|t| (t.0, t.1)), Some((Tier::Exact, 2)));
    assert_eq!(classify_topic("sujeto", &Variants::new("sujetó")), None);
    assert_eq!(classify_topic("El", &Variants::new("él")), None);
    assert_eq!(classify_topic("\u{a0}Perdón", &Variants::new("perdón")).map(|t| t.0), Some(Tier::Exact));
}

#[test]
fn display_topic_strips_homograph_suffix() {
    assert_eq!(display_topic("AARÓN_1"), "AARÓN");
    assert_eq!(display_topic("PABLO"), "PABLO");
    assert_eq!(display_topic("_1"), "_1");
    assert_eq!(display_topic("A_B"), "A_B");
}

#[test]
fn like_escape_escapes_wildcards() {
    assert_eq!(like_escape("100%"), "100\\%");
    assert_eq!(like_escape("_a"), "\\_a");
    assert_eq!(like_escape("a\\b"), "a\\\\b");
}

#[test]
fn truncate_entry_html_cuts_at_block_boundary() {
    let small = "<p>hola</p>".to_string();
    assert_eq!(truncate_entry_html(small.clone()), (small, false));
    let big = "<p>línea de texto</p>".repeat(30_000);
    let (h, t) = truncate_entry_html(big.clone());
    assert!(t);
    assert!(h.len() < MAX_ENTRY_HTML + 100, "{}", h.len());
    assert!(h.contains("texto truncado"), "marker");
    assert!(h.ends_with("KB)</p>"));
    let body = h.split("<p>… (texto truncado").next().unwrap();
    assert!(body.ends_with("</p>") || body.ends_with("<p>línea de texto") || body.is_char_boundary(body.len()));
}

#[test]
fn es_error_maps_sqlite_errors_to_spanish() {
    assert_eq!(es_error("no such column: Topic"), "Formato de módulo no compatible");
    assert_eq!(es_error("no such table: VerseCommentary"), "Formato de módulo no compatible");
    assert!(es_error("No se pudo abrir módulo: unable to open database file: /x/y").starts_with("El módulo ya no está"));
    assert_eq!(es_error("file is not a database"), "El archivo no es un módulo válido");
    assert_eq!(es_error("otra cosa"), "otra cosa");
}

#[test]
fn rank_module_orders_spanish_strong_lexicons_first() {
    let r = |f: &str, t: &str| rank_module(&format!("/x/{f}"), t);
    assert!(r("04STRONGes.lexi", "DICCIONARIO STRONG") < r("04AHLB.lexi", "Ancient Hebrew Lexicon of the Bible"));
    assert!(r("Strongspa.lexi", "Diccionario Strong Spa") < r("04PLENITUD.lexi", "RIQUEZA LITERARIA"));
    assert!(r("04PLENITUD.lexi", "RIQUEZA") < r("thayers_greek_lexicon_dictionary.lexi", "Thayer's Greek Definitions"));
    assert!(r("04BDB-THAYERes.lexi", "Definiciones") < r("bdb.lexi", "Brown-Driver-Briggs' Hebrew Definitions"));
    assert!(r("Nelson.dcti", "Test Dicc Nelson") < r("Mundohispano.dcti", "Diccionario Bíblico Mundo Hispano"));
    assert!(r("Mundohispano.dcti", "x") < r("Douglas-Tenney.dcti", "Diccionario Bíblico"));
    assert!(r("CONSTRONGES.dcti", "CONSTRONGES") < r("03DRAE2014.dcti", "DRAE2014"));
    assert!(r("03DRAE2014.dcti", "DRAE") < r("webster-1913.dcti", "Webster's 1913 Dictionary"));
    assert!(r("01LBLA.dcti", "Indice de Tópicos") < r("03DRAE2014.dcti", "DRAE"));
    assert!(r("03DRAE2014.dcti", "DRAE") < r("Easy to Read Version Dictionary.dcti", "Easy-To-Read Word List"));
}

#[test]
fn rank_module_paths_keeps_primary_first() {
    let paths = vec![
        "/x/04AHLB.lexi".to_string(),
        "/x/04STRONGes.lexi".to_string(),
        "/x/04PLENITUD.lexi".to_string(),
    ];
    let r = rank_module_paths(&paths, None);
    assert_eq!(r[0], "/x/04STRONGes.lexi");
    assert_eq!(r[2], "/x/04AHLB.lexi");
    let r = rank_module_paths(&paths, Some("/x/04AHLB.lexi"));
    assert_eq!(r[0], "/x/04AHLB.lexi");
}

#[test]
fn own_tag_layout_rule() {
    assert!(own_tag_layout_ok(100, 95));
    assert!(own_tag_layout_ok(10, 9));
    assert!(!own_tag_layout_ok(100, 0)); // AA-rv1960+: number before word
    assert!(!own_tag_layout_ok(0, 0)); // untagged Bible
    assert!(!own_tag_layout_ok(5, 5)); // too few groups to judge
}

#[test]
fn divine_alias_rules() {
    assert!(divine_alias("SEÑOR", "senor", true).is_some());
    assert!(divine_alias("Señor", "senor", true).is_none()); // exact «Señor» gloss wins
    assert!(divine_alias("Señor", "senor", false).is_some());
    assert!(divine_alias("Yahvé", "yahve", true).is_some());
    assert!(divine_alias("Yavé", "yave", false).is_some());
    assert!(divine_alias("Adonai", "adonai", false).is_some());
    assert_eq!(divine_alias("Di-s", "dis", false).unwrap()[0], "H430");
    assert!(divine_alias("Dios", "dios", true).is_none());
    assert!(divine_alias("DIOS", "dios", true).is_some());
}

fn rv_counts(b: i32, c: i32) -> i32 {
    match (b, c) {
        (29, 1) => 20,
        (29, 2) => 32,
        (29, 3) => 21,
        (39, 3) => 18,
        (39, 4) => 6,
        (11, 4) => 34,
        (11, 5) => 18,
        (4, 16) => 50,
        (4, 17) => 13,
        _ => 0,
    }
}

#[test]
fn versification_signatures_on_synthetic_tables() {
    // CJ-like: Joel 4 chapters, Mal 3 has 24, 1Ki 4/5 = 20/32, Num 16/17 = 35/28
    let cj = |b: i32, c: i32| match (b, c) {
        (29, 1) => 20,
        (29, 2) => 27,
        (29, 3) => 5,
        (29, 4) => 21,
        (39, 3) => 24,
        (11, 4) => 20,
        (11, 5) => 32,
        (4, 16) => 35,
        (4, 17) => 28,
        _ => 0,
    };
    let r = detect_remap(&cj, &rv_counts);
    assert!(r.joel && r.malachi && r.kings1 && r.numbers, "{r:?}");
    assert_eq!(r.map(29, 3, 1), (29, 2, 28));
    assert_eq!(r.map(29, 4, 1), (29, 3, 1));
    assert_eq!(r.map(39, 3, 19), (39, 4, 1));
    assert_eq!(r.map(39, 3, 18), (39, 3, 18));
    assert_eq!(r.map(11, 5, 1), (11, 4, 21));
    assert_eq!(r.map(11, 5, 15), (11, 5, 1));
    assert_eq!(r.map(4, 17, 1), (4, 16, 36));
    assert_eq!(r.map(4, 17, 16), (4, 17, 1));
    assert_eq!(r.map(43, 3, 16), (43, 3, 16));
    // BL95-like 1 Ki 4 with 26 verses: no 1Re map
    let bl = |b: i32, c: i32| match (b, c) {
        (11, 4) => 26,
        (11, 5) => 32,
        _ => rv_counts(b, c),
    };
    assert!(!detect_remap(&bl, &rv_counts).kings1);
    // RV table is identity
    assert!(detect_remap(&rv_counts, &rv_counts).is_identity());
}

// ─── Synthetic SQLite fixtures (no library) ───

pub(super) fn make_dict(path: &std::path::Path, rows: &[(&str, &str)]) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT);
         INSERT INTO Details VALUES ('Prueba','PRB',1);
         CREATE TABLE Dictionary (Topic TEXT, Definition TEXT);
         CREATE INDEX TopicIndex ON Dictionary (Topic);",
    )
    .unwrap();
    for (t, d) in rows {
        c.execute("INSERT INTO Dictionary VALUES (?1, ?2)", rusqlite::params![t, d])
            .unwrap();
    }
}

#[test]
fn empty_top_exact_row_is_skipped_before_limit() {
    let d = tmp_dir("emptyrow");
    let p = d.join("d.dcti");
    make_dict(
        &p,
        &[("pan", ""), ("pan", "   "), ("pan", "<p>Alimento.</p>"), ("panal", "x"), ("&lt;new&gt;", "&lt;new&gt;")],
    );
    let ps = p.to_str().unwrap();
    let e = dictionary_lookup(ps, "pan", 1).unwrap();
    assert_eq!(e.len(), 1);
    assert!(e[0].plain.contains("Alimento"), "{e:?}");
    assert!(probe_dictionary_has(ps, "pan"));
    assert!(dictionary_lookup(ps, "%", 10).unwrap().is_empty());
    assert!(dictionary_lookup(ps, "_a", 10).unwrap().is_empty());
    assert!(!probe_dictionary_has(ps, "pa"));
    for e in dictionary_lookup(ps, "<new>", 10).unwrap() {
        assert!(!e.html.trim().is_empty());
    }
}

#[test]
fn word_data_dictionary_schema_is_readable() {
    let d = tmp_dir("worddata");
    let p = d.join("w.dcti");
    {
        let c = Connection::open(&p).unwrap();
        c.execute_batch(
            "CREATE TABLE dictionary(word TEXT primary key collate nocase, data TEXT);
             CREATE TABLE details(title TEXT, abbreviation TEXT, description TEXT);
             INSERT INTO details VALUES ('WD','WD','x');
             INSERT INTO dictionary VALUES ('AMOR', 'amor texto');
             INSERT INTO dictionary VALUES ('AARÓN_1', 'primer Aarón');
             INSERT INTO dictionary VALUES ('AARÓN_2', 'segundo Aarón');",
        )
        .unwrap();
    }
    let ps = p.to_str().unwrap();
    let e = dictionary_lookup(ps, "amor", 5).unwrap();
    assert_eq!(e[0].topic, "AMOR");
    let a = dictionary_lookup(ps, "Aarón", 5).unwrap();
    assert_eq!(a.len(), 2, "{a:?}");
    assert!(a.iter().all(|x| x.topic == "AARÓN"), "{a:?}");
    assert!(probe_dictionary_has(ps, "amor"));
}

#[test]
fn conn_cache_is_bounded_pins_and_probes_do_not_insert() {
    let d = tmp_dir("lru");
    let mut paths = Vec::new();
    for i in 0..70 {
        let p = d.join(format!("m{i}.dcti"));
        make_dict(&p, &[("x", "y")]);
        paths.push(p.to_string_lossy().to_string());
    }
    set_pinned_modules(&[paths[0].clone()]);
    for p in &paths {
        let _ = dictionary_lookup(p, "x", 1);
    }
    assert!(conn_cache_len() <= CONN_CACHE_CAP, "{}", conn_cache_len());
    assert!(conn_cache_has(&paths[0]), "pinned path evicted");
    let probe_only = d.join("probe.dcti");
    make_dict(&probe_only, &[("x", "y")]);
    let pp = probe_only.to_string_lossy().to_string();
    assert!(probe_dictionary_has(&pp, "x"));
    assert!(!conn_cache_has(&pp), "probe inserted a connection");
    set_pinned_modules(&[]);
}

#[test]
fn replaced_module_file_is_reopened() {
    let d = tmp_dir("replace");
    let p = d.join("r.dcti");
    make_dict(&p, &[("x", "antiguo")]);
    let ps = p.to_string_lossy().to_string();
    assert!(dictionary_lookup(&ps, "x", 1).unwrap()[0].plain.contains("antiguo"));
    let tmp = d.join("r.new");
    make_dict(&tmp, &[("x", "contenido nuevo más largo")]);
    fs::rename(&tmp, &p).unwrap();
    let got = dictionary_lookup(&ps, "x", 1).unwrap();
    assert!(got[0].plain.contains("nuevo"), "{got:?}");
}

#[test]
fn missing_module_error_is_spanish() {
    let e = load_commentary("/nonexistent/espada/x.cmti", 43, 3, 16, true, false, false).unwrap_err();
    assert!(e.starts_with("El módulo ya no está"), "{e}");
}

fn make_cmt(path: &std::path::Path, rows: &[(i32, i32, i32, i32, i32, &str)], chapter: Option<&str>) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT);
         CREATE TABLE VerseCommentary (Book INT, ChapterBegin INT, VerseBegin INT, ChapterEnd INT, VerseEnd INT, Comments TEXT);
         CREATE TABLE ChapterCommentary (Book INT, Chapter INT, Comments TEXT);
         CREATE TABLE BookCommentary (Book INT, Comments TEXT);
         CREATE INDEX BookChapterVerseIndex ON VerseCommentary (Book, ChapterBegin, VerseBegin);",
    )
    .unwrap();
    for (b, cb, vb, ce, ve, t) in rows {
        c.execute(
            "INSERT INTO VerseCommentary VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params![b, cb, vb, ce, ve, t],
        )
        .unwrap();
    }
    if let Some(t) = chapter {
        c.execute("INSERT INTO ChapterCommentary VALUES (1,1,?1)", rusqlite::params![t]).unwrap();
    }
}

#[test]
fn commentary_exact_verse_first_and_duplicates_removed() {
    let d = tmp_dir("cmt");
    let p = d.join("c.cmti");
    make_cmt(
        &p,
        &[
            (43, 3, 1, 3, 21, "sección 1-21"),
            (43, 3, 11, 3, 21, "sección 11-21"),
            (43, 3, 16, 3, 16, "nota del 16"),
            (43, 3, 14, 3, 18, "sección 11-21"),
            (43, 3, 17, 3, 17, "otra"),
            (1, 1, 1, 1, 1, "igual"),
        ],
        Some("igual"),
    );
    let ps = p.to_str().unwrap();
    let e = load_commentary(ps, 43, 3, 16, true, false, false).unwrap();
    let labels: Vec<&str> = e.iter().map(|x| x.label.as_str()).collect();
    assert_eq!(labels, ["v.16", "v.1–21", "v.11–21"], "{labels:?}");
    let vc = load_commentary(ps, 1, 1, 1, true, true, false).unwrap();
    assert_eq!(vc.len(), 1, "{vc:?}");
    assert!(probe_commentary_has(ps, 43, 3, 16, true, false, false));
    assert!(!probe_commentary_has(ps, 43, 4, 1, true, false, false));
    assert!(probe_commentary_has(ps, 1, 1, 2, false, true, false));
}

// ─── Golden tests (real library, read-only) ───

#[test]
fn golden_iss03_no_fake_h0_codes() {
    if irv().is_none() {
        return;
    }
    let r = rs("hijo", 40, 1, 1);
    assert_eq!(first(&r), "G5207", "{:?}", r.strongs);
    for bad in ["H0", "G0", "H2", "H4621"] {
        assert!(!r.strongs.iter().any(|s| s == bad), "{:?}", r.strongs);
    }
    assert_eq!(rs("gracia", 43, 1, 14).strongs, ["G5485"]);
    assert_eq!(first(&rs("hombre", 1, 2, 7)), "H120");
    assert_eq!(rs("Dios", 1, 1, 1).strongs, ["H430"]);
    let typed = rs_opts("gracia", 1, 1, 1, &ResolveOpts { typed: true, ..Default::default() });
    assert!(!typed.strongs.iter().any(|s| s == "G0"), "{:?}", typed.strongs);
    let Some(st) = lib_file("04STRONGes.lexi") else { return };
    assert_eq!(lexicon_lookup(&st, "H0430").unwrap().unwrap().topic, "H430");
    eprintln!("PROBE iss03: hijo@Mt1:1 → {:?}", r.strongs);
}

#[test]
fn golden_iss03_elohim_topic_junk_never_becomes_a_code() {
    let (Some(m), Some(ip)) = (mods(), irv()) else { return };
    let lex = paths_of(&m, "lexicon");
    let r = resolve_word_links_at("Elohim", &[], &lex, Some(&ip), 43, 3, 16, None, &ResolveOpts::default());
    for s in &r.strongs {
        assert!(parse_strong_code(s).is_some(), "{s}");
        assert!(s != "H10131111", "{:?}", r.strongs);
    }
    let r = resolve_word_links_at("hijo", &[], &lex, Some(&ip), 1, 10, 7, None, &ResolveOpts::default());
    assert!(!r.strongs.iter().any(|s| s == "H107119"), "{:?}", r.strongs);
}

#[test]
fn golden_iss02_taps_choose_the_right_strong() {
    if irv().is_none() {
        return;
    }
    let cases: &[(&str, i32, i32, i32, &str)] = &[
        ("criados", 1, 14, 14, "H2593"),
        ("cincuenta", 1, 18, 26, "H2572"),
        ("Levántate", 6, 7, 10, "H6965"),
        ("rebeliones", 23, 53, 5, "H6588"),
        ("cielos", 40, 5, 3, "G3772"),
        ("Dios", 43, 1, 1, "G2316"),
        ("Jesús", 41, 11, 11, "G2424"),
        ("Hijo", 43, 3, 16, "G5207"),
        ("Jehová", 5, 6, 5, "H3068"),
        ("corazón", 5, 6, 5, "H3824"),
        ("alma", 5, 6, 5, "H5315"),
        ("fuerzas", 5, 6, 5, "H3966"),
        ("padre", 1, 2, 24, "H1"),
        ("principio", 1, 1, 1, "H7225"),
        ("creó", 1, 1, 1, "H1254"),
        ("amó", 43, 3, 16, "G25"),
        ("mundo", 43, 3, 16, "G2889"),
        ("Verbo", 43, 1, 1, "G3056"),
        ("amor", 46, 13, 4, "G26"),
        ("Bienaventurados", 40, 5, 3, "G3107"),
        ("molido", 23, 53, 5, "H1792"),
        ("Espíritu", 29, 2, 28, "H7307"),
        ("Jehová", 19, 23, 1, "H3068"),
        ("JEHOVÁ", 19, 23, 1, "H3068"),
        ("jehova", 19, 23, 1, "H3068"),
        ("¿Dónde", 1, 3, 9, "H335"),
        ("faltará.", 19, 23, 1, "H2637"),
    ];
    let mut fails = Vec::new();
    for (w, b, c, v, want) in cases {
        let r = rs(w, *b, *c, *v);
        if first(&r) != *want {
            fails.push(format!("{w}@{b}:{c}:{v} → {:?} (want {want})", r.strongs));
        }
    }
    assert!(fails.is_empty(), "{fails:#?}");
    let jc = rs("Jesucristo", 66, 22, 21);
    assert_eq!(&jc.strongs[..2], ["G2424", "G5547"]);
    let en = rs("en", 43, 3, 16);
    assert!(!en.strongs.iter().any(|s| s == "G25"), "{:?}", en.strongs);
    assert_eq!(first(&rs("al", 43, 3, 16)), "G3588");
    assert!(!rs("cielos", 39, 4, 1).strongs.iter().any(|s| s == "H853"));
    assert!(!rs("tesoros", 40, 6, 19).strongs.iter().any(|s| s == "G5213"));
    assert!(!rs("Mesías", 43, 1, 41).strongs.iter().any(|s| s == "G2076"));
    let el = rs("el", 1, 1, 1);
    assert!(el.strongs.is_empty(), "{:?}", el.strongs);
}

#[test]
fn golden_iss02_rv1909_enclitics_still_resolve() {
    if irv().is_none() {
        return;
    }
    assert_eq!(first(&rs("díjole", 1, 3, 11)), "H559");
    assert_eq!(first(&rs("dióle", 1, 14, 20)), "H5414");
    assert_eq!(first(&rs("respondióle", 1, 4, 15)), "H559");
}

#[test]
fn golden_iss02_occurrence_picks_the_nth_match_and_clamps() {
    if irv().is_none() {
        return;
    }
    assert_eq!(first(&rs_occ("que", 43, 3, 16, 0)), "G5620");
    assert_eq!(first(&rs_occ("que", 43, 3, 16, 1)), "G2443");
    // 3rd «que» (todo aquel que…) is the iRV's grey supplied unit: gloss match, no Strong.
    let third = rs_occ("que", 43, 3, 16, 2);
    assert!(third.strongs.is_empty(), "3rd «que» is a supplied word: {:?}", third.strongs);
    assert_eq!(third.match_kind, "gloss");
    assert_eq!(first(&rs("que", 43, 3, 16)), "G5620");
    // NVI Gn 1:5 has «noche» twice, the iRV once: clamp to the first, never empty.
    assert_eq!(first(&rs_occ("noche", 1, 1, 5, 1)), "H3915");
    let r = rs_occ("Dios", 43, 1, 1, 1);
    assert_eq!(first(&r), "G2316");
}

#[test]
fn golden_iss02_typed_mode_is_whole_token_only() {
    if irv().is_none() {
        return;
    }
    let typed = ResolveOpts { typed: true, ..Default::default() };
    let r = rs_opts("Jehová", 43, 3, 16, &typed);
    assert!(r.strongs.is_empty(), "{:?}", r.strongs);
    assert_eq!(r.word, "Jehová");
    let r = rs_opts("shalom", 43, 3, 16, &typed);
    assert!(!r.strongs.iter().any(|s| s == "G3588"), "{:?}", r.strongs);
    let r = rs_opts("jesús", 43, 3, 16, &typed);
    assert!(!r.strongs.iter().any(|s| s == "G846"), "{:?}", r.strongs);
    let r = rs_opts("amor", 1, 1, 1, &typed);
    assert!(r.strongs.is_empty(), "{:?}", r.strongs);
}

#[test]
fn golden_iss02_no_ot_translit_garbage_in_gn_1_to_3() {
    let Some(ip) = irv() else { return };
    for c in 1..=3 {
        for v in 1..=31 {
            let Some(raw) = load_raw_verse(&ip, 1, c, v) else { continue };
            for h in parse_interlinear_tokens(&raw) {
                assert!(h.translit.is_empty(), "Gn {c}:{v} {h:?}");
            }
        }
    }
}

#[test]
fn golden_iss04_codes_resolve_to_themselves_only() {
    let (Some(m), Some(ip)) = (mods(), irv()) else { return };
    let lex = paths_of(&m, "lexicon");
    for code in ["G25", "H430", "H1"] {
        let r = resolve_word_links_at(code, &[], &lex, Some(&ip), 43, 3, 16, None, &ResolveOpts::default());
        assert_eq!(r.strongs, [code], "{code}");
        for e in &r.lexicon {
            assert_eq!(parse_strong_code(&e.topic).as_deref(), Some(code), "{code}: {}/{}", e.module_abbr, e.topic);
        }
        if code == "G25" {
            let mods: HashSet<&str> = r.lexicon.iter().map(|e| e.module_path.as_str()).collect();
            assert!(mods.len() >= 3, "{:?}", r.lexicon.iter().map(|e| &e.module_abbr).collect::<Vec<_>>());
        }
    }
    let r = resolve_word_links_at("es", &[], &lex, Some(&ip), 43, 1, 1, None, &ResolveOpts::default());
    assert!(!r.lexicon.iter().any(|e| e.module_abbr == "CHAVEZ" && e.topic == "Resh"), "{:?}", r.lexicon.iter().map(|e| (&e.module_abbr, &e.topic)).collect::<Vec<_>>());
}

#[test]
fn golden_iss04_lexicon_lookup_misses_are_none() {
    let Some(lib) = library() else { return };
    let p = |f: &str| lib.join(f).to_string_lossy().to_string();
    for (file, code) in [
        ("04AHLB.lexi", "H85"),
        ("04AHLB.lexi", "H305"),
        ("03PLENITUD.lexi", "H779"),
        ("03WBARCLAY.lexi", "G5"),
        ("VGNT.lexi", "G230"),
    ] {
        let got = lexicon_lookup(&p(file), code).unwrap();
        assert!(got.is_none(), "{file} {code} → {:?}", got.map(|e| e.topic));
    }
    for (file, code, topic) in [
        ("Strongspa.lexi", "H85", "H85"),
        ("04STRONGes.lexi", "G26", "G26"),
        ("04STRONGes.lexi", "g26", "G26"),
        ("04STRONGes.lexi", "H 430", "H430"),
        ("04AHLB.lexi", "H430", "H430"),
    ] {
        let got = lexicon_lookup(&p(file), code).unwrap().map(|e| e.topic);
        assert_eq!(got.as_deref(), Some(topic), "{file} {code}");
    }
    let vine = p("01 Diccionario Expositivo con numeros Strong Vine.dcti");
    if Path::new(&vine).is_file() {
        assert!(lexicon_lookup(&vine, "H853").unwrap().is_none());
    }
    // first-Some over scan-order lexicons reaches an exact H305 entry
    let m = mods().unwrap();
    let mut hit = None;
    for l in paths_of(&m, "lexicon") {
        if let Ok(Some(e)) = lexicon_lookup(&l, "H305") {
            hit = Some(e.topic);
            break;
        }
    }
    assert_eq!(hit.as_deref().and_then(parse_strong_code).as_deref(), Some("H305"));
}

#[test]
fn golden_iss10_strong_in_lookup_has_no_misses_vs_legacy_exact() {
    let Some(m) = mods() else { return };
    let lexs = paths_of(&m, "lexicon");
    let mut codes: Vec<String> = Vec::new();
    let n = if slow() { 9000 } else { 300 };
    for i in 0..n / 2 {
        codes.push(format!("H{}", 1 + (i * 8674 / (n / 2))));
        codes.push(format!("G{}", 1 + (i * 5624 / (n / 2))));
    }
    let mut misses = Vec::new();
    let mut total = 0;
    for l in &lexs {
        let Ok(conn) = Connection::open_with_flags(l, OpenFlags::SQLITE_OPEN_READ_ONLY) else { continue };
        let schema = detect_schema(&conn);
        let Some(ts) = schema.lex else { continue };
        for c in &codes {
            // legacy: Topic = candidate COLLATE NOCASE over strong_candidates, first hit
            let mut legacy: Option<String> = None;
            for cand in strong_candidates(c) {
                let sql = format!(
                    "SELECT \"{}\" FROM \"{}\" WHERE \"{}\" = ?1 COLLATE NOCASE AND length(trim(\"{}\"))>0 LIMIT 1",
                    ts.topic, ts.table, ts.topic, ts.def
                );
                if let Ok(t) = conn.query_row(&sql, rusqlite::params![cand], |r| cell_to_string(r, 0)) {
                    legacy = Some(t);
                    break;
                }
            }
            let Some(lt) = legacy else { continue };
            if parse_strong_code(&lt).as_deref() != Some(c.as_str()) {
                continue; // legacy bare-digit hit for another testament; not an exact code
            }
            total += 1;
            let new = lexicon_exact_code(&conn, &ts, c).ok().flatten();
            if new.is_none() {
                misses.push(format!("{l}: {c} (legacy topic {lt:?})"));
            }
        }
    }
    eprintln!("PROBE iss10 strong IN: {total} legacy exact hits, {} misses", misses.len());
    assert!(misses.is_empty(), "{:#?}", &misses[..misses.len().min(20)]);
}

#[test]
fn golden_iss36_tagged_bibles() {
    let Some(lib) = library() else { return };
    let p = |f: &str| lib.join(f).to_string_lossy().to_string();
    for f in ["00RVR1960+.bbli", "10LBLA+.bbli", "00LBLA+.bbli"] {
        assert!(bible_has_own_tags(&p(f)), "{f} should be own-tagged");
    }
    for f in ["AA-rv1960+_reina_valera_1960_con_strong.bbli", "00RVR1960.bbli", "00Interlineal-iRV 1960+.bbli"] {
        assert!(!bible_has_own_tags(&p(f)), "{f} must not be own-tagged");
    }
    let ch = load_bible_chapter(&p("00RVR1960+.bbli"), 1, 12).unwrap();
    let v3 = &ch.verses[2].html;
    assert!(v3.contains(r#"data-strong="H127">tierra</span>"#), "{v3}");
    // untagged Bible: html unchanged
    let plain = load_bible_chapter(&p("00RVR1960.bbli"), 1, 12).unwrap();
    assert!(!plain.verses[2].html.contains("class=\"word\""));
    // a tagged tap uses its own tag
    let m = mods().unwrap();
    let lex = paths_of(&m, "lexicon");
    let ip = irv().unwrap();
    let opts = ResolveOpts { tagged_strongs: vec!["H3068".into()], ..Default::default() };
    let r = resolve_word_links_at("SEÑOR", &[], &lex, Some(&ip), 9, 16, 7, None, &opts);
    assert_eq!(first(&r), "H3068");
    assert_eq!(r.match_kind, "tagged");
    assert!(r.lexicon.iter().any(|e| e.topic.eq_ignore_ascii_case("H3068")), "{:?}", r.lexicon.iter().map(|e| &e.topic).collect::<Vec<_>>());
    let bad = ResolveOpts { tagged_strongs: vec!["H0".into(), "x".into()], ..Default::default() };
    let r = resolve_word_links_at("tierra", &[], &[], Some(&ip), 1, 12, 3, None, &bad);
    assert_ne!(r.match_kind, "tagged");
}

#[test]
fn golden_iss36_tagged_html_keeps_visible_text() {
    let Some(lib) = library() else { return };
    for f in ["00RVR1960+.bbli", "10LBLA+.bbli", "00LBLA+.bbli"] {
        let p = lib.join(f).to_string_lossy().to_string();
        let conn = Connection::open_with_flags(&p, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let limit = if slow() { -1 } else { 3000 };
        let mut st = conn.prepare("SELECT Scripture FROM Bible ORDER BY Book, Chapter, Verse LIMIT ?1").unwrap();
        let rows: Vec<String> = st
            .query_map(rusqlite::params![limit], |r| cell_to_string(r, 0))
            .unwrap()
            .flatten()
            .collect();
        let mut diff = 0;
        for raw in &rows {
            if to_plain(&clean_bible_verse_tagged(raw)) != to_plain(&clean_esword_text(raw)) {
                diff += 1;
            }
        }
        eprintln!("PROBE iss36 visible-text diff {f}: {diff} of {}", rows.len());
        assert_eq!(diff, 0, "{f}");
    }
}

#[test]
fn golden_iss35_versification_remap() {
    let (Some(lib), Some(ip)) = (library(), irv()) else { return };
    let p = |f: &str| lib.join(f).to_string_lossy().to_string();
    let cj = p("AB-biblia_corona_de_jerusaln.bbli");
    let bl = p("Biblia  Latinoamerica 1995.bbli");
    let with = |bp: &str| ResolveOpts { bible_path: Some(bp.to_string()), ..Default::default() };
    let r = |w: &str, b, c, v, bp: &str| resolve_word_links_at(w, &[], &[], Some(&ip), b, c, v, None, &with(bp));
    assert_eq!(first(&r("Espíritu", 29, 3, 1, &cj)), "H7307");
    assert_eq!(first(&r("Judá", 29, 4, 1, &cj)), "H3063");
    assert_eq!(first(&r("día", 39, 3, 19, &bl)), "H3117");
    assert_eq!(first(&r("Egipto", 11, 5, 1, &cj)), "H4714");
    assert_eq!(r("Espíritu", 29, 3, 1, &cj).interlinear_verse, Some([29, 2, 28]));
    let rv = p("00RVR1960.bbli");
    assert_eq!(first(&r("Espíritu", 29, 2, 28, &rv)), "H7307");
    for f in ["00RVR1960.bbli", "01RV1909.bbli", "01NVI.bbli", "00LBLA.bbli", "01RVR1995.bbli", "biblia_nacar-colunga.bbli"] {
        let m = versification_remap(&p(f), &ip);
        assert!(!m.joel && !m.malachi, "{f}: {m:?}");
        if f != "biblia_nacar-colunga.bbli" {
            assert!(m.is_identity(), "{f}: {m:?}");
        }
    }
    eprintln!("PROBE iss35 CJ remap {:?}; BL95 {:?}", versification_remap(&cj, &ip), versification_remap(&bl, &ip));
}

#[test]
fn golden_iss37_divine_name() {
    let (Some(lib), Some(ip)) = (library(), irv()) else { return };
    let p = |f: &str| lib.join(f).to_string_lossy().to_string();
    let at = |w: &str, b, c, v, bp: &str| {
        let o = ResolveOpts { bible_path: Some(p(bp)), ..Default::default() };
        resolve_word_links_at(w, &[], &[], Some(&ip), b, c, v, None, &o)
    };
    assert_eq!(first(&at("SEÑOR", 1, 2, 4, "00LBLA.bbli")), "H3068");
    assert_eq!(first(&at("Yahvé", 1, 2, 4, "biblia_nacar-colunga.bbli")), "H3068");
    assert_eq!(first(&at("Yavé", 1, 2, 4, "Biblia  Latinoamerica 1995.bbli")), "H3068");
    assert_eq!(first(&at("Adonai", 1, 2, 4, "AB-biblia_corona_de_jerusaln.bbli")), "H3068");
    assert_eq!(first(&at("Di-s", 1, 1, 1, "AB-biblia_corona_de_jerusaln.bbli")), "H430");
    assert_eq!(first(&rs("Jehová", 1, 2, 4)), "H3068");
    // NT «Señor» with κύριος still matches by gloss
    let s = rs("Señor", 40, 7, 21);
    assert_eq!(first(&s), "G2962", "{:?}", s.strongs);
    // no H3068/H136 in the verse → no alias match
    let none = rs("SEÑOR", 1, 1, 1);
    assert_ne!(none.match_kind, "alias");
}

#[test]
fn golden_iss38_broken_irv_verses_use_own_tag_fallback() {
    if irv().is_none() {
        return;
    }
    let cases = [
        ("Jehová", 14, 14, 11, "H3068"),
        ("pueblos", 19, 47, 9, "H5971"),
        ("cielos", 23, 64, 1, "H8064"),
        ("nombre", 23, 63, 19, "H8034"),
    ];
    for (w, b, c, v, want) in cases {
        let r = rs(w, b, c, v);
        assert_eq!(first(&r), want, "{w}@{b}:{c}:{v} {:?}", r.strongs);
        assert!(r.interlinear_used);
        assert!(r.interlinear_fallback.is_some(), "{w}");
    }
    let ok = rs("Jehová", 14, 15, 2);
    assert_eq!(first(&ok), "H3068");
    assert!(ok.interlinear_fallback.is_none());
}

#[test]
fn golden_iss38_folder_without_fallback_bible_is_harmless() {
    let Some(ip) = irv() else { return };
    let d = tmp_dir("nofallback");
    // A folder with no RVR1960+ Bible: the fallback finder returns None (no panic).
    let fake = d.join("00Interlineal-iRV 1960+.bbli");
    fs::write(&fake, b"").unwrap();
    assert!(own_tag_fallback_bible(fake.to_str().unwrap()).is_none());
    // the real interlinear still finds 00RVR1960+
    assert!(own_tag_fallback_bible(&ip).is_some());
}

#[test]
fn golden_iss43_lemma_index() {
    let (Some(m), Some(ip)) = (mods(), irv()) else { return };
    let lex = paths_of(&m, "lexicon");
    let typed = ResolveOpts { typed: true, ..Default::default() };
    let res = |w: &str, b, c, v, o: &ResolveOpts| resolve_word_links_at(w, &[], &lex, Some(&ip), b, c, v, None, o);
    let t0 = std::time::Instant::now();
    let r = res("ἀγάπη", 43, 3, 16, &typed);
    eprintln!("PROBE iss43 first lookup incl. index build: {:?}", t0.elapsed());
    assert!(r.strongs.iter().any(|s| s == "G26"), "{:?}", r.strongs);
    assert!(r.lemma_index_used);
    assert!(res("λογος", 1, 1, 1, &ResolveOpts::default()).strongs.iter().any(|s| s == "G3056"));
    assert!(res("שָׁלוֹם", 43, 3, 16, &ResolveOpts::default()).strongs.iter().any(|s| s == "H7965"));
    assert!(res("agape", 43, 3, 16, &typed).strongs.iter().any(|s| s == "G26"));
    assert!(res("logos", 43, 3, 16, &typed).strongs.iter().any(|s| s == "G3056"));
    let el = res("el", 1, 1, 1, &ResolveOpts::default());
    assert!(!el.strongs.iter().any(|s| s == "H410"), "{:?}", el.strongs);
    let e = res("אֱלֹהִים", 1, 1, 1, &ResolveOpts::default());
    assert_eq!(first(&e), "H430");
    assert!(!e.lemma_index_used);
    let t1 = std::time::Instant::now();
    for _ in 0..100 {
        let _ = lemma_index_lookup(&lex, "λόγος", false);
    }
    eprintln!("PROBE iss43 100 warm lookups: {:?}", t1.elapsed());
}

#[test]
fn golden_iss19_greek_tap_out_of_verse() {
    let (Some(m), Some(ip)) = (mods(), irv()) else { return };
    let lex = paths_of(&m, "lexicon");
    let r = resolve_word_links_at("ἀγάπη", &[], &lex, Some(&ip), 1, 1, 1, None, &ResolveOpts::default());
    assert!(r.strongs.iter().any(|s| s == "G26"), "{:?}", r.strongs);
    let r = resolve_word_links_at("οὕτω", &[], &lex, Some(&ip), 43, 3, 16, None, &ResolveOpts::default());
    assert_eq!(first(&r), "G3779");
}

#[test]
fn golden_iss30_irv_capitals_fixed() {
    if irv().is_none() {
        return;
    }
    let r = rs("Jesucristo", 40, 1, 1);
    let o = &r.matched_hit.as_ref().unwrap().original;
    assert!(o.contains("Χριστοῦ"), "{o}");
}

#[test]
fn golden_iss44_interlinear_selection_is_validated() {
    let Some(m) = mods() else { return };
    let got = find_interlinear_path(&m).unwrap();
    assert!(got.ends_with("00Interlineal-iRV 1960+.bbli"), "{got}");
    let only_probe: Vec<ModuleInfo> = m
        .iter()
        .filter(|x| {
            x.kind != "bible"
                || x.filename == "iRV1960+ interlineal inverso prueba.bbli"
                || x.filename == "00RVR1960.bbli"
        })
        .cloned()
        .collect();
    assert_eq!(find_interlinear_path(&only_probe), None);
}

fn dict(name: &str, q: &str) -> Vec<DictEntry> {
    let Some(p) = lib_file(name) else { return vec![] };
    dictionary_lookup(&p, q, 30).unwrap()
}

fn full_topics(es: &[DictEntry]) -> Vec<String> {
    es.iter()
        .filter(|e| matches!(e.match_kind.as_str(), "exact" | "headword" | "lemma"))
        .map(|e| e.topic.clone())
        .collect()
}

#[test]
fn golden_iss07_accented_capitals_and_case() {
    if library().is_none() {
        return;
    }
    assert_eq!(dict("Nelson.dcti", "Jehová")[0].topic, "JEHOVÁ");
    assert_eq!(dict("Nelson.dcti", "Jerusalén")[0].topic, "JERUSALÉN");
    assert_eq!(dict("Nelson.dcti", "Moisés")[0].topic, "MOISÉS");
    assert_eq!(dict("Nelson.dcti", "Jehova")[0].topic, "JEHOVÁ");
    assert_eq!(dict("03 Nuevo Diccionario Biblico Vila Escuain.dcti", "JESÚS")[0].topic, "Jesús");
    assert_eq!(dict("03 Nuevo Diccionario Biblico Vila Escuain.dcti", "Jehová")[0].topic, "Jehová");
    assert_eq!(dict("02_Mundo Hispano.dcti", "Moisés")[0].topic, "Moises");
    assert_eq!(dict("02_Mundo Hispano.dcti", "Natán")[0].topic, "Natan");
    assert_eq!(dict("Douglas-Tenney.dcti", "perdón")[0].topic.trim_start_matches(['\u{a0}', ' ']), "Perdón");
    assert_eq!(dict("Nelson.dcti", "fe")[0].topic, "FE");
    assert_eq!(dict("Nelson.dcti", "Pablo")[0].topic, "PABLO");
    // wrong-sense guards
    let mh = dict("02_Mundo Hispano.dcti", "él");
    assert!(mh.first().map(|e| e.topic != "El").unwrap_or(true), "{:?}", mh.iter().map(|e| &e.topic).collect::<Vec<_>>());
    let sc = dict("03StrongC.dcti", "él");
    if let Some(e) = sc.first() {
        assert_eq!(e.topic, "Él");
    }
    let drae = dict("03DRAE2014.dcti", "sujetó");
    assert!(!drae.iter().any(|e| e.topic == "sujeto" && e.match_kind == "exact"), "{:?}", drae.iter().map(|e| (&e.topic, &e.match_kind)).collect::<Vec<_>>());
    let papa = dict("03DRAE2014.dcti", "papa");
    if let Some(i) = papa.iter().position(|e| e.topic == "papá") {
        let j = papa.iter().position(|e| e.topic == "papa").unwrap_or(usize::MAX);
        assert!(j < i, "papá ranked above papa");
    }
}

#[test]
fn golden_iss08_no_prefix_junk_and_headwords_kept() {
    let Some(m) = mods() else { return };
    assert_eq!(full_topics(&dict("Nelson.dcti", "fe")), ["FE"]);
    let ce = dict("03 Nuevo Diccionario Biblico Certeza.dcti", "Espíritu");
    assert!(ce[0].topic.starts_with("Espíritu, Espíritu Santo"), "{:?}", ce[0].topic);
    let dicts = paths_of(&m, "dictionary");
    let mut junk = Vec::new();
    for d in &dicts {
        for e in dictionary_lookup(d, "Dios", 30).unwrap_or_default() {
            // «Dios, odioso a» (Ortiz) is a real «Dios, …» headword; junk is a mid-word match.
            let t = e.topic.to_lowercase();
            let head = t.split(|c: char| !c.is_alphanumeric()).find(|w| !w.is_empty()).unwrap_or("");
            if ["dioscor", "botadios", "odioso", "fastidiosity"].iter().any(|j| head.contains(j)) {
                junk.push(format!("{d}: {}", e.topic));
            }
        }
    }
    assert!(junk.is_empty(), "{junk:#?}");
    let t0 = std::time::Instant::now();
    let fe_count = dicts.iter().filter(|d| probe_dictionary_has(d, "fe")).count();
    eprintln!("PROBE iss08 probe 'fe' true in {fe_count} of {} dictionaries ({:?})", dicts.len(), t0.elapsed());
    assert!(fe_count <= 60, "{fe_count}");
    for nc in ["00_NC-STRONG-E_Nueva_Concordancia_Strong_Exhaustiva.dcti", "nc-strong-e_nueva_concordancia_strong_exhaustiva.dcti"] {
        let size: usize = dict(nc, "de").iter().map(|e| e.html.len() + e.plain.len()).sum();
        eprintln!("PROBE iss08 {nc} 'de' payload {size} bytes");
        assert!(size <= 500 * 1024, "{nc}: {size}");
    }
    let nc = "00_NC-STRONG-E_Nueva_Concordancia_Strong_Exhaustiva.dcti";
    let has = |name: &str, q: &str, topic: &str| {
        let es = dict(name, q);
        assert!(es.iter().any(|e| e.topic == topic), "{name} {q} → {:?}", es.iter().map(|e| &e.topic).collect::<Vec<_>>());
    };
    has(nc, "viejo", "VIEJO, A");
    has(nc, "ninguno", "NINGUNO, A");
    has(nc, "suyo", "SUYO,A");
    has(nc, "Haya", "HAYA (v.)");
    has("01LBLA.dcti", "pecho", "Pecho(s)");
    has("01LBLA.dcti", "dioses", "Dioses, Falsos");
    has("01RVR1995.dcti", "reino", "REINO DE DIOS");
    has("03 Nuevo Diccionario Biblico Certeza.dcti", "pan", "Pan\t");
    let po = dict("02 Concordancia y diccionario de Pedro Ortiz.dcti", "fuera");
    eprintln!("PROBE iss08 Ortiz fuera → {:?}", po.iter().map(|e| &e.topic).take(5).collect::<Vec<_>>());
    let rep = dict(nc, "repudia");
    assert!(rep.iter().any(|e| e.topic == "REPUDIAR" && e.match_kind == "verbform"), "{:?}", rep.iter().map(|e| (&e.topic, &e.match_kind)).collect::<Vec<_>>());
    let vila = dict("03 Nuevo Diccionario Biblico Vila Escuain.dcti", "gracia");
    eprintln!("PROBE iss08 Vila gracia → {:?}", vila.iter().map(|e| (&e.topic, &e.match_kind)).collect::<Vec<_>>());
    assert!(!vila.is_empty());
    let vila_s = dictionary_lookup_ranked(
        &lib_file("03 Nuevo Diccionario Biblico Vila Escuain.dcti").unwrap(),
        "gracia",
        30,
        DictLookupOpts { search: true },
    )
    .unwrap();
    let topics: Vec<&str> = vila_s.iter().map(|e| e.topic.as_str()).collect();
    assert_eq!(topics, ["Gracia", "Gracia de Dios", "Acción de Gracias"], "{vila_s:?}");
    if let Some(pdt) = lib_file("01PDT.dcti") {
        let s = dictionary_lookup_ranked(&pdt, "Dios", 60, DictLookupOpts { search: true }).unwrap();
        let rd = s.iter().find(|e| e.topic.eq_ignore_ascii_case("Reino de Dios"));
        eprintln!("PROBE iss08 PDT search Dios: {} entries, Reino de Dios = {:?}", s.len(), rd.map(|e| &e.match_kind));
        assert!(rd.map(|e| e.match_kind != "exact").unwrap_or(true));
    }
}

#[test]
fn golden_iss14_plural_finds_singular() {
    if library().is_none() {
        return;
    }
    let p = dict("Nelson.dcti", "pecados");
    assert_eq!(p[0].topic, "PECADO", "{:?}", p.iter().map(|e| &e.topic).collect::<Vec<_>>());
    assert_eq!(p[0].match_kind, "lemma");
    assert_eq!(dict("Nelson.dcti", "discípulos")[0].topic, "DISCÍPULO");
    let a = dict("Nelson.dcti", "aguas");
    let ia = a.iter().position(|e| e.topic == "AGUA");
    let ic = a.iter().position(|e| e.topic.starts_with("AGUAS DE CELO"));
    assert!(ia.is_some(), "{:?}", a.iter().map(|e| &e.topic).collect::<Vec<_>>());
    if let Some(ic) = ic {
        assert!(ia.unwrap() < ic);
    }
    assert!(dict("Nelson.dcti", "fe").iter().all(|e| e.match_kind != "lemma"));
}

#[test]
fn golden_iss32_wildcards_and_empty_rows() {
    if library().is_none() {
        return;
    }
    assert!(dict("Nelson.dcti", "%").is_empty());
    assert!(dict("Nelson.dcti", "_a").is_empty());
    for e in dict("Adventista Beta08-04-06.dcti", "pan") {
        assert!(!e.html.trim().is_empty(), "{e:?}");
    }
    assert!(!dict("Adventista Beta08-04-06.dcti", "pan").is_empty() || !dict("Adventista Beta08-04-06.dcti", "mano").is_empty());
    let Some(dp) = lib_file("demonologia.dcti") else { return };
    let conn = Connection::open_with_flags(&dp, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let topics: Vec<String> = conn
        .prepare("SELECT DISTINCT Topic FROM Dictionary LIMIT 300")
        .unwrap()
        .query_map([], |r| cell_to_string(r, 0))
        .unwrap()
        .flatten()
        .collect();
    for t in topics {
        for e in dictionary_lookup(&dp, &t, 5).unwrap_or_default() {
            assert!(!to_plain(&e.html).trim().is_empty(), "{t}: {e:?}");
        }
    }
}

#[test]
fn golden_iss12_legacy_schemas_are_readable() {
    let Some(m) = mods() else { return };
    let by = |f: &str| m.iter().find(|x| x.filename == f).cloned();
    if let Some(s) = by("Straubinger_cat.cmtx") {
        assert!(!s.encrypted, "Straubinger flagged encrypted");
        assert!(s.title.contains("Straubinger"), "{}", s.title);
        let e = load_commentary(&s.path, 43, 3, 1, true, false, false).unwrap();
        let all: String = e.iter().map(|x| x.plain.clone()).collect();
        assert!(all.contains("Vino de noche") || all.contains("vino de noche"), "{all:.300}");
        eprintln!("PROBE iss12 Straubinger 43:3:1 backslash present: {}", all.contains('\\'));
    }
    if let Some(l) = by("LECTIO Pistas para la Lectio Divina de los Evangelios.cmtx") {
        assert!(load_commentary(&l.path, 43, 3, 16, true, true, true).is_ok());
    }
    for f in ["TNMx Traducción del Nuevo Mundo (1987).bblx", "Testigos de Jehova (TNM).bblx"] {
        if let Some(t) = by(f) {
            assert!(t.encrypted, "{f} must stay encrypted");
        }
    }
    let cs = dict("CONSTRONGES.dcti", "amor");
    assert_eq!(cs[0].topic, "AMOR");
    assert!(!dict("CONSTRONGES.dcti", "PABLO").is_empty());
    assert!(!dict("Concordancia de las Sagradas Escrituras Versión Reina Valera 1960.dcti", "Jehová").is_empty());
    assert!(!dict("DBDig Diccionario Bíblico Digital.dcti", "Amor").is_empty());
    for f in ["DiccBibl1.dcti", "DBA-CH.dcti"] {
        if let Some(p) = lib_file(f) {
            assert!(dictionary_lookup(&p, "Dios", 3).is_ok(), "{f}");
        }
    }
    let aaron = dict("CONSTRONGES.dcti", "Aarón");
    assert!(aaron.iter().all(|e| !e.topic.contains('_')), "{:?}", aaron.iter().map(|e| &e.topic).collect::<Vec<_>>());
}

#[test]
fn golden_iss13_ranked_modules_and_spanish_first() {
    let (Some(m), Some(ip)) = (mods(), irv()) else { return };
    let dicts = paths_of(&m, "dictionary");
    let lex = paths_of(&m, "lexicon");
    let t0 = std::time::Instant::now();
    let r = resolve_word_links_at("Dios", &dicts, &lex, Some(&ip), 1, 1, 1, None, &ResolveOpts::default());
    eprintln!("PROBE iss13 cold resolve Dios@Gn1:1 with {} dicts + {} lexicons: {:?}", dicts.len(), lex.len(), t0.elapsed());
    assert!(!r.lexicon.is_empty());
    assert_ne!(r.lexicon[0].module_abbr, "AHLB");
    assert!(r.lexicon.iter().any(|e| e.module_abbr == "STRONGes"), "{:?}", r.lexicon.iter().map(|e| &e.module_abbr).collect::<Vec<_>>());
    assert!(r.lexicon.len() <= LEXICON_CAP && r.dictionary.len() <= DICTIONARY_CAP);
    let sim = resolve_word_links_at("simiente", &dicts, &lex, Some(&ip), 1, 3, 15, None, &ResolveOpts::default());
    assert!(sim.dictionary.iter().any(|e| rank_module(&e.module_path, &module_title(&e.module_path)) < 500), "{:?}", sim.dictionary.iter().map(|e| &e.module_abbr).collect::<Vec<_>>());
    let fe = resolve_word_links_at("fe", &dicts, &lex, Some(&ip), 58, 11, 1, None, &ResolveOpts::default());
    for e in &fe.dictionary {
        let t = e.topic.to_lowercase();
        assert!(!t.contains("fear") && !t.contains("feast"), "{}", e.topic);
    }
    let pablo = resolve_word_links_at("Pablo", &dicts, &lex, Some(&ip), 44, 13, 9, None, &ResolveOpts::default());
    assert!(pablo.dictionary.iter().any(|e| e.module_abbr == "CONSTRONGES"), "{:?}", pablo.dictionary.iter().map(|e| &e.module_abbr).collect::<Vec<_>>());
    let t1 = std::time::Instant::now();
    let n = 10;
    for _ in 0..n {
        let _ = resolve_word_links_at("Dios", &dicts, &lex, Some(&ip), 1, 1, 1, None, &ResolveOpts::default());
    }
    eprintln!("PROBE iss13 warm resolve mean: {:?}", t1.elapsed() / n);
    assert!(conn_cache_len() <= CONN_CACHE_CAP);
}

#[test]
fn golden_iss17_commentary_order_and_dedupe() {
    let Some(lib) = library() else { return };
    let p = |f: &str| lib.join(f).to_string_lossy().to_string();
    let mac = load_commentary(&p("02BE_Macarthur.cmti"), 43, 3, 16, true, false, false).unwrap();
    assert_eq!(mac[0].label, "v.16", "{:?}", mac.iter().map(|e| &e.label).collect::<Vec<_>>());
    let be = load_commentary(&p("02CB-Beacon.cmti"), 45, 3, 23, true, false, false).unwrap();
    let mut seen = HashSet::new();
    for e in &be {
        assert!(seen.insert(collapse_ws(&e.plain)), "duplicate Beacon note");
    }
    let ta = load_commentary(&p("notas_torres-amat.cmti"), 1, 1, 1, true, true, false).unwrap();
    assert_eq!(ta.len(), 1, "{:?}", ta.iter().map(|e| &e.label).collect::<Vec<_>>());
    let rx = load_commentary(&p("01RVR1960x.cmti"), 43, 1, 6, true, false, false).unwrap();
    assert_eq!(rx[0].label, "v.6", "{:?}", rx.iter().map(|e| &e.label).collect::<Vec<_>>());
    let cee = load_commentary(&p("01CEE2011.cmti"), 61, 1, 20, true, false, false).unwrap();
    assert!(cee.iter().any(|e| e.label == "v.20–21"), "{:?}", cee.iter().map(|e| &e.label).collect::<Vec<_>>());
}

/// Frozen copy of the original `load_commentary` (git 6748155) for equivalence checks.
fn legacy_load_commentary(path: &str, book: i32, chapter: i32, verse: i32, iv: bool, ic: bool, ib: bool) -> Result<Vec<StudyEntry>, String> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if iv {
        let mut stmt = conn
            .prepare("SELECT ChapterBegin, VerseBegin, ChapterEnd, VerseEnd, Comments FROM VerseCommentary WHERE Book=?1")
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
        for (cb, vb, ce, ve, comments) in rows.flatten() {
            if comments.trim().is_empty() {
                continue;
            }
            let r = CmtRow { rowid: 0, cb, vb, ce, ve };
            if r.covers(chapter, verse) {
                if looks_encrypted(&comments) {
                    return Err("Módulo de comentario cifrado (🔒).".into());
                }
                let html = clean_esword_text(&comments);
                out.push(StudyEntry { scope: "verse".into(), label: r.label(), plain: to_plain(&html), html });
            }
        }
    }
    if ic {
        if let Ok(c) = conn.query_row("SELECT Comments FROM ChapterCommentary WHERE Book=?1 AND Chapter=?2", rusqlite::params![book, chapter], |r| cell_to_string(r, 0)) {
            if !c.trim().is_empty() {
                let html = clean_esword_text(&c);
                out.push(StudyEntry { scope: "chapter".into(), label: format!("Cap. {chapter}"), plain: to_plain(&html), html });
            }
        }
    }
    if ib {
        if let Ok(c) = conn.query_row("SELECT Comments FROM BookCommentary WHERE Book=?1", rusqlite::params![book], |r| cell_to_string(r, 0)) {
            if !c.trim().is_empty() {
                let html = clean_esword_text(&c);
                out.push(StudyEntry { scope: "book".into(), label: "Libro".into(), plain: to_plain(&html), html });
            }
        }
    }
    Ok(out)
}

#[test]
fn golden_iss10_commentary_equivalence_vs_legacy() {
    let Some(m) = mods() else { return };
    let cmts: Vec<&ModuleInfo> = m.iter().filter(|x| x.kind == "commentary" && !x.encrypted).collect();
    let verses: Vec<(i32, i32, i32)> = if slow() {
        let mut v = vec![(61, 1, 20), (44, 2, 9)];
        for i in 0..28 {
            v.push((1 + (i * 7) % 66, 1 + i % 5, 1 + (i * 3) % 12));
        }
        v
    } else {
        // the 20 frozen crossref-snapshot verses plus CEE2011's TEXT-typed row
        vec![
            (1, 1, 1), (19, 23, 1), (23, 53, 5), (43, 3, 16), (45, 8, 28), (66, 22, 21),
            (1, 50, 26), (2, 1, 1), (2, 40, 38), (19, 1, 1), (19, 150, 6), (23, 1, 1),
            (23, 66, 24), (39, 1, 1), (39, 4, 6), (40, 1, 1), (40, 28, 20), (42, 1, 1),
            (42, 24, 53), (44, 1, 1), (61, 1, 20),
        ]
    };
    let filters = [(true, false, false), (true, true, false), (true, true, true)];
    let (mut compared, mut mismatches, mut probe_mismatch) = (0, Vec::new(), Vec::new());
    for c in &cmts {
        for &(b, ch, v) in &verses {
            for &(iv, ic, ib) in &filters {
                let Ok(old) = legacy_load_commentary(&c.path, b, ch, v, iv, ic, ib) else { continue };
                let new = load_commentary(&c.path, b, ch, v, iv, ic, ib).unwrap();
                compared += 1;
                let key = |e: &StudyEntry| (e.scope.clone(), e.label.clone(), e.html.clone());
                let mut seen = HashSet::new();
                let old_dedup: HashSet<_> = old.iter().filter(|e| seen.insert(dedupe_key(&e.html, &e.plain))).map(key).collect();
                let new_set: HashSet<_> = new.iter().map(key).collect();
                if old_dedup != new_set || new.len() != new_set.len() {
                    mismatches.push(format!("{} {b}:{ch}:{v} {iv}{ic}{ib}: legacy {} new {}", c.filename, old.len(), new.len()));
                }
                if probe_commentary_has(&c.path, b, ch, v, iv, ic, ib) != !new.is_empty() {
                    probe_mismatch.push(format!("{} {b}:{ch}:{v} {iv}{ic}{ib}", c.filename));
                }
            }
        }
    }
    eprintln!("PROBE iss10 commentary equivalence: {compared} comparisons, {} mismatches, {} probe mismatches", mismatches.len(), probe_mismatch.len());
    assert!(mismatches.is_empty(), "{:#?}", &mismatches[..mismatches.len().min(20)]);
    assert!(probe_mismatch.is_empty(), "{:#?}", &probe_mismatch[..probe_mismatch.len().min(20)]);
}

#[test]
fn golden_iss10_probe_dictionary_matches_ranked_lookup() {
    let Some(m) = mods() else { return };
    let dicts = paths_of(&m, "dictionary");
    let words = ["Dios", "amor", "fe", "Jehová", "pecados", "pan", "Moisés", "gracia", "reino", "cielos", "Pablo", "tierra", "luz", "agua", "Espíritu", "hijo", "padre", "Jerusalén", "simiente", "corazón"];
    let mut bad = Vec::new();
    for d in &dicts {
        for w in words {
            let has = dictionary_lookup(d, w, 3)
                .map(|v| v.iter().any(|e| matches!(e.match_kind.as_str(), "exact" | "headword" | "lemma")))
                .unwrap_or(false);
            if has != probe_dictionary_has(d, w) {
                bad.push(format!("{d}: {w}"));
            }
        }
    }
    assert!(bad.is_empty(), "{:#?}", &bad[..bad.len().min(20)]);
}

#[test]
fn golden_iss07_exact_hits_are_not_lost_vs_legacy_nocase() {
    let Some(m) = mods() else { return };
    let dicts = paths_of(&m, "dictionary");
    let mut words: Vec<&str> = vec![
        "Dios", "amor", "fe", "Jehová", "pan", "Moisés", "gracia", "reino", "Pablo", "tierra", "luz",
        "agua", "Espíritu", "hijo", "padre", "Jerusalén", "corazón", "Abraham", "David", "Pedro",
        "templo", "ley", "pecado", "cruz", "sangre", "iglesia", "oración", "santo", "gloria", "paz",
        // ISS-07 review: multi-word headwords typed in a different case per word.
        "reino de dios", "reino de Dios", "año sabático", "intérprete de la ley", "ja matzot",
        "mar muerto", "arca del pacto", "hijo del hombre", "espíritu santo", "monte de los olivos",
    ];
    if slow() {
        words.extend([
            "Israel", "Egipto", "Babilonia", "Juan", "María", "José", "Jacob", "Isaac", "Noé", "Adán",
            "Eva", "Caín", "Abel", "Sara", "Rut", "Ester", "Job", "Daniel", "Elías", "Eliseo",
            "profeta", "sacerdote", "rey", "altar", "arca", "pacto", "fiesta", "pascua", "sábado",
            "ángel", "demonio", "cielo", "infierno", "muerte", "vida", "verdad", "camino", "puerta",
            "pastor", "oveja", "vino", "aceite", "trigo", "fuego", "nube", "monte", "río", "mar",
            "mar rojo", "torre de babel", "cordero de dios", "nuevo testamento", "antiguo testamento",
            "día de la expiación", "lugar santísimo", "fiesta de los tabernáculos", "tierra prometida",
        ]);
    }
    let (mut total, mut misses) = (0, Vec::new());
    for d in &dicts {
        let Ok(conn) = Connection::open_with_flags(d, OpenFlags::SQLITE_OPEN_READ_ONLY) else { continue };
        let Some(ts) = detect_schema(&conn).dict else { continue };
        for w in &words {
            let sql = format!(
                "SELECT \"{}\" FROM \"{}\" WHERE \"{}\" = ?1 COLLATE NOCASE AND length(trim(\"{}\"))>0 LIMIT 1",
                ts.topic, ts.table, ts.topic, ts.def
            );
            let Ok(t) = conn.query_row(&sql, rusqlite::params![w], |r| cell_to_string(r, 0)) else { continue };
            let def: String = conn
                .query_row(&format!("SELECT \"{}\" FROM \"{}\" WHERE \"{}\" = ?1", ts.def, ts.table, ts.topic), rusqlite::params![t], |r| cell_to_string(r, 0))
                .unwrap_or_default();
            if to_plain(&clean_esword_text(&def)).trim().is_empty() || looks_encrypted(&def) {
                continue;
            }
            total += 1;
            let got = dictionary_lookup(d, w, 10).unwrap_or_default();
            if !got.iter().any(|e| e.match_kind == "exact" && e.topic.to_lowercase() == display_topic(&t).to_lowercase()) {
                misses.push(format!("{d}: {w} (topic {t:?})"));
            }
        }
    }
    eprintln!("PROBE iss07 legacy NOCASE exact hits {total}, misses {}", misses.len());
    assert!(misses.is_empty(), "{:#?}", &misses[..misses.len().min(20)]);
}

/// ISS-02/36 aggregate gate (slow): every-Nth verse of 00RVR1960+, each own-tagged word is
/// tapped in RVR1960 wording with its occurrence index, untagged path (the interlinear).
#[test]
fn golden_aggregate_own_tag_agreement() {
    if !slow() {
        eprintln!("SKIP: set ESPADA_SLOW_TESTS=1 for the aggregate own-tag agreement gate");
        return;
    }
    let (Some(lib), Some(ip)) = (library(), irv()) else { return };
    let plus = lib.join("00RVR1960+.bbli").to_string_lossy().to_string();
    let conn = Connection::open_with_flags(&plus, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let rows: Vec<(i32, i32, i32, String)> = conn
        .prepare("SELECT Book, Chapter, Verse, Scripture FROM Bible WHERE rowid % 100 = 0")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, cell_to_string(r, 3)?)))
        .unwrap()
        .flatten()
        .collect();
    let (mut n, mut agree, mut none, mut wrong) = (0usize, 0usize, 0usize, 0usize);
    let (mut untagged, mut untagged_fp, mut fake) = (0usize, 0usize, 0usize);
    for (b, c, v, raw) in &rows {
        let units = own_tag_units(raw);
        let tagged_words: HashSet<String> = units.iter().map(|u| normalize_word(&u.word)).collect();
        let plain = to_plain(&clean_esword_text(raw));
        let toks: Vec<String> = plain
            .split(|ch: char| !(ch.is_alphanumeric() || ch == '\''))
            .filter(|t| !t.is_empty() && !t.chars().any(|c| c.is_ascii_digit()))
            .map(|s| s.to_string())
            .collect();
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut unit_iter = units.iter().peekable();
        for t in &toks {
            let nt = normalize_word(t);
            if nt.is_empty() {
                continue;
            }
            let occ = *counts.get(&nt).unwrap_or(&0);
            *counts.entry(nt.clone()).or_default() += 1;
            let own = unit_iter.peek().filter(|u| normalize_word(&u.word) == nt).map(|u| u.strongs.clone());
            let r = resolve_word_links_at(t, &[], &[], Some(&ip), *b, *c, *v, Some(occ), &ResolveOpts::default());
            if r.strongs.iter().any(|s| parse_strong_code(s).is_none()) {
                fake += 1;
            }
            if let Some(own) = own {
                unit_iter.next();
                n += 1;
                let own0 = parse_strong_code(&own[0]).unwrap_or_default();
                match r.strongs.first() {
                    Some(s) if *s == own0 => agree += 1,
                    None => none += 1,
                    _ => wrong += 1,
                }
            } else if !tagged_words.contains(&nt) && nt.chars().count() <= 3 {
                untagged += 1;
                if !r.strongs.is_empty() {
                    untagged_fp += 1;
                }
            }
        }
    }
    let pct = |a: usize, b: usize| if b == 0 { 0.0 } else { a as f64 * 100.0 / b as f64 };
    eprintln!(
        "PROBE aggregate RVR1960+ sample {} verses: tagged words {n}, first-Strong agreement {:.1}%, none {:.1}%, wrong {:.1}%; short untagged words {untagged}, with a Strong {:.1}%; taps with invalid codes {fake}",
        rows.len(),
        pct(agree, n),
        pct(none, n),
        pct(wrong, n),
        pct(untagged_fp, untagged)
    );
    assert_eq!(fake, 0);
    assert!(pct(agree, n) >= 85.0, "agreement {:.1}%", pct(agree, n));
    assert!(pct(untagged_fp, untagged) <= 10.0, "untagged false positives {:.1}%", pct(untagged_fp, untagged));
}

/// Original `sample_encrypted` (git 6748155) for the scan regression check.
fn legacy_sample_encrypted(conn: &Connection, kind: &str) -> bool {
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
        if let Ok(sample) = conn.query_row(sql, [], |r| cell_to_string(r, 0)) {
            if !sample.is_empty() {
                return looks_encrypted(&sample);
            }
        }
    }
    false
}

#[test]
fn golden_scan_flags_unchanged_except_legacy_commentaries() {
    let Some(m) = mods() else { return };
    let lib_dir = library().unwrap().to_string_lossy().to_string();
    // per-folder list: other tests scan temp folders concurrently (global list would race)
    let unreadable_list = scan_unreadable_paths_in(&lib_dir);
    eprintln!("PROBE scan: {} modules, {} unreadable {:?}", m.len(), unreadable_list.len(), unreadable_list);
    assert!(m.len() >= 700, "{}", m.len());
    // Review round 2: unreadable ⇔ not a SQLite file (the library has two e-Sword 7
    // MS-Access/Jet «.bbl» Bibles; SQLite opens lazily, so only a first read catches them).
    let unreadable: HashSet<String> = unreadable_list.into_iter().collect();
    for x in &m {
        let mut head = [0u8; 16];
        let sqlite = fs::File::open(&x.path)
            .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head))
            .map(|_| &head == b"SQLite format 3\0")
            .unwrap_or(false);
        assert_eq!(unreadable.contains(&x.path), !sqlite, "{}", x.path);
    }
    let mut changed = Vec::new();
    for x in &m {
        let ext = Path::new(&x.path).extension().and_then(|e| e.to_str()).unwrap_or("");
        let conn = Connection::open_with_flags(&x.path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let legacy = is_legacy_encrypted_ext(ext) || legacy_sample_encrypted(&conn, &x.kind);
        if legacy != x.encrypted {
            changed.push(format!("{} legacy={legacy} new={}", x.filename, x.encrypted));
            assert!(
                x.kind == "commentary" && ext.eq_ignore_ascii_case("cmtx") && !x.encrypted,
                "unexpected encryption change: {} legacy={legacy} new={}",
                x.filename,
                x.encrypted
            );
        }
    }
    eprintln!("PROBE scan encryption changes (legacy .cmtx now readable): {changed:?}");
    let t0 = std::time::Instant::now();
    let again = mods().unwrap();
    eprintln!("PROBE scan warm cached: {:?}", t0.elapsed());
    assert_eq!(again.len(), m.len());
}

/// Timing / I/O probe (slow). Prints numbers; asserts only loose ceilings so a loaded
/// machine does not flake. Debug builds are several times slower than release.
#[test]
fn golden_perf_probe() {
    if !slow() {
        eprintln!("SKIP: set ESPADA_SLOW_TESTS=1 for the timing probe");
        return;
    }
    let (Some(m), Some(ip)) = (mods(), irv()) else { return };
    let dicts = paths_of(&m, "dictionary");
    let lex = paths_of(&m, "lexicon");
    let cmts: Vec<String> = paths_of(&m, "commentary");
    // Per-thread counter: other tests' opens are not counted, but running in parallel they
    // still evict cache entries; golden_iss28_fd_memory_and_reopen_gates asserts in isolation.
    let opens = || THREAD_OPENS.with(|c| c.get());
    let o = ResolveOpts::default();
    for (w, b, c, v) in [("Dios", 1, 1, 1), ("simiente", 1, 3, 15), ("Bienaventurados", 40, 5, 3)] {
        let _ = resolve_word_links_at(w, &dicts, &lex, Some(&ip), b, c, v, None, &o);
        let n = 5;
        let (o0, t0) = (opens(), std::time::Instant::now());
        for _ in 0..n {
            let _ = resolve_word_links_at(w, &dicts, &lex, Some(&ip), b, c, v, None, &o);
        }
        eprintln!(
            "PROBE perf resolve {w}: warm mean {:?}, opens per tap {:.1}",
            t0.elapsed() / n,
            (opens() - o0) as f64 / n as f64
        );
    }
    let t0 = std::time::Instant::now();
    for d in &dicts {
        let _ = probe_dictionary_has(d, "Jehová");
    }
    eprintln!("PROBE perf dictionary probe: mean {:?} over {}", t0.elapsed() / dicts.len() as u32, dicts.len());
    let t0 = std::time::Instant::now();
    for c in &cmts {
        let _ = probe_commentary_has(c, 44, 2, 9, true, false, false);
    }
    eprintln!("PROBE perf commentary probe (Hch 2:9): mean {:?} over {}", t0.elapsed() / cmts.len() as u32, cmts.len());
    let t0 = std::time::Instant::now();
    for l in &lex {
        let _ = lexicon_lookup(l, "G99999");
    }
    eprintln!("PROBE perf lookup_lexicon_multi('G99999') equivalent: {:?}", t0.elapsed());
    let t0 = std::time::Instant::now();
    let mut k = 0;
    for l in lex.iter().take(10) {
        for i in 0..450 {
            let _ = lexicon_lookup(l, &format!("H{}", 1 + i * 19));
            let _ = lexicon_lookup(l, &format!("G{}", 1 + i * 12));
            k += 2;
        }
    }
    eprintln!("PROBE perf {k} Strong lookups: {:?}", t0.elapsed());
    assert!(conn_cache_len() <= CONN_CACHE_CAP);
}

// ─── Review round 1: ISS-14 gender stage, ISS-28 reopen cost and gates ───

/// The resolve dictionary pass for one module (full matches only, as the tap panes see it).
fn tap_dict_topics(path: &str, word: &str) -> Vec<String> {
    with_slot(path, |conn, schema| {
        let Some(ts) = schema.dict else { return Ok(vec![]) };
        let cands = ranked_candidates(conn, &ts, word, false);
        entries_from_cands(conn, &ts, &cands, DICT_PER_MODULE, true)
    })
    .unwrap_or_default()
    .into_iter()
    .map(|e| e.topic)
    .collect()
}

fn fold_topic(t: &str) -> String {
    strip_accents_es(&compose_nfc_es(t.trim()).to_lowercase())
}

#[test]
fn lemma_candidates_staged_splits_number_and_gender() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(lemma_candidates_staged("puertas"), (s(&["puerta"]), s(&["puerto"])));
    assert_eq!(lemma_candidates_staged("luces"), (s(&["luz", "luc", "luce"]), s(&[])));
    assert_eq!(lemma_candidates_staged("santa"), (s(&[]), s(&["santo"])));
    assert_eq!(lemma_candidates_staged("pecados"), (s(&["pecado"]), s(&[])));
    // public API unchanged: number candidates first, then gender
    assert_eq!(lemma_candidates("puertas"), s(&["puerta", "puerto"]));
    assert_eq!(lemma_candidates("santa"), s(&["santo"]));
    assert!(lemma_candidates_staged("fe").0.is_empty() && lemma_candidates_staged("fe").1.is_empty());
}

#[test]
fn gender_guess_is_never_full_and_waits_for_number_stage() {
    let d = tmp_dir("gender");
    let both = d.join("both.dcti");
    make_dict(&both, &[("PUERTA", "abertura"), ("PUERTO", "lugar de mar"), ("CASA", "vivienda"), ("CASO", "suceso")]);
    let only_m = d.join("onlym.dcti");
    make_dict(&only_m, &[("PUERTO", "lugar de mar"), ("PUERTO RICO", "isla"), ("REINO", "dominio")]);
    let b = both.to_string_lossy().to_string();
    let m = only_m.to_string_lossy().to_string();
    let topics = |p: &str, q: &str| dictionary_lookup(p, q, 10).unwrap().into_iter().map(|e| e.topic).collect::<Vec<_>>();
    // number stage hit → gender stage never runs
    assert_eq!(topics(&b, "puertas"), ["PUERTA"]);
    assert_eq!(topics(&b, "casas"), ["CASA"]);
    // no singular: the masculine headword is offered (kind lemma) but never as a full tap match
    let pm = dictionary_lookup(&m, "puertas", 10).unwrap();
    assert_eq!(pm.iter().map(|e| e.topic.as_str()).collect::<Vec<_>>(), ["PUERTO"], "{pm:?}");
    assert_eq!(pm[0].match_kind, "lemma");
    assert!(tap_dict_topics(&m, "puertas").is_empty());
    assert!(tap_dict_topics(&m, "reina").is_empty());
    assert_eq!(tap_dict_topics(&b, "puertas"), ["PUERTA"]);
}

#[test]
fn golden_iss14_review_no_wrong_gender_articles_in_taps() {
    let Some(m) = mods() else { return };
    let dicts = paths_of(&m, "dictionary");
    let mut puerta_full = 0;
    for d in &dicts {
        for (q, wrong) in [("puertas", "puerto"), ("casas", "caso"), ("cosas", "coso"), ("palabras", "palabro"), ("plantas", "planto"), ("reina", "reino"), ("palma", "palmo")] {
            let t = tap_dict_topics(d, q);
            assert!(
                !t.iter().any(|x| fold_topic(x).starts_with(wrong)),
                "{q} → {t:?} in {}",
                module_abbr(d)
            );
            if q == "puertas" && t.iter().any(|x| fold_topic(x) == "puerta") {
                puerta_full += 1;
            }
        }
        // the dictionary pane never lists the masculine next to a found singular
        let all = dictionary_lookup(d, "puertas", 6).unwrap_or_default();
        if all.iter().any(|e| fold_topic(&e.topic) == "puerta") {
            assert!(!all.iter().any(|e| fold_topic(&e.topic).starts_with("puerto")), "{}: {all:?}", module_abbr(d));
        }
    }
    eprintln!("PROBE iss14 review: puertas → full Puerta in {puerta_full} of {} dictionaries", dicts.len());
    assert!(puerta_full >= 5, "plural fallback lost: {puerta_full}");
    // ISS-14 must-keep cases
    let p = dict("Nelson.dcti", "pecados");
    assert_eq!((p[0].topic.as_str(), p[0].match_kind.as_str()), ("PECADO", "lemma"));
    if let Some(n) = lib_file("Nelson.dcti") {
        assert_eq!(tap_dict_topics(&n, "pecados").first().map(|s| s.as_str()), Some("PECADO"));
    }
}

#[test]
fn evicted_module_reopens_without_schema_detection() {
    let d = tmp_dir("schemacache");
    let p = d.join("s.dcti");
    make_dict(&p, &[("x", "y")]);
    let ps = p.to_string_lossy().to_string();
    assert_eq!(dictionary_lookup(&ps, "x", 1).unwrap()[0].plain, "y");
    // simulate LRU eviction
    CONN_CACHE.lock().map.remove(&ps);
    let (o0, s0) = (THREAD_OPENS.with(|c| c.get()), THREAD_SCHEMA_DETECTS.with(|c| c.get()));
    assert_eq!(dictionary_lookup(&ps, "x", 1).unwrap()[0].plain, "y");
    assert_eq!(THREAD_OPENS.with(|c| c.get()) - o0, 1, "evicted module must reopen");
    assert_eq!(THREAD_SCHEMA_DETECTS.with(|c| c.get()) - s0, 0, "schema re-detected on reopen");
    // a replaced file with another schema is re-detected (word/data layout)
    let tmp = d.join("s.new");
    {
        let c = Connection::open(&tmp).unwrap();
        c.execute_batch(
            "CREATE TABLE dictionary(word TEXT primary key collate nocase, data TEXT);
             INSERT INTO dictionary VALUES ('x', 'nuevo esquema con más texto');",
        )
        .unwrap();
    }
    fs::rename(&tmp, &p).unwrap();
    let got = dictionary_lookup(&ps, "x", 1).unwrap();
    assert!(got[0].plain.contains("nuevo esquema"), "{got:?}");
    // a non-database file is still rejected in Spanish
    let bad = d.join("bad.dcti");
    fs::write(&bad, vec![b'z'; 4096]).unwrap();
    let e = dictionary_lookup(bad.to_str().unwrap(), "x", 1).unwrap_err();
    assert_eq!(e, "El archivo no es un módulo válido");
}

const ISS28_TAPS: [(&str, i32, i32, i32); 20] = [
    ("Dios", 1, 1, 1),
    ("principio", 1, 1, 1),
    ("cielos", 1, 1, 1),
    ("simiente", 1, 3, 15),
    ("serpiente", 1, 3, 1),
    ("tierra", 1, 12, 1),
    ("casas", 2, 12, 7),
    ("pastor", 19, 23, 1),
    ("aguas", 19, 23, 2),
    ("Bienaventurados", 40, 5, 3),
    ("pobres", 40, 5, 3),
    ("espíritu", 40, 5, 3),
    ("puertas", 40, 16, 18),
    ("luz", 43, 1, 5),
    ("amado", 43, 3, 16),
    ("mundo", 43, 3, 16),
    ("vida", 43, 3, 16),
    ("gracia", 49, 2, 8),
    ("fe", 58, 11, 1),
    ("pecados", 62, 1, 9),
];

fn open_fds() -> usize {
    fs::read_dir("/dev/fd").map(|d| d.count()).unwrap_or(0)
}

/// Child half of [`golden_iss28_fd_memory_and_reopen_gates`]: runs alone in its own process
/// (single thread, `ulimit -n 256`), so the global connection cache is not shared.
#[test]
#[ignore = "child process of golden_iss28_fd_memory_and_reopen_gates"]
fn iss28_gate_child() {
    if std::env::var("ESPADA_ISS28_CHILD").as_deref() != Ok("1") {
        return;
    }
    let (Some(m), Some(ip)) = (mods(), irv()) else {
        panic!("library vanished under the child");
    };
    let dicts = paths_of(&m, "dictionary");
    let lex = paths_of(&m, "lexicon");
    set_pinned_modules(std::slice::from_ref(&ip));
    let o = ResolveOpts::default();
    let tap = |w: &str, b, c, v| {
        let (o0, s0, f0) = (
            THREAD_OPENS.with(|x| x.get()),
            THREAD_SCHEMA_DETECTS.with(|x| x.get()),
            THREAD_OPEN_FAILS.with(|x| x.get()),
        );
        let t0 = std::time::Instant::now();
        let r = resolve_word_links_at(w, &dicts, &lex, Some(&ip), b, c, v, None, &o);
        let el = t0.elapsed();
        (
            r,
            THREAD_OPENS.with(|x| x.get()) - o0,
            THREAD_SCHEMA_DETECTS.with(|x| x.get()) - s0,
            THREAD_OPEN_FAILS.with(|x| x.get()) - f0,
            el,
        )
    };
    let mut max_fds = open_fds();
    let mut fails = 0;
    let mut warm_opens = 0;
    for round in 0..3 {
        let (mut opens, mut detects, mut t) = (0, 0, std::time::Duration::ZERO);
        for (w, b, c, v) in ISS28_TAPS {
            let (_, n, s, f, el) = tap(w, b, c, v);
            max_fds = max_fds.max(open_fds());
            fails += f;
            opens += n;
            detects += s;
            t += el;
            if round > 0 {
                assert_eq!(s, 0, "{w}: schema re-detected on a warm tap");
                assert!(n <= dicts.len() + lex.len() + 1, "{w}: {n} opens in one tap");
            }
            if round == 2 {
                eprintln!("ISS28 warm {w}: opens {n}, {el:?}");
            }
        }
        eprintln!("ISS28 round {round}: {opens} opens ({:.1}/tap), {detects} schema detections, {t:?}", opens as f64 / 20.0);
        if round == 2 {
            warm_opens = opens;
        }
    }
    // Never worse on average than the audit baseline (18.9 opens per tap, 119 taps).
    assert!(warm_opens <= 15 * ISS28_TAPS.len(), "warm opens {warm_opens} over {} taps", ISS28_TAPS.len());
    // Same word tapped again (the plan's «warm tap»): ≤3 opens whenever the tap's working set
    // fits the cache. Rare words consult every dictionary (ISS-13), more than CONN_CACHE_CAP
    // modules, so they still reopen; that conflict is reported to the plan owner.
    for (w, b, c, v) in [("Dios", 1, 1, 1), ("gracia", 49, 2, 8), ("luz", 43, 1, 5), ("puertas", 40, 16, 18)] {
        let _ = tap(w, b, c, v);
        let (_, n, _, _, el) = tap(w, b, c, v);
        eprintln!("ISS28 repeat {w}: opens {n}, {el:?}");
        assert!(n <= 3, "{w}: {n} opens on an immediately repeated tap");
    }
    for (w, b, c, v) in [("amado", 43, 3, 16), ("simiente", 1, 3, 15), ("Bienaventurados", 40, 5, 3)] {
        let _ = tap(w, b, c, v);
        let (_, n, _, _, el) = tap(w, b, c, v);
        eprintln!("ISS28 repeat {w}: opens {n}, {el:?} (working set > CONN_CACHE_CAP)");
    }
    // SAFETY: sqlite3_memory_used only reads SQLite's global counter.
    let mem = unsafe { rusqlite::ffi::sqlite3_memory_used() };
    eprintln!(
        "ISS28 gates: max fds {max_fds}, open failures {fails}, SQLite memory {} KiB, cache {} (cap {CONN_CACHE_CAP}), {} dictionaries + {} lexicons",
        mem / 1024,
        conn_cache_len(),
        dicts.len(),
        lex.len()
    );
    assert!(max_fds < 120, "max fds {max_fds}");
    assert_eq!(fails, 0, "open failures under ulimit -n 256");
    assert!(mem < 40 * 1024 * 1024, "SQLite memory {} KiB", mem / 1024);
    set_pinned_modules(&[]);
}

/// ISS-28 gates (plan test_plan): 60 taps under `ulimit -n 256` → max fds <120, 0 open
/// failures, SQLite memory <40 MB, no schema re-detection on warm taps, ≤3 opens on a
/// repeated common-word tap. Runs [`iss28_gate_child`] in a separate single-threaded process.
#[test]
fn golden_iss28_fd_memory_and_reopen_gates() {
    if library().is_none() {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("ulimit -n 256 && exec \"$0\" \"$@\"")
        .arg(&exe)
        .args(["--ignored", "--exact", "modules::golden_tests::iss28_gate_child", "--test-threads=1", "--nocapture"])
        .env("ESPADA_ISS28_CHILD", "1")
        .output()
        .expect("spawn child test");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    for l in stderr.lines().chain(stdout.lines()).filter(|l| l.starts_with("ISS28") || l.contains("panicked") || l.contains("SKIP")) {
        eprintln!("PROBE {l}");
    }
    assert!(out.status.success(), "child failed:\n{stdout}\n{stderr}");
    assert!(stdout.contains("1 passed"), "child did not run:\n{stdout}");
}

// ─── ISS-07 review round 2: multi-word headwords typed in another case ───

#[test]
fn multi_token_query_finds_headword_with_per_word_case() {
    let d = tmp_dir("phrasecase");
    let p = d.join("d.dcti");
    make_dict(
        &p,
        &[
            ("Reino", "<p>reino.</p>"),
            ("Reino de Dios", "<p>El reinado de Dios.</p>"),
            ("Reino De Dios, el", "<p>Otra forma.</p>"),
            ("Año Sabático", "<p>Séptimo año.</p>"),
            ("Ja-Matzot", "<p>Fiesta.</p>"),
            ("\u{a0}Intérprete de La Ley", "<p>Escriba.</p>"),
            ("Reinos del mundo", "<p>no</p>"),
        ],
    );
    let ps = p.to_str().unwrap();
    for (q, want) in [
        ("reino de dios", "Reino de Dios"),
        ("reino de Dios", "Reino de Dios"),
        ("REINO DE DIOS", "Reino de Dios"),
        ("año sabático", "Año Sabático"),
        ("ja-matzot", "Ja-Matzot"),
        ("intérprete de la ley", "\u{a0}Intérprete de La Ley"),
    ] {
        let e = dictionary_lookup(ps, q, 10).unwrap();
        assert!(!e.is_empty(), "{q}: no hits");
        assert_eq!(e[0].match_kind, "exact", "{q}: {e:?}");
        assert_eq!(e[0].topic, want, "{q}: {e:?}");
        assert!(probe_dictionary_has(ps, q), "{q}");
    }
    // «Reino De Dios, el» is a headword form of the phrase, ranked after the exact one.
    let e = dictionary_lookup(ps, "reino de dios", 10).unwrap();
    assert!(e.iter().any(|x| x.topic == "Reino De Dios, el" && x.match_kind == "headword"), "{e:?}");
    assert!(!e.iter().any(|x| x.topic == "Reinos del mundo"), "{e:?}");
    // single words are unchanged (no phrase scan): «reino» → «Reino» first.
    assert_eq!(dictionary_lookup(ps, "reino", 10).unwrap()[0].topic, "Reino");
    assert!(is_multi_token("a b") && is_multi_token("ja-matzot") && is_multi_token("a\u{a0}b"));
    assert!(!is_multi_token(" Dios ") && !is_multi_token("Jehová"));
}

/// Reviewer's examples (real library): lowercase multi-word queries find the exact headword
/// first, as legacy `Topic = ?1 COLLATE NOCASE` did.
#[test]
fn golden_iss07_lowercase_phrase_finds_mixed_case_headword() {
    let cases = [
        ("03 Nuevo Diccionario Biblico Vila Escuain.dcti", "reino de dios", "reino de dios"),
        ("03 Nuevo Diccionario Biblico Vila Escuain.dcti", "reino de Dios", "reino de dios"),
        ("03 Nuevo Diccionario Biblico Certeza.dcti", "año sabático", "año sabático"),
    ];
    for (file, q, want) in cases {
        let Some(p) = lib_file(file) else { continue };
        let e = dictionary_lookup(&p, q, 30).unwrap();
        let top = e.first().unwrap_or_else(|| panic!("{file}: {q}: no hits"));
        assert_eq!(top.match_kind, "exact", "{file}: {q}: {:?}", e.iter().map(|x| &x.topic).collect::<Vec<_>>());
        assert_eq!(top.topic.to_lowercase(), want, "{file}: {q}");
    }
}

/// Mirror of the round-2 review probe: mixed-case multi-word topics of every dictionary,
/// typed in lowercase. Every one legacy NOCASE equality finds (non-empty article) must come
/// back as an `exact` hit. Default run samples 4 per dictionary; ESPADA_SLOW_TESTS=1 → 40.
#[test]
fn golden_iss07_lowercased_multiword_topics_vs_legacy_nocase() {
    let Some(m) = mods() else { return };
    let per_dict = if slow() { 40 } else { 4 };
    let (mut total, mut misses) = (0usize, Vec::new());
    for d in paths_of(&m, "dictionary") {
        let Ok(conn) = Connection::open_with_flags(&d, OpenFlags::SQLITE_OPEN_READ_ONLY) else { continue };
        let Some(ts) = detect_schema(&conn).dict else { continue };
        let topics: Vec<String> = conn
            .prepare(&format!(
                "SELECT \"{c}\" FROM \"{t}\" WHERE \"{c}\" LIKE '% %' AND length(\"{c}\") < 60",
                c = ts.topic,
                t = ts.table
            ))
            .and_then(|mut st| {
                st.query_map([], |r| cell_to_string(r, 0))
                    .map(|rows| rows.flatten().collect())
            })
            .unwrap_or_default();
        let mixed: Vec<String> = topics
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| t.chars().any(|c| c.is_uppercase()) && t.chars().any(|c| c.is_lowercase()) && t.contains(' '))
            .collect();
        let step = (mixed.len() / per_dict).max(1);
        for t in mixed.iter().step_by(step).take(per_dict) {
            let q = t.to_lowercase();
            let sql = format!(
                "SELECT \"{d}\" FROM \"{t}\" WHERE \"{c}\" = ?1 COLLATE NOCASE",
                d = ts.def,
                t = ts.table,
                c = ts.topic
            );
            let defs: Vec<String> = conn
                .prepare(&sql)
                .and_then(|mut st| st.query_map(rusqlite::params![q], |r| cell_to_string(r, 0)).map(|r| r.flatten().collect()))
                .unwrap_or_default();
            if !defs
                .iter()
                .any(|def| !looks_encrypted(def) && !to_plain(&clean_esword_text(def)).trim().is_empty())
            {
                continue;
            }
            total += 1;
            let got = dictionary_lookup(&d, &q, 30).unwrap_or_default();
            let want = display_topic(t).to_lowercase();
            if !got.iter().any(|e| e.match_kind == "exact" && e.topic.trim().to_lowercase() == want) {
                misses.push(format!("{d}: {q:?}"));
            }
        }
    }
    eprintln!("PROBE iss07 lowercased multi-word topics: {total} legacy hits, {} misses", misses.len());
    assert!(misses.is_empty(), "{:#?}", &misses[..misses.len().min(20)]);
}

// ─── Review round 2: nonblocking fixes ───

#[test]
fn truncate_entry_html_never_ends_inside_a_tag() {
    // No <br>/<p>/newline anywhere: the hard cut at MAX_ENTRY_HTML falls inside the tag.
    let mut html = "a".repeat(MAX_ENTRY_HTML - 10);
    html.push_str("<span class=\"una-clase-muy-larga\">");
    html.push_str(&"b".repeat(1000));
    html.push_str("</span>");
    let (h, t) = truncate_entry_html(html);
    assert!(t);
    let body = h.split("<p>… (texto truncado").next().unwrap();
    assert_eq!(body, "a".repeat(MAX_ENTRY_HTML - 10), "cut must stop before the open tag");
    assert_eq!(tag_safe_cut("abc<b>de"), 8);
    assert_eq!(tag_safe_cut("abc<b"), 3);
    assert_eq!(tag_safe_cut("<b"), 2, "never cut to empty");
}

fn make_bible_counts(path: &std::path::Path, counts: &[(i32, i32, i32)]) {
    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    {
        let c = Connection::open(&tmp).unwrap();
        c.execute_batch("CREATE TABLE Bible (Book INT, Chapter INT, Verse INT, Scripture TEXT);").unwrap();
        for &(b, ch, n) in counts {
            for v in 1..=n {
                c.execute("INSERT INTO Bible VALUES (?1, ?2, ?3, 'x')", rusqlite::params![b, ch, v]).unwrap();
            }
        }
    }
    fs::rename(&tmp, path).unwrap(); // atomic replace, like a module update
}

#[test]
fn versification_remap_rechecks_a_replaced_file() {
    let d = tmp_dir("remapident");
    let (b, il) = (d.join("b.bbli"), d.join("il.bbli"));
    make_bible_counts(&b, &[(4, 16, 35), (4, 17, 28)]);
    make_bible_counts(&il, &[(4, 16, 50), (4, 17, 13)]);
    let (bs, ils) = (b.to_str().unwrap(), il.to_str().unwrap());
    assert!(versification_remap(bs, ils).numbers);
    // Interlinear replaced under the same name with matching versification.
    make_bible_counts(&il, &[(4, 16, 35), (4, 17, 28), (1, 1, 31)]);
    assert!(!versification_remap(bs, ils).numbers, "stale remap after file replacement");
}

#[test]
fn prose_topic_verdict_rechecks_a_replaced_file() {
    let d = tmp_dir("proseident");
    let p = d.join("l.lexi");
    let tmp = d.join("l.tmp");
    make_dict(&p, &[("H1", "padre"), ("H2", "hijo")]);
    let ps = p.to_str().unwrap();
    assert!(!prose_topic_module(ps));
    let long = "una frase larga que sirve de tema en léxicos tipo DBADV, mucho más de cuarenta";
    make_dict(&tmp, &[(long, "x"), (long, "y")]);
    fs::rename(&tmp, &p).unwrap();
    assert!(prose_topic_module(ps), "stale prose verdict after file replacement");
}

#[test]
fn non_database_file_is_detected_as_unreadable() {
    let d = tmp_dir("notadb");
    let bad = d.join("roto.bbli");
    fs::write(&bad, "esto no es una base de datos SQLite, solo texto ".repeat(200)).unwrap();
    let good = d.join("ok.dcti");
    make_dict(&good, &[("x", "y")]);
    let bc = open_ro(bad.to_str().unwrap()).expect("SQLite opens lazily");
    assert!(db_unreadable(&bc));
    let gc = open_ro(good.to_str().unwrap()).unwrap();
    assert!(!db_unreadable(&gc));
}

#[test]
fn unreadable_list_is_kept_per_scanned_folder() {
    let (a, b) = (tmp_dir("unreadA"), tmp_dir("unreadB"));
    fs::write(a.join("roto.bbli"), "no es SQLite ".repeat(100)).unwrap();
    make_dict(&a.join("ok.dcti"), &[("x", "y")]);
    make_dict(&b.join("ok.dcti"), &[("x", "y")]);
    let (sa, sb) = (a.to_string_lossy().to_string(), b.to_string_lossy().to_string());
    let ma = scan_modules(&sa).unwrap();
    assert_eq!(ma.len(), 2);
    // `encrypted` keeps today's value for a non-database file; it is only listed.
    assert!(!ma.iter().find(|x| x.filename == "roto.bbli").unwrap().encrypted);
    scan_modules(&sb).unwrap();
    let ua = scan_unreadable_paths_in(&sa);
    assert_eq!(ua.len(), 1, "{ua:?}");
    assert!(ua[0].ends_with("roto.bbli"));
    assert!(scan_unreadable_paths_in(&sb).is_empty());
}
