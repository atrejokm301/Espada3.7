//! The Spanish words a Spanish Strong's dictionary gives for a Strong's number: the list of
//! Reina-Valera renderings after the entry's last «:» («…festín de amor: — ágape, amado,
//! amor.»), with the transliteration and the original word. Ported from the Windows build
//! (module-pipeline 2f0840f, `strong_gloss.rs`); the lookup itself is `modules::strong_gloss`.
//!
//! The Léxico shows them as the «ES» line for a Strong's number — the only Spanish there is for
//! a number typed by hand, clicked in a definition or opened from the Índice. For particles the
//! list is how the RV renders the word in places, not its plain meaning (G1722 «en» lists
//! «exhortación, interior, bajo…»), so the page labels it as the Strong's renderings.
//!
//! The three layouts installed (the entry's text, one line per `<br>`):
//! - Strong ° / Strong (Esp): `ἀγάπη\nagápe\nde G25; amor, …: ágape, amado, amor.`
//! - STRONGes / Strong-ES: `ἀγάπη\nagápe; de G25 ; …: — ágape, amado, amor.` — a Hebrew entry
//!   opens with «— ».
//! - Strong SPA: no original script at all: `agápe\nde G25 amor, …: ágape, amado, amor.`

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrongGloss {
    /// The code asked for, normalised: «G26».
    pub code: String,
    /// The renderings: «ágape, amado, amor».
    pub spanish: String,
    /// «agápe»; empty when the entry has none that reads cleanly.
    pub translit: String,
    /// «ἀγάπη»; empty in modules without the original script (Strong SPA).
    pub original: String,
    /// The module's abbreviation («Strong °»), so the page can say where the words came from.
    pub source: String,
    /// The module's file name.
    pub module: String,
}

/// Longest list shown; longer ones end at an item boundary with «…».
const MAX_LIST_CHARS: usize = 300;

static RE_CODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b[GH]\s?\d{1,5}\b").unwrap());
static RE_CODE_NOTE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\s*\[[^\]]*\b[GH]\s?\d{1,5}\b[^\]]*\]").unwrap());
// Cross-reference words, as they open the note after a list («Compare G5590», «Véase G3729»).
// Not «ver»: it is a rendering itself (εἴδω «ver, mirar…»).
static RE_XREF: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(?:[Vv]éase|[Cc]ompárese|[Cc]omp\.|Compare\b)").unwrap());

/// «G26», «g0026», «G 26» → «G26». A bare number is not a code here: which testament it
/// belongs to is exactly what is unknown.
pub fn strong_code(code: &str) -> Option<String> {
    let t: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let mut chars = t.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    if letter != 'G' && letter != 'H' {
        return None;
    }
    let digits = chars.as_str();
    if digits.is_empty() || digits.len() > 6 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    Some(format!("{letter}{n}"))
}

/// What one entry gives: the renderings, the transliteration and the original word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub spanish: String,
    pub translit: String,
    pub original: String,
}

/// Private-use characters carry no text; U+3000 stands in for a lost accented letter in
/// Strong SPA; a Hebrew entry of STRONGes opens with «— ».
fn clean_head(raw: &str) -> String {
    let text: String = raw
        .chars()
        .filter(|c| !('\u{E000}'..='\u{F8FF}').contains(c))
        .map(|c| if c == '\u{3000}' { ' ' } else { c })
        .collect();
    text.trim_start_matches(|c: char| c.is_whitespace() || is_dash(c)).to_string()
}

/// The entry's renderings, transliteration and original word, or `None` when it has no list.
pub fn parse_entry(raw: &str) -> Option<Entry> {
    if raw.starts_with("[Módulo cifrado") || raw.starts_with("[Contenido binario") {
        return None;
    }
    let text = clean_head(raw);
    let spanish = renderings(&text)?;
    let (original, rest) = original_word(&text);
    Some(Entry { spanish, translit: translit(rest), original })
}

fn is_dash(c: char) -> bool {
    matches!(c, '—' | '–' | '-')
}

fn is_script(c: char) -> bool {
    let u = c as u32;
    (0x0370..=0x03FF).contains(&u)
        || (0x1F00..=0x1FFF).contains(&u)
        || (0x0590..=0x05FF).contains(&u)
        || (0xFB1D..=0xFB4F).contains(&u)
}

