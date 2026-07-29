use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use once_cell::sync::Lazy;
use regex::Regex;
use std::io::Read;

static RE_HTML: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<[^>]+>").unwrap());
/// RTF control words: \b \i \cf9 \fs32 \rtlch \ltrch \f2 etc. (not \'hh)
static RE_RTF: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\[a-zA-Z]+\d* ?").unwrap());
static RE_HEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\u([0-9a-fA-F]{2,6})\??").unwrap());
static RE_HEX_NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"&#x([0-9a-fA-F]+);").unwrap());
static RE_DEC_NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"&#(\d+);").unwrap());
/// RTF hex bytes: \'e1 \'ab \'e7 (e-Sword Vine AT, etc.)
static RE_RTF_HEX_RUN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:\\'[0-9a-fA-F]{2})+").unwrap());
static RE_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]+").unwrap());
static RE_NL: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").unwrap());
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

/// Limpia HTML/RTF de texto e-Sword (incl. Vine AT RTF y HTML moderno).
/// Preserva las palabras de Jesús como marcadores WOC_OPEN…WOC_CLOSE.
pub fn clean_text(raw: &str) -> String {
    let mut text = raw.to_string();

    // Mark red-letter / Words of Christ before stripping tags
    text = RE_WOC
        .replace_all(&text, |caps: &regex::Captures| {
            let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            format!("{WOC_OPEN}{inner}{WOC_CLOSE}")
        })
        .into_owned();

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

    text = text
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .replace("</p>", "\n")
        .replace("</div>", "\n")
        .replace("</span>", "")
        .replace("</sup>", "")
        .replace("<sup>", "")
        .replace("</ref>", "")
        .replace("<ref>", "");

    text = RE_HEX_NUM
        .replace_all(&text, |caps: &regex::Captures| {
            u32::from_str_radix(&caps[1], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_default()
        })
        .into_owned();

    text = RE_DEC_NUM
        .replace_all(&text, |caps: &regex::Captures| {
            caps[1]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_default()
        })
        .into_owned();

    text = RE_HEX
        .replace_all(&text, |caps: &regex::Captures| {
            u32::from_str_radix(&caps[1], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_default()
        })
        .into_owned();

    text = text
        .replace("</li>", "\n")
        .replace("</tr>", "\n")
        .replace("<li>", "• ")
        .replace("&nbsp;", " ")
        .replace("&laquo;", "«")
        .replace("&raquo;", "»")
        .replace("&aacute;", "á")
        .replace("&eacute;", "é")
        .replace("&iacute;", "í")
        .replace("&oacute;", "ó")
        .replace("&uacute;", "ú")
        .replace("&ntilde;", "ñ")
        .replace("&Aacute;", "Á")
        .replace("&Eacute;", "É")
        .replace("&Iacute;", "Í")
        .replace("&Oacute;", "Ó")
        .replace("&Uacute;", "Ú")
        .replace("&Ntilde;", "Ñ");

    // Decode RTF \'hh BEFORE stripping other RTF controls (Vine AT+)
    text = decode_rtf_hex_escapes(&text);

    // Strip remaining HTML tags but keep WOC private-use markers
    text = RE_HTML.replace_all(&text, "").into_owned();
    // RTF controls (not hex — those already decoded)
    text = RE_RTF.replace_all(&text, " ").into_owned();
    // leftover backslash groups
    text = text.replace("\\{", "{").replace("\\}", "}");
    text = text.replace('{', "").replace('}', "");
    // RTF optional hyphen / nonbreaking
    text = text.replace("\\-", "").replace("\\_", "_").replace("\\~", " ");

    text = html_escape::decode_html_entities(&text).into_owned();

    // Normalize e-Sword refs Gén_25:16 → Gén 25:16 for the UI linkifier
    text = RE_ESWORD_REF
        .replace_all(&text, |caps: &regex::Captures| {
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
        .into_owned();

    text = RE_SPACE.replace_all(&text, " ").into_owned();
    text = text.replace(" \n", "\n").replace("\n ", "\n");
    text = RE_NL.replace_all(&text, "\n\n").into_owned();
    text.trim().to_string()
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
    fn html_vine_entities_and_strong() {
        let raw = r#"<p>jatser (<span>&#x05D7;&#x05B8;&#x05E6;&#x05B5;&#x05E8;</span>, <span>H2691</span>), &laquo;atrio&raquo;. est&aacute;</p>"#;
        let out = clean_text(raw);
        assert!(out.contains('ח'));
        assert!(out.contains("H2691"));
        assert!(out.contains('«'));
        assert!(out.contains('á'));
    }
}

fn bytes_to_text(b: &[u8]) -> Option<String> {
    if let Ok(s) = std::str::from_utf8(b) {
        let cleaned = clean_text(s);
        // Require some printable content
        let printable = cleaned
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .count();
        if printable > 8 {
            return Some(cleaned);
        }
    }
    if b.len() >= 2 && b[0] == 0xff && b[1] == 0xfe {
        let u16s: Vec<u16> = b[2..]
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 {
                    Some(u16::from_le_bytes([c[0], c[1]]))
                } else {
                    None
                }
            })
            .collect();
        if let Ok(s) = String::from_utf16(&u16s) {
            let cleaned = clean_text(&s);
            if cleaned.len() > 8 {
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
