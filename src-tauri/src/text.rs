//! Decode e-Sword HTML / classic RTF and prepare display HTML.
//!
//! `clean_esword_text` pipeline (order matters — see audit ISS-05):
//!  pre. Shrink embedded `<img …>` tags (data URIs can be 15 MB) before any regex pass.
//!  rtf. Classic RTF blob → text; otherwise headerless RTF fragments and a *whitelist* of
//!       RTF control words (`\par`, `\'85`, `\u8217`…) on HTML/plain fields (ISS-12/ISS-24).
//!  0.  Literal ⟦ ⟧ in module text are parked as private sentinels (shown again as text),
//!      so module text can never forge an internal marker.
//!  1b. Repairs: `<br` glued to a capital letter, unterminated `</span ` closers.
//!  1.  Known e-Sword tags → ⟦…⟧ markers (ref, num, red, b, i/em, italic/bold spans, br, p,
//!      tvm → span.morph, sup → sup.fn, h1-6, table cells, img).
//!  2.  Drop double-escaped *formatting* (`&lt;i&gt;`, `&lt;tw://…&gt;`) — never script/img,
//!      and never real angle-bracket prose such as «&lt;servil&gt;».
//!  3.  Strip remaining real tags while the text is still entity-encoded, with guardrails.
//!  4.  Decode entities until stable (≤ 2 passes), per text segment.
//!  5.  HTML-escape `&`, `<`, `>` in text.
//!  6.  Markers become the only live tags; payloads are escaped exactly once.
//!  7.  Inline tags are balanced (stray closers dropped, open ones closed at the end).

use encoding_rs::WINDOWS_1252;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

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
static RE_I_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<(?:i|em)(?:\s[^>]*)?>").unwrap());
static RE_I_CLOSE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</(?:i|em)>").unwrap());
static RE_B_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<b(?:\s[^>]*)?>").unwrap());
static RE_B_CLOSE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</b>").unwrap());
static RE_BR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
static RE_P: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</?p(?:\s[^>]*)?>").unwrap());
static RE_FONT_RED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)<font[^>]*color\s*=\s*["']?(?:#?(?:ff0000|c00000|800000|cc0000|e85d5d|red))["']?[^>]*>"#)
        .unwrap()
});
static RE_SPAN_STYLE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)<span[^>]*style\s*=\s*["'][^"']*font-weight\s*:\s*bold[^"']*["'][^>]*>"#).unwrap());
static RE_SPAN_ITALIC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)<span[^>]*style\s*=\s*["'][^"']*font-style\s*:\s*italic[^"']*["'][^>]*>"#).unwrap());
/// `<span …>` / `</span>` / `<font …>` / `</font>` — converted with a stack so every closer
/// matches its own opener (today every `</span>` became a stray `</strong>`).
static RE_SPAN_FONT_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)<(/?)(span|font)(\s[^<>]*)?>").unwrap());
static RE_STRIP_TAGS: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
/// `Gén_25:16`, `1Co_13:4` → space (ISS-25: optional numeric book prefix). The boundary is
/// explicit (not `\b`): a Hebrew point before `xo_15:2` used to count as a boundary only
/// because a stray `</strong>` sat between them.
static RE_GEN_UNDERSCORE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(^|[^A-Za-z0-9áéíóúÁÉÍÓÚñÑ_\.])((?:[123])?[A-Za-záéíóúÁÉÍÓÚñÑ\.]+)_(\d+:\d+(?:-\d+)?)").unwrap()
});
static RE_MULTI_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]{2,}").unwrap());

// ---- ISS-20: morphology codes and superscripts ----
/// Paired and bounded only: an unclosed `<tvm>`/`<sup>` must never hide the rest of an entry.
static RE_TVM: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)<tvm(?:\s[^<>]*)?>([^<]{0,80}?)</tvm\s*>").unwrap());
static RE_SUP_PAIR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<sup(?:\s[^<>]*)?>(.{0,200}?)</sup\s*>").unwrap());

// ---- ISS-24: structure ----
static RE_H_PAIR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<h[1-6](?:\s[^<>]*)?>(.{0,300}?)</h[1-6]\s*>").unwrap());
static RE_TD_BOUNDARY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)</t[dh]\s*>\s*<t[dh](?:\s[^<>]*)?>").unwrap());
static RE_TR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</?tr(?:\s[^<>]*)?>").unwrap());
static RE_HAS_TD: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<td[\s>]").unwrap());
static RE_TD_RUN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"⟦TD⟧(?:(?:\s|&nbsp;|&#160;)*⟦TD⟧)+").unwrap());
static RE_TD_EDGE: Lazy<Regex> =
    Lazy::new(|| {
        Regex::new(r"(?i)(?:\A|\n)(?:[ \t]|</?(?:td|th|tr|table|tbody|thead|tfoot)(?:\s[^<>]*)?>)*⟦TD⟧|⟦TD⟧(?:[ \t]|</?(?:td|th|tr|table|tbody|thead|tfoot)(?:\s[^<>]*)?>)*(?:\n|\z)")
            .unwrap()
    });
static RE_IMG_SHORT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<img(?:\s[^<>]*)?/?>").unwrap());

// ---- ISS-05 step 1b: repairs ----
static RE_BR_GLUED: Lazy<Regex> = Lazy::new(|| Regex::new(r"<br([A-ZÁÉÍÓÚÑ])").unwrap());
/// A bare tag name glued onto the next tag: `</p<p class="x">`, `<WG1520</num>`, `</lx</num>`.
static RE_BARE_TAG_BEFORE_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"</?[A-Za-z][A-Za-z0-9]{0,15}(<[A-Za-z/])").unwrap());
static RE_CLOSE_UNTERMINATED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)</(?:span|p|font|a|i|b|lx|div)\s+([^<>]*)(<|\z)").unwrap()
});

// ---- ISS-05 step 2: double-escaped formatting (narrow allowlist) ----
static RE_DBL_ALLOW: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)&lt;/?(?:i|b|u|br|p|span|font|sup|sub|term|new|div|em|strong)(?:\s+[a-z-]+\s*=[^<>\n]{0,200}?)?\s*/?&gt;|&lt;a\s+[a-z-]+\s*=[^<>\n]{0,200}?&gt;|&lt;/a\s*&gt;",
    )
    .unwrap()
});
static RE_DBL_TW: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)&lt;tw://[^<>\n]{0,200}?&gt;").unwrap());

