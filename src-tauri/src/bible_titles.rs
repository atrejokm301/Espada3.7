//! Section titles inside Bible verse text («El pacto de Dios con Abraham», «Salmo de David.»),
//! ported from the Windows build (module-pipeline 2f0840f, `bible_titles.rs`): the detection is
//! the same; where Windows marks the titles in place, the Mac cuts them out of the verse
//! (`Titles::split`) and `modules::load_bible_chapter` sends them beside it.
//!
//! e-Sword Bibles have no heading table: a title travels inside a verse's text —
//! - HTML: a bold paragraph before the verse (RVR1960x, DHH, BTX, LBLA+…), or an italic line
//!   closed by `<br>` inside an open paragraph (RV1960's psalm titles);
//! - plain text: lines before the verse, after a leading line break (RV1960-T, NTV, PDT and
//!   80-odd more). Mid-chapter, RV1960-T puts the title at the END of the previous verse,
//!   after a blank line;
//! - RTF: `{\b …}\par` groups before the verse; the DHH in RTF also puts them after it.
//!
//! A misread line is at worst drawn as a title — never dropped: the page shows every title, and
//! the verse's words plus its titles' words are exactly the verse's words before (tests).

use once_cell::sync::Lazy;
use regex::Regex;

/// Longest line read as a title.
const MAX_TITLE_CHARS: usize = 90;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    /// A title of this verse, drawn above it.
    Lead,
    /// A title of this verse set in italics only (psalm titles): drawn lighter.
    LeadMinor,
    /// A title that belongs to the NEXT verse (RV1960-T and the RTF DHH put it at the end of this one).
    Next,
}

impl Kind {
    /// The name the page gets (`VerseTitle.kind`).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Kind::Lead => "lead",
            Kind::LeadMinor => "minor",
            Kind::Next => "next",
        }
    }
}

#[derive(Debug)]
struct Span {
    start: usize,
    end: usize,
    kind: Kind,
}

/// Titles found in one verse (byte ranges of its raw text).
#[derive(Debug, Default)]
pub(crate) struct Titles {
    spans: Vec<Span>,
}

impl Titles {
    pub(crate) fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Titles read from plain-text lines before the verse (the per-chapter guard counts them).
    pub(crate) fn has_text_lead(&self, raw: &str) -> bool {
        !raw.contains('<')
            && !raw.contains("{\\")
            && self.spans.iter().any(|s| s.kind != Kind::Next)
    }

    /// `raw` cut into its titles (raw text, in order) and the verse itself: `raw` without the
    /// titles, everything else kept byte for byte. Each piece is cleaned on its own.
    pub(crate) fn split<'a>(&self, raw: &'a str) -> (Vec<(Kind, &'a str)>, String) {
        let mut titles = Vec::with_capacity(self.spans.len());
        let mut body = String::with_capacity(raw.len());
        let mut at = 0;
        for s in &self.spans {
            body.push_str(&raw[at..s.start]);
            titles.push((s.kind, &raw[s.start..s.end]));
            at = s.end;
        }
        body.push_str(&raw[at..]);
        (titles, body)
    }
}

/// The titles of one verse. `text_lines`: also read plain-text lines (off when a chapter would
/// have too many — a module that starts every verse on a new line).
pub(crate) fn find_titles(raw: &str, text_lines: bool) -> Titles {
    let mut spans = Vec::new();
    let has_tag = raw.contains('<');
    let has_rtf = raw.contains("{\\");
    if has_tag {
        spans = html_lead(raw);
    } else if has_rtf {
        spans = rtf_lead(raw);
        spans.extend(rtf_next(raw, spans.last().map_or(0, |s| s.end)));
    } else if text_lines && (raw.contains('\n') || raw.contains('\r')) {
        spans = text_lead(raw);
        spans.extend(text_next(raw, spans.last().map_or(0, |s| s.end)));
    }
    Titles { spans }
}

// ----- shared helpers -----

