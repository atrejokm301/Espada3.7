use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use once_cell::sync::Lazy;
use regex::Regex;
use std::io::Read;

static RE_HTML: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<[^>]+>").unwrap());
/// RTF control words: \b \i \cf9 \fs32 \rtlch \ltrch \f2 etc. (not \'hh)
static RE_RTF: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\[a-zA-Z]+\d* ?").unwrap());
/// RTF paragraph / line breaks. `\b` keeps `\pard` (paragraph *defaults*) out.
static RE_RTF_BREAK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\(?:par|line)\b ?").unwrap());
static RE_RTF_TAB: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\tab\b ?").unwrap());
/// Unicode escapes, two incompatible dialects sharing one spelling:
/// - RTF `\uN` / `\uN?` is **decimal**, and may be a negative i16 for cp > 0x7FFF
/// - JS/JSON `\uXXXX` is **hex**
/// Disambiguated in the replacer: a `?` suffix or `-` prefix means RTF.
static RE_UNI_ESC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\\u(-?[0-9a-fA-F]{1,6})(\?)?").unwrap());
/// RTF hex bytes: \'e1 \'ab \'e7 (e-Sword Vine AT, etc.)
static RE_RTF_HEX_RUN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:\\'[0-9a-fA-F]{2})+").unwrap());
/// e-Sword style Gén_25:16 → normalize before linkify
static RE_ESWORD_REF: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b((?:[123]\s*)?[A-Za-zÁÉÍÓÚÜÑáéíóúüñ.]{2,})\s*_(\d{1,3})\s*:\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?",
    )
    .unwrap()
});
/// Words of Christ / red letter markup in e-Sword HTML
static RE_WOC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?is)<(?:span|font)[^>]*(?:color\s*[:=]\s*["']?(?:red|#f{2}0{2}0{2}|#c00|#c00000|#cc0000|#ff0000|#e53935|#b71c1c)|style\s*=\s*["'][^"']*color\s*:\s*(?:red|#f{2}0{2}0{2}|#c00|#ff0000)[^"']*["'])[^>]*>(.*?)</(?:span|font)>"#,
    )
    .unwrap()
});
/// Keep Strong’s visible after stripping <num> tags
static RE_NUM_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<num>\s*([HG]\s*\d{1,5})\s*</num>").unwrap());
static RE_HEB_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<heb>(.*?)</heb>").unwrap());

/// Markers preserved through cleaning for UI red-letter rendering.
pub const WOC_OPEN: &str = "\u{E000}";
pub const WOC_CLOSE: &str = "\u{E001}";

/// Section titles for the page (see `sections.rs`): `TITLE_OPEN Diccionario Strong TITLE_CLOSE`,
/// or for an author line `TITLE_OPEN San Agustín, TITLE_SPLIT de sermone Domini, 1,1 TITLE_CLOSE`.
pub const TITLE_OPEN: &str = "\u{E002}";
pub const TITLE_CLOSE: &str = "\u{E003}";
pub const TITLE_SPLIT: &str = "\u{E004}";

/// Bold and italic runs kept through cleaning while section titles are looked for; they never
/// reach the page. Each bold-open is followed by one character U+F0000 + n: the markup that
/// opened the run is `sigs[n]` (see `clean_text_emphasis`).
pub(crate) const EM_BOLD_OPEN: char = '\u{E010}';
pub(crate) const EM_BOLD_CLOSE: char = '\u{E011}';
pub(crate) const EM_ITALIC_OPEN: char = '\u{E012}';
pub(crate) const EM_ITALIC_CLOSE: char = '\u{E013}';
pub(crate) const EM_SIG_BASE: u32 = 0xF0000;
const EM_SIG_LAST: u32 = 0xFFFFD;

/// Bold runs: a heading tag, <b>, <strong>, or a span set in bold. Inner groups 1–4.
static RE_EM_BOLD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?is)<h[1-6]\b[^>]*>(.*?)</h[1-6]\s*>|<b\b[^>]*>(.*?)</b\s*>|<strong\b[^>]*>(.*?)</strong\s*>|<span\b[^>]*font-weight\s*:\s*(?:bold|[6-9]00)[^>]*>(.*?)</span\s*>",
    )
    .unwrap()
});
/// Italic runs: <i>, <em>, or a span set in italics. Inner groups 1–3.
static RE_EM_ITALIC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?is)<i\b[^>]*>(.*?)</i\s*>|<em\b[^>]*>(.*?)</em\s*>|<span\b[^>]*font-style\s*:\s*italic[^>]*>(.*?)</span\s*>",
    )
    .unwrap()
});
static RE_EM_RULE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<hr\b[^>]*>").unwrap());

