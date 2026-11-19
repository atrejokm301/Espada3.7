//! Tests for the features ported from the Windows build (module-pipeline, 2026-11-18): the
//! Léxico's topic index, the devotionals, the Spanish Strong's renderings, section titles in
//! combined modules and in Bibles.
//!
//! Synthetic SQLite fixtures go to the system temp dir. Real-library checks read named modules
//! only (never a whole-library scan) and print SKIP when the library is absent.

use super::golden_tests::{library, make_dict, tmp_dir};
use super::*;

fn make_lexicon(path: &Path, rows: &[(&str, &str)]) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT);
         INSERT INTO Details VALUES ('Léxico prueba','LXP',1);
         CREATE TABLE Lexicon (Topic TEXT, Definition TEXT);
         CREATE INDEX TopicIndex ON Lexicon (Topic);",
    )
    .unwrap();
    for (t, d) in rows {
        c.execute("INSERT INTO Lexicon VALUES (?1, ?2)", rusqlite::params![t, d]).unwrap();
    }
}

// ─── Léxico topic index ───

#[test]
fn strong_prefix_knows_strong_codes_only() {
    for t in ["G26", "h1254", "G 26", "G00001", "H1254a", " g7 "] {
        assert!(strong_prefix(t).is_some(), "{t}");
    }
    for t in ["1", "26", "A200001", "G", "G123456", "Gal", "Génesis", "H12ab", ""] {
        assert_eq!(strong_prefix(t), None, "{t}");
    }
    assert_eq!(strong_prefix("h1254"), Some('H'));
}

#[test]
fn extract_lemma_takes_the_entry_headword_only() {
    assert_eq!(extract_lemma("ἀγάπη agápe; de G25; amor", 'G', "G26").as_deref(), Some("ἀγάπη"));
    assert_eq!(extract_lemma("Número Strong: G26 Lexema: ἀγάπη", 'G', "G26").as_deref(), Some("ἀγάπη"));
    let h = "בָּרָא bara; raíz primaria";
    assert_eq!(extract_lemma(h, 'H', "H1254").as_deref(), Some("בָּרָא"));
    // LSJ marks the stem with a hyphen.
    assert_eq!(extract_lemma("ἀγάπ-η, ἡ, love", 'G', "G26").as_deref(), Some("ἀγάπη"));
    // A word glued to another script, an occurrence count, another number's word, too late.
    assert_eq!(extract_lemma("ἀγάπē amor", 'G', "G26"), None);
    assert_eq!(extract_lemma("θεε (16) θεον (12)", 'G', "G2316"), None);
    assert_eq!(extract_lemma("λαλέω[G2980] habla", 'G', "G3056"), None);
    assert_eq!(extract_lemma("λόγος G3056 palabra", 'G', "G3056").as_deref(), Some("λόγος"));
    let late = format!("{} ἀγάπη", "x".repeat(60));
    assert_eq!(extract_lemma(&late, 'G', "G26"), None);
    // A Hebrew code never takes a Greek word.
    assert_eq!(extract_lemma("λόγος", 'H', "H1"), None);
}

#[test]
fn lexicon_index_orders_codes_and_reads_their_headwords() {
    let dir = tmp_dir("lexindex");
    let p = dir.join("strong.lexi");
    make_lexicon(
        &p,
        &[
            ("H2", "אָב ab; padre"),
            ("G10", "ἀγαθοεργέω agathoergéo; hacer el bien"),
            ("G1", "Α Alfa; primera letra"),
            ("H1", "אֵב eb; verdor"),
            ("G2", "Ἀαρών Aarón; Aarón"),
            ("G0026", "ἀγάπη agápe; amor"),
        ],
    );
    let ps = p.to_string_lossy().to_string();
    let s = topic_index(&ps, None, None, 0, 50, true).unwrap();
    let topics: Vec<&str> = s.rows.iter().map(|r| r.topic.as_str()).collect();
    assert_eq!(topics, ["G1", "G2", "G10", "G0026", "H1", "H2"], "natural order, G before H");
    assert!(s.strong_keyed);
    assert_eq!((s.greek_start, s.hebrew_start), (Some(0), Some(4)));
    let lemma = |t: &str| s.rows.iter().find(|r| r.topic == t).map(|r| r.lemma.clone()).unwrap();
    assert_eq!(lemma("G0026"), "ἀγάπη");
    assert_eq!(lemma("H2"), "אָב");
    assert_eq!(lemma("G1"), "Α");
    // A typed code finds its row whatever its zeros and spaces.
    let q = topic_index(&ps, Some("g 26"), None, 0, 1, true).unwrap();
    assert!(q.exact, "{q:?}");
    assert_eq!(q.rows[0].topic, "G0026");
    // Opening a row reads exactly that article.
    let e = topic_entry_by_rowid(&ps, q.rows[0].rowid, true).unwrap().expect("entry");
    assert!(e.html.contains("agápe"), "{}", e.html);
}

