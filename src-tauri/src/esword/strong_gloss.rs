//! The Spanish words a Spanish Strong's dictionary gives for a Strong's number: the list of
//! Reina-Valera renderings after the entry's last «:» («…festín de amor: — ágape, amado,
//! amor.»), with the transliteration and the original word.
//!
//! The Léxico shows them as the «ES» line for a number looked up by hand, clicked in a
//! definition or opened from the Índice — where no Bible word supplies one. For particles the
//! list is how the RV renders the word in places, not its plain meaning (G1722 «en» lists
//! «exhortación, interior, bajo…»), so the page labels it as Strong's renderings.
//!
//! The three layouts installed (`clean_text` output):
//! - Strong ° / Strong (Esp): `ἀγάπη\nagápe\nde G25; amor, …: ágape, amado, amor.`
//! - STRONGes / Strong-ES: `ἀγάπη\nagápe; de G25 ; …: — ágape, amado, amor.` — the list is
//!   red in the module, so it arrives inside the words-of-Christ markers U+E000…U+E001, and a
//!   Hebrew entry opens with «— ».
//! - Strong SPA: no original script at all: `agápe\nde G25 amor, …: ágape, amado, amor.`

use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::types::Value;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use super::reader::{column_set_cached, module_labels_cached, table_exists_cached, with_conn};
use super::text::cell_to_text;

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
const CACHE_MAX: usize = 4096;

static CACHE: Lazy<Mutex<HashMap<(String, String), Option<StrongGloss>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static RE_CODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b[GH]\s?\d{1,5}\b").unwrap());
static RE_CODE_NOTE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\s*\[[^\]]*\b[GH]\s?\d{1,5}\b[^\]]*\]").unwrap());
// Cross-reference words, as they open the note after a list («Compare G5590», «Véase G3729»).
// Not «ver»: it is a rendering itself (εἴδω «ver, mirar…»).
static RE_XREF: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(?:[Vv]éase|[Cc]ompárese|[Cc]omp\.|Compare\b)").unwrap());

/// The Spanish renderings of `code` from the first of `module_paths` that has a readable
/// entry for it. The caller passes only Spanish Strong's dictionaries, its preferred one
/// first; a module that is missing, unreadable or lacks the entry is skipped.
pub fn strong_gloss(code: &str, module_paths: &[String]) -> Option<StrongGloss> {
    let code = strong_code(code)?;
    module_paths
        .iter()
        .find_map(|path| gloss_in(Path::new(path), &code))
}

/// «G26», «g0026», «G 26» → «G26». A bare number is not a code here: which testament it
/// belongs to is exactly what is unknown.
fn strong_code(code: &str) -> Option<String> {
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

fn gloss_in(path: &Path, code: &str) -> Option<StrongGloss> {
    let key = (path.to_string_lossy().into_owned(), code.to_string());
    if let Some(hit) = CACHE.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }
    let found = with_conn(path, |conn, path_key| {
        let table = if table_exists_cached(conn, path_key, "Lexicon") {
            "Lexicon"
        } else if table_exists_cached(conn, path_key, "Dictionary") {
            "Dictionary"
        } else {
            return Ok(None);
        };
        let cols = column_set_cached(conn, path_key, table);
        let has = |name: &str| cols.iter().any(|c| c.eq_ignore_ascii_case(name));
        if !has("Topic") || !has("Definition") {
            return Ok(None);
        }
        // Exact topics only, as the modules spell them (G26, G0026, G00026): a prefix match
        // would hand G260 the words of G26's neighbour.
        let (letter, digits) = code.split_at(1);
        let n: u32 = digits.parse().unwrap_or(0);
        let keys = [code.to_string(), format!("{letter}{n:04}"), format!("{letter}{n:05}")];
        let sql = format!("SELECT Definition FROM [{table}] WHERE Topic IN (?1, ?2, ?3) LIMIT 1");
        let def: Option<Value> = match conn.prepare_cached(&sql) {
            Ok(mut stmt) => stmt
                .query_row(rusqlite::params![keys[0], keys[1], keys[2]], |r| r.get(0))
                .ok(),
            Err(_) => None,
        };
        let Some(text) = def.as_ref().and_then(cell_to_text) else {
            return Ok(None);
        };
        let Some(entry) = parse_entry(&text) else {
            return Ok(None);
        };
        let (title, abbr) = module_labels_cached(conn, path);
        Ok(Some(StrongGloss {
            code: code.to_string(),
            spanish: entry.spanish,
            translit: entry.translit,
            original: entry.original,
            source: if abbr.trim().is_empty() { title } else { abbr },
            module: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }))
    })
    .ok()
    .flatten();
    if let Ok(mut cache) = CACHE.lock() {
        if cache.len() >= CACHE_MAX {
            cache.clear();
        }
        cache.insert(key, found.clone());
    }
    found
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    spanish: String,
    translit: String,
    original: String,
}

