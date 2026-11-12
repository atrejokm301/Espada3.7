//! WP2 golden tests on real library rows (read-only). Each skips with a visible message
//! when the module library is absent. Most fail on the pre-WP2 cleaner (`legacy`).

use super::gate_tests::{cell, library_dir, open_ro, visible};
use super::{clean_bible_verse_tagged, clean_esword_text, legacy, own_tag_units, to_plain};

fn one(file: &str, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Option<String> {
    let dir = library_dir()?;
    let path = format!("{dir}/{file}");
    let Some(conn) = open_ro(&path) else {
        eprintln!("SKIP: cannot open {path}");
        return None;
    };
    match conn.query_row(sql, params, |r| cell(r, 0)) {
        Ok(s) => Some(s),
        Err(e) => panic!("{file}: {sql}: {e}"),
    }
}

fn verse(file: &str, b: i32, c: i32, v: i32) -> Option<String> {
    one(
        file,
        "SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3",
        &[&b, &c, &v],
    )
}

fn topic(file: &str, table: &str, t: &str) -> Option<String> {
    one(
        file,
        &format!("SELECT Definition FROM {table} WHERE Topic=?1"),
        &[&t],
    )
}

#[test]
fn golden_nelson_amor_keeps_khesed_paragraph() {
    let Some(raw) = topic("Nelson.dcti", "Dictionary", "AMOR") else { return };
    let old = visible(&legacy::clean_esword_text(&raw));
    let h = clean_esword_text(&raw);
    let v = visible(&h);
    assert!(!old.contains("khesed"), "baseline already had it?");
    assert!(v.contains("khesed"), "{v}");
    assert!(h.contains("&lt;ahab"), "{h}");
    assert!(v.len() > old.len() + 400, "old {} new {}", old.len(), v.len());
    assert!(to_plain(&h).contains("khesed"));
}

#[test]
fn golden_bl1995_temor_servil_visible() {
    let Some(raw) = one(
        "Biblia  Latinoamerica 1995.cmti",
        "SELECT Comments FROM VerseCommentary WHERE Book=1 AND ChapterBegin=16 AND VerseBegin<=13 AND VerseEnd>=13",
        &[],
    ) else {
        return;
    };
    let v = visible(&clean_esword_text(&raw));
    assert!(v.contains("temor <servil> a Dios"), "{v}");
}

#[test]
fn golden_bdb_thayer_g5794_glued_br_paragraph_visible() {
    let Some(raw) = topic("04BDB-THAYERes.lexi", "Lexicon", "G5794") else { return };
    let v = visible(&clean_esword_text(&raw));
    assert!(v.contains("El modo imperativo"), "{v}");
}

#[test]
fn golden_cevallos_mk_4_8_and_rev_21_6_lose_no_words() {
    for (b, c, vs) in [(41, 4, 8), (66, 21, 6)] {
        let Some(raw) = verse("00A-CEVALLOS NT-INT+.bbli", b, c, vs) else { return };
        let old = visible(&legacy::clean_esword_text(&raw));
        let new = visible(&clean_esword_text(&raw));
        let mut have: std::collections::HashMap<&str, i32> = Default::default();
        for w in new.split(|c: char| !c.is_alphabetic()) {
            *have.entry(w).or_default() += 1;
        }
        for w in old.split(|c: char| !c.is_alphabetic()).filter(|w| w.chars().count() >= 2) {
            let e = have.entry(w).or_default();
            *e -= 1;
            assert!(*e >= 0, "{b}:{c}:{vs} lost word {w:?}\nOLD {old}\nNEW {new}");
        }
    }
}

#[test]
fn golden_adventista_abraham_has_no_rtf_par() {
    let Some(raw) = topic("Adventista Beta08-04-06.dcti", "Dictionary", "Abraham.") else { return };
    assert!(raw.contains("\\par"), "fixture changed");
    let h = clean_esword_text(&raw);
    assert!(!h.contains("\\par"), "{h}");
}

#[test]
fn golden_bdb_h346_c1_apostrophe_decodes() {
    let Some(raw) = topic("bdb.lexi", "Lexicon", "H346") else { return };
    let h = clean_esword_text(&raw);
    assert!(h.contains('’'), "{h}");
    assert!(!h.chars().any(|c| ('\u{80}'..='\u{9f}').contains(&c)), "C1 left in {h}");
}

#[test]
fn golden_nc_strong_a_has_no_split_entity_refs() {
    let Some(raw) = topic(
        "00_NC-STRONG-E_Nueva_Concordancia_Strong_Exhaustiva.dcti",
        "Dictionary",
        "A",
    ) else {
        return;
    };
    let count = |h: &str| h.matches(r#"data-raw="e;n"#).count() + h.matches(r#">e;n "#).count();
    let old = count(&legacy::clean_esword_text(&raw));
    let h = clean_esword_text(&raw);
    let new = count(&h);
    eprintln!("NC-STRONG-E 'A': raw 'e;n' ref spans old={old} new={new}");
    assert!(old > 0, "baseline expected split-entity refs");
    assert_eq!(new, 0);
    assert!(h.contains(r#"data-book="gén""#), "spliced refs expected");
}

#[test]
fn golden_rvr1960_plus_gn_12_3_tierra_tagged_h127() {
    let Some(raw) = verse("00RVR1960+.bbli", 1, 12, 3) else { return };
    let u = own_tag_units(&raw);
    assert_eq!(
        u.iter().find(|x| x.word == "tierra").map(|x| x.strongs.clone()),
        Some(vec!["H127".to_string()]),
        "{u:?}"
    );
    assert!(!u.iter().any(|x| x.word == "familias"));
    let h = clean_bible_verse_tagged(&raw);
    assert!(h.contains(r#"<span class="word" data-strong="H127">tierra</span>"#), "{h}");
    assert_eq!(visible(&h), visible(&clean_esword_text(&raw)));
}

#[test]
fn golden_lbla_plus_1sa_16_7_senor_tagged_h3068() {
    for f in ["10LBLA+.bbli", "00LBLA+.bbli"] {
        let Some(raw) = verse(f, 9, 16, 7) else { return };
        let u = own_tag_units(&raw);
        assert_eq!(
            u.iter().find(|x| x.word == "SEÑOR").map(|x| x.strongs[0].clone()),
            Some("H3068".to_string()),
            "{f}: {u:?}"
        );
    }
}

#[test]
fn golden_rv1960_and_irv_visible_text_unchanged() {
    for (f, refs) in [
        ("00RVR1960.bbli", vec![(43, 3, 16), (1, 4, 23), (19, 23, 1), (1, 1, 1)]),
        ("00Interlineal-iRV 1960+.bbli", vec![(1, 1, 1), (43, 3, 16), (9, 16, 7), (19, 23, 1)]),
    ] {
        for (b, c, v) in refs {
            let Some(raw) = verse(f, b, c, v) else { return };
            assert_eq!(
                visible(&clean_esword_text(&raw)),
                visible(&legacy::clean_esword_text(&raw)),
                "{f} {b}:{c}:{v}"
            );
        }
    }
}