#[test]
fn dictionary_index_rows_carry_no_headword() {
    let dir = tmp_dir("dictindex-nolemma");
    let p = dir.join("d.dcti");
    make_dict(&p, &[("G26", "ἀγάπη amor"), ("AMOR", "Afecto")]);
    let s = topic_index(&p.to_string_lossy(), None, None, 0, 10, false).unwrap();
    assert!(s.rows.iter().all(|r| r.lemma.is_empty()), "the Diccionario never reads headwords");
    let json = serde_json::to_string(&s.rows[0]).unwrap();
    assert!(!json.contains("lemma"), "an empty headword is not sent: {json}");
}

#[test]
fn a_module_with_both_tables_indexes_the_one_its_tab_reads() {
    let dir = tmp_dir("bothtables");
    let p = dir.join("both.lexi");
    let c = Connection::open(&p).unwrap();
    c.execute_batch(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT);
         INSERT INTO Details VALUES ('Ambos','AMB',1);
         CREATE TABLE Dictionary (Topic TEXT, Definition TEXT);
         INSERT INTO Dictionary VALUES ('AMOR', 'Afecto');
         CREATE TABLE Lexicon (Topic TEXT, Definition TEXT);
         INSERT INTO Lexicon VALUES ('G26', 'ἀγάπη amor');
         INSERT INTO Lexicon VALUES ('G25', 'ἀγαπάω amar');",
    )
    .unwrap();
    drop(c);
    let ps = p.to_string_lossy().to_string();
    let lex = topic_index(&ps, None, None, 0, 10, true).unwrap();
    let dict = topic_index(&ps, None, None, 0, 10, false).unwrap();
    assert_eq!(lex.total, 2);
    assert_eq!(dict.total, 1);
    assert_eq!(dict.rows[0].topic, "AMOR");
    let e = topic_entry_by_rowid(&ps, lex.rows[0].rowid, true).unwrap().unwrap();
    assert_eq!(e.topic, lex.rows[0].topic, "the entry comes from the Lexicon table");
}

/// Real Spanish Strong lexicon (named module only): Greek and Hebrew starts and headwords.
#[test]
fn real_stronges_index_has_greek_and_hebrew_headwords() {
    let Some(lib) = library() else { return };
    let p = lib.join("03STRONGes.lexi");
    if !p.exists() {
        eprintln!("SKIP: 03STRONGes.lexi not in the library");
        return;
    }
    let ps = p.to_string_lossy().to_string();
    let s = topic_index(&ps, Some("G26"), None, 0, 3, true).unwrap();
    assert!(s.exact && s.strong_keyed, "{s:?}");
    assert!(s.greek_start.is_some() && s.hebrew_start.is_some());
    assert_eq!(s.rows[0].lemma, "ἀγάπη", "{:?}", s.rows[0]);
    let h = topic_index(&ps, Some("H1254"), None, 0, 1, true).unwrap();
    assert!(h.rows[0].lemma.starts_with('ב'), "{:?}", h.rows[0]);
}

// ─── «ES» line: Spanish Strong's renderings ───

fn lib_module(name: &str) -> Option<String> {
    let p = library()?.join(name);
    if p.exists() {
        Some(p.to_string_lossy().to_string())
    } else {
        eprintln!("SKIP: {name} not in the library");
        None
    }
}