/// e-Sword classic RTF Hebrew font bytes (consonants 0xE0+ and niqqud 0xC0+).
fn esword_hebrew_byte(b: u8) -> Option<char> {
    match b {
        0xE0..=0xFA => {
            // א..ת (and a few beyond mapped if present)
            char::from_u32(0x05D0 + u32::from(b - 0xE0))
        }
        // Niqqud / points (common e-Sword mapping)
        0xC0 => Some('\u{05B0}'), // sheva
        0xC1 => Some('\u{05B1}'), // hataf segol
        0xC2 => Some('\u{05B2}'), // hataf patah
        0xC3 => Some('\u{05B3}'), // hataf qamats
        0xC4 => Some('\u{05B4}'), // hiriq
        0xC5 => Some('\u{05B5}'), // tsere
        0xC6 => Some('\u{05B6}'), // segol
        0xC7 => Some('\u{05B7}'), // patah
        0xC8 => Some('\u{05B8}'), // qamats
        0xC9 => Some('\u{05B9}'), // holam
        0xCA => Some('\u{05BA}'), // holam haser
        0xCB => Some('\u{05BB}'), // qubuts
        0xCC => Some('\u{05BC}'), // dagesh
        0xCD => Some('\u{05BD}'), // meteg
        0xCE => Some('\u{05BE}'), // maqaf (approx)
        0xCF => Some('\u{05BF}'), // rafe
        0xD0 => Some('\u{05C0}'),
        0xD1 => Some('\u{05C1}'), // shin dot
        0xD2 => Some('\u{05C2}'), // sin dot
        0xD3 => Some('\u{05C3}'),
        0xD4 => Some('\u{05C4}'),
        0xD5 => Some('\u{05C5}'),
        0xD6 => Some('\u{05C6}'),
        0xD7 => Some('\u{05C7}'), // qamats qatan
        _ => None,
    }
}

fn cp1252_byte(b: u8) -> char {
    // encoding_rs not required — map Latin-1 + common CP1252 extras used in Spanish RTF
    match b {
        0x00..=0x7F => b as char,
        0x80 => '€',
        0x82 => '‚',
        0x83 => 'ƒ',
        0x84 => '„',
        0x85 => '…',
        0x86 => '†',
        0x87 => '‡',
        0x88 => 'ˆ',
        0x89 => '‰',
        0x8A => 'Š',
        0x8B => '‹',
        0x8C => 'Œ',
        0x8E => 'Ž',
        0x91 => '‘',
        0x92 => '’',
        0x93 => '“',
        0x94 => '”',
        0x95 => '•',
        0x96 => '–',
        0x97 => '—',
        0x98 => '˜',
        0x99 => '™',
        0x9A => 'š',
        0x9B => '›',
        0x9C => 'œ',
        0x9E => 'ž',
        0x9F => 'Ÿ',
        0xA0..=0xFF => char::from_u32(u32::from(b)).unwrap_or('�'), // same as Latin-1 for A0-FF
        _ => '�',
    }
}

fn decode_rtf_hex_run(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    // Single byte → always Windows-1252 (Spanish: á é « »)
    if bytes.len() == 1 {
        return cp1252_byte(bytes[0]).to_string();
    }
    let heb_like = bytes
        .iter()
        .filter(|&&b| (0xE0..=0xFA).contains(&b) || (0xC0..=0xD7).contains(&b))
        .count();
    // Runs of Hebrew font bytes (e.g. \'e7\'c8\'f6\'c5\'f8 → חָצֵר)
    if heb_like * 2 >= bytes.len() {
        let mut out = String::new();
        for &b in bytes {
            if let Some(c) = esword_hebrew_byte(b) {
                out.push(c);
            } else {
                out.push(cp1252_byte(b));
            }
        }
        return out;
    }
    bytes.iter().map(|&b| cp1252_byte(b)).collect()
}

/// Decode e-Sword RTF `\'hh` sequences (Vine AT+, older .dctx modules).
fn decode_rtf_hex_escapes(text: &str) -> String {
    RE_RTF_HEX_RUN
        .replace_all(text, |caps: &regex::Captures| {
            let run = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            let mut bytes = Vec::new();
            let mut i = 0;
            let bts = run.as_bytes();
            while i + 3 < bts.len() {
                // \'XX
                if bts[i] == b'\\' && bts[i + 1] == b'\'' {
                    let h = &run[i + 2..i + 4];
                    if let Ok(v) = u8::from_str_radix(h, 16) {
                        bytes.push(v);
                    }
                    i += 4;
                } else {
                    i += 1;
                }
            }
            decode_rtf_hex_run(&bytes)
        })
        .into_owned()
}

/// Which cleaning stages an input actually needs. Most Bible verses are plain
/// text; scanning once up front lets us skip ~30 full-string rewrites per row.
#[derive(Default, Clone, Copy)]
struct Shape {
    tag: bool,
    entity: bool,
    backslash: bool,
    brace: bool,
    underscore: bool,
}

impl Shape {
    fn of(s: &str) -> Self {
        let mut sh = Shape::default();
        for &b in s.as_bytes() {
            match b {
                b'<' | b'>' => sh.tag = true,
                b'&' => sh.entity = true,
                b'\\' => sh.backslash = true,
                b'{' | b'}' => sh.brace = true,
                b'_' => sh.underscore = true,
                _ => {}
            }
        }
        sh
    }

    fn is_plain(&self) -> bool {
        !(self.tag || self.entity || self.backslash || self.brace)
    }
}