static RE_TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());
static RE_ENTITY: Lazy<Regex> = Lazy::new(|| Regex::new(r"&#?[A-Za-z0-9]+;").unwrap());
static RE_RTF_CTRL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\\[a-zA-Z]+-?\d* ?|\\'[0-9a-fA-F]{2}|[{}]").unwrap());
static RE_BOLD_STYLE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"font-weight\s*:\s*(?:bold|[6-9]00)").unwrap());
static RE_ITALIC_STYLE: Lazy<Regex> = Lazy::new(|| Regex::new(r"font-style\s*:\s*italic").unwrap());
static RE_BLANK_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\r?\n[ \t\u{a0}]*\r?\n").unwrap());
static RE_RTF_BOLD: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\b(?:[\s\\]|$)").unwrap());

/// Visible text left after removing markup.
fn visible(s: &str) -> String {
    let t = RE_TAG.replace_all(s, " ");
    let t = RE_ENTITY.replace_all(&t, |c: &regex::Captures| {
        let e = &c[0];
        if e == "&nbsp;" || e == "&#160;" {
            " ".to_string()
        } else {
            html_escape::decode_html_entities(e).into_owned()
        }
    });
    RE_RTF_CTRL.replace_all(&t, " ").into_owned()
}

fn has_visible(s: &str) -> bool {
    visible(s).chars().any(char::is_alphanumeric)
}

// ----- HTML -----

/// `<b>`, `<strong>`, `<i>`, `<em>`, or a span / font set in bold or italics, at the start of
/// `s`: (name, bold, italic, length of the opening tag).
fn emphasis_open(s: &str) -> Option<(String, bool, bool, usize)> {
    if !s.starts_with('<') {
        return None;
    }
    let end = s.find('>')?;
    let tag = &s[1..end];
    let name: String = tag
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    let attrs = tag[name.len()..].to_ascii_lowercase();
    let (bold, italic) = match name.as_str() {
        "b" | "strong" => (true, false),
        "i" | "em" => (false, true),
        "span" | "font" => (RE_BOLD_STYLE.is_match(&attrs), RE_ITALIC_STYLE.is_match(&attrs)),
        _ => return None,
    };
    (bold || italic).then_some((name, bold, italic, end + 1))
}

/// `<p>`, `<div>` or `<h1>`…`<h6>` at the start of `s`: (name, attributes, opening length).
fn block_open(s: &str) -> Option<(String, String, usize)> {
    if !s.starts_with('<') {
        return None;
    }
    let end = s.find('>')?;
    let tag = &s[1..end];
    let name: String = tag
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    let is_block = name == "p"
        || name == "div"
        || (name.len() == 2 && name.starts_with('h') && matches!(name.as_bytes()[1], b'1'..=b'6'));
    is_block.then(|| (name.clone(), tag[name.len()..].to_ascii_lowercase(), end + 1))
}

/// Length of the element that opens at the start of `s` (through its matching closing tag),
/// and where that closing tag starts.
fn element_len(s: &str, name: &str) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut i = 0;
    while let Some(lt) = s[i..].find('<') {
        let at = i + lt;
        let gt = at + s[at..].find('>')?;
        let tag = &s[at + 1..gt];
        let closing = tag.starts_with('/');
        let tname: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        if tname == name {
            if closing {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((gt + 1, at));
                }
            } else if !tag.ends_with('/') {
                depth += 1;
            }
        }
        i = gt + 1;
    }
    None
}

/// Whitespace, `&nbsp;`, line breaks and stray closing tags at `pos`.
fn skip_noise(raw: &str, mut pos: usize) -> usize {
    loop {
        let rest = &raw[pos..];
        let trimmed = rest.trim_start();
        if trimmed.len() != rest.len() {
            pos += rest.len() - trimmed.len();
            continue;
        }
        let lower: String = rest.chars().take(8).collect::<String>().to_ascii_lowercase();
        if lower.starts_with("&nbsp;") {
            pos += 6;
            continue;
        }
        if lower.starts_with("<br") || lower.starts_with("</") {
            if let Some(gt) = rest.find('>') {
                pos += gt + 1;
                continue;
            }
        }
        return pos;
    }
}