/// The three installed layouts (named modules only): same words, source named.
#[test]
fn real_spanish_strong_dictionaries_give_the_renderings() {
    let Some(strong) = lib_module("01 Diccionario strong.lexi") else { return };
    let mut paths = vec![strong];
    paths.extend(lib_module("03STRONGes.lexi"));
    let g = |code: &str| strong_gloss(code, &paths).unwrap().unwrap_or_else(|| panic!("{code}"));
    let g26 = g("G26");
    assert_eq!((g26.spanish.as_str(), g26.translit.as_str(), g26.original.as_str()), ("ágape, amado, amor", "agápe", "ἀγάπη"));
    assert_eq!(g26.source, "Strong °");
    assert_eq!(g("g0026"), g26, "spelling of the code does not matter");
    assert_eq!(g("G4151").spanish, "aliento, espíritu, espiritual, viento");
    assert_eq!(g("G4154").spanish, "soplar, viento");
    assert!(g("H1254").spanish.starts_with("atravesar, creación, creador, crear"), "{}", g("H1254").spanish);
    assert_eq!(g("H853").spanish, "[como tal no aparece en español]");
    assert_eq!(strong_gloss("G99999", &paths), Ok(None));
    assert_eq!(strong_gloss("26", &paths), Ok(None), "a bare number has no testament");
    // STRONGes alone (its Hebrew entries open with «— »).
    if let Some(es) = lib_module("03STRONGes.lexi") {
        let h = strong_gloss("H3068", &[es]).unwrap().expect("H3068 in STRONGes");
        assert!(h.spanish.starts_with("Jehová"), "{h:?}");
        assert_eq!(h.source, "STRONGes");
    }
    // Strong SPA: no Greek/Hebrew, same words.
    if let Some(spa) = lib_module("Strongspa.lexi") {
        let s = strong_gloss("G26", &[spa]).unwrap().expect("G26 in Strong SPA");
        assert_eq!(s.spanish, "ágape, amado, amor");
        assert_eq!(s.original, "");
    }
}

/// A missing file, a module without the entry or without a list is skipped, never an error.
#[test]
fn unusable_gloss_sources_are_skipped() {
    let dir = tmp_dir("gloss-skip");
    let nolist = dir.join("nolist.lexi");
    make_lexicon(&nolist, &[("G26", "ἀγάπη amor, afecto")]);
    let good = dir.join("good.lexi");
    make_lexicon(&good, &[("G0026", "ἀγάπη<br>agápe<br>de G25; amor: ágape, amado, amor.")]);
    let paths = vec![
        dir.join("missing.lexi").to_string_lossy().to_string(),
        nolist.to_string_lossy().to_string(),
        good.to_string_lossy().to_string(),
    ];
    let g = strong_gloss("G26", &paths).unwrap().expect("falls through to the module with a list");
    assert_eq!((g.spanish.as_str(), g.translit.as_str(), g.original.as_str()), ("ágape, amado, amor", "agápe", "ἀγάπη"));
    assert_eq!((g.module.as_str(), g.source.as_str()), ("good.lexi", "LXP"));
    // A readable module without a list: «no list». Nothing readable at all: an error, which the
    // page does not remember (it asks again).
    assert_eq!(strong_gloss("G26", &paths[..2]), Ok(None));
    assert!(strong_gloss("G26", &paths[..1]).is_err(), "only a missing module");
    // Only the first four sources are read.
    let many: Vec<String> = std::iter::repeat(paths[0].clone()).take(4).chain([paths[2].clone()]).collect();
    assert!(strong_gloss("G26", &many).is_err());
}

// ─── Devocional (.devi) ───

fn make_devotional(path: &Path, rows: &[(&str, &str, &str)]) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT);
         INSERT INTO Details VALUES ('Manantiales en el desierto','MED',1);
         CREATE TABLE Devotional (Month INT, Day INT, Devotion TEXT);",
    )
    .unwrap();
    for (m, d, text) in rows {
        // Month/Day as given: some modules store them as text.
        c.execute("INSERT INTO Devotional VALUES (?1, ?2, ?3)", rusqlite::params![m, d, text]).unwrap();
    }
}

#[test]
fn a_devotional_is_listed_as_its_own_kind() {
    let dir = tmp_dir("devi-scan");
    make_devotional(&dir.join("manantiales.devi"), &[("1", "1", "<p>Año nuevo</p>")]);
    make_dict(&dir.join("d.dcti"), &[("AMOR", "Afecto")]);
    let list = scan_modules(&dir.to_string_lossy()).unwrap();
    let dev = list.iter().find(|m| m.filename == "manantiales.devi").expect("listed");
    assert_eq!((dev.kind.as_str(), dev.abbreviation.as_str(), dev.encrypted), ("devotional", "MED", false));
    assert_eq!(list.iter().filter(|m| m.kind == "devotional").count(), 1);
}