/// Block-level tags that carry layout, collapsed in one pass instead of
/// thirteen sequential `String::replace` rewrites.
fn replace_block_tags(s: &str) -> String {
    const NEWLINE: &[&str] = &["<br/>", "<br />", "<br>", "</p>", "</div>", "</li>", "</tr>"];
    const DROP: &[&str] = &["</span>", "</sup>", "<sup>", "</ref>", "<ref>"];

    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let rest = &s[i..];
            if let Some(t) = NEWLINE.iter().find(|t| rest.starts_with(**t)) {
                out.push('\n');
                i += t.len();
                continue;
            }
            if let Some(t) = DROP.iter().find(|t| rest.starts_with(**t)) {
                i += t.len();
                continue;
            }
            if rest.starts_with("<li>") {
                out.push_str("• ");
                i += 4;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// The named entities e-Sword modules actually emit for Spanish text. Anything
/// else is left for `html_escape` at the end of the pipeline.
fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        "nbsp" => ' ',
        "laquo" => '«',
        "raquo" => '»',
        "aacute" => 'á',
        "eacute" => 'é',
        "iacute" => 'í',
        "oacute" => 'ó',
        "uacute" => 'ú',
        "ntilde" => 'ñ',
        "Aacute" => 'Á',
        "Eacute" => 'É',
        "Iacute" => 'Í',
        "Oacute" => 'Ó',
        "Uacute" => 'Ú',
        "Ntilde" => 'Ñ',
        _ => return None,
    })
}

/// Numeric (`&#x05D7;` / `&#1488;`) and known named entities in a single pass.
/// A numeric entity whose code point is invalid is dropped, matching the
/// previous regex behaviour.
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            // Entity names/numbers are short; cap the scan so a bare '&' in
            // prose doesn't drag us across the whole string. The cap has to
            // land on a char boundary — multi-byte text sits right next to
            // entities all over these modules.
            let mut window_end = (i + 14).min(s.len());
            while window_end > i && !s.is_char_boundary(window_end) {
                window_end -= 1;
            }
            let window = &s[i..window_end];
            if let Some(semi) = window.find(';') {
                let body = &window[1..semi];
                let mut matched = None;
                if let Some(hex) = body.strip_prefix("#x") {
                    if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                        matched = Some(u32::from_str_radix(hex, 16).ok().and_then(char::from_u32));
                    }
                } else if let Some(dec) = body.strip_prefix('#') {
                    if !dec.is_empty() && dec.bytes().all(|b| b.is_ascii_digit()) {
                        matched = Some(dec.parse::<u32>().ok().and_then(char::from_u32));
                    }
                } else if let Some(c) = named_entity(body) {
                    matched = Some(Some(c));
                }
                if let Some(decoded) = matched {
                    if let Some(c) = decoded {
                        out.push(c);
                    }
                    i += semi + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn is_latin_letter(c: char) -> bool {
    c.is_ascii_alphabetic()
        || matches!(
            c,
            'á' | 'é' | 'í' | 'ó' | 'ú' | 'ü' | 'ñ' | 'Á' | 'É' | 'Í' | 'Ó' | 'Ú' | 'Ü' | 'Ñ'
                | 'à' | 'è' | 'ì' | 'ò' | 'ù' | 'â' | 'ê' | 'î' | 'ô' | 'û' | 'ç' | 'Ç'
                // Macron vowels used by the Greek/Hebrew transliterations these
                // lexicons print alongside the headword (kōmē, šālôm).
                | 'ā' | 'ē' | 'ī' | 'ō' | 'ū' | 'Ā' | 'Ē' | 'Ī' | 'Ō' | 'Ū'
                | 'š' | 'Š' | 'ḥ' | 'ṣ' | 'ṭ' | 'ĕ' | 'ă' | 'ŏ'
        )
}

fn is_greek_char(c: char) -> bool {
    matches!(c as u32, 0x0370..=0x03FF | 0x1F00..=0x1FFF)
}

fn is_hebrew_char(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x05FF)
}

/// Some modules were built by decoding Windows-1252 Spanish text as CP1253
/// (Greek) or CP1255 (Hebrew), so accented letters ship as Greek/Hebrew ones:
/// `tambiιn`, `posiciσn`, `Seסor`, `Aplicaciףn`. Each mapping below is the
/// exact byte round-trip, verified against the installed library.
fn unmojibake(c: char) -> Option<char> {
    Some(match c {
        // CP1252 byte rendered through CP1253
        'α' => 'á',
        'ι' => 'é',
        'ν' => 'í',
        'σ' => 'ó',
        'ϊ' => 'ú',
        'ρ' => 'ñ',
        'ό' => 'ü',
        'Α' => 'Á',
        'Ι' => 'É',
        'Ν' => 'Í',
        'Σ' => 'Ó',
        'Ϊ' => 'Ú',
        'Ρ' => 'Ñ',
        'Ώ' => '¿',
        '\u{0385}' => '¡',
        // Romanized Greek in these lexicons spells omega as a macron o; the
        // module shipped it as tau instead (kτmē → kōmē, chτrizτ → chōrizō,
        // IΤANNOU → IŌANNOU). Only fires inside a Latin word, so real τ is safe.
        'τ' => 'ō',
        'Τ' => 'Ō',
        // CP1252 byte rendered through CP1255
        '\u{05D1}' => 'á',
        '\u{05D9}' => 'é',
        '\u{05DD}' => 'í',
        '\u{05E3}' => 'ó',
        '\u{05EA}' => 'ú',
        '\u{05E1}' => 'ñ',
        '\u{05B1}' => 'Á',
        '\u{05B9}' => 'É',
        '\u{05BD}' => 'Í',
        '\u{05C3}' => 'Ó',
        '\u{05C1}' => 'Ñ',
        _ => return None,
    })
}

