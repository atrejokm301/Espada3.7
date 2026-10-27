//! Decode e-Sword HTML / classic RTF and prepare display HTML.

use encoding_rs::WINDOWS_1252;
use once_cell::sync::Lazy;
use regex::Regex;

static RE_RTF_HEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\'([0-9a-fA-F]{2})").unwrap());
static RE_RTF_UNICODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\u(-?\d+)\??").unwrap());
static RE_RTF_CTRL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\\[a-zA-Z]+(-?\d+)?[ ]?").unwrap());
static RE_RTF_GROUP: Lazy<Regex> = Lazy::new(|| Regex::new(r"[{}]").unwrap());
static RE_HTML_ENTITY_NUM: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"&#(x?[0-9a-fA-F]+);").unwrap());
static RE_NUM_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)<num>\s*([HG]\d+)\s*</num>").unwrap());
/// e-Sword cross-references: <ref>Gén 1:1-2</ref>
static RE_REF_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)<ref(?:\s[^>]*)?>(.*?)</ref>").unwrap());
static RE_RED_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<red>").unwrap());
static RE_RED_CLOSE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</red>").unwrap());
static RE_BLU_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<blu>").unwrap());
static RE_BLU_CLOSE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</blu>").unwrap());
static RE_I_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<i(?:\s[^>]*)?>").unwrap());
static RE_I_CLOSE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</i>").unwrap());
static RE_B_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<b(?:\s[^>]*)?>").unwrap());
static RE_B_CLOSE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</b>").unwrap());
static RE_BR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
static RE_P: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</?p(?:\s[^>]*)?>").unwrap());
static RE_FONT_RED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)<font[^>]*color\s*=\s*["']?(?:#?(?:ff0000|c00000|800000|cc0000|e85d5d|red))["']?[^>]*>"#)
        .unwrap()
});
static RE_FONT_CLOSE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</font>").unwrap());
static RE_SPAN_STYLE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)<span[^>]*style\s*=\s*["'][^"']*font-weight\s*:\s*bold[^"']*["'][^>]*>"#).unwrap());
static RE_SPAN_CLOSE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</span>").unwrap());
static RE_STRIP_TAGS: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
static RE_GEN_UNDERSCORE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b([A-Za-záéíóúÁÉÍÓÚñÑ\.]+)_(\d+:\d+(?:-\d+)?)").unwrap());
static RE_MULTI_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]{2,}").unwrap());

/// Convert raw e-Sword field text into cleaned plain-ish HTML fragments.
pub fn clean_esword_text(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let mut s = raw.to_string();

    // Classic RTF blob
    if s.contains("\\rtf") || s.starts_with('{') && s.contains("\\f") {
        s = rtf_to_text(&s);
    }

    // Preserve Strong's numbers from <num>
    s = RE_NUM_TAG
        .replace_all(&s, |caps: &regex::Captures| {
            format!("⟦STRONG:{}⟧", caps[1].to_uppercase())
        })
        .to_string();

    // Preserve e-Sword <ref>…</ref> bible cross-refs (must not be stripped)
    s = RE_REF_TAG
        .replace_all(&s, |caps: &regex::Captures| {
            let inner = caps[1].trim();
            // Collapse internal tags if any
            let plain = RE_STRIP_TAGS.replace_all(inner, "");
            let plain = plain.replace('\n', " ").trim().to_string();
            if plain.is_empty() {
                String::new()
            } else {
                format!("⟦REF:{plain}⟧")
            }
        })
        .to_string();

    // Words of Christ / blue interlinear markers / common font colors
    s = RE_RED_OPEN.replace_all(&s, "⟦WOC⟧").to_string();
    s = RE_RED_CLOSE.replace_all(&s, "⟦/WOC⟧").to_string();
    s = RE_FONT_RED.replace_all(&s, "⟦WOC⟧").to_string();
    s = RE_FONT_CLOSE.replace_all(&s, "⟦/WOC⟧").to_string();
    s = RE_BLU_OPEN.replace_all(&s, "⟦BLU⟧").to_string();
    s = RE_BLU_CLOSE.replace_all(&s, "⟦/BLU⟧").to_string();
    s = RE_I_OPEN.replace_all(&s, "⟦I⟧").to_string();
    s = RE_I_CLOSE.replace_all(&s, "⟦/I⟧").to_string();
    s = RE_B_OPEN.replace_all(&s, "⟦B⟧").to_string();
    s = RE_B_CLOSE.replace_all(&s, "⟦/B⟧").to_string();
    s = RE_SPAN_STYLE.replace_all(&s, "⟦B⟧").to_string();
    s = RE_SPAN_CLOSE.replace_all(&s, "⟦/B⟧").to_string();
    s = RE_BR.replace_all(&s, "\n").to_string();
    s = RE_P.replace_all(&s, "\n").to_string();

    // Numeric HTML entities (Hebrew etc.)
    s = decode_numeric_entities(&s);

    // Named entities
    s = html_escape::decode_html_entities(&s).to_string();

    // Strip remaining tags
    s = RE_STRIP_TAGS.replace_all(&s, "").to_string();

    // Gén_25:16 → Gén 25:16
    s = RE_GEN_UNDERSCORE
        .replace_all(&s, |caps: &regex::Captures| {
            format!("{} {}", &caps[1], &caps[2])
        })
        .to_string();

    // Restore markers as HTML
    s = restore_ref_markers(&s);
    s = restore_strong_markers(&s);
    s = s.replace("⟦WOC⟧", r#"<span class="woc">"#);
    s = s.replace("⟦/WOC⟧", "</span>");
    s = s.replace("⟦BLU⟧", r#"<span class="interlinear">"#);
    s = s.replace("⟦/BLU⟧", "</span>");
    s = s.replace("⟦I⟧", "<em>");
    s = s.replace("⟦/I⟧", "</em>");
    s = s.replace("⟦B⟧", "<strong>");
    s = s.replace("⟦/B⟧", "</strong>");

    s = RE_MULTI_SPACE.replace_all(&s, " ").to_string();
    s = s.replace("\r\n", "\n").replace('\r', "\n");
    // Soft line breaks → <br>
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.len() > 1 {
        lines.join("<br>")
    } else {
        s.trim().to_string()
    }
}

fn restore_strong_markers(s: &str) -> String {
    static RE_PLACE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"⟦STRONG:([HG]\d+)⟧").unwrap());
    static RE_BROKEN: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"<span class="strong-ref" data-strong="([HG]\d+)""></span>"#).unwrap()
    });

    let mut out = RE_PLACE
        .replace_all(s, |caps: &regex::Captures| {
            let code = &caps[1];
            format!(r#"<span class="strong-ref" data-strong="{code}">{code}</span>"#)
        })
        .to_string();
    out = RE_BROKEN
        .replace_all(&out, |caps: &regex::Captures| {
            let code = &caps[1];
            format!(r#"<span class="strong-ref" data-strong="{code}">{code}</span>"#)
        })
        .to_string();
    out
}

/// Turn ⟦REF:Gén 1:1-2⟧ into a clickable bible-ref span (book/ch/verse attributes when parseable).
fn restore_ref_markers(s: &str) -> String {
    static RE_PLACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"⟦REF:([^⟧]+)⟧").unwrap());
    // "Gén 1:1-2" | "Mt. 5:12" | "1 Juan 3:16-18"
    static RE_PARSE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?i)^\s*((?:[123]\s*)?[A-Za-zÁÉÍÓÚáéíóúñÑ\.]+)\s+(\d{1,3})\s*[:：\.]\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?\s*$",
        )
        .unwrap()
    });

    RE_PLACE
        .replace_all(s, |caps: &regex::Captures| {
            let raw = caps[1].trim();
            if let Some(p) = RE_PARSE.captures(raw) {
                let book = p[1].trim().trim_end_matches('.');
                let ch = &p[2];
                let v1 = &p[3];
                let v2 = p.get(4).map(|m| m.as_str()).unwrap_or(v1);
                let label = if v1 == v2 {
                    format!("{book} {ch}:{v1}")
                } else {
                    format!("{book} {ch}:{v1}-{v2}")
                };
                format!(
                    r#"<span class="bible-ref" data-book="{book}" data-chapter="{ch}" data-verse="{v1}" data-verse-end="{v2}" title="Ver pasaje">{label}</span>"#
                )
            } else {
                // Unparsed — still mark for frontend linkify / hover by raw text
                format!(
                    r#"<span class="bible-ref bible-ref-raw" data-raw="{raw}" title="Ver pasaje">{raw}</span>"#,
                    raw = html_escape::encode_text(raw)
                )
            }
        })
        .to_string()
}