#[test]
fn devotional_days_and_readings() {
    let dir = tmp_dir("devi-read");
    let p = dir.join("m.devi");
    make_devotional(
        &p,
        &[
            ("2", "29", "<p>Bisiesto</p>"),
            ("1", "2", "<p>Segundo día: <b>Jn 3:16</b></p>"),
            ("01", "01", "<p>Primer día, «Señor»</p>"),
            ("1", "2", "<p>Duplicado</p>"),
            ("13", "1", "<p>Mes imposible</p>"),
            ("3", "1", ""),
        ],
    );
    let ps = p.to_string_lossy().to_string();
    // Text months/days count, duplicates once, impossible dates out; an empty reading still a day row.
    assert_eq!(devotional_days(&ps).unwrap(), vec![(1, 1), (1, 2), (2, 29), (3, 1)]);
    let jan1 = devotional_entry(&ps, 1, 1).unwrap().expect("1 de enero");
    assert_eq!((jan1.month, jan1.day, jan1.abbreviation.as_str()), (1, 1, "MED"));
    assert!(jan1.html.contains("Primer día, «Señor»"), "{}", jan1.html);
    let jan2 = devotional_entry(&ps, 1, 2).unwrap().unwrap();
    assert!(jan2.html.contains("<strong>Jn 3:16</strong>") && !jan2.html.contains("Duplicado"), "first row wins: {}", jan2.html);
    assert!(devotional_entry(&ps, 2, 29).unwrap().is_some(), "29 de febrero");
    assert!(devotional_entry(&ps, 3, 1).unwrap().is_none(), "an empty reading is no reading");
    assert!(devotional_entry(&ps, 12, 25).unwrap().is_none(), "a day it has no reading for");
    assert!(devotional_entry(&ps, 13, 1).unwrap().is_none(), "never a date that does not exist");
    assert!(devotional_entry(&ps, 1, 0).unwrap().is_none());
}

#[test]
fn a_module_without_the_devotional_table_is_a_spanish_error() {
    let dir = tmp_dir("devi-wrong");
    let p = dir.join("not.devi");
    make_dict(&p, &[("AMOR", "Afecto")]);
    let ps = p.to_string_lossy().to_string();
    let e = devotional_days(&ps).unwrap_err();
    assert!(e.contains("no es un devocional"), "{e}");
    assert!(devotional_entry(&ps, 1, 1).unwrap_err().contains("Devotional"));
}

/// A legacy .devx is judged by its content, like the Windows build does: a plain one opens.
#[test]
fn a_plain_devx_is_not_marked_encrypted() {
    let dir = tmp_dir("devx-plain");
    let p = dir.join("antiguo.devx");
    make_devotional(&p, &[("1", "1", "<p>Lectura del primero de enero</p>")]);
    let list = scan_modules(&dir.to_string_lossy()).unwrap();
    let m = list.iter().find(|m| m.filename == "antiguo.devx").expect("listed");
    assert_eq!((m.kind.as_str(), m.encrypted), ("devotional", false));
    assert!(devotional_entry(&p.to_string_lossy(), 1, 1).unwrap().is_some());
}

/// A devotional whose table has other columns is an error, not «no reading» for every day.
#[test]
fn a_damaged_devotional_is_an_error_not_an_empty_day() {
    let dir = tmp_dir("devi-cols");
    let p = dir.join("raro.devi");
    let c = Connection::open(&p).unwrap();
    c.execute_batch(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT);
         INSERT INTO Details VALUES ('Raro','RAR',1);
         CREATE TABLE Devotional (Mes INT, Dia INT, Texto TEXT);
         INSERT INTO Devotional VALUES (1, 1, 'x');",
    )
    .unwrap();
    drop(c);
    let e = devotional_entry(&p.to_string_lossy(), 1, 1).unwrap_err();
    assert_eq!(e, "Formato de módulo no compatible", "a Spanish error the page shows as such");
}

#[test]
fn an_encrypted_devotional_says_so() {
    let dir = tmp_dir("devi-enc");
    let p = dir.join("cifrado.devx");
    let junk: String = (0..200).map(|i| char::from_u32(1 + (i % 7)).unwrap()).collect();
    make_devotional(&p, &[("1", "1", junk.as_str())]);
    let ps = p.to_string_lossy().to_string();
    let e = devotional_entry(&ps, 1, 1).unwrap_err();
    assert!(e.contains("cifrado"), "{e}");
    let list = scan_modules(&dir.to_string_lossy()).unwrap();
    assert!(list.iter().any(|m| m.kind == "devotional" && m.encrypted), "listed with 🔒");
}

// ─── Section titles in Bible verses ───

fn make_bible_rows(path: &Path, rows: &[(i32, i32, i32, &str)]) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT, Strongs BOOL);
         INSERT INTO Details VALUES ('Biblia con títulos','BCT',1,0);
         CREATE TABLE Bible (Book INT, Chapter INT, Verse INT, Scripture TEXT);
         CREATE INDEX BookChapterVerseIndex ON Bible (Book, Chapter, Verse);",
    )
    .unwrap();
    for (b, ch, v, t) in rows {
        c.execute("INSERT INTO Bible VALUES (?1,?2,?3,?4)", rusqlite::params![b, ch, v, t]).unwrap();
    }
}