/// Repair the mis-encoded accents described on [`unmojibake`].
///
/// Only a *lone* Greek/Hebrew letter touching a Latin word is repaired. A run
/// of two or more is genuine Greek or Hebrew (`ἄλφα`, `אָב`) and is never
/// touched, so real lexicon headwords survive intact.
fn repair_codepage_mojibake(text: String) -> String {
    // Every character we repair is Greek (U+0370-03FF) or Hebrew (U+0590-05FF),
    // whose UTF-8 lead bytes all fall in 0xCD..=0xD7. A byte scan rejects the
    // overwhelming majority of rows without decoding a single char.
    if !text.as_bytes().iter().any(|&b| (0xCD..=0xD7).contains(&b)) {
        return text;
    }
    if !text.chars().any(|c| unmojibake(c).is_some()) {
        return text;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let Some(fixed) = unmojibake(c) else {
            out.push(c);
            continue;
        };
        let prev = if i > 0 { chars[i - 1] } else { ' ' };
        let next = chars.get(i + 1).copied().unwrap_or(' ');
        let same_script = |x: char| {
            if is_greek_char(c) {
                is_greek_char(x)
            } else {
                is_hebrew_char(x)
            }
        };
        let touches_latin = is_latin_letter(prev) || is_latin_letter(next);
        if touches_latin && !same_script(prev) && !same_script(next) {
            out.push(fixed);
        } else {
            out.push(c);
        }
    }
    out
}

/// Limpia HTML/RTF de texto e-Sword (incl. Vine AT RTF y HTML moderno).
/// Preserva las palabras de Jesús como marcadores WOC_OPEN…WOC_CLOSE.
pub fn clean_text(raw: &str) -> String {
    clean_impl(raw, None)
}

/// `clean_text`, with the bold and italic runs of the markup left in as EM_* markers, and a line
/// break at headings and horizontal rules (which `clean_text` glues to the next line:
/// «BDB-THAYERἈγρίππας»). Only the study panels use it, through `sections.rs` — never Bible
/// text, where breaking at the headings two Bibles carry inside verses would move word positions.
pub(crate) fn clean_text_emphasis(raw: &str, sigs: &mut Vec<String>) -> String {
    clean_impl(raw, Some(sigs))
}

/// The markup that opened a bold run: the opening tags right before it (nothing visible in
/// between) plus its own, lowercased with whitespace folded. «Diccionario Strong» in the
/// Multiléxico: `<p style="text-align:center;"><span style="color:#008080;font-weight:bold;">`.
fn emphasis_signature(before: &str, own: &str) -> String {
    let mut rest = before.trim_end();
    let mut tags: Vec<&str> = Vec::new();
    while tags.len() < 4 && rest.ends_with('>') {
        let Some(lt) = rest.rfind('<') else { break };
        let tag = &rest[lt..];
        let name = tag[1..].trim_start().to_ascii_lowercase();
        let closes_or_breaks = name.starts_with('/')
            || name.starts_with('!')
            || name.starts_with("br")
            || name.starts_with("hr");
        if closes_or_breaks {
            break;
        }
        tags.push(tag);
        rest = rest[..lt].trim_end();
    }
    let mut sig = String::new();
    for tag in tags.iter().rev().copied().chain(std::iter::once(own)) {
        let mut space = false;
        for c in tag.chars() {
            if c.is_whitespace() {
                space = true;
                continue;
            }
            if space && !sig.ends_with('<') {
                sig.push(' ');
            }
            space = false;
            sig.push(if c == '\'' { '"' } else { c.to_ascii_lowercase() });
        }
    }
    sig
}

fn mark_emphasis(text: &str, sigs: &mut Vec<String>) -> String {
    let bold = RE_EM_BOLD.replace_all(text, |caps: &regex::Captures| {
        let whole = caps.get(0).expect("match");
        let open_len = whole.as_str().find('>').map_or(0, |i| i + 1);
        let own = &whole.as_str()[..open_len];
        let sig = emphasis_signature(&text[..whole.start()], own);
        let n = match sigs.iter().position(|s| *s == sig) {
            Some(n) => n,
            None => {
                sigs.push(sig);
                sigs.len() - 1
            }
        };
        let sig_char = char::from_u32((EM_SIG_BASE + n as u32).min(EM_SIG_LAST)).unwrap_or(EM_BOLD_OPEN);
        let inner = (1..=4).find_map(|g| caps.get(g)).map_or("", |m| m.as_str());
        let heading = own.len() > 2 && own.as_bytes()[2].is_ascii_digit();
        let mut s = String::with_capacity(inner.len() + 12);
        if heading {
            s.push('\n');
        }
        s.push(EM_BOLD_OPEN);
        s.push(sig_char);
        s.push_str(inner);
        s.push(EM_BOLD_CLOSE);
        if heading {
            s.push('\n');
        }
        s
    });
    let italic = RE_EM_ITALIC.replace_all(&bold, |caps: &regex::Captures| {
        let inner = (1..=3).find_map(|g| caps.get(g)).map_or("", |m| m.as_str());
        format!("{EM_ITALIC_OPEN}{inner}{EM_ITALIC_CLOSE}")
    });
    RE_EM_RULE.replace_all(&italic, "\n").into_owned()
}

