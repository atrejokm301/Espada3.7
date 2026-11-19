//! Codepage repair (`tambiιn` → `también`). Checked on the library on 2026-11-18 with a
//! before/after dump of 13 named modules: 15 661 words repaired in 5 damaged modules, every one
//! by the mapping alone, and not one change in the Bibles or the clean Greek lexicons.

use super::{clean_bible_text, clean_bible_verse_tagged, clean_esword_text, repair_codepage_mojibake, to_plain};

fn shown(raw: &str) -> String {
    to_plain(&clean_esword_text(raw))
}

#[test]
fn damaged_accents_come_back() {
    // CP1253 (Greek) and CP1255 (Hebrew) damage, inside a word and at its end.
    assert_eq!(shown("también la posiciσn"), "también la posición");
    assert_eq!(shown("tambiιn está"), "también está");
    assert_eq!(shown("Seסor, Aplicaciףn"), "Señor, Aplicación");
    assert_eq!(shown("Este mσdulo no estα asociado"), "Este módulo no está asociado");
    assert_eq!(shown("ortografםa espaρola"), "ortografía española");
    // At a word's start, and the opening question mark.
    assert_eq!(shown("ιl dijo: ΏQuι es el ϊltimo?"), "él dijo: ¿Qué es el último?");
    assert_eq!(shown("los יxitos y los νdolos"), "los éxitos y los ídolos");
    // Capitals and the transliterated omega.
    assert_eq!(shown("PRΣLOGO de IΤANNOU, kτmē"), "PRÓLOGO de IŌANNOU, kōmē");
}

#[test]
fn real_greek_and_hebrew_are_never_touched() {
    for raw in ["ἀγάπη agápe", "λόγος", "ὁ θεός", "אָב padre", "בְּרֵאשִׁית", "πνεῦμα (pneuma)", "ν", "α y ω"] {
        assert_eq!(shown(raw), raw, "{raw}");
    }
    // A run of two or more Greek letters is Greek, even beside Latin letters.
    assert_eq!(shown("δe 600"), "δe 600");
    assert_eq!(shown("parΧ"), "parΧ", "Χ is not a damaged accent");
}

#[test]
fn a_lookalike_letter_with_its_latin_word_in_the_text_stays() {
    // Biblia Comentada: «νida» is «vida» typed with a Greek nu, not a damaged «ída».
    assert_eq!(shown("la fuente de una νida colmada; la vida eterna"), "la fuente de una νida colmada; la vida eterna");
    // Without «vida» in the text it reads as damage (ν → í), like every «νdolos».
    assert_eq!(shown("los νdolos"), "los ídolos");
}

#[test]
fn repair_sees_through_inline_tags_and_entities() {
    assert_eq!(clean_esword_text("tambi<b>ι</b>n"), "tambi<strong>é</strong>n");
    // An entity that decodes to the damaged letter (repair runs after decoding).
    assert_eq!(shown("tambi&#953;n"), "también");
    // Attribute text is never visible text.
    let html = r#"<span class="woc" title="tambiιn">x</span>"#.to_string();
    assert_eq!(repair_codepage_mojibake(html.clone()), html);
}

#[test]
fn bible_text_keeps_its_letters() {
    // OGNTe+ transliterations write Latin «I» as a Greek capital iota.
    assert_eq!(to_plain(&clean_bible_text("<TF>Ιisu</Tf> <TF>Ιsaak</Tf>")), "Ιisu Ιsaak");
    assert_eq!(clean_bible_text("tambiιn"), "tambiιn");
    assert_eq!(clean_bible_verse_tagged("tambiιn"), "tambiιn");
    // The study panels' cleaner would read it as «É».
    assert_eq!(shown("Ιsaak"), "Ésaak");
}

#[test]
fn text_without_greek_or_hebrew_is_returned_as_is() {
    let s = "Porque de tal manera amó Dios al mundo".repeat(2000);
    assert_eq!(repair_codepage_mojibake(s.clone()), s);
}