/// Words of plain text, the way the page splits them (letters/digits runs).
fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
}

/// The verse's titles and body together hold exactly the words of the verse's whole text.
fn assert_same_words(path: &str, b: i32, c: i32, v: &VerseRow) {
    let raw = load_raw_verse(path, b, c, v.verse).unwrap();
    let lead: Vec<String> = v.titles.iter().filter(|t| t.kind != "next").map(|t| to_plain(&t.html)).collect();
    let next: Vec<String> = v.titles.iter().filter(|t| t.kind == "next").map(|t| to_plain(&t.html)).collect();
    let joined = format!("{} {} {}", lead.join(" "), v.plain, next.join(" "));
    assert_eq!(words(&joined), words(&to_plain(&clean_bible_text(&raw))), "words moved in {b}:{c}:{}", v.verse);
}

#[test]
fn a_chapter_sends_its_section_titles_beside_the_verses() {
    let dir = tmp_dir("titles-chapter");
    let p = dir.join("t.bbli");
    make_bible_rows(
        &p,
        &[
            (48, 3, 5, " Aquel, pues, ¿lo hace por las obras de la ley, o por el oír con fe? \r\n\r\nEl pacto de Dios con Abraham\r\n"),
            (48, 3, 6, "<p align=\"center\"><span style=\"font-weight:bold;\">El pacto de Dios con Abraham</span></p> Así Abraham creyó a Dios."),
            (48, 3, 7, "Sabed, por tanto, que los que son de fe, éstos son hijos de Abraham."),
            (19, 23, 1, "<p style=\"text-align:center;\"><i>Salmo de David.</i> <br /> Jehová es mi pastor; nada me faltará."),
        ],
    );
    let ps = p.to_string_lossy().to_string();
    let ch = load_bible_chapter(&ps, 48, 3).unwrap();
    let v = |n: i32| ch.verses.iter().find(|x| x.verse == n).unwrap();
    assert_eq!(v(5).titles, vec![VerseTitle { kind: "next".into(), html: "El pacto de Dios con Abraham".into() }]);
    assert!(!v(5).plain.contains("pacto"), "the title is not verse text: {}", v(5).plain);
    assert_eq!(v(6).titles.len(), 1);
    assert_eq!((v(6).titles[0].kind.as_str(), to_plain(&v(6).titles[0].html).as_str()), ("lead", "El pacto de Dios con Abraham"));
    assert_eq!(v(6).plain, "Así Abraham creyó a Dios.");
    assert!(v(7).titles.is_empty());
    for row in &ch.verses {
        assert_same_words(&ps, 48, 3, row);
    }
    let ps23 = load_bible_chapter(&ps, 19, 23).unwrap();
    assert_eq!(ps23.verses[0].titles[0].kind, "minor", "an italic psalm title is drawn lighter");
    assert_eq!(ps23.verses[0].plain, "Jehová es mi pastor; nada me faltará.");
    // A single verse is cut exactly as its chapter is.
    assert_eq!(load_verse_text(&ps, 48, 3, 6).unwrap(), "Así Abraham creyó a Dios.");
    assert_eq!(load_verse_text(&ps, 48, 3, 5).unwrap(), v(5).plain);
    // Titles are not serialized when a verse has none (fixtures and old pages keep their shape).
    let json = serde_json::to_string(v(7)).unwrap();
    assert!(!json.contains("titles"), "{json}");
}

#[test]
fn a_module_that_opens_every_verse_on_a_new_line_has_no_text_titles() {
    let dir = tmp_dir("titles-guard");
    let p = dir.join("lines.bbli");
    let rows: Vec<(i32, i32, i32, String)> =
        (1..=10).map(|v| (1, 1, v, format!("\r\nVersículo número {v}\r\nY sigue el texto del versículo {v}.\r\n"))).collect();
    let rows_ref: Vec<(i32, i32, i32, &str)> = rows.iter().map(|(b, c, v, t)| (*b, *c, *v, t.as_str())).collect();
    make_bible_rows(&p, &rows_ref);
    let ps = p.to_string_lossy().to_string();
    let ch = load_bible_chapter(&ps, 1, 1).unwrap();
    assert!(ch.verses.iter().all(|v| v.titles.is_empty()), "every verse's first line would have been a title");
    assert!(load_verse_text(&ps, 1, 1, 3).unwrap().starts_with("Versículo número 3"), "same rule for a single verse");
}

fn lib_bible(name: &str) -> Option<String> {
    lib_module(name)
}