/// `collapse_whitespace` + trim: the whitespace pass of `finish`, for text rebuilt from
/// `clean_text_emphasis` lines.
pub(crate) fn tidy_whitespace(s: &str) -> String {
    collapse_whitespace(s).trim().to_string()
}

fn clean_impl(raw: &str, emphasis: Option<&mut Vec<String>>) -> String {
    let shape = Shape::of(raw);

    // Plain prose (most Bible verses) skips every markup stage.
    if shape.is_plain() {
        let mut text = raw.to_string();
        if shape.underscore {
            text = normalize_esword_refs(&text);
        }
        return finish(text);
    }

    let mut text = raw.to_string();

    let mut has_tag = shape.tag;
    let mut has_backslash = shape.backslash;

    if has_tag {
        // Mark red-letter / Words of Christ before stripping tags
        text = RE_WOC
            .replace_all(&text, |caps: &regex::Captures| {
                let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                format!("{WOC_OPEN}{inner}{WOC_CLOSE}")
            })
            .into_owned();

        if let Some(sigs) = emphasis {
            text = mark_emphasis(&text, sigs);
        }

        // Preserve Strong’s codes from <num>H2691</num> before tag strip
        text = RE_NUM_TAG
            .replace_all(&text, |caps: &regex::Captures| {
                let code = caps
                    .get(1)
                    .map(|m| m.as_str())
                    .unwrap_or("")
                    .replace(' ', "")
                    .to_uppercase();
                format!(" {code} ")
            })
            .into_owned();

        // Preserve Hebrew inside <heb>…</heb>
        text = RE_HEB_TAG
            .replace_all(&text, |caps: &regex::Captures| {
                caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string()
            })
            .into_owned();

        text = replace_block_tags(&text);
    }

    if shape.entity {
        text = decode_entities(&text);
        // An entity can decode *into* markup (`&#60;` → `<`), so re-check
        // rather than trusting the shape of the raw input.
        has_tag = has_tag || text.contains('<');
        has_backslash = has_backslash || text.contains('\\');
    }

    if has_backslash {
        text = RE_UNI_ESC
        .replace_all(&text, |caps: &regex::Captures| {
            let body = &caps[1];
            let is_rtf = caps.get(2).is_some() || body.starts_with('-');
            let cp = if is_rtf {
                // RTF \uN is decimal; negative values are the signed-16-bit
                // spelling of code points above 0x7FFF (\u-3913? => U+F0B7).
                body.parse::<i32>()
                    .ok()
                    .map(|n| if n < 0 { (n + 65536) as u32 } else { n as u32 })
                    // Not actually decimal after all — fall back to hex.
                    .or_else(|| u32::from_str_radix(body, 16).ok())
            } else {
                u32::from_str_radix(body, 16).ok()
            };
            cp.and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_default()
        })
        .into_owned();

        // Decode RTF \'hh BEFORE stripping other RTF controls (Vine AT+)
        text = decode_rtf_hex_escapes(&text);
    }

    // Strip remaining HTML tags but keep WOC private-use markers
    if has_tag {
        text = RE_HTML.replace_all(&text, "").into_owned();
    }

    if has_backslash {
        // Structural RTF controls first: \par / \line / \tab carry the layout.
        // The generic sweep below turns every control word into a space, so
        // without this an entire RTF commentary collapses into one long blob.
        text = RE_RTF_BREAK.replace_all(&text, "\n").into_owned();
        text = RE_RTF_TAB.replace_all(&text, "\t").into_owned();
        // Remaining RTF controls (not hex — those already decoded)
        text = RE_RTF.replace_all(&text, " ").into_owned();
        // leftover backslash groups
        text = text.replace("\\{", "{").replace("\\}", "}");
    }
    if text.contains('{') || text.contains('}') {
        text = text.replace('{', "").replace('}', "");
    }
    if has_backslash {
        // RTF optional hyphen / nonbreaking
        text = text.replace("\\-", "").replace("\\_", "_").replace("\\~", " ");
    }

    if shape.entity {
        text = html_escape::decode_html_entities(&text).into_owned();
    }

    if text.contains('_') {
        text = normalize_esword_refs(&text);
    }

    finish(text)
}