fn decode_numeric_entities(s: &str) -> String {
    RE_HTML_ENTITY_NUM
        .replace_all(s, |caps: &regex::Captures| {
            let body = &caps[1];
            let code = if body.starts_with('x') || body.starts_with('X') {
                u32::from_str_radix(&body[1..], 16).ok()
            } else {
                body.parse::<u32>().ok()
            };
            code.and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_else(|| caps[0].to_string())
        })
        .to_string()
}

/// Basic RTF → Unicode text (e-Sword Hebrew font bytes + Windows-1252 accents).
pub fn rtf_to_text(rtf: &str) -> String {
    // Hex escapes \'e7
    let mut s = RE_RTF_HEX
        .replace_all(rtf, |caps: &regex::Captures| {
            if let Ok(b) = u8::from_str_radix(&caps[1], 16) {
                let bytes = [b];
                let (cow, _, _) = WINDOWS_1252.decode(&bytes);
                cow.into_owned()
            } else {
                String::new()
            }
        })
        .to_string();

    s = RE_RTF_UNICODE
        .replace_all(&s, |caps: &regex::Captures| {
            if let Ok(n) = caps[1].parse::<i32>() {
                let u = if n < 0 { (n as i32 + 65536) as u32 } else { n as u32 };
                char::from_u32(u).map(|c| c.to_string()).unwrap_or_default()
            } else {
                String::new()
            }
        })
        .to_string();

    // Drop RTF control words
    s = RE_RTF_CTRL.replace_all(&s, "").to_string();
    s = RE_RTF_GROUP.replace_all(&s, "").to_string();
    s = s.replace("\\", "");
    s = s.replace('\0', "");
    RE_MULTI_SPACE.replace_all(&s, " ").trim().to_string()
}

/// Strip to plain text (for tooltips / copy).
pub fn to_plain(htmlish: &str) -> String {
    let s = RE_STRIP_TAGS.replace_all(htmlish, "");
    html_escape::decode_html_entities(&s)
        .replace('\u{00a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Detect likely encrypted/unreadable module content.
pub fn looks_encrypted(sample: &str) -> bool {
    if sample.is_empty() {
        return false;
    }
    // High ratio of non-printable / replacement chars
    let total = sample.chars().count().max(1);
    let bad = sample
        .chars()
        .filter(|c| {
            (*c as u32) < 9
                || (*c as u32) == 0xFFFD
                || (c.is_control() && *c != '\n' && *c != '\r' && *c != '\t')
        })
        .count();
    bad * 100 / total > 30
}