fn titles_of(path: &str, b: i32, c: i32, v: i32) -> Vec<(String, String)> {
    let ch = load_bible_chapter(path, b, c).unwrap();
    let row = ch.verses.iter().find(|x| x.verse == v).unwrap();
    assert_same_words(path, b, c, row);
    row.titles.iter().map(|t| (t.kind.clone(), to_plain(&t.html))).collect()
}

fn pair(k: &str, t: &str) -> (String, String) {
    (k.to_string(), t.to_string())
}

/// Named Bibles of the library (read-only): titles where they are, none where there are none.
#[test]
fn real_bibles_titles_where_they_are() {
    if let Some(t) = lib_bible("rv1960-t_reina_valera_1960_con_ttulos_y_notas.bbli") {
        assert_eq!(titles_of(&t, 48, 3, 1), [pair("lead", "El Espíritu se recibe por la fe")]);
        assert_eq!(titles_of(&t, 48, 3, 5), [pair("next", "El pacto de Dios con Abraham")]);
        assert_eq!(titles_of(&t, 1, 2, 3), [pair("next", "El hombre en el huerto del Edén")]);
        assert_eq!(titles_of(&t, 19, 119, 9), [pair("lead", "Bet")]);
        assert_eq!(titles_of(&t, 19, 23, 1), [pair("lead", "Jehová es mi pastor"), pair("lead", "Salmo de David.")]);
        assert!(titles_of(&t, 19, 23, 2).is_empty());
    }
    if let Some(x) = lib_bible("00RVR1960x.bbli") {
        assert_eq!(titles_of(&x, 48, 3, 6), [pair("lead", "El pacto de Dios con Abraham")]);
    }
    if let Some(p) = lib_bible("00RVR1960.bbli") {
        assert_eq!(titles_of(&p, 19, 23, 1), [pair("minor", "Salmo de David.")]);
        assert!(titles_of(&p, 48, 3, 6).is_empty());
    }
    if let Some(s) = lib_bible("00RVR1960+.bbli") {
        let got = titles_of(&s, 19, 23, 1);
        assert!(got.len() == 1 && got[0].1.starts_with("Salmo"), "{got:?}");
    }
}

/// A word of a section title still gets its Strong from the tagged twin: «David» in plain
/// RVR1960 Ps 23:1 («Salmo de David.» is a title there).
#[test]
fn a_title_word_tap_still_reaches_the_twin() {
    let (Some(plain), Some(_), Some(il)) =
        (lib_bible("00RVR1960.bbli"), lib_bible("00RVR1960+.bbli"), lib_bible("00Interlineal-iRV 1960+.bbli"))
    else {
        return;
    };
    let opts = ResolveOpts { bible_path: Some(plain.clone()), ..ResolveOpts::default() };
    let r = resolve_word_links_at("David", &[], &[], Some(&il), 19, 23, 1, Some(0), &opts);
    assert_eq!(r.strongs.first().map(String::as_str), Some("H1732"), "{r:?}");
    assert!(r.tagged_twin.is_some(), "answered by the twin: {r:?}");
    // A word of the verse itself, after the title.
    let r = resolve_word_links_at("pastor", &[], &[], Some(&il), 19, 23, 1, Some(0), &opts);
    assert_eq!(r.strongs.first().map(String::as_str), Some("H7462"), "{r:?}");
}

/// Words never move, over a spread of chapters of named Bibles (never the whole library):
/// `cargo test --release --lib -- --ignored titles_sweep_named --nocapture`.
#[test]
#[ignore = "sweep: named Bibles × 12 chapters, words never move"]
fn titles_sweep_named() {
    let names = [
        "rv1960-t_reina_valera_1960_con_ttulos_y_notas.bbli",
        "00RVR1960x.bbli",
        "00RVR1960.bbli",
        "00RVR1960+.bbli",
        "00LBLA.bbli",
        "00LBLA+.bbli",
        "Biblia Reina Valera 1960.bblx",
        "RV60a.bblx",
    ];
    let chapters = [(1, 1), (1, 2), (19, 3), (19, 23), (19, 119), (23, 53), (40, 5), (42, 15), (45, 8), (48, 3), (49, 2), (66, 21)];
    for n in names {
        let Some(p) = lib_bible(n) else { continue };
        let (mut verses, mut lead, mut next) = (0, 0, 0);
        for (b, c) in chapters {
            let Ok(ch) = load_bible_chapter(&p, b, c) else { continue };
            for v in &ch.verses {
                verses += 1;
                assert_same_words(&p, b, c, v);
                lead += v.titles.iter().filter(|t| t.kind != "next").count();
                next += v.titles.iter().filter(|t| t.kind == "next").count();
            }
        }
        eprintln!("{n}: {verses} verses, {lead} titles before a verse, {next} after");
    }
}