/// Normalize e-Sword refs Gén_25:16 → Gén 25:16 for the UI linkifier.
fn normalize_esword_refs(text: &str) -> String {
    RE_ESWORD_REF
        .replace_all(text, |caps: &regex::Captures| {
            let book = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
            let ch = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let v1 = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let v2 = caps.get(4).map(|m| m.as_str());
            if let Some(v2) = v2 {
                format!("{book} {ch}:{v1}-{v2}")
            } else {
                format!("{book} {ch}:{v1}")
            }
        })
        .into_owned()
}

/// Collapse space/tab runs, drop spaces hugging a newline, cap blank lines at
/// one, and trim — in a single pass.
///
/// This replaced `RE_SPACE` + two `String::replace` + `RE_NL`, which together
/// were the most expensive thing in the pipeline (128 ms of a 407 ms release
/// run over the module corpus, more than HTML stripping and entity decoding
/// combined) purely to squeeze runs of spaces.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space_pending = false;
    let mut newline_run = 0usize;
    for c in s.chars() {
        match c {
            ' ' | '\t' => space_pending = true,
            '\n' => {
                newline_run += 1;
                space_pending = false;
            }
            _ => {
                if newline_run > 0 {
                    if !out.is_empty() {
                        for _ in 0..newline_run.min(2) {
                            out.push('\n');
                        }
                    }
                    newline_run = 0;
                    space_pending = false;
                } else if space_pending {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    space_pending = false;
                }
                out.push(c);
            }
        }
    }
    out
}

/// Whitespace normalization + codepage repair, shared by both paths.
fn finish(text: String) -> String {
    let collapsed = collapse_whitespace(&text);
    // `collapse_whitespace` only folds ' ' and '\t'; the trim still has to run
    // for the other whitespace these modules leak — trailing '\r' in the
    // interlinear bibles, leading U+00A0 in the topic dictionaries.
    let trimmed = collapsed.trim();
    let owned = if trimmed.len() == collapsed.len() {
        collapsed
    } else {
        trimmed.to_string()
    };
    repair_codepage_mojibake(owned)
}

/// Intenta descomprimir blobs comunes; detecta cifrado sqliteplus.
pub fn decode_blob(b: &[u8]) -> Option<String> {
    if b.is_empty() {
        return None;
    }

    // zlib
    if let Ok(d) = read_all(ZlibDecoder::new(b)) {
        if let Some(s) = bytes_to_text(&d) {
            return Some(s);
        }
    }
    // gzip
    if let Ok(d) = read_all(GzDecoder::new(b)) {
        if let Some(s) = bytes_to_text(&d) {
            return Some(s);
        }
    }
    // raw deflate
    if let Ok(d) = read_all(DeflateDecoder::new(b)) {
        if let Some(s) = bytes_to_text(&d) {
            return Some(s);
        }
    }
    // skip headers then zlib/deflate
    for skip in [1usize, 2, 4, 8, 16] {
        if b.len() <= skip {
            continue;
        }
        let slice = &b[skip..];
        if let Ok(d) = read_all(ZlibDecoder::new(slice)) {
            if let Some(s) = bytes_to_text(&d) {
                return Some(s);
            }
        }
        if let Ok(d) = read_all(DeflateDecoder::new(slice)) {
            if let Some(s) = bytes_to_text(&d) {
                return Some(s);
            }
        }
    }

    if let Some(s) = bytes_to_text(b) {
        return Some(s);
    }

    if looks_encrypted(b) {
        // Proprietary per-row encryption used by older e-Sword formats
        // (.cmtx/.bblx/.dctx/.lexx “sqliteplus”). Not zlib — intentional DRM.
        return Some(
            "[Módulo cifrado por e-Sword (sqliteplus). \
El contenido solo lo abre e-Sword; no es texto comprimido legible. \
Buscá la versión moderna .cmti/.bbli/.dcti/.lexi del mismo recurso si existe \
(p. ej. Strong ° .lexi en lugar de strong.lexx).]"
                .into(),
        );
    }

    Some(
        "[Contenido binario no legible en este módulo. \
Puede ser un formato antiguo o cifrado.]"
            .into(),
    )
}

fn read_all<R: Read>(mut r: R) -> Result<Vec<u8>, ()> {
    let mut out = Vec::new();
    r.read_to_end(&mut out).map_err(|_| ())?;
    if out.is_empty() {
        return Err(());
    }
    Ok(out)
}