fn starts_with_break(s: &str) -> bool {
    let l: String = s.chars().take(6).collect::<String>().to_ascii_lowercase();
    l.starts_with("<br") || l.starts_with("</p") || l.starts_with("</div")
}

/// Bold / italic present anywhere in a run.
fn emphasis_inside(s: &str) -> (bool, bool) {
    let l = s.to_ascii_lowercase();
    let bold = l.contains("<b>") || l.contains("<b ") || l.contains("<strong") || RE_BOLD_STYLE.is_match(&l);
    let italic = l.contains("<i>") || l.contains("<i ") || l.contains("<em") || RE_ITALIC_STYLE.is_match(&l);
    (bold, italic)
}

/// All the visible text of `inner` sits inside bold / italic runs.
fn all_emphasized(inner: &str) -> bool {
    let mut rest = String::new();
    let mut i = 0;
    while i < inner.len() {
        let s = &inner[i..];
        if let Some((name, _, _, _)) = emphasis_open(s) {
            if let Some((len, _)) = element_len(s, &name) {
                i += len;
                continue;
            }
        }
        let ch = s.chars().next().unwrap_or(' ');
        rest.push(ch);
        i += ch.len_utf8();
    }
    !has_visible(&rest)
}

/// An italic-only title («Salmo de David.», «Alef») against an italic poetry line: it sits in a
/// centred paragraph, each of its lines reads as a title, and plain verse text follows.
/// RVR1960x and rv60 set poetry in italics too — «Si siete veces será vengado Caín,» then
/// «Lamec…», or «Pozo, el cual cavaron los señores.» before the narrative — but never centred.
fn italic_title_ok(run: &str, rest: &str, centred: bool) -> bool {
    if !centred {
        return false;
    }
    let lines_ok = RE_BR
        .split(run)
        .map(|piece| visible(piece).split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
        .all(|t| lead_line(&t));
    lines_ok && has_visible(rest) && !all_emphasized(rest)
}

static RE_BR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
/// «(1 Cr.1.1–4)»: the parallel passage RVR1960x centres under a title.
static RE_PARALLEL_REF: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\([^()]{1,60}\)$").unwrap());

fn html_lead(raw: &str) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut pos = 0;
    // Inside a centred paragraph we stepped into (RV1960: centred <p>, italic title, <br>, verse).
    let mut in_centred = false;
    for _ in 0..12 {
        pos = skip_noise(raw, pos);
        let rest = &raw[pos..];
        if rest.is_empty() {
            break;
        }
        if let Some((name, attrs, open_len)) = block_open(rest) {
            if let Some((len, close_at)) = element_len(rest, &name) {
                let inner = &rest[open_len..close_at];
                let heading = name.starts_with('h');
                let bold_block = RE_BOLD_STYLE.is_match(&attrs);
                if (heading || bold_block || all_emphasized(inner)) && has_visible(inner) {
                    let (b, it) = emphasis_inside(inner);
                    let minor = it && !b && !heading && !bold_block;
                    if !minor || italic_title_ok(inner, &rest[len..], attrs.contains("center") || in_centred) {
                        spans.push(Span {
                            start: pos,
                            end: pos + len,
                            kind: if minor { Kind::LeadMinor } else { Kind::Lead },
                        });
                        pos += len;
                        continue;
                    }
                }
                let centred = attrs.contains("center");
                let text = visible(inner).split_whitespace().collect::<Vec<_>>().join(" ");
                if !spans.is_empty() && centred && RE_PARALLEL_REF.is_match(&text) && has_visible(&rest[len..]) {
                    spans.push(Span { start: pos, end: pos + len, kind: Kind::LeadMinor });
                    pos += len;
                    continue;
                }
            }
            // A paragraph that holds the verse: look inside it (RV1960 opens a centred
            // paragraph, then the italic psalm title, <br>, and the verse).
            in_centred = attrs.contains("center");
            pos += open_len;
            continue;
        }
        if let Some((name, bold, italic, _)) = emphasis_open(rest) {
            if let Some((len, _)) = element_len(rest, &name) {
                let after = &rest[len..];
                let after = &after[after.len() - after.trim_start().len()..];
                let after = after.strip_prefix("&nbsp;").unwrap_or(after).trim_start();
                if starts_with_break(after) && has_visible(&rest[..len]) {
                    let (b, it) = emphasis_inside(&rest[..len]);
                    let minor = (italic || it) && !(bold || b);
                    if !minor || italic_title_ok(&rest[..len], &rest[len..], in_centred) {
                        spans.push(Span {
                            start: pos,
                            end: pos + len,
                            kind: if minor { Kind::LeadMinor } else { Kind::Lead },
                        });
                        pos += len;
                        continue;
                    }
                }
            }
        }
        break;
    }
    // The verse itself has to follow; a verse that is all title is no title.
    match spans.last() {
        Some(last) if has_visible(&raw[last.end..]) => spans,
        _ => Vec::new(),
    }
}