// ─── Section titles in combined modules (sections.rs) ───

/// The section titles the main panel would show for one lexicon/dictionary topic.
fn shown_titles(path: &str, lexicon: bool, topic: &str) -> (Vec<String>, String, String) {
    let lookup = || if lexicon { lexicon_lookup(path, topic) } else { dictionary_lookup(path, topic, 1).map(|v| v.into_iter().next()) };
    let plain = lookup().unwrap().expect("entry").html;
    let html = with_section_titles(path, true, || lookup().unwrap().expect("entry").html);
    let open = r#"<span class="sec-title">"#;
    let mut titles = Vec::new();
    let mut rest = html.as_str();
    while let Some(a) = rest.find(open) {
        let after = &rest[a + open.len()..];
        let end = after.find("<br>").unwrap_or(after.len());
        titles.push(to_plain(after[..end].trim_end_matches("</span>")));
        rest = &after[end..];
    }
    (titles, plain, html)
}

/// The Multiléxico (named module): each dictionary it combines is a section title, its text is
/// unchanged, and only the main panel's lookups get them.
#[test]
fn real_multilexico_sections() {
    let Some(p) = lib_module("multilxico_strong-chvez-tuggy-vine-swanson.lexi") else { return };
    let (g67, plain_html, html) = shown_titles(&p, true, "G67");
    assert_eq!(g67, ["Diccionario Strong", "Diccionario Tuggy", "Diccionario Swanson"]);
    assert_eq!(to_plain(&html), to_plain(&plain_html), "the same text, plus the headings");
    let (h1254, _, _) = shown_titles(&p, true, "H1254");
    assert_eq!(h1254, ["Diccionario Strong", "Diccionario Chávez", "Diccionario Vine AT"]);
    // Without the flag (side pane, probes, tooltips) nothing is wrapped, rules known or not.
    assert!(!lexicon_lookup(&p, "G67").unwrap().unwrap().html.contains("sec-title"));
    // A plain Strong dictionary has no sections at all.
    if let Some(s) = lib_module("01 Diccionario strong.lexi") {
        let (none, _, _) = shown_titles(&s, true, "G26");
        assert!(none.is_empty(), "{none:?}");
    }
}

/// Release builds abort on a panic, so every new parser is fed random tricky markup: tags cut in
/// half, RTF groups, entities, multi-byte letters right before '<', CR/LF runs. None may panic,
/// and a verse's titles plus its body are always the verse's own bytes.
#[test]
fn new_parsers_never_panic_on_odd_module_text() {
    const PARTS: &[&str] = &[
        "<p align=\"center\">", "</p>", "<b>", "</b>", "<i>", "</i>", "<br/>", "<br", "&nbsp;", "&#953;", "&",
        "<span style=\"font-weight:bold;\">", "<span style=\"font-style:italic;\">", "</span>", "<h1>", "</h1>", "<hr>",
        "{\\b ", "{\\qc \\b ", "}", "{", "\\par ", "\\par", "\\", "\r\n", "\n\n", "\r", " ", "  ", "Título", "Salmo de David.",
        "(Dt. 1.9-18)", "Selah", "é", "ἀγάπη", "א", "ב\u{5bc}", "\u{301}", "ιl", "posiciσn", "G26", "H1254", ":", ";", ",", ".",
        "— ", "<", ">", "Ώ", "<p style=\"text-align:center;\">", "<strong>", "</strong>", "<em>", "</em>",
    ];
    let mut seed: u64 = 0x5eed_1234_abcd_ef01;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let rules = crate::sections::learn(&[
        "<p><b>Diccionario Strong</b></p><p>a</p><p><b>Diccionario Tuggy</b></p><p>b</p>".to_string(),
        "<p><b>Diccionario Strong</b></p><p>c</p><p><b>Diccionario Tuggy</b></p><p>d</p>".to_string(),
        "<p><b>Diccionario Strong</b></p><p>e</p><p><b>Diccionario Tuggy</b></p><p>f</p>".to_string(),
    ]);
    assert!(!rules.is_empty(), "fixture: some rules to apply");
    for case in 0..3000 {
        let n = 1 + (next() % 24) as usize;
        let raw: String = (0..n).map(|_| PARTS[(next() % PARTS.len() as u64) as usize]).collect();
        for text_lines in [true, false] {
            let t = crate::bible_titles::find_titles(&raw, text_lines);
            let (titles, body) = t.split(&raw);
            let rebuilt: usize = titles.iter().map(|(_, p)| p.len()).sum::<usize>() + body.len();
            assert_eq!(rebuilt, raw.len(), "case {case}: titles + body are the verse's bytes: {raw:?}");
            let _ = render_bible_verse(&raw, case % 2 == 0, text_lines);
        }
        let html = clean_esword_text(&raw);
        let wrapped = crate::sections::wrap_titles(&html, &raw, &rules);
        assert_eq!(to_plain(&wrapped), to_plain(&html), "case {case}: wrapping never changes the text: {raw:?}");
        let _ = crate::strong_gloss::parse_entry(&to_plain(&html));
        let _ = extract_lemma(&to_plain(&html), if case % 2 == 0 { 'G' } else { 'H' }, "G26");
    }
}