fn looks_encrypted(b: &[u8]) -> bool {
    if b.len() < 32 {
        return false;
    }
    let zlibish = b[0] == 0x78 && matches!(b[1], 0x01 | 0x5e | 0x9c | 0xda);
    let gzip = b[0] == 0x1f && b[1] == 0x8b;
    if zlibish || gzip {
        return false;
    }
    let mut seen = [false; 256];
    let sample = &b[..b.len().min(256)];
    for &x in sample {
        seen[x as usize] = true;
    }
    let unique = seen.iter().filter(|&&x| x).count();
    unique > 180
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_vine_ot_rtf_hebrew_and_spanish() {
        let raw = r"{\b\i\cf9 jatser} ({\cf12\f2\rtlch\fs32\'e7\'c8\'f6\'c5\'f8}, {\cf14\ul H2691}), \'abatrio; recinto\'bb. Este vocablo est\'e1 relacionado";
        let out = clean_text(raw);
        assert!(
            out.contains('ח') && out.contains('צ') && out.contains('ר'),
            "expected Hebrew חצר in {out:?}"
        );
        assert!(
            out.contains('«') && out.contains('»'),
            "expected guillemets in {out:?}"
        );
        assert!(
            out.contains('á') || out.contains("esta"),
            "expected Spanish accents in {out:?}"
        );
        assert!(
            out.contains("H2691"),
            "expected Strong’s H2691 in {out:?}"
        );
    }

    #[test]
    fn normalizes_esword_underscore_refs() {
        let raw = r"en {\cf11\ul Gén_25:16} : texto";
        let out = clean_text(raw);
        assert!(
            out.contains("Gén 25:16") || out.contains("Gén_25:16"),
            "ref normalize failed: {out:?}"
        );
    }

    #[test]
    fn rtf_paragraph_breaks_survive() {
        let raw = r"Primer parrafo.\par Segundo parrafo.\par\par Tercero.\pard\b listo";
        let out = clean_text(raw);
        assert!(
            out.contains("Primer parrafo.\nSegundo parrafo."),
            "\\par should become a newline: {out:?}"
        );
        assert!(
            !out.contains("\\pard"),
            "\\pard must still be stripped as a control word: {out:?}"
        );
        assert!(out.contains("listo"), "text after \\pard lost: {out:?}");
    }

    #[test]
    fn rtf_unicode_escapes_are_decimal() {
        // 舠? is a left double quote (U+201C), NOT hex 0x8220.
        let out = clean_text(r"dijo \u8220?Padre\u8221? y callo");
        assert!(
            out.contains('\u{201C}') && out.contains('\u{201D}'),
            "RTF \\uN must decode as decimal: {out:?}"
        );
        // Negative i16 spelling for code points above 0x7FFF
        let neg = clean_text(r"bullet \u-3913? aqui");
        assert!(
            neg.contains('\u{F0B7}'),
            "negative RTF \\u must wrap to +65536: {neg:?}"
        );
        // JS-style hex (no `?` suffix) keeps decoding as hex
        let hex = clean_text(r"caf\u00e9 listo");
        assert!(hex.contains('é'), "hex \\uXXXX regressed: {hex:?}");
    }

    #[test]
    fn short_utf8_blob_is_text_not_binary() {
        let out = decode_blob("amar".as_bytes()).expect("short text blob");
        assert_eq!(out, "amar", "short entries must not be called binary: {out:?}");
    }

    #[test]
    fn html_vine_entities_and_strong() {
        let raw = r#"<p>jatser (<span>&#x05D7;&#x05B8;&#x05E6;&#x05B5;&#x05E8;</span>, <span>H2691</span>), &laquo;atrio&raquo;. est&aacute;</p>"#;
        let out = clean_text(raw);
        assert!(out.contains('ח'));
        assert!(out.contains("H2691"));
        assert!(out.contains('«'));
        assert!(out.contains('á'));
    }

    /// Per-stage timing over the same corpus, so optimisation effort goes where
    /// the milliseconds actually are instead of where they look like they are.
    ///
    ///   CORPUS_IN=corpus_raw.jsonl cargo test --lib -- --ignored stage_bench --nocapture
    #[test]
    #[ignore = "harness: needs CORPUS_IN"]
    fn stage_bench() {
        use std::io::{BufRead, BufReader};
        use std::time::Instant;

        let inp = std::env::var("CORPUS_IN").expect("set CORPUS_IN");
        let src = BufReader::new(std::fs::File::open(&inp).expect("open CORPUS_IN"));
        let mut raws: Vec<String> = Vec::new();
        for line in src.lines() {
            let line = line.expect("read");
            if line.trim().is_empty() {
                continue;
            }
            let rec: serde_json::Value = serde_json::from_str(&line).expect("parse");
            if let Some(r) = rec.get("raw").and_then(|v| v.as_str()) {
                raws.push(r.to_string());
            }
        }
        let total_bytes: usize = raws.iter().map(|r| r.len()).sum();
        eprintln!("corpus: {} rows, {} KiB\n", raws.len(), total_bytes / 1024);

        macro_rules! bench {
            ($name:expr, $body:expr) => {{
                let t = Instant::now();
                let mut sink = 0usize;
                for raw in &raws {
                    let f: &dyn Fn(&str) -> String = &$body;
                    sink = sink.wrapping_add(f(raw).len());
                }
                let ms = t.elapsed().as_millis();
                eprintln!("  {:<28} {:>6} ms   (sink {})", $name, ms, sink & 0xF);
            }};
        }

        bench!("Shape::of + clone", |s: &str| {
            let _ = Shape::of(s);
            s.to_string()
        });
        bench!("RE_WOC", |s: &str| RE_WOC.replace_all(s, "$1").into_owned());
        bench!("RE_NUM_TAG", |s: &str| RE_NUM_TAG
            .replace_all(s, " $1 ")
            .into_owned());
        bench!("RE_HEB_TAG", |s: &str| RE_HEB_TAG
            .replace_all(s, "$1")
            .into_owned());
        bench!("replace_block_tags", |s: &str| replace_block_tags(s));
        bench!("decode_entities", |s: &str| decode_entities(s));
        bench!("RE_HTML strip", |s: &str| RE_HTML
            .replace_all(s, "")
            .into_owned());
        bench!("html_escape decode", |s: &str| {
            html_escape::decode_html_entities(s).into_owned()
        });
        bench!("collapse_whitespace", |s: &str| collapse_whitespace(s));
        bench!("RE_ESWORD_REF", |s: &str| normalize_esword_refs(s));
        bench!("repair_mojibake", |s: &str| repair_codepage_mojibake(
            s.to_string()
        ));
        bench!("FULL clean_text", |s: &str| clean_text(s));
    }

    /// Golden-corpus harness: run `clean_text` over raw cells sampled from the
    /// installed modules and dump the result, so a decoder change can be diffed
    /// line-for-line against the previous behaviour instead of eyeballed.
    ///
    ///   CORPUS_IN=corpus_raw.jsonl CORPUS_OUT=clean_before.jsonl \
    ///     cargo test --lib -- --ignored golden_dump --nocapture
    #[test]
    #[ignore = "harness: needs CORPUS_IN / CORPUS_OUT"]
    fn golden_dump() {
        use std::io::{BufRead, BufReader, BufWriter, Write};

        let inp = std::env::var("CORPUS_IN").expect("set CORPUS_IN");
        let outp = std::env::var("CORPUS_OUT").expect("set CORPUS_OUT");
        let src = BufReader::new(std::fs::File::open(&inp).expect("open CORPUS_IN"));
        let mut dst = BufWriter::new(std::fs::File::create(&outp).expect("create CORPUS_OUT"));

        let started = std::time::Instant::now();
        let mut rows = 0usize;
        let mut bytes_in = 0usize;
        for line in src.lines() {
            let line = line.expect("read line");
            if line.trim().is_empty() {
                continue;
            }
            let rec: serde_json::Value = serde_json::from_str(&line).expect("parse corpus line");
            let id = rec.get("id").and_then(|v| v.as_str()).unwrap_or("?");
            let Some(raw) = rec.get("raw").and_then(|v| v.as_str()) else {
                continue; // blob rows are exercised through decode_blob, not here
            };
            bytes_in += raw.len();
            let cleaned = clean_text(raw);
            let out = serde_json::json!({ "id": id, "clean": cleaned });
            writeln!(dst, "{out}").expect("write");
            rows += 1;
        }
        dst.flush().expect("flush");
        let ms = started.elapsed().as_millis();
        eprintln!(
            "golden_dump: {rows} rows, {} KiB in, {ms} ms ({:.1} rows/ms)",
            bytes_in / 1024,
            rows as f64 / ms.max(1) as f64
        );
    }
}