/// Private-use characters (the red-text markers among them) carry no text; U+3000 stands in
/// for a lost accented letter in Strong SPA; a Hebrew entry of STRONGes opens with «— ».
fn clean_head(raw: &str) -> String {
    let text: String = raw
        .chars()
        .filter(|c| !('\u{E000}'..='\u{F8FF}').contains(c))
        .map(|c| if c == '\u{3000}' { ' ' } else { c })
        .collect();
    text.trim_start_matches(|c: char| c.is_whitespace() || is_dash(c))
        .to_string()
}

fn parse_entry(raw: &str) -> Option<Entry> {
    if raw.starts_with("[Módulo cifrado") || raw.starts_with("[Contenido binario") {
        return None;
    }
    let text = clean_head(raw);
    let spanish = renderings(&text)?;
    let (original, rest) = original_word(&text);
    Some(Entry {
        spanish,
        translit: translit(rest),
        original,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrongLemma {
    pub code: String,
    /// The original word as the dictionary writes it («ἀγάπη», «בֵּית לֶחֶם»).
    pub original: String,
}

static LEMMAS: Lazy<Mutex<HashMap<String, std::sync::Arc<Vec<StrongLemma>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// The original word of every entry in `module_paths` (Spanish Strong's dictionaries), for the
/// page's word → number table: a Greek or Hebrew word in a text, hovered or searched, goes
/// straight to its number instead of a scan of a whole module's definitions (0.7 s in
/// Multiléxico, and no match there: it spells Greek as entities). Each module is read once —
/// only the start of each entry — and kept; a pair repeated across modules is sent once.
/// Strong ° writes every holam as a yod in its Hebrew (1 624 words, in the file itself), so
/// its spellings go in beside STRONGes': a word copied from a text built on either matches.
pub fn strong_lemmas(module_paths: &[String]) -> Vec<StrongLemma> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in module_paths {
        for l in lemmas_in(Path::new(p)).iter() {
            if seen.insert(l.clone()) {
                out.push(l.clone());
            }
        }
    }
    out
}

fn lemmas_in(path: &Path) -> std::sync::Arc<Vec<StrongLemma>> {
    let key = path.to_string_lossy().into_owned();
    if let Some(hit) = LEMMAS.lock().ok().and_then(|m| m.get(&key).cloned()) {
        return hit;
    }
    let list = with_conn(path, |conn, path_key| {
        let table = if table_exists_cached(conn, path_key, "Lexicon") {
            "Lexicon"
        } else if table_exists_cached(conn, path_key, "Dictionary") {
            "Dictionary"
        } else {
            return Ok(Vec::new());
        };
        let cols = column_set_cached(conn, path_key, table);
        let has = |name: &str| cols.iter().any(|c| c.eq_ignore_ascii_case(name));
        if !has("Topic") || !has("Definition") {
            return Ok(Vec::new());
        }
        // The word heads the entry: its first few hundred characters are enough.
        let sql = format!(
            "SELECT Topic, CASE WHEN typeof(Definition) = 'text' THEN substr(Definition, 1, 400) END \
             FROM [{table}]"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Value>(1)?)))
            .map_err(|e| e.to_string())?;
        let mut v = Vec::new();
        for (topic, def) in rows.flatten() {
            let Some(code) = strong_code(&topic) else { continue };
            let Some(text) = cell_to_text(&def) else { continue };
            let (original, _) = original_word(&clean_head(&text));
            if !original.is_empty() {
                v.push(StrongLemma { code, original });
            }
        }
        Ok(v)
    })
    .unwrap_or_default();
    let list = std::sync::Arc::new(list);
    if let Ok(mut m) = LEMMAS.lock() {
        m.insert(key, list.clone());
    }
    list
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
        .filter(|&(i, c)| {
            c == ':'
                && !text[i + 1..]
                    .trim_start()
                    .starts_with(|d: char| d.is_ascii_digit())
        })
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
/// breathings and accents (οὐδείς = ο υ U+0313 δ ε ι U+0301 ς), which would otherwise end the
/// word at «ου».
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
    let t = r[..end]
        .trim()
        .trim_end_matches(|c: char| matches!(c, ',' | '.' | ':'))
        .trim();
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
    use std::time::Instant;

    const ESWORD: &str = r"C:\Program Files (x86)\e-Sword";

    fn installed(name: &str) -> Option<String> {
        let p = Path::new(ESWORD).join(name);
        p.exists().then(|| p.to_string_lossy().into_owned())
    }

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

    /// Each layout, as `clean_text` hands it over (strings copied from the installed modules).
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
        let g4151 = "πνεῦμα\npneúma; de G4154 ; corriente de aire, …, el Espíritu Santo: — \u{e000}aliento, espíritu, espiritual, viento.\u{e001} Comp. G5590 .";
        assert_eq!(parsed(g4151).0, "aliento, espíritu, espiritual, viento");
        let g1810 = "ἐξαίφνης\nexaífnes; de G1537 y la base de G160 ; de repente: — \u{e000}repente, repentinamente.\u{e001} comp. G1819 .";
        assert_eq!(parsed(g1810).0, "repente, repentinamente");
        let g3588 = "ὁ\njo\nincluye el femenino ἡ jé y el neutro τό tó en todas sus inflexiones; artículo def.; el, la, lo (a veces suplido, otras veces no, en el español): cosa, el, este, ese, quien, uno.\nὅ jó. Véase G3729.";
        assert_eq!(parsed(g3588), ("cosa, el, este, ese, quien, uno".into(), "jo".into(), "ὁ".into()));
        let h3068 = "— י\u{5b0}הו\u{5b8}\u{5b9}ה Yejová; de H1961 ; (el) auto Existente o Eterno; Jehová, nombre nacional judío de Dios: \u{e000}— Jehová, el Señor.\u{e001} Comp. H3050 , H3069 .";
        let (es, tr, orig) = parsed(h3068);
        assert_eq!((es.as_str(), tr.as_str()), ("Jehová, el Señor", "Yejová"));
        assert!(orig.starts_with('י'), "{orig}");
        let h1 = "ab\npalabra primaria; padre en aplicaci\u{3000}n literalmente e inmedia, o figurativamente, y remota:- abuelo, antepasado, descendencia, padre, paterno, x patrimonio, principal. Compare nombres en «Abi».";
        assert_eq!(parsed(h1).0, "abuelo, antepasado, descendencia, padre, paterno, patrimonio, principal");
        // Notes: one naming another number is dropped, one after the list's period ends it.
        let h1758 = "ד\u{5bc}ו\u{5bc}ש\u{5c1}\ndush\nraíz primaria; pisotear o pisar (uvas): hollar, quebrar, trillar, hierba [Jer 50:11, por error por H1877].";
        assert_eq!(parsed(h1758).0, "hollar, quebrar, trillar, hierba");
        let h3240 = "י\u{5b8}נ\u{5b7}ח\nyanákj\nraíz primaria; depositar; por implicación permitir quedarse: dejar, permitir, poner, quedar, reposar. (aquí se hace referencia a las formas jifil, habría que referirse más bien a H5117, y las otras aquí.)";
        assert_eq!(parsed(h3240).0, "dejar, permitir, poner, quedar, reposar");
        let h165 = "א\u{5b1}הי\nejí\naparentemente variación ortográfica de H346; donde: seré (Ose 13:10, Ose 13:14) [que a menudo se traduce de la misma forma hebrea de H1961].";
        assert_eq!(parsed(h165).0, "seré (Ose 13:10, Ose 13:14)");
    }

    #[test]
    fn verse_references_and_abbreviations_do_not_end_the_list() {
        let h3605 = "כ\u{5bc}יל\nkol\no (Jer 33:8) כ\u{5bc}ויל kol; de H3634; propiamente el todo; de aquí, todo, cualquiera o cada uno (en el singular solo, pero a menudo en sentido plural): alguna, -o, cada, nada, ninguna, toda, -as, -o, -os, y.";
        assert_eq!(parsed(h3605).0, "alguna, -o, cada, nada, ninguna, toda, -as, -o, -os, y");
        let g2532 = "καί\nkaí; apar. partícula prim., teniendo fuerza copulativa; y, también, etc.; a menudo usado en conexión con otras partículas o palabras pequeñas: — \u{e000}así, cuando, de la misma manera, en verdad, pero, porque, si (cond.\u{e001}).";
        assert_eq!(parsed(g2532).0, "así, cuando, de la misma manera, en verdad, pero, porque, si (cond.)");
        let h853 = "— א\u{5b5}ת et; apar. contr. de H226 en el sentido demost. de entidad; prop. propio (pero gen. usado para señalar más def. el obj. de un verbo o prep., igualmente o es decir): \u{e000}— [\u{e001}\u{e000}como tal no aparece en español\u{e001}\u{e000}].\u{e001}";
        assert_eq!(parsed(h853).0, "[como tal no aparece en español]");
        assert_eq!(parsed(h853).1, "et");
    }

    #[test]
    fn separate_accent_marks_stay_in_the_word() {
        // STRONGes G3762: οὐδείς with its breathing and accent as combining marks.
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

    #[test]
    fn real_spanish_strong_dictionaries() {
        let Some(strong) = installed("01 Diccionario strong.lexi") else {
            return;
        };
        let mut paths = vec![strong];
        paths.extend(installed("03STRONGes.lexi"));
        let g = |code: &str| strong_gloss(code, &paths).expect(code);
        let g26 = g("G26");
        assert_eq!((g26.spanish.as_str(), g26.translit.as_str(), g26.original.as_str()), ("ágape, amado, amor", "agápe", "ἀγάπη"));
        assert_eq!(g26.source, "Strong °");
        assert_eq!(g("g0026"), g26, "spelling of the code does not matter");
        assert_eq!(g("G4151").spanish, "aliento, espíritu, espiritual, viento");
        assert_eq!(g("G4154").spanish, "soplar, viento");
        assert!(g("H1254").spanish.starts_with("atravesar, creación, creador, crear"));
        assert_eq!(g("H853").spanish, "[como tal no aparece en español]");
        assert_eq!(strong_gloss("G99999", &paths), None);
        assert_eq!(strong_gloss("26", &paths), None);
    }

    /// The word → number table: lemma forms of both testaments, Strong °'s yod-for-holam
    /// spelling beside STRONGes' correct one, multi-word names whole, and quick once read.
    #[test]
    fn lemma_table_from_the_spanish_strong_dictionaries() {
        let (Some(strong), Some(stronges)) = (installed("01 Diccionario strong.lexi"), installed("03STRONGes.lexi")) else {
            return;
        };
        let t0 = Instant::now();
        let table = strong_lemmas(&[stronges.clone(), strong.clone()]);
        let first = t0.elapsed();
        // By number: the modules order Hebrew marks differently and may use either Greek
        // accent form, so typed literals would test the keyboard, not the table.
        let originals = |code: &str| -> Vec<String> {
            table.iter().filter(|l| l.code == code).map(|l| l.original.clone()).collect()
        };
        let letters = |s: &str| s.chars().filter(|c| c.is_alphabetic()).count();
        let g5569 = originals("G5569");
        assert!(!g5569.is_empty() && g5569.iter().all(|o| o.starts_with('ψ') && letters(o) == 11), "{g5569:?}");
        assert!(originals("G4151").iter().all(|o| o.starts_with('π')), "{:?}", originals("G4151"));
        assert!(!originals("H1254").is_empty());
        // H3068 in STRONGes' holam (U+05B9) and in Strong °'s yod-for-holam: both spellings
        // are in the table, so a word copied from either kind of text finds the number.
        let h3068 = originals("H3068");
        assert!(h3068.iter().any(|o| o.contains('\u{5b9}')), "{h3068:?}");
        assert!(h3068.iter().any(|o| o.chars().filter(|&c| c == '\u{5d9}').count() == 2), "{h3068:?}");
        assert!(originals("H1035").iter().any(|o| o.contains(' ')), "multi-word names stay whole");
        // One dictionary's ~14 000 numbers, plus the other's differing spellings (merged when
        // identical: most Greek is).
        let numbers: std::collections::HashSet<&str> = table.iter().map(|l| l.code.as_str()).collect();
        assert!(numbers.len() > 14_000 && table.len() > numbers.len() + 1_500, "{} pairs, {} numbers", table.len(), numbers.len());
        let t1 = Instant::now();
        assert_eq!(strong_lemmas(&[stronges, strong]).len(), table.len());
        assert!(t1.elapsed() < first / 4 + std::time::Duration::from_millis(30), "kept after the first read");
    }

    /// A module that is missing, unreadable or without the entry is skipped, not an error.
    #[test]
    fn unusable_modules_are_skipped() {
        let mut paths = vec![r"C:\no\such\module.lexi".to_string()];
        paths.extend(installed("strong-PLUS.lexi")); // no Definition column
        paths.extend(installed("Strongspa.lexi"));
        let Some(spa) = installed("Strongspa.lexi") else {
            return;
        };
        let got = strong_gloss("G26", &paths).expect("falls through to Strong SPA");
        assert_eq!(got.spanish, "ágape, amado, amor");
        assert_eq!(got.module, Path::new(&spa).file_name().unwrap().to_string_lossy());
    }

    /// Every entry of each layout: how many give a list, what a suspicious one looks like, and
    /// whether the layouts agree code by code. Report only:
    /// `cargo test --lib -- --ignored gloss_sweep --nocapture`.
    #[test]
    #[ignore]
    fn gloss_sweep() {
        let layouts = ["01 Diccionario strong.lexi", "03STRONGes.lexi", "Strongspa.lexi"];
        let mut per: Vec<HashMap<String, String>> = Vec::new();
        for m in layouts {
            let Some(p) = installed(m) else { continue };
            let t0 = Instant::now();
            // Every row read once and parsed directly: a lookup per code would rescan the table.
            let rows: Vec<(String, Value)> = with_conn(Path::new(&p), |conn, _| {
                let mut st = conn
                    .prepare("SELECT Topic, Definition FROM Lexicon")
                    .map_err(|e| e.to_string())?;
                let v = st
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Value>(1)?)))
                    .map_err(|e| e.to_string())?
                    .flatten()
                    .collect();
                Ok(v)
            })
            .unwrap();
            let mut got = HashMap::new();
            let mut none = Vec::new();
            let mut no_translit = 0;
            for (topic, def) in &rows {
                let Some(code) = strong_code(topic) else { continue };
                match cell_to_text(def).and_then(|t| parse_entry(&t)) {
                    Some(e) => {
                        if e.translit.is_empty() {
                            no_translit += 1;
                        }
                        got.insert(code, e.spanish);
                    }
                    None => none.push(code),
                }
            }
            eprintln!(
                "{m}: {} rows, {} lists, {} without {:?}, {} without translit, in {:?}",
                rows.len(),
                got.len(),
                none.len(),
                none.iter().take(14).collect::<Vec<_>>(),
                no_translit,
                t0.elapsed()
            );
            per.push(got);
        }
        let norm = |s: &str| {
            s.to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == ',')
                .collect::<String>()
        };
        for (i, j) in [(0, 1), (0, 2)] {
            if per.len() <= j {
                continue;
            }
            let (mut same, mut diff) = (0, Vec::new());
            for (code, a) in &per[i] {
                if let Some(b) = per[j].get(code) {
                    if norm(a) == norm(b) {
                        same += 1;
                    } else {
                        diff.push((code.clone(), a.clone(), b.clone()));
                    }
                }
            }
            diff.sort();
            eprintln!("{} vs {}: {same} agree, {} differ", layouts[i], layouts[j], diff.len());
            for d in diff.iter().take(30) {
                eprintln!("    {}: «{}» | «{}»", d.0, d.1, d.2);
            }
        }
    }
}