/// The list after the last «:» that does not start a verse reference («Jer 33:8»).
fn renderings(text: &str) -> Option<String> {
    let sep = text
        .char_indices()
        .filter(|&(i, c)| c == ':' && !text[i + 1..].trim_start().starts_with(|d: char| d.is_ascii_digit()))
        .map(|(i, _)| i)
        .last()?;
    let list = text[sep + 1..].trim_start_matches(|c: char| c.is_whitespace() || is_dash(c));
    let list = &list[..list_end(list)];
    // A bracketed note pointing at another number is not a rendering: «saquear [por H957]».
    let list = RE_CODE_NOTE.replace_all(list, "");
    let items: Vec<String> = list.split(',').map(tidy_item).filter(|s| !s.is_empty()).collect();
    if items.is_empty() {
        return None;
    }
    let joined = items.join(", ");
    let looks_wrong = joined.chars().any(is_script)
        || RE_CODE.is_match(&joined)
        || RE_XREF.is_match(&joined)
        || !joined.chars().any(char::is_alphabetic);
    if looks_wrong {
        return None;
    }
    Some(cap_list(&items))
}

/// Where the list stops: a line break, or a period ending it — one followed by the end, a
/// line break or a new sentence (a capital, the original script, «»), as in «… viento.
/// Compare G5590.» or «… uno.\nὅ jó. Véase G3729.». «(cond.)» is not an end.
fn list_end(list: &str) -> usize {
    for (i, c) in list.char_indices() {
        if c == '\n' {
            return i;
        }
        if c != '.' {
            continue;
        }
        let after = list[i + 1..].trim_start_matches(' ');
        if after.is_empty() || after.starts_with('\n') {
            return i;
        }
        if list[i + 1..].starts_with(' ') {
            if let Some(n) = after.chars().next() {
                // A note after the list opens a new sentence too: «… reposar. (aquí se hace
                // referencia…)», «… tapa. [En Sal 141:3, dál, irregular]».
                if n.is_uppercase() || is_script(n) || matches!(n, '«' | '(' | '[') {
                    return i;
                }
            }
            // STRONGes abbreviates the cross-reference in lower case: «… repentinamente. comp. G…».
            let word = after.to_lowercase();
            if word.starts_with("comp") || word.starts_with("véase") {
                return i;
            }
        }
    }
    list.len()
}

/// One rendering, tidied: spacing around brackets fixed and Strong's «x» (an idiom marker,
/// «x patrimonio») and «+» dropped.
fn tidy_item(item: &str) -> String {
    let mut s = item.split_whitespace().collect::<Vec<_>>().join(" ");
    for (from, to) in [(" )", ")"), ("( ", "("), (" ]", "]"), ("[ ", "["), (" .", ".")] {
        s = s.replace(from, to);
    }
    let s = s.trim_matches(|c: char| c.is_whitespace() || c == ';');
    let s = s
        .strip_prefix("x ")
        .or_else(|| s.strip_prefix("X "))
        .or_else(|| s.strip_prefix("+ "))
        .unwrap_or(s);
    s.trim().to_string()
}

fn cap_list(items: &[String]) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        let extra = if i == 0 { item.chars().count() } else { item.chars().count() + 2 };
        if i > 0 && out.chars().count() + extra > MAX_LIST_CHARS {
            out.push_str(", …");
            return out;
        }
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(item);
    }
    out
}

/// The original word at the head of the entry («ἀγάπη», «בֵּית לֶחֶם»), and the text after it.
/// A combining mark inside the word belongs to it: STRONGes writes some Greek with separate
/// breathings and accents (οὐδείς = ο υ U+0313 δ ε ι U+0301 ς).
fn original_word(text: &str) -> (String, &str) {
    let t = text.trim_start();
    let mut end = 0;
    for (i, c) in t.char_indices() {
        if is_script(c) || (end > 0 && ('\u{0300}'..='\u{036F}').contains(&c)) {
            end = i + c.len_utf8();
        } else if !(c == ' ' && end > 0) {
            break;
        }
    }
    if end == 0 {
        return (String::new(), t);
    }
    (t[..end].to_string(), &t[end..])
}