// ---- ISS-05 step 3: tag strip with guardrails ----
/// Well-formed tag whose quoted attribute values may contain `<`, `>` or markers
/// (bdbthayer: `<a href="tw://[self]?<num>H1129</num>" style="…">`).
static RE_TAG_QUOTED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"<[A-Za-z][A-Za-z0-9]*(?:\s+[A-Za-z_:][-A-Za-z0-9_:.]*(?:\s*=\s*(?:"[^"]{0,300}"|'[^']{0,300}'|[^\s"'<>=`]+))?)*\s*/?>"#,
    )
    .unwrap()
});
/// A tag-shaped run that hits another '<', a marker or the end of text before any '>'.
static RE_TAG_UNTERMINATED: Lazy<Regex> =
    Lazy::new(|| Regex::new("(</?[A-Za-z][A-Za-z0-9]*[^<>⟦\u{1}]{0,200})(<|⟦|\u{1}|\\z)").unwrap());
static RE_TAG_FIRST_PASS: Lazy<Regex> =
    Lazy::new(|| Regex::new("<[A-Za-z/!?][^<>⟦\u{1}]{0,2000}>").unwrap());
/// Repeat passes: never span a newline, a marker or more than 200 chars.
static RE_TAG_REPEAT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<[A-Za-z/!?][^<>⟦\n]{0,200}>").unwrap());

// ---- ISS-29: NC-STRONG split entity `g&eacut<ref>e;n 1:8</ref>` ----
static RE_SPLIT_ENTITY_REF: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(\p{L}*)&([a-z]{1,7})<ref>([a-z]{0,7};)").unwrap()
});

// ---- RTF on non-RTF fields (ISS-24 whitelist, ISS-12 headerless fragments) ----
/// The delimiter after the word is checked in [`replace_rtf_words`], not matched here, so
/// `\par\par` converts both words.
static RE_RTF_WORD_TEXT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\\(par|line|tab|ldblquote|rdblquote|lquote|rquote|emdash|endash|nosupersub)")
        .unwrap()
});
static RE_RTF_WORD_TOGGLE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\(i0|i|b0|b)").unwrap());
static RE_RTF_UNICODE_STRICT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\\u(-?\d{1,6})(?:\\'[0-9a-fA-F]{2}|\?| )?").unwrap());
/// A headerless RTF group: formatting words, or paragraph alignment (`{\qc \b Título\par}`, the
/// centred section titles of the RTF Bibles: RV1960 .bblx, BTX3, RV60a, Straubinger).
static RE_RTF_FRAGMENT_GROUP: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{\\(?:i|b|ul|cf\d+|f\d+|fs\d+|super|sub|plain|ulnone|q[clrj]|pard)[^a-z]").unwrap());
static RE_RTF_GROUP_SIMPLE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{((?:\\[a-z]{1,32}-?\d{0,10} ?)+)([^{}\\]*)\}").unwrap());
static RE_RTF_HEADER_WORD: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\\([a-z]{1,32})(-?\d{0,10})").unwrap());
static RE_RTF_REF_TEXT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*((?:[123]\s?)?[\p{L}\.]+)[_ ]\s*(\d{1,3}:\d{1,3}(?:\s*[-–]\s*\d{1,3})?)\s*$").unwrap()
});
static RE_RTF_DESTINATION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{\\\*?\\?(?:fonttbl|colortbl|stylesheet|info)\b").unwrap());

// ---- ISS-36: own Strong tags `palabra <tvm>m</tvm> <num>X</num>[<num>Y</num>]` ----
static RE_OWN_GROUP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)<tvm>[^<]*</tvm>\s*((?:<num>\s*[HG]\d+\s*</num>\s*)+)").unwrap()
});
static RE_NUM_GROUP: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(?:<num>\s*[HG]\d+\s*</num>\s*)+").unwrap());
static RE_STRONG_CODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)[HG]\d+").unwrap());

/// Sentinels for literal ⟦ ⟧ found in module text (step 0).
const PARK_OPEN: char = '\u{2}';
const PARK_CLOSE: char = '\u{3}';

/// Internal marker: `⟦NAME⟧` or `⟦NAME:payload⟧` (payload never contains ⟦ or ⟧).
static RE_MARKER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"⟦(/?[A-Z]+)(?::([^⟦⟧]*))?⟧").unwrap());

/// Options for [`clean_esword_text_with`]. `Default` reproduces [`clean_esword_text`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanOptions {
    /// ISS-36: wrap each word that carries its own `<tvm>…</tvm> <num>…</num>` tag in
    /// `<span class="word" data-strong="H127 H859">…</span>`.
    pub tagged_words: bool,
    /// ISS-24 / open product question 7: show «[imagen]» where an embedded `<img>` was.
    /// Default `false` keeps today's behaviour (the image is removed silently).
    pub image_placeholder: bool,
    /// Skip [`repair_codepage_mojibake`]: Bible text. A Bible's words are tapped by position,
    /// and OGNTe+ writes the Latin «I» of its transliterations as a Greek capital iota
    /// («Ιisu», «Ιsaak»), which the repair would read as a damaged «É».
    pub keep_codepage: bool,
}

/// One word of a Strong-tagged Bible verse together with its own Strong numbers
/// (ISS-36). `strongs[0]` is the primary code; multi-`<num>` groups keep every code.
#[allow(dead_code)] // additive API for WP3 (modules.rs) and WP5 (lib.rs)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OwnTagUnit {
    pub word: String,
    pub strongs: Vec<String>,
}

/// Layout counters for detecting the `palabra <tvm> <num>` layout (ISS-36, WP3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct OwnTagLayoutStats {
    /// Maximal runs of adjacent `<num>` tags in the verse.
    pub num_groups: usize,
    /// Runs immediately preceded by a `<tvm>…</tvm>`.
    pub after_tvm: usize,
    /// Runs that attach to a word (`palabra <tvm>m</tvm> <num>…`).
    pub attached: usize,
}

/// Convert raw e-Sword field text into cleaned plain-ish HTML fragments.
pub fn clean_esword_text(raw: &str) -> String {
    clean_esword_text_with(raw, CleanOptions::default())
}

/// Like [`clean_esword_text`], but each word carrying its own Strong tag becomes
/// `<span class="word" data-strong="H127">tierra</span>` (ISS-36). Visible text is identical.
#[allow(dead_code)] // additive API for WP3 (modules.rs) and WP5 (lib.rs)
pub fn clean_bible_verse_tagged(raw: &str) -> String {
    clean_esword_text_with(
        raw,
        CleanOptions {
            tagged_words: true,
            keep_codepage: true,
            ..CleanOptions::default()
        },
    )
}

/// [`clean_esword_text`] for Bible text: the same cleaning, without the codepage repair
/// (see [`CleanOptions::keep_codepage`]).
pub fn clean_bible_text(raw: &str) -> String {
    clean_esword_text_with(
        raw,
        CleanOptions {
            keep_codepage: true,
            ..CleanOptions::default()
        },
    )
}

/// Words with their own Strong tags, in verse order. Only the word directly before a single
/// `<tvm>` that is directly followed by `<num>` gets the group; other words are omitted.
#[allow(dead_code)] // additive API for WP3 (modules.rs) and WP5 (lib.rs)
pub fn own_tag_units(raw: &str) -> Vec<OwnTagUnit> {
    scan_own_tags(raw)
        .0
        .into_iter()
        .map(|h| OwnTagUnit {
            word: raw[h.word_start..h.word_end].to_string(),
            strongs: h.strongs,
        })
        .collect()
}

/// Layout counters for one raw verse (see [`OwnTagLayoutStats`]).
#[allow(dead_code)] // additive API for WP3 (modules.rs) and WP5 (lib.rs)
pub fn own_tag_layout_stats(raw: &str) -> OwnTagLayoutStats {
    scan_own_tags(raw).1
}

/// Full cleaner with options.
pub fn clean_esword_text_with(raw: &str, opts: CleanOptions) -> String {
    if raw.is_empty() {
        return String::new();
    }
    // pre: embedded images first, so a 15 MB data URI never reaches the regex passes.
    let mut s = shrink_images(raw);

    // Classic RTF blob
    if s.contains("\\rtf") || s.starts_with('{') && s.contains("\\f") {
        s = rtf_to_text(&s);
    } else if s.contains('\\') {
        s = rtf_whitelist(&s);
    }

    // (0) Module text may never forge our markers: park literal ⟦ ⟧ as sentinels; they are
    // restored as plain (escaped) text. Entity forms only decode inside text segments.
    if s.contains([PARK_OPEN, PARK_CLOSE]) {
        s = s.replace([PARK_OPEN, PARK_CLOSE], "");
    }
    if s.contains(['⟦', '⟧']) {
        s = s.replace('⟦', "\u{2}").replace('⟧', "\u{3}");
    }

    // ISS-36: own-tag word markers (inserted on the scrubbed raw text).
    if opts.tagged_words {
        s = insert_word_markers(&s);
    }

    // (1b) Repairs that must run before tag conversion.
    if s.contains("<br") {
        s = RE_BR_GLUED.replace_all(&s, "\n$1").into_owned();
    }
    if s.contains('<') {
        s = RE_BARE_TAG_BEFORE_TAG.replace_all(&s, "$1").into_owned();
        s = RE_CLOSE_UNTERMINATED
            .replace_all(&s, |c: &regex::Captures| format!(" {}{}", &c[1], &c[2]))
            .into_owned();
    }

    // ISS-29: move a split entity and its leading letters back inside the <ref>.
    s = RE_SPLIT_ENTITY_REF
        .replace_all(&s, |c: &regex::Captures| {
            let entity = format!("&{}{}", &c[2], &c[3]);
            let decoded = html_escape::decode_html_entities(&entity);
            if decoded.chars().count() == 1 && decoded != entity {
                format!("<ref>{}{}", &c[1], entity)
            } else {
                c[0].to_string()
            }
        })
        .into_owned();

    // (1) Known tags → markers.
    // Preserve Strong's numbers from <num>
    s = RE_NUM_TAG
        .replace_all(&s, |caps: &regex::Captures| {
            format!("⟦STRONG:{}⟧", caps[1].to_uppercase())
        })
        .into_owned();

    // Preserve e-Sword <ref>…</ref> bible cross-refs (must not be stripped)
    s = RE_REF_TAG
        .replace_all(&s, |caps: &regex::Captures| {
            let inner = caps[1].trim();
            // Collapse internal tags if any
            let plain = RE_STRIP_TAGS.replace_all(inner, "");
            // `style=""<ref>>ua 17:26</ref>`: a doubled '>' is damage, not text.
            let plain = plain.replace('\n', " ").trim().trim_start_matches('>').trim().to_string();
            // An unterminated tag left in the payload (`<ref>Jn 3:16 <b x</ref>`) would let
            // the step-3 tag strip eat the marker's closing ⟧ and the text after it. Encode
            // it; render_ref decodes it back, so the raw label still reads '<'.
            let plain = plain.replace('<', "&lt;");
            if plain.contains(['⟦', '⟧']) {
                // An earlier marker inside (Strongspa: an unclosed `<ref>` running across a
                // `<num>` to a later `</ref>`) would break the REF marker and show «⟦REF:…»
                // as text. Not a reference: keep the content (and its markers) unwrapped.
                return caps[1].to_string();
            }
            if plain.is_empty() {
                String::new()
            } else {
                format!("⟦REF:{plain}⟧")
            }
        })
        .into_owned();

    // Morphology codes (ISS-20): self-contained marker, text only.
    s = RE_TVM
        .replace_all(&s, |caps: &regex::Captures| {
            let inner = RE_MARKER.replace_all(&caps[1], "");
            let inner = inner.replace(['\n', '\r'], " ");
            format!("⟦MORPH:{inner}⟧")
        })
        .into_owned();

    // Words of Christ / blue interlinear markers / common font colors
    s = RE_RED_OPEN.replace_all(&s, "⟦WOC⟧").into_owned();
    s = RE_RED_CLOSE.replace_all(&s, "⟦/WOC⟧").into_owned();
    s = RE_BLU_OPEN.replace_all(&s, "⟦BLU⟧").into_owned();
    s = RE_BLU_CLOSE.replace_all(&s, "⟦/BLU⟧").into_owned();
    s = convert_span_font(&s);
    s = RE_I_OPEN.replace_all(&s, "⟦I⟧").into_owned();
    s = RE_I_CLOSE.replace_all(&s, "⟦/I⟧").into_owned();
    s = RE_B_OPEN.replace_all(&s, "⟦B⟧").into_owned();
    s = RE_B_CLOSE.replace_all(&s, "⟦/B⟧").into_owned();
    s = RE_SUP_PAIR.replace_all(&s, "⟦FN⟧${1}⟦/FN⟧").into_owned();
    s = RE_H_PAIR.replace_all(&s, "\n⟦B⟧${1}⟦/B⟧\n").into_owned();
    if RE_HAS_TD.is_match(&s) {
        // Only real tables: OGNTe+ uses <Tr> for transliterations.
        s = RE_TD_BOUNDARY.replace_all(&s, "⟦TD⟧").into_owned();
        s = RE_TR.replace_all(&s, "\n").into_owned();
        s = tidy_cell_separators(&s);
    }
    s = RE_IMG_SHORT.replace_all(&s, "⟦IMG⟧").into_owned();
    s = RE_BR.replace_all(&s, "\n").into_owned();
    s = RE_P.replace_all(&s, "\n").into_owned();

    // (2) Double-escaped formatting (still encoded here).
    if s.contains("&lt;") || s.contains("&LT;") {
        s = replace_unless_cuts_marker(&RE_DBL_ALLOW, &s, "");
        s = replace_unless_cuts_marker(&RE_DBL_TW, &s, "");
    }

    // (3) Strip remaining real tags while the text is still encoded.
    if s.contains('<') {
        s = strip_tags_guarded(&s);
    }

    // (4)+(5)+(6)+(7) Decode text, escape it, restore markers as the only tags.
    s = restore_markers(&s, opts);

    s = RE_MULTI_SPACE.replace_all(&s, " ").into_owned();
    s = s.replace("\r\n", "\n").replace('\r', "\n");
    // Soft line breaks → <br>
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let out = if lines.len() > 1 {
        lines.join("<br>")
    } else {
        s.trim().to_string()
    };
    if opts.keep_codepage {
        out
    } else {
        repair_codepage_mojibake(out)
    }
}

// ─── Codepage repair (ported from the Windows build, module-pipeline f578470) ───
//
// Some modules were built by decoding Windows-1252 Spanish through the Greek (CP1253) or
// Hebrew (CP1255) table, so every accented letter ships as the letter at the same byte:
// «tambiιn», «posiciσn», «Seסor», «Aplicaciףn», «ΏQuι». A sampled scan of this Mac's library
// (2026-11-18) found it in 20 modules — RAE Ortografía, Frases y principios, Usos y costumbres,
// Biblia Comentada, Ravasi… — 28 727 letters inside words, 7 379 at a word's end and 2 171 at
// its start («ιl», «ϊltimo»).

/// The letter a damaged accent stands for: the exact CP1252 byte round-trip (CP1253, CP1255).
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
        // Romanized Greek in these lexicons spells omega as a macron o; the module shipped it
        // as tau (kτmē → kōmē, IΤANNOU → IŌANNOU). Like every letter here, only a lone τ that
        // touches a Latin letter is repaired, so a Greek word's τ is safe.
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

/// The Latin letter a Greek one is drawn like. A module may type it for that letter at a
/// word's start («νida» for «vida» in Biblia Comentada): not codepage damage.
fn latin_lookalike(c: char) -> Option<char> {
    Some(match c {
        'Α' => 'a',
        'Ι' => 'i',
        'Ν' => 'n',
        'Ρ' => 'p',
        'Τ' => 't',
        'α' => 'a',
        'ι' => 'i',
        'ν' => 'v',
        'ρ' => 'p',
        _ => return None,
    })
}

fn is_latin_letter(c: char) -> bool {
    c.is_ascii_alphabetic()
        || matches!(
            c,
            'á' | 'é' | 'í' | 'ó' | 'ú' | 'ü' | 'ñ' | 'Á' | 'É' | 'Í' | 'Ó' | 'Ú' | 'Ü' | 'Ñ'
                | 'à' | 'è' | 'ì' | 'ò' | 'ù' | 'â' | 'ê' | 'î' | 'ô' | 'û' | 'ç' | 'Ç'
                // Macron vowels of the transliterations lexicons print beside the headword
                // (kōmē, šālôm).
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

/// One visible character of cleaned HTML, at byte `at` (`None`: a separator — a `<br>` or an
/// entity — that is not a letter).
type Visible = (Option<usize>, char);

/// The visible characters of the cleaner's own output: tags are skipped (inline tags join
/// their neighbours, `<br>` separates them), an entity is one non-letter. The output only
/// carries tags the cleaner wrote and text it escaped, so every `<` opens a tag.
fn visible_chars(html: &str) -> Vec<Visible> {
    let mut out = Vec::with_capacity(html.len());
    let mut i = 0;
    while i < html.len() {
        let rest = &html[i..];
        let c = rest.chars().next().unwrap_or(' ');
        if c == '<' {
            if let Some(end) = rest.find('>') {
                if rest.len() > 3 && rest.as_bytes()[1..3].eq_ignore_ascii_case(b"br") {
                    out.push((None, ' '));
                }
                i += end + 1;
                continue;
            }
        } else if c == '&' {
            if let Some(end) = rest.find(';').filter(|&e| e <= 10) {
                out.push((None, ' '));
                i += end + 1;
                continue;
            }
        }
        out.push((Some(i), c));
        i += c.len_utf8();
    }
    out
}

/// Repair the accents described on [`unmojibake`] in cleaned display HTML.
///
/// Only a *lone* Greek/Hebrew letter touching a Latin word is repaired: a run of two or more
/// is genuine Greek or Hebrew (`ἄλφα`, `אָב`) and never touched, so real headwords survive. At
/// a word's start a letter drawn like a Latin one is left alone when the same text also has
/// the word spelled with that Latin letter («νida» beside «vida»).
fn repair_codepage_mojibake(html: String) -> String {
    // Every repaired character is Greek (U+0370–03FF) or Hebrew (U+0590–05FF): UTF-8 lead
    // bytes 0xCD..=0xD7. A byte scan rejects almost every text without decoding a char.
    if !html.as_bytes().iter().any(|b| (0xCD..=0xD7).contains(b)) {
        return html;
    }
    if !html.chars().any(|c| unmojibake(c).is_some()) {
        return html;
    }
    let vis = visible_chars(&html);
    let mut vocab: Option<std::collections::HashSet<String>> = None;
    let mut fixes: Vec<(usize, char, char)> = Vec::new();
    for (k, &(at, c)) in vis.iter().enumerate() {
        let (Some(at), Some(fixed)) = (at, unmojibake(c)) else {
            continue;
        };
        let prev = if k > 0 { vis[k - 1].1 } else { ' ' };
        let next = vis.get(k + 1).map_or(' ', |v| v.1);
        let same_script = |x: char| if is_greek_char(c) { is_greek_char(x) } else { is_hebrew_char(x) };
        if same_script(prev) || same_script(next) {
            continue;
        }
        let (latin_before, latin_after) = (is_latin_letter(prev), is_latin_letter(next));
        if !latin_before && !latin_after {
            continue;
        }
        if !latin_before {
            if let Some(look) = latin_lookalike(c) {
                let mut word = String::from(look);
                word.extend(
                    vis[k + 1..]
                        .iter()
                        .map(|v| v.1)
                        .take_while(|&x| is_latin_letter(x))
                        .flat_map(char::to_lowercase),
                );
                let vocab = vocab.get_or_insert_with(|| latin_words(&vis));
                if vocab.contains(&word) {
                    continue;
                }
            }
        }
        fixes.push((at, c, fixed));
    }
    if fixes.is_empty() {
        return html;
    }
    let mut out = String::with_capacity(html.len());
    let mut last = 0;
    for (at, c, fixed) in fixes {
        out.push_str(&html[last..at]);
        out.push(fixed);
        last = at + c.len_utf8();
    }
    out.push_str(&html[last..]);
    out
}

/// The lower-case Latin words of a text (for the look-alike check).
fn latin_words(vis: &[Visible]) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let mut cur = String::new();
    for &(_, c) in vis {
        if is_latin_letter(c) {
            cur.extend(c.to_lowercase());
        } else if !cur.is_empty() {
            set.insert(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        set.insert(cur);
    }
    set
}

/// One separator between cells: empty cells collapse, none at the start or end of a row.
fn tidy_cell_separators(s: &str) -> String {
    let s = RE_TD_RUN.replace_all(s, "⟦TD⟧");
    RE_TD_EDGE
        .replace_all(&s, |c: &regex::Captures| {
            if c[0].contains('\n') {
                "\n"
            } else {
                ""
            }
        })
        .into_owned()
}

/// Replace every literal `<img …>` with a bare `<img>` using memchr-speed scans only.
fn shrink_images(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Option<String> = None;
    let mut last = 0;
    let mut from = 0;
    while let Some(off) = raw[from..].find('<') {
        let i = from + off;
        from = i + 1;
        let is_img = bytes.len() >= i + 5
            && bytes[i + 1..i + 4].eq_ignore_ascii_case(b"img")
            && matches!(bytes[i + 4], b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>');
        if !is_img {
            continue;
        }
        let Some(close) = raw[i..].find('>') else {
            break;
        };
        let end = i + close + 1;
        let o = out.get_or_insert_with(|| String::with_capacity(raw.len().min(1 << 16)));
        o.push_str(&raw[last..i]);
        o.push_str("<img>");
        last = end;
        from = end;
    }
    match out {
        Some(mut o) => {
            o.push_str(&raw[last..]);
            o
        }
        None => raw.to_string(),
    }
}

/// `<span>`/`<font>` with a stack: bold span → B, italic span → I, red font → WOC, others → nothing.
fn convert_span_font(s: &str) -> String {
    if !s.contains('<') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut span_stack: Vec<&'static str> = Vec::new();
    let mut font_stack: Vec<&'static str> = Vec::new();
    let mut last = 0;
    for c in RE_SPAN_FONT_TAG.captures_iter(s) {
        let m = c.get(0).unwrap();
        out.push_str(&s[last..m.start()]);
        last = m.end();
        let closing = !c[1].is_empty();
        let is_span = c[2].eq_ignore_ascii_case("span");
        let tag = m.as_str();
        if closing {
            let st = if is_span { &mut span_stack } else { &mut font_stack };
            if let Some(close) = st.pop() {
                out.push_str(close);
            }
        } else if is_span {
            if RE_SPAN_STYLE.is_match(tag) {
                out.push_str("⟦B⟧");
                span_stack.push("⟦/B⟧");
            } else if RE_SPAN_ITALIC.is_match(tag) {
                out.push_str("⟦I⟧");
                span_stack.push("⟦/I⟧");
            } else {
                span_stack.push("");
            }
        } else if RE_FONT_RED.is_match(tag) {
            out.push_str("⟦WOC⟧");
            font_stack.push("⟦/WOC⟧");
        } else {
            font_stack.push("");
        }
    }
    out.push_str(&s[last..]);
    out
}

/// ISS-05 step 3. Order: quoted-attribute tags, unterminated fragments, one generic pass,
/// then at most 3 bounded repeat passes (never across a newline, marker or 200 chars).
/// True when `m` holds only whole markers: every ⟧ closes a ⟦ inside `m` and none is left
/// open. (Literal brackets are parked in step 0, so any ⟦ ⟧ here is a marker's.)
fn holds_whole_markers(m: &str) -> bool {
    let mut depth = 0usize;
    for ch in m.chars() {
        match ch {
            '⟦' => depth += 1,
            '⟧' if depth == 0 => return false,
            '⟧' => depth -= 1,
            _ => {}
        }
    }
    depth == 0
}

/// `re.replace_all(s, rep)`, except a match that would cut a marker in two is kept as is.
/// A quoted attribute may hold a whole marker (bdbthayer `href="tw://[self]?<num>H1129</num>"`),
/// but a quote or `&lt;…&gt;` that starts or ends inside a payload must not eat its ⟦ or ⟧.
fn replace_unless_cuts_marker(re: &Regex, s: &str, rep: &str) -> String {
    re.replace_all(s, |c: &regex::Captures| {
        let m = &c[0];
        if holds_whole_markers(m) {
            rep.to_string()
        } else {
            m.to_string()
        }
    })
    .into_owned()
}

fn strip_tags_guarded(s: &str) -> String {
    const SENTINEL: char = '\u{1}';
    let mut s = replace_unless_cuts_marker(&RE_TAG_QUOTED, s, "\u{1}");
    for _ in 0..3 {
        let n = RE_TAG_UNTERMINATED
            .replace_all(&s, |c: &regex::Captures| c[2].to_string())
            .into_owned();
        if n == s {
            break;
        }
        s = n;
    }
    s = RE_TAG_FIRST_PASS.replace_all(&s, "\u{1}").into_owned();
    for _ in 0..3 {
        let n = RE_TAG_REPEAT.replace_all(&s, "\u{1}").into_owned();
        if n == s {
            break;
        }
        s = n;
    }
    if s.contains(SENTINEL) {
        s = s.replace(SENTINEL, "");
    }
    s
}

#[derive(Clone)]
struct OpenInline {
    kind: &'static str,
    opener: String,
}

fn inline_kind(name: &str) -> Option<(&'static str, bool)> {
    let (closing, base) = match name.strip_prefix('/') {
        Some(b) => (true, b),
        None => (false, name),
    };
    let kind = match base {
        "WOC" => "WOC",
        "BLU" => "BLU",
        "I" => "I",
        "B" => "B",
        "FN" => "FN",
        "W" => "W",
        _ => return None,
    };
    Some((kind, closing))
}

fn inline_closer(kind: &str) -> &'static str {
    match kind {
        "I" => "</em>",
        "B" => "</strong>",
        "FN" => "</sup>",
        _ => "</span>",
    }
}

fn inline_opener(kind: &str, payload: Option<&str>) -> String {
    match kind {
        "WOC" => r#"<span class="woc">"#.to_string(),
        "BLU" => r#"<span class="interlinear">"#.to_string(),
        "I" => "<em>".to_string(),
        "B" => "<strong>".to_string(),
        "FN" => r#"<sup class="fn">"#.to_string(),
        "W" => format!(
            r#"<span class="word" data-strong="{}">"#,
            html_escape::encode_double_quoted_attribute(payload.unwrap_or(""))
        ),
        _ => String::new(),
    }
}

/// Steps 4–7: decode + escape text segments, render markers, balance inline tags.
/// A closer whose opener has more than this many tags open above it is treated as stray
/// instead of closing and reopening them all (crafted misnesting made that quadratic).
/// Only the top of the stack is searched, so deep stacks of unclosed openers (ebc.cmti
/// leaves one `<red>` per footnote open) stay cheap and keep their formatting.
const MAX_REOPEN_PER_CLOSER: usize = 8;

fn restore_markers(s: &str, opts: CleanOptions) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 4);
    let mut stack: Vec<OpenInline> = Vec::new();
    // Bytes the balancer may spend re-emitting tags (closers + reopeners). Keeps the output
    // linear in the input even for long `⟦W:…⟧` payloads; real rows stay far below it.
    let mut reopen_budget = s.len();
    let mut last = 0;
    for c in RE_MARKER.captures_iter(s) {
        let m = c.get(0).unwrap();
        push_text_segment(&mut out, &s[last..m.start()]);
        last = m.end();
        let name = c.get(1).unwrap().as_str();
        let payload = c.get(2).map(|p| p.as_str());
        if let Some((kind, closing)) = inline_kind(name) {
            if !closing {
                let opener = inline_opener(kind, payload);
                out.push_str(&opener);
                stack.push(OpenInline { kind, opener });
                continue;
            }
            let lo = stack.len().saturating_sub(MAX_REOPEN_PER_CLOSER + 1);
            if let Some(pos) = stack[lo..].iter().rposition(|o| o.kind == kind).map(|i| lo + i) {
                let cost: usize = stack[pos + 1..]
                    .iter()
                    .map(|o| o.opener.len() + inline_closer(o.kind).len())
                    .sum();
                if cost > reopen_budget {
                    continue; // stray
                }
                reopen_budget -= cost;
                let above: Vec<OpenInline> = stack.drain(pos + 1..).collect();
                for o in above.iter().rev() {
                    out.push_str(inline_closer(o.kind));
                }
                out.push_str(inline_closer(kind));
                stack.pop();
                for o in above {
                    out.push_str(&o.opener);
                    stack.push(o);
                }
            }
            // else: stray closer — dropped
            continue;
        }
        match name {
            "REF" => out.push_str(&render_ref(payload.unwrap_or(""))),
            "STRONG" => {
                let code = payload.unwrap_or("");
                out.push_str(&format!(
                    r#"<span class="strong-ref" data-strong="{code}">{code}</span>"#
                ));
            }
            "MORPH" => {
                let text = unpark(&decode_entities_stable(payload.unwrap_or("")));
                out.push_str(r#"<span class="morph">"#);
                out.push_str(&html_escape::encode_text(&text));
                out.push_str("</span>");
            }
            "IMG" => {
                if opts.image_placeholder {
                    out.push_str("[imagen]");
                }
            }
            "TD" => out.push_str(" · "),
            _ => {}
        }
    }
    push_text_segment(&mut out, &s[last..]);
    for o in stack.iter().rev() {
        out.push_str(inline_closer(o.kind));
    }
    out
}

fn push_text_segment(out: &mut String, seg: &str) {
    if seg.is_empty() {
        return;
    }
    let mut t = unpark(&decode_entities_stable(seg));
    if t.contains('_') {
        t = convert_underscore_refs(&t);
    }
    out.push_str(&html_escape::encode_text(&t));
}

/// Literal ⟦ ⟧ parked in step 0 come back as ordinary text.
fn unpark(s: &str) -> String {
    if s.contains([PARK_OPEN, PARK_CLOSE]) {
        s.replace(PARK_OPEN, "⟦").replace(PARK_CLOSE, "⟧")
    } else {
        s.to_string()
    }
}

fn convert_underscore_refs(s: &str) -> String {
    RE_GEN_UNDERSCORE
        .replace_all(s, |caps: &regex::Captures| format!("{}{} {}", &caps[1], &caps[2], &caps[3]))
        .into_owned()
}

/// Decode numeric (with Windows-1252 C1 repair) and named entities, at most twice.
/// The webview used to decode once more after Rust, so two passes keep what users saw.
fn decode_entities_stable(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let once = decode_entities_once(s);
    if once.contains('&') && once != s {
        decode_entities_once(&once)
    } else {
        once
    }
}

fn decode_entities_once(s: &str) -> String {
    let s = decode_numeric_entities(s);
    html_escape::decode_html_entities(&s).into_owned()
}

/// Turn ⟦REF:Gén 1:1-2⟧ into a clickable bible-ref span (book/ch/verse attributes when parseable).
fn render_ref(payload: &str) -> String {
    // "Gén 1:1-2" | "Mt. 5:12" | "1 Juan 3:16-18" | "Ef 5:21-6:9"
    static RE_PARSE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?i)^\s*((?:[123]\s*)?[A-Za-zÁÉÍÓÚáéíóúñÑ\.]+)\s+(\d{1,3})\s*[:：\.]\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3})(?:\s*[:：]\s*(\d{1,3}))?)?\s*$",
        )
        .unwrap()
    });
    let decoded = unpark(&decode_entities_stable(payload));
    let decoded = convert_underscore_refs(&decoded);
    let raw = decoded.trim();
    if let Some(p) = RE_PARSE.captures(raw) {
        let book = p[1].trim().trim_end_matches('.');
        let book_attr = html_escape::encode_double_quoted_attribute(book);
        let book_text = html_escape::encode_text(book);
        let ch = &p[2];
        let v1 = &p[3];
        match (p.get(4).map(|m| m.as_str()), p.get(5).map(|m| m.as_str())) {
            (Some(c2), Some(v2)) => {
                let (ch_n, c2_n) = (ch.parse::<u32>().unwrap_or(0), c2.parse::<u32>().unwrap_or(0));
                if c2_n == ch_n {
                    // "3:16-3:20" is a same-chapter range.
                    return ref_span_simple(&book_attr, &book_text, ch, v1, v2);
                }
                if c2_n > ch_n {
                    // ISS-25: cross-chapter. New attributes go AFTER the existing ones and
                    // data-verse-end is omitted, so crossrefs.rs keeps its current parse.
                    return format!(
                        r#"<span class="bible-ref" data-book="{book_attr}" data-chapter="{ch}" data-verse="{v1}" data-chapter-end="{c2}" data-verse-end2="{v2}" title="Ver pasaje">{book_text} {ch}:{v1}-{c2}:{v2}</span>"#
                    );
                }
                // Reversed chapters: leave unparsed (today's behaviour).
            }
            (v2, _) => {
                return ref_span_simple(&book_attr, &book_text, ch, v1, v2.unwrap_or(v1));
            }
        }
    }
    // Unparsed — still mark for frontend linkify / hover by raw text
    format!(
        r#"<span class="bible-ref bible-ref-raw" data-raw="{attr}" title="Ver pasaje">{text}</span>"#,
        attr = html_escape::encode_double_quoted_attribute(raw),
        text = html_escape::encode_text(raw)
    )
}

fn ref_span_simple(book_attr: &str, book_text: &str, ch: &str, v1: &str, v2: &str) -> String {
    let label = if v1 == v2 {
        format!("{book_text} {ch}:{v1}")
    } else {
        format!("{book_text} {ch}:{v1}-{v2}")
    };
    format!(
        r#"<span class="bible-ref" data-book="{book_attr}" data-chapter="{ch}" data-verse="{v1}" data-verse-end="{v2}" title="Ver pasaje">{label}</span>"#
    )
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
            match code {
                // ISS-24: Windows-1252 code points written as numeric entities (&#146; → ’).
                Some(c @ 0x80..=0x9F) => {
                    let byte = [c as u8];
                    WINDOWS_1252.decode(&byte).0.into_owned()
                }
                _ => code
                    .and_then(char::from_u32)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| caps[0].to_string()),
            }
        })
        .into_owned()
}

/// ISS-24: whitelisted RTF control words on non-RTF fields. Emits only newlines or plain
/// characters (plus `<b>`/`<i>` inside detected headerless RTF fragments, ISS-12). Unknown
/// words such as `\the`, `\talmud` or `..\cdshop` are never touched.
fn rtf_whitelist(s: &str) -> String {
    let mut s = RE_RTF_UNICODE_STRICT
        .replace_all(s, |caps: &regex::Captures| {
            caps[1]
                .parse::<i32>()
                .ok()
                .map(|n| if n < 0 { (n + 65536) as u32 } else { n as u32 })
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_else(|| caps[0].to_string())
        })
        .into_owned();
    s = RE_RTF_HEX
        .replace_all(&s, |caps: &regex::Captures| {
            u8::from_str_radix(&caps[1], 16)
                .map(|b| WINDOWS_1252.decode(&[b]).0.into_owned())
                .unwrap_or_default()
        })
        .into_owned();
    s = replace_rtf_words(&s, &RE_RTF_WORD_TEXT, false, |w| match w {
        "par" | "line" => "\n",
        "tab" => " ",
        "ldblquote" => "“",
        "rdblquote" => "”",
        "lquote" => "‘",
        "rquote" => "’",
        "emdash" => "—",
        "endash" => "–",
        _ => "",
    });
    let fragment = RE_RTF_FRAGMENT_GROUP.is_match(&s);
    if fragment {
        s = convert_rtf_fragment(&s);
    }
    replace_rtf_words(&s, &RE_RTF_WORD_TOGGLE, true, |w| match (w, fragment) {
        ("b", true) => "<b>",
        ("b0", true) => "</b>",
        ("i", true) => "<i>",
        ("i0", true) => "</i>",
        _ => "",
    })
}

/// Replace the control words `re` finds (group 1 = the word). A word followed by a lowercase
/// ASCII letter (or a digit when `digit_joins`) is not that control word and stays as text.
/// RTF treats one following space as part of the control word; any other delimiter is left
/// in place, so the next control word (`\par\par`) is still seen.
fn replace_rtf_words(
    s: &str,
    re: &Regex,
    digit_joins: bool,
    rep: impl Fn(&str) -> &'static str,
) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for c in re.captures_iter(s) {
        let m = c.get(0).unwrap();
        let next = s[m.end()..].chars().next();
        if matches!(next, Some(ch) if ch.is_ascii_lowercase() || (digit_joins && ch.is_ascii_digit())) {
            continue;
        }
        out.push_str(&s[last..m.start()]);
        out.push_str(rep(&c[1]));
        last = m.end() + usize::from(next == Some(' '));
    }
    out.push_str(&s[last..]);
    out
}

/// ISS-12: headerless RTF fragments (Straubinger/LECTIO .cmtx): `{\i x}` → italics,
/// `{\cf2\b 1.}` → bold, `{\cf11\ul Jua_7:50}` → `<ref>`, font/colour tables dropped.
fn convert_rtf_fragment(s: &str) -> String {
    let mut s = drop_rtf_destinations(s);
    for _ in 0..8 {
        let n = RE_RTF_GROUP_SIMPLE
            .replace_all(&s, |caps: &regex::Captures| {
                let header = &caps[1];
                let content = &caps[2];
                let mut italic = false;
                let mut bold = false;
                let mut underline = false;
                for w in RE_RTF_HEADER_WORD.captures_iter(header) {
                    let off = &w[2] == "0";
                    match &w[1] {
                        "i" => italic = !off,
                        "b" => bold = !off,
                        "ul" => underline = !off,
                        "ulnone" => underline = false,
                        _ => {}
                    }
                }
                if underline {
                    if let Some(r) = RE_RTF_REF_TEXT.captures(content) {
                        return format!("<ref>{} {}</ref>", r[1].trim(), &r[2]);
                    }
                }
                let mut t = content.to_string();
                if italic {
                    t = format!("<i>{t}</i>");
                }
                if bold {
                    t = format!("<b>{t}</b>");
                }
                t
            })
            .into_owned();
        if n == s {
            break;
        }
        s = n;
    }
    s
}

/// Remove `{\fonttbl …}`, `{\colortbl …}`, `{\stylesheet …}` and `{\info …}` groups (nested braces).
fn drop_rtf_destinations(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(m) = RE_RTF_DESTINATION.find(rest) {
        out.push_str(&rest[..m.start()]);
        let mut depth = 0usize;
        let mut end = rest.len();
        for (i, ch) in rest[m.start()..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = m.start() + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

struct OwnTagHit {
    word_start: usize,
    word_end: usize,
    strongs: Vec<String>,
}

fn is_word_char(c: char) -> bool {
    c.is_alphabetic()
        || ('\u{0300}'..='\u{036F}').contains(&c)
        || ('\u{0591}'..='\u{05C7}').contains(&c)
        || ('\u{1DC0}'..='\u{1DFF}').contains(&c)
}

/// The word that ends right before byte `end` (after optional spaces), if it is a whole word.
fn word_before(raw: &str, end: usize) -> Option<(usize, usize)> {
    let head = raw[..end].trim_end_matches([' ', '\t', '\u{a0}']);
    let word_end = head.len();
    let mut word_start = word_end;
    for (i, c) in head.char_indices().rev() {
        if is_word_char(c) {
            word_start = i;
        } else {
            break;
        }
    }
    if word_start == word_end {
        return None;
    }
    let before = &head[..word_start];
    match before.chars().next_back() {
        None => {}
        Some(c) if c.is_alphanumeric() || c == '&' || c == '#' => return None,
        // `<brPalabra`, `</b palabra`: the letters are a (damaged) tag name, not a word.
        Some('<') => return None,
        Some('/') if before.ends_with("</") => return None,
        Some(';') => {
            // `&quot;Yo` is a boundary; `Se&ntilde;or` is the tail of a word.
            if let Some(amp) = before.rfind('&') {
                let ent = &before[amp..];
                if ent.len() <= 12 {
                    let dec = decode_entities_once(ent);
                    if dec != ent && dec.chars().any(|c| c.is_alphanumeric()) {
                        return None;
                    }
                }
            }
        }
        Some(_) => {}
    }
    Some((word_start, word_end))
}

fn scan_own_tags(raw: &str) -> (Vec<OwnTagHit>, OwnTagLayoutStats) {
    let mut stats = OwnTagLayoutStats::default();
    let mut hits = Vec::new();
    if !raw.contains('<') {
        return (hits, stats);
    }
    stats.num_groups = RE_NUM_GROUP.find_iter(raw).count();
    for c in RE_OWN_GROUP.captures_iter(raw) {
        stats.after_tvm += 1;
        let m = c.get(0).unwrap();
        let Some((ws, we)) = word_before(raw, m.start()) else {
            continue;
        };
        let strongs: Vec<String> = RE_STRONG_CODE
            .find_iter(&c[1])
            .map(|x| x.as_str().to_uppercase())
            .collect();
        if strongs.is_empty() {
            continue;
        }
        stats.attached += 1;
        hits.push(OwnTagHit {
            word_start: ws,
            word_end: we,
            strongs,
        });
    }
    (hits, stats)
}

fn insert_word_markers(s: &str) -> String {
    let (hits, _) = scan_own_tags(s);
    if hits.is_empty() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + hits.len() * 24);
    let mut last = 0;
    for h in hits {
        out.push_str(&s[last..h.word_start]);
        out.push_str("⟦W:");
        out.push_str(&h.strongs.join(" "));
        out.push('⟧');
        out.push_str(&s[h.word_start..h.word_end]);
        out.push_str("⟦/W⟧");
        last = h.word_end;
    }
    out.push_str(&s[last..]);
    out
}

/// Basic RTF → Unicode text (e-Sword Hebrew font bytes + Windows-1252 accents).
pub fn rtf_to_text(rtf: &str) -> String {
    // Font/colour tables are destinations, not text (they leaked as "Arial;").
    let rtf = drop_rtf_destinations(rtf);
    // Hex escapes \'e7
    let mut s = RE_RTF_HEX
        .replace_all(&rtf, |caps: &regex::Captures| {
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
                let u = if n < 0 { (n + 65536) as u32 } else { n as u32 };
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
///
/// Morphology codes (`span.morph`) and footnote markers (`sup.fn`) are dropped; line and
/// block boundaries become spaces so words never glue together (ISS-20, ISS-24).
pub fn to_plain(htmlish: &str) -> String {
    static RE_PLAIN_DROP: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?is)<span class="morph">.*?</span>|<sup class="fn">.*?</sup>"#).unwrap()
    });
    static RE_PLAIN_BREAK: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)<br\s*/?>|</?p(?:\s[^>]*)?>|</(?:div|li|tr|td|th|h[1-6])\s*>|<hr(?:\s[^>]*)?/?>")
            .unwrap()
    });
    let s = RE_PLAIN_DROP.replace_all(htmlish, "");
    let s = RE_PLAIN_BREAK.replace_all(&s, " ");
    let s = RE_STRIP_TAGS.replace_all(&s, "");
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

#[cfg(test)]
mod legacy;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod golden_tests;
#[cfg(test)]
mod gate_tests;
#[cfg(test)]
mod qa_tests;
#[cfg(test)]
mod codepage_tests;