fn bytes_to_text(b: &[u8]) -> Option<String> {
    if let Ok(s) = std::str::from_utf8(b) {
        let stray_ctrl = s
            .chars()
            .filter(|c| c.is_control() && *c != '\n' && *c != '\r' && *c != '\t')
            .count();
        let cleaned = clean_text(s);
        if stray_ctrl == 0 {
            // Clean UTF-8 with no stray control bytes is a strong signal for real
            // text — ciphertext/compressed blobs essentially never decode this way.
            // Length is NOT a useful filter here: short dictionary entries
            // ("amar", "cf. H430") were being reported as unreadable binary.
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        } else {
            // Mostly-printable but with stray control bytes — keep the old,
            // stricter bar so binary noise doesn't slip through.
            let printable = cleaned
                .chars()
                .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                .count();
            if printable > 8 {
                return Some(cleaned);
            }
        }
    }
    // UTF-16, either endianness (some legacy modules ship BE)
    let utf16 = match b {
        [0xff, 0xfe, rest @ ..] => Some((rest, true)),
        [0xfe, 0xff, rest @ ..] => Some((rest, false)),
        _ => None,
    };
    if let Some((rest, little)) = utf16 {
        let u16s: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| {
                if little {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        if let Ok(s) = String::from_utf16(&u16s) {
            let cleaned = clean_text(&s);
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }
    None
}

/// Convierte valor SQLite (texto o blob) a String limpia.
pub fn cell_to_text(value: &rusqlite::types::Value) -> Option<String> {
    match value {
        rusqlite::types::Value::Text(s) => {
            let cleaned = clean_text(s);
            if cleaned.is_empty() {
                None
            } else {
                Some(cleaned)
            }
        }
        rusqlite::types::Value::Blob(b) => decode_blob(b),
        rusqlite::types::Value::Null => None,
        other => Some(format!("{other:?}")),
    }
}

/// True if blob looks like sqliteplus ciphertext (not readable plain text).
pub fn is_encrypted_blob(b: &[u8]) -> bool {
    if bytes_to_text(b).is_some() {
        return false;
    }
    if decode_blob(b).map(|s| s.starts_with("[Módulo cifrado")).unwrap_or(false) {
        return true;
    }
    looks_encrypted(b)
}