/// The transliteration right after the original word, up to «;» or the line's end.
fn translit(rest: &str) -> String {
    let r = rest.trim_start();
    let end = r.find(|c: char| c == ';' || c == '\n').unwrap_or(r.len());
    let t = r[..end].trim().trim_end_matches(|c: char| matches!(c, ',' | '.' | ':')).trim();
    let reads = !t.is_empty()
        && t.chars().count() <= 40
        && t.split_whitespace().count() <= 4
        && t.chars().any(char::is_alphabetic)
        && !t.chars().any(|c| c.is_ascii_digit() || is_script(c));
    if reads {
        t.to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> (String, String, String) {
        let e = parse_entry(text).expect("parses");
        (e.spanish, e.translit, e.original)
    }

    #[test]
    fn codes_need_their_testament_letter() {
        assert_eq!(strong_code("G26").as_deref(), Some("G26"));
        assert_eq!(strong_code(" g0026 ").as_deref(), Some("G26"));
        assert_eq!(strong_code("H 1254").as_deref(), Some("H1254"));
        assert_eq!(strong_code("26"), None);
        assert_eq!(strong_code("G"), None);
        assert_eq!(strong_code("G26a"), None);
        assert_eq!(strong_code("amor"), None);
    }

    /// Each layout, as its text arrives (strings copied from the installed modules).
    #[test]
    fn the_three_layouts_give_the_same_words() {
        let strong = "ἀγάπη\nagápe\nde G25; amor, i.e. afecto o benevolencia; específicamente (plural) festín de amor: ágape, amado, amor.";
        let stronges = "ἀγάπη\nagápe; de G25 ; amor, i.e. afecto o benevolencia; espec. (plur.) festín de amor: — \u{e000}ágape, amado, amor.\u{e001}";
        let spa = "agápe\nde G25 amor, i.e. afecto o benevolencia; específicamente (plural) festín de amor: ágape, amado, amor.";
        let want = ("ágape, amado, amor".to_string(), "agápe".to_string());
        for t in [strong, stronges, spa] {
            let (es, tr, _) = parsed(t);
            assert_eq!((es, tr), want, "{t}");
        }
        assert_eq!(parsed(strong).2, "ἀγάπη");
        assert_eq!(parsed(spa).2, "");
    }

    #[test]
    fn cross_references_and_notes_after_the_list_are_left_out() {
        let g4151 = "πνεῦμα\npneúma; de G4154 ; corriente de aire, …, el Espíritu Santo: — aliento, espíritu, espiritual, viento. Comp. G5590 .";
        assert_eq!(parsed(g4151).0, "aliento, espíritu, espiritual, viento");
        let g1810 = "ἐξαίφνης\nexaífnes; de G1537 y la base de G160 ; de repente: — repente, repentinamente. comp. G1819 .";
        assert_eq!(parsed(g1810).0, "repente, repentinamente");
        let g3588 = "ὁ\njo\nincluye el femenino ἡ jé y el neutro τό tó en todas sus inflexiones; artículo def.; el, la, lo (a veces suplido, otras veces no, en el español): cosa, el, este, ese, quien, uno.\nὅ jó. Véase G3729.";
        assert_eq!(parsed(g3588), ("cosa, el, este, ese, quien, uno".into(), "jo".into(), "ὁ".into()));
        let h3068 = "— י\u{5b0}הו\u{5b8}\u{5b9}ה Yejová; de H1961 ; (el) auto Existente o Eterno; Jehová, nombre nacional judío de Dios: — Jehová, el Señor. Comp. H3050 , H3069 .";
        let (es, tr, orig) = parsed(h3068);
        assert_eq!((es.as_str(), tr.as_str()), ("Jehová, el Señor", "Yejová"));
        assert!(orig.starts_with('י'), "{orig}");
        let h1 = "ab\npalabra primaria; padre en aplicaci\u{3000}n literalmente e inmedia, o figurativamente, y remota:- abuelo, antepasado, descendencia, padre, paterno, x patrimonio, principal. Compare nombres en «Abi».";
        assert_eq!(parsed(h1).0, "abuelo, antepasado, descendencia, padre, paterno, patrimonio, principal");
        let h1758 = "ד\u{5bc}ו\u{5bc}ש\u{5c1}\ndush\nraíz primaria; pisotear o pisar (uvas): hollar, quebrar, trillar, hierba [Jer 50:11, por error por H1877].";
        assert_eq!(parsed(h1758).0, "hollar, quebrar, trillar, hierba");
        let h3240 = "י\u{5b8}נ\u{5b7}ח\nyanákj\nraíz primaria; depositar; por implicación permitir quedarse: dejar, permitir, poner, quedar, reposar. (aquí se hace referencia a las formas jifil, habría que referirse más bien a H5117, y las otras aquí.)";
        assert_eq!(parsed(h3240).0, "dejar, permitir, poner, quedar, reposar");
    }

    #[test]
    fn verse_references_and_abbreviations_do_not_end_the_list() {
        let h3605 = "כ\u{5bc}יל\nkol\no (Jer 33:8) כ\u{5bc}ויל kol; de H3634; propiamente el todo; de aquí, todo, cualquiera o cada uno (en el singular solo, pero a menudo en sentido plural): alguna, -o, cada, nada, ninguna, toda, -as, -o, -os, y.";
        assert_eq!(parsed(h3605).0, "alguna, -o, cada, nada, ninguna, toda, -as, -o, -os, y");
        let g2532 = "καί\nkaí; apar. partícula prim., teniendo fuerza copulativa; y, también, etc.; a menudo usado en conexión con otras partículas o palabras pequeñas: — así, cuando, de la misma manera, en verdad, pero, porque, si (cond.).";
        assert_eq!(parsed(g2532).0, "así, cuando, de la misma manera, en verdad, pero, porque, si (cond.)");
        let h853 = "— א\u{5b5}ת et; apar. contr. de H226 en el sentido demost. de entidad; prop. propio (pero gen. usado para señalar más def. el obj. de un verbo o prep., igualmente o es decir): — [como tal no aparece en español].";
        assert_eq!(parsed(h853).0, "[como tal no aparece en español]");
        assert_eq!(parsed(h853).1, "et");
    }

    #[test]
    fn separate_accent_marks_stay_in_the_word() {
        let g3762 = "\u{3bf}\u{3c5}\u{313}\u{3b4}\u{3b5}\u{3b9}\u{301}\u{3c2}\noudeís; incl. fem. ...: nadie, ninguno.";
        let (_, _, orig) = parsed(g3762);
        assert_eq!(orig, "\u{3bf}\u{3c5}\u{313}\u{3b4}\u{3b5}\u{3b9}\u{301}\u{3c2}");
    }

    #[test]
    fn two_word_originals_and_transliterations() {
        let h1035 = "ב\u{5bc}\u{5b5}ית ל\u{5b6}ח\u{5b6}ם\nBeit Lekjem\nde H1004 y H3899; casa de pan; Bet Lekjem, un lugar en Palestina: Belén.";
        let (es, tr, orig) = parsed(h1035);
        assert_eq!((es.as_str(), tr.as_str()), ("Belén", "Beit Lekjem"));
        assert_eq!(orig.split(' ').count(), 2, "{orig}");
    }

    #[test]
    fn entries_without_a_list_give_nothing() {
        assert_eq!(parse_entry("ἀγάπη\nagápe\nde G25; amor, i.e. afecto"), None);
        // A verse reference is not a list.
        assert_eq!(parse_entry("λόγος\nlógos\nver Jn 1:1"), None);
        // What follows the last colon must read as Spanish words, not a code or Greek.
        assert_eq!(parse_entry("x\ny\ncompare: G26."), None);
        assert_eq!(parse_entry("[Módulo cifrado: no se puede leer]"), None);
    }

    #[test]
    fn long_lists_end_at_an_item() {
        let items: Vec<String> = (0..80).map(|i| format!("palabra{}", "x".repeat(i % 7))).collect();
        let text = format!("λόγος\nlógos\nde algo: {}.", items.join(", "));
        let es = parsed(&text).0;
        assert!(es.ends_with(", …") && es.chars().count() <= MAX_LIST_CHARS + 3, "{es}");
    }
}