// ─── Legacy extensions judged by content (2026-11-18) ───

fn pseudo_random_bytes(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

#[test]
fn ciphertext_is_told_apart_from_text_stored_as_bytes() {
    // e-Sword ciphertext: random-looking, a whole number of 16-byte blocks, short ones too.
    for (n, seed) in [(80usize, 1u64), (96, 2), (176, 3), (512, 4)] {
        assert!(blob_is_ciphertext(&pseudo_random_bytes(n, seed)), "{n} random bytes");
    }
    let es = "En el principio creó Dios los cielos y la tierra. Y la tierra estaba desordenada y vacía";
    let mut utf8 = es.as_bytes().to_vec();
    utf8.truncate(80);
    assert!(!blob_is_ciphertext(&utf8), "UTF-8 text");
    let utf16: Vec<u8> = es.encode_utf16().flat_map(|u| u.to_le_bytes()).take(160).collect();
    assert!(!blob_is_ciphertext(&utf16), "UTF-16 text");
    let cp1252: Vec<u8> = es.chars().map(|c| if c == 'ó' { 0xF3 } else if c == 'í' { 0xED } else { c as u8 }).take(80).collect();
    assert!(!blob_is_ciphertext(&cp1252), "Windows-1252 text");
    let mut zlib = pseudo_random_bytes(96, 9);
    zlib[0] = 0x78;
    zlib[1] = 0x9c;
    assert!(!blob_is_ciphertext(&zlib), "zlib data is compressed text, not ciphertext");
    assert!(!blob_is_ciphertext(&pseudo_random_bytes(81, 5)), "not a whole number of blocks");
    assert!(!blob_is_ciphertext(&pseudo_random_bytes(16, 6)), "too short to tell");
}

#[test]
fn legacy_bibles_dictionaries_and_lexicons_are_locked_only_when_their_text_is_ciphertext() {
    let dir = tmp_dir("legacy-content");
    // Plain legacy modules: readable.
    make_bible_rows(&dir.join("plain.bblx"), &[(1, 1, 1, "En el principio creó Dios los cielos y la tierra.")]);
    let c = Connection::open(dir.join("plain.dctx")).unwrap();
    c.execute_batch("CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT); INSERT INTO Details VALUES ('D','D',1);
                     CREATE TABLE Dictionary (Topic TEXT, Definition TEXT); INSERT INTO Dictionary VALUES ('AMOR','Afecto profundo');").unwrap();
    drop(c);
    make_lexicon(&dir.join("plain.lexx"), &[("G26", "ἀγάπη amor")]);
    // Ciphertext Bible: every verse a random 80–176-byte blob.
    let c = Connection::open(dir.join("cifrada.bblx")).unwrap();
    c.execute_batch("CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT); INSERT INTO Details VALUES ('C','C',1);
                     CREATE TABLE Bible (Book INT, Chapter INT, Verse INT, Scripture BLOB);").unwrap();
    for v in 1..=3 {
        c.execute("INSERT INTO Bible VALUES (1,1,?1,?2)", rusqlite::params![v, pseudo_random_bytes(80 + 16 * v as usize, v as u64)]).unwrap();
    }
    drop(c);
    let list = scan_modules(&dir.to_string_lossy()).unwrap();
    let enc = |f: &str| list.iter().find(|m| m.filename == f).unwrap_or_else(|| panic!("{f} listed")).encrypted;
    assert!(!enc("plain.bblx"), "a plain .bblx opens");
    assert!(!enc("plain.dctx"), "a plain .dctx opens");
    assert!(!enc("plain.lexx"), "a plain .lexx opens");
    assert!(enc("cifrada.bblx"), "ciphertext stays 🔒");
}