// ----- RTF -----

/// Whitespace and `\par` breaks at `pos`.
fn skip_rtf_breaks(raw: &str, mut pos: usize) -> usize {
    loop {
        let rest = &raw[pos..];
        let t = rest.trim_start();
        pos += rest.len() - t.len();
        match t.strip_prefix("\\par") {
            Some(r) if !r.starts_with(|c: char| c.is_ascii_alphanumeric()) => pos += 4,
            _ => return pos,
        }
    }
}

/// A `{…}` group with no group inside, at the start of `s`: its length.
fn rtf_group(s: &str) -> Option<usize> {
    if !s.starts_with('{') {
        return None;
    }
    let close = s.find('}')?;
    (!s[1..close].contains('{')).then_some(close + 1)
}

/// A bold group with text: `{\b 2. Dios nos hace libres por la fe}`, `{\qc \b La creación\par}`.
fn rtf_title_group(g: &str) -> bool {
    RE_RTF_BOLD.is_match(g) && has_visible(g)
}

/// The lines of a group (`\par` breaks them): «Nombramiento de jueces», «(Dt. 1.9-18)».
fn rtf_lines(g: &str) -> Vec<String> {
    RE_RTF_PAR
        .split(g)
        .map(|piece| visible(piece).split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
        .collect()
}

static RE_RTF_PAR: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\par\b ?").unwrap());

/// Every line of a group reads as a title (`first`: the rule for its first line), the others
/// may be its parallel passage.
fn rtf_lines_ok(g: &str, first: fn(&str) -> bool) -> bool {
    let lines = rtf_lines(g);
    !lines.is_empty()
        && lines.iter().enumerate().all(|(i, l)| {
            if i == 0 {
                first(l)
            } else {
                first(l) || RE_PARALLEL_REF.is_match(l)
            }
        })
}

fn rtf_lead(raw: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut pos = 0;
    loop {
        pos = skip_rtf_breaks(raw, pos);
        let rest = &raw[pos..];
        let Some(len) = rtf_group(rest) else { break };
        let g = &rest[..len];
        // The title ends its line: `\par` inside the group, or right after it.
        let ends_line = g.contains("\\par") || rest[len..].trim_start().starts_with("\\par");
        if !rtf_title_group(g) || !ends_line || !rtf_lines_ok(g, lead_line) {
            break;
        }
        spans.push(Span { start: pos, end: pos + len, kind: Kind::Lead });
        pos += len;
    }
    match spans.last() {
        Some(last) if has_visible(&raw[last.end..]) => spans,
        _ => Vec::new(),
    }
}

/// Titles at the END of the verse, after a paragraph break: they belong to the next verse
/// (RV1960 .bblx `…con fe? \par \par {\qc \b El pacto de Dios con Abraham\par}`; the RTF DHH
/// `…Mesías.{\cf2\super [h]}\par \par {\b\i Nacimento de Jesús}`).
fn rtf_next(raw: &str, after: usize) -> Vec<Span> {
    let mut end = raw.trim_end().len();
    let mut spans = Vec::new();
    loop {
        let s = raw[..end].trim_end();
        let s = match s.strip_suffix("\\par") {
            Some(t) if t.ends_with('}') || t.trim_end().ends_with('}') => t.trim_end(),
            _ => s,
        };
        if !s.ends_with('}') {
            break;
        }
        let Some(open) = s.rfind('{') else { break };
        let g = &s[open..];
        if g[1..g.len() - 1].contains('}') || !rtf_title_group(g) {
            break;
        }
        if !rtf_lines_ok(g, next_line) {
            break;
        }
        spans.push(Span { start: open, end: s.len(), kind: Kind::Next });
        end = open;
    }
    spans.reverse();
    let Some(first) = spans.first().map(|s| s.start) else { return spans };
    // A paragraph break before them, and the verse before that.
    let before = raw[..first].trim_end();
    if first < after || !before.ends_with("\\par") || !has_visible(&raw[after..first]) {
        return Vec::new();
    }
    spans
}

// ----- plain text -----

/// Non-empty lines of `s` as trimmed byte ranges (offset by `base`).
fn content_lines(s: &str, base: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for piece in s.split('\n') {
        let line = piece.trim_end_matches('\r');
        let lead = line.len() - line.trim_start().len();
        let t = line.trim();
        if !t.is_empty() && t.chars().any(|c| !c.is_whitespace()) {
            let a = base + start + lead;
            out.push((a, a + t.len()));
        }
        start += piece.len() + 1;
    }
    out
}

/// A title line before the verse: short, opens in capitals or a number, and doesn't run on
/// («,» «;» «:») or ask («?» «!» — the first line of a psalm verse: «¿Con qué limpiará el joven…?»).
fn lead_line(t: &str) -> bool {
    let n = t.chars().count();
    if n == 0 || n > MAX_TITLE_CHARS || !t.chars().any(char::is_alphanumeric) {
        return false;
    }
    if t.ends_with([',', ';', ':', '?', '!']) {
        return false;
    }
    let first = t.trim_start_matches(['¿', '¡', '«', '"', '“', '(', '[']).chars().next();
    !matches!(first, Some(c) if c.is_lowercase())
}

/// A title after the verse (for the next one): short, two words or more, opens in capitals,
/// and no closing punctuation — the last line of a stanza ends in one, «Selah» is one word.
fn next_line(t: &str) -> bool {
    lead_line(t)
        && t.split_whitespace().count() >= 2
        && !t.ends_with(['.', ',', ';', ':', '?', '!', '»', '"', '”', '\'', ')', ']'])
        && t.chars().next().is_some_and(|c| c.is_uppercase() || c.is_ascii_digit() || "¿¡«".contains(c))
}

fn text_lead(raw: &str) -> Vec<Span> {
    let start = raw.len() - raw.trim_start_matches([' ', '\t', '\u{a0}']).len();
    if !raw[start..].starts_with(['\r', '\n']) {
        return Vec::new();
    }
    let lines = content_lines(raw, 0);
    let mut spans = Vec::new();
    for (k, &(a, b)) in lines.iter().enumerate() {
        // An indented line is the verse: RV1960-T opens it with a space after its titles
        // («Los filisteos capturan el arca» / « Y Samuel habló a todo Israel.» / «Por aquel…»).
        let indented = a > 0 && matches!(raw.as_bytes()[a - 1], b' ' | b'\t');
        if k + 1 == lines.len() || indented || !lead_line(&raw[a..b]) {
            break;
        }
        spans.push(Span { start: a, end: b, kind: Kind::Lead });
    }
    spans
}

fn text_next(raw: &str, after: usize) -> Vec<Span> {
    let body_end = raw.trim_end().len();
    let body = &raw[..body_end];
    let Some(blank) = RE_BLANK_LINE.find_iter(body).last() else { return Vec::new() };
    if blank.start() < after || !has_visible(&raw[after..blank.start()]) {
        return Vec::new();
    }
    let lines = content_lines(&body[blank.end()..], blank.end());
    // A title, and maybe its parallel passage under it: «Nombramiento de jueces» «(Dt. 1.9-18)».
    let ok = match lines.as_slice() {
        [(a, b)] => next_line(&raw[*a..*b]),
        [(a, b), (c, d)] => next_line(&raw[*a..*b]) && RE_PARALLEL_REF.is_match(raw[*c..*d].trim()),
        _ => false,
    };
    if !ok {
        return Vec::new();
    }
    lines.into_iter().map(|(a, b)| Span { start: a, end: b, kind: Kind::Next }).collect()
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{clean_bible_text, to_plain};

    fn plain(raw: &str) -> String {
        to_plain(&clean_bible_text(raw))
    }

    /// (kind, text) of every title of a verse, as the page shows it.
    fn spans_shown(raw: &str) -> Vec<(Kind, String)> {
        let t = find_titles(raw, true);
        t.split(raw).0.into_iter().map(|(k, r)| (k, plain(r))).collect()
    }

    /// The page's words: runs of Hebrew, of Greek, or of Latin letters and digits.
    fn page_words(text: &str) -> Vec<String> {
        fn class(c: char) -> u8 {
            let u = c as u32;
            if (0x0590..=0x05FF).contains(&u) || (0xFB1D..=0xFB4F).contains(&u) {
                1
            } else if (0x0370..=0x03FF).contains(&u) || (0x1F00..=0x1FFF).contains(&u) {
                2
            } else if c.is_alphanumeric() {
                3
            } else {
                0
            }
        }
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut cur_class = 0u8;
        for c in text.chars() {
            let k = class(c);
            if k != cur_class && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            if k != 0 {
                cur.push(c);
            }
            cur_class = k;
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    }

    /// The titles' words and the verse's words, in order, are exactly the verse's words before:
    /// nothing dropped, nothing doubled.
    pub(crate) fn same_words(raw: &str) {
        let t = find_titles(raw, true);
        let (titles, body) = t.split(raw);
        let part = |next: bool| -> Vec<String> {
            titles.iter().filter(|(k, _)| (*k == Kind::Next) == next).map(|(_, r)| plain(r)).collect()
        };
        let joined = format!("{} {} {}", part(false).join(" "), plain(&body), part(true).join(" "));
        assert_eq!(page_words(&joined), page_words(&plain(raw)), "words moved: {raw:?}");
    }

    use Kind::{Lead, LeadMinor as Minor, Next};

    fn kinds(v: &[(Kind, &str)]) -> Vec<(Kind, String)> {
        v.iter().map(|(k, t)| (*k, t.to_string())).collect()
    }

    #[test]
    fn plain_text_titles_before_and_after() {
        let gal31 = " \r\n\r\nEl Espíritu se recibe por la fe\r\n ¡Oh gálatas insensatos! ¿quién os fascinó?\r\n";
        assert_eq!(spans_shown(gal31), kinds(&[(Lead, "El Espíritu se recibe por la fe")]));
        let gal35 = " Aquel, pues, ¿lo hace por las obras de la ley, o por el oír con fe? \r\n\r\nEl pacto de Dios con Abraham\r\n";
        assert_eq!(spans_shown(gal35), kinds(&[(Next, "El pacto de Dios con Abraham")]));
        let ex1812 = " Y tomó Jetro, suegro de Moisés, holocaustos delante de Dios. \n\nNombramiento de jueces\n(Dt. 1.9-18)\n";
        assert_eq!(spans_shown(ex1812), kinds(&[(Next, "Nombramiento de jueces"), (Next, "(Dt. 1.9-18)")]));
        let sam41 = " \n\nLos filisteos capturan el arca\n Y Samuel habló a todo Israel. \nPor aquel tiempo salió Israel a encontrar en batalla a los filisteos.\n";
        assert_eq!(spans_shown(sam41), kinds(&[(Lead, "Los filisteos capturan el arca")]));
        let ps119_9 = " \r\nBet\r\n¿Con qué limpiará el joven su camino? \r\nCon guardar tu palabra.\r\n";
        assert_eq!(spans_shown(ps119_9), kinds(&[(Lead, "Bet")]));
        let ps119_1 = " \r\n\r\nExcelencias de la ley de Dios\r\n\r\nAlef\r\nBienaventurados los perfectos de camino, \r\nLos que andan en la ley de Jehová.\r\n";
        assert_eq!(spans_shown(ps119_1), kinds(&[(Lead, "Excelencias de la ley de Dios"), (Lead, "Alef")]));
        let ps23_1 = " \r\n\r\nJehová es mi pastor\r\nSalmo de David.\r\nJehová es mi pastor; nada me faltará.\r\n";
        assert_eq!(spans_shown(ps23_1), kinds(&[(Lead, "Jehová es mi pastor"), (Lead, "Salmo de David.")]));
        for raw in [gal31, gal35, ex1812, sam41, ps119_9, ps119_1, ps23_1] {
            same_words(raw);
        }
    }

    #[test]
    fn poetry_and_selah_are_not_titles() {
        // No leading line break: a psalm verse in two lines.
        assert!(spans_shown(" En lugares de delicados pastos me hará descansar; \r\nJunto a aguas de reposo me pastoreará.\r\n").is_empty());
        assert!(spans_shown("(Melodía de David.)\r\n Jehová es mi Pastor.  Nada me faltará. \r\n").is_empty());
        // A closing line of a stanza after a blank line ends in punctuation; «Selah» is one word.
        assert!(spans_shown(" Muchos dicen de mi alma: \r\n\r\nNo hay para él salvación en Dios.\r\n").is_empty());
        assert!(spans_shown(" Muchos dicen de mi alma: No hay salvación para él. \r\n\r\nSelah\r\n").is_empty());
        // HTML: «Selah» right-aligned AFTER the verse stays in the verse.
        assert!(spans_shown("Muchos dicen de mi alma:</p> No hay salvación para él en Elohim.</p> <p align=\"right\"><i>Selah</i></p>").is_empty());
        // A verse that is only a title-like line is no title.
        assert!(spans_shown("<p align=\"center\"><b>Solo esto</b></p>").is_empty());
    }

    #[test]
    fn html_titles() {
        let rvr = "<p align=\"center\"><span style=\"font-weight:bold;\">El pacto de Dios con Abraham</span></p> Así Abraham creyó a Dios, y le fue contado por justicia.<span style=\"color:#804DB3;\"><sup>a</sup></span>";
        assert_eq!(spans_shown(rvr), kinds(&[(Lead, "El pacto de Dios con Abraham")]));
        let dhh = "<p align=\"center\"><b>2. Dios nos hace libres por la fe (3.1—5.12)</b><br/><i><b>La experiencia cristiana del Espíritu</b></i></p> ¡Gálatas, duros para entender!";
        let got = spans_shown(dhh);
        assert_eq!(got.len(), 1);
        assert!(got[0].1.starts_with("2. Dios nos hace libres") && got[0].1.ends_with("del Espíritu"), "{got:?}");
        let rv1960_ps = "&nbsp;</p><p style=\"text-align:center;\"><i>Salmo de David.</i><br /> Jehová es mi pastor; nada me faltará.";
        assert_eq!(spans_shown(rv1960_ps), kinds(&[(Minor, "Salmo de David.")]));
        let btih = "<p style=\"text-align:center;font-weight:bold;\">II.— SALVADOS POR LA GRACIA</p><p style=\"text-align:center;font-weight:bold;\">Salvados por la fe</p> Gálatas, ¿cómo son tan insensatos?";
        assert_eq!(spans_shown(btih).len(), 2);
        // Italic poetry (RVR1960x, rv60) is no title; an italic psalm title before plain text is.
        assert!(spans_shown("<span style=\"font-style:italic;\">Si siete veces será vengado Caín,</span></p><span style=\"font-style:italic;\">Lamec en verdad setenta veces siete lo será.</span>").is_empty());
        assert!(spans_shown("<i>Bienaventurados los perfectos de camino,</i> </p><p lang=\"en-US\" style=\"\"> <i>Los que andan en la ley de Jehová.</i>").is_empty());
        assert!(spans_shown("<i>Bendito el varón que confía en Jehová.</i><br/><i>Y cuya confianza es Jehová.</i>").is_empty());
        let gen51 = "<p align=\"center\"><span style=\"font-weight:bold;\">Los descendientes de Adán</span></p><p align=\"center\">(1 Cr.1.1–4)</p> Este es el libro de las generaciones de Adán.";
        assert_eq!(spans_shown(gen51), kinds(&[(Lead, "Los descendientes de Adán"), (Minor, "(1 Cr.1.1–4)")]));
        // A bold first word inside the verse is not a title.
        assert!(spans_shown("<b>Jehová</b> es mi pastor; nada me faltará.").is_empty());
        for raw in [rvr, dhh, rv1960_ps, btih, gen51] {
            same_words(raw);
        }
    }

    #[test]
    fn rtf_titles_before_and_after() {
        let gal = " \\par {\\b 2. Dios nos hace libres por la fe}\\par {\\b\\i La experiencia cristiana del Espíritu}\\par ¡Gálatas insensatos!, ¿quién os embrujó?{\\cf2\\super [a]}\r ";
        assert_eq!(
            spans_shown(gal),
            kinds(&[(Lead, "2. Dios nos hace libres por la fe"), (Lead, "La experiencia cristiana del Espíritu")])
        );
        let mat = "hasta el nacimiento del Mesías.{\\cf2\\super [h]}\\par \\par {\\b\\i Nacimento de Jesús}";
        assert_eq!(spans_shown(mat), kinds(&[(Next, "Nacimento de Jesús")]));
        let gal31 = " \\par \\par {\\qc \\b El Espíritu se recibe por la fe\\par}  ¡Oh gálatas insensatos! ¿quién os fascinó?\r ";
        assert_eq!(spans_shown(gal31), kinds(&[(Lead, "El Espíritu se recibe por la fe")]));
        let gal35 = " Aquel, pues, ¿lo hace por las obras de la ley, o por el oír con fe? \\par \\par {\\qc \\b El pacto de Dios con Abraham\\par}\r ";
        assert_eq!(spans_shown(gal35), kinds(&[(Next, "El pacto de Dios con Abraham")]));
        let ex1812 = " Y tomó Jetro, suegro de Moisés, holocaustos delante de Dios. \\par \\par {\\qc \\b Nombramiento de jueces\\par (Dt. 1.9-18)\\par}\r ";
        let got = spans_shown(ex1812);
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(got[0].0 == Next && got[0].1 == "Nombramiento de jueces (Dt. 1.9-18)", "{got:?}");
        let gal36 = " Así Abraham creyó a Dios, y le fue contado por justicia.{\\super\\cf6 (A)}\r ";
        assert!(spans_shown(gal36).is_empty());
        for raw in [gal, mat, gal31, gal35, ex1812, gal36] {
            same_words(raw);
        }
    }

    #[test]
    fn split_keeps_every_byte_outside_the_titles() {
        let raw = "<p align=\"center\"><b>Título</b></p> Verso.";
        let t = find_titles(raw, true);
        let (titles, body) = t.split(raw);
        assert_eq!(titles.len(), 1);
        assert_eq!(format!("{}{}", titles[0].1, body), raw);
        let none = find_titles("Así Abraham creyó.", true);
        assert!(none.is_empty());
        assert_eq!(none.split("Así Abraham creyó."), (Vec::new(), "Así Abraham creyó.".to_string()));
    }
}
