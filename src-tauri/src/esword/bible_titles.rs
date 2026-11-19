//! Section titles inside Bible verse text («El pacto de Dios con Abraham», «Salmo de David.»).
//!
//! e-Sword Bibles have no heading table: a title travels inside a verse's text —
//! - HTML: a bold paragraph before the verse (RVR1960x, DHH, BTX, LBLA+…), or an italic line
//!   closed by `<br>` inside an open paragraph (RV1960's psalm titles). reina_valera_1960.bbli,
//!   rv60a.bbli and the DHH 2002 .bbli put mid-chapter titles in bold paragraphs at the END of
//!   the previous verse;
//! - plain text: lines before the verse, after a leading line break (RV1960-T, NTV, PDT and
//!   80-odd more). Mid-chapter, RV1960-T puts the title at the END of the previous verse,
//!   after a blank line. rv1960.bbli frames them in asterisks instead: «* *   Título*»;
//! - RTF: `{\b …}\par` groups before the verse; the DHH in RTF also puts them after it, and
//!   Reina Valera 1960.bblx centres them in a paragraph of their own (`\qc {\par \b …}`).
//!
//! The Bible tab used to show them as the verse's first (or last) words. `mark_titles` wraps
//! them in markers IN PLACE: the text and its words stay exactly as before, so word marks
//! (keyed by word number, shared by every Bible) and the Strong's lookup (which counts words)
//! don't move, and a misread line is at worst drawn as a title — never dropped from the verse.
//! The page draws a marked span above its verse, and a next-verse span above the next verse.

use once_cell::sync::Lazy;
use regex::Regex;

/// A title of this verse.
pub const HEAD_OPEN: char = '\u{E005}';
/// A title of this verse set in italics only (psalm titles): drawn lighter.
pub const HEAD_MINOR: char = '\u{E008}';
/// A title that belongs to the NEXT verse (RV1960-T and the RTF DHH put it at the end of this one).
pub const HEAD_NEXT: char = '\u{E007}';
pub const HEAD_CLOSE: char = '\u{E006}';

/// Longest line read as a title.
const MAX_TITLE_CHARS: usize = 90;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Lead,
    LeadMinor,
    Next,
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
    /// Asterisks that only frame a title (rv1960.bbli «* *   Dios llama a Abram*»): shown as
    /// spaces. An asterisk is never part of a word, so the word numbers don't move.
    blanks: Vec<usize>,
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

    /// `raw` with the markers inserted (clean it with `clean_text`).
    pub(crate) fn mark(&self, raw: &str) -> String {
        let blanked;
        let raw = if self.blanks.is_empty() {
            raw
        } else {
            let mut b = raw.as_bytes().to_vec();
            for &i in &self.blanks {
                if b.get(i) == Some(&b'*') {
                    b[i] = b' ';
                }
            }
            blanked = String::from_utf8(b).unwrap_or_else(|_| raw.to_string());
            blanked.as_str()
        };
        let mut out = String::with_capacity(raw.len() + self.spans.len() * 6);
        let mut at = 0;
        for s in &self.spans {
            out.push_str(&raw[at..s.start]);
            out.push(match s.kind {
                Kind::Lead => HEAD_OPEN,
                Kind::LeadMinor => HEAD_MINOR,
                Kind::Next => HEAD_NEXT,
            });
            out.push_str(&raw[s.start..s.end]);
            out.push(HEAD_CLOSE);
            at = s.end;
        }
        out.push_str(&raw[at..]);
        out
    }
}

/// The titles of one verse. `text_lines`: also read plain-text lines (off when a chapter would
/// have too many — a module that starts every verse on a new line).
pub(crate) fn find_titles(raw: &str, text_lines: bool) -> Titles {
    let mut spans = Vec::new();
    let mut blanks = Vec::new();
    let has_tag = raw.contains('<');
    let has_rtf = raw.contains("{\\");
    if has_tag {
        spans = html_lead(raw);
        spans.extend(html_next(raw, spans.last().map_or(0, |s| s.end)));
    } else if has_rtf {
        spans = rtf_lead(raw);
        spans.extend(rtf_next(raw, spans.last().map_or(0, |s| s.end)));
    } else if text_lines {
        if raw.contains('*') {
            spans = star_lead(raw, &mut blanks);
            spans.extend(star_next(raw, spans.last().map_or(0, |s| s.end), &mut blanks));
        }
        if spans.is_empty() && (raw.contains('\n') || raw.contains('\r')) {
            spans = text_lead(raw);
            spans.extend(text_next(raw, spans.last().map_or(0, |s| s.end)));
        }
    }
    Titles { spans, blanks }
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

/// One piece of a verse's HTML, read from the start (`html_next` reads them from the end).
struct Item<'a> {
    start: usize,
    end: usize,
    kind: ItemKind<'a>,
}

enum ItemKind<'a> {
    /// `<p>…</p>`, `<div>…</div>`, `<h2>…</h2>`.
    Block { attrs: String, name: String, inner: &'a str },
    /// A paragraph opened and left open (the next verse closes it).
    Open { attrs: String },
    /// `<b>…</b>`, `<i>…</i>` or a bold / italic span.
    Emph,
    /// Any other tag.
    Tag,
    Text,
}

fn html_items(raw: &str, from: usize) -> Vec<Item<'_>> {
    let mut items = Vec::new();
    let mut pos = from;
    while pos < raw.len() {
        let rest = &raw[pos..];
        if !rest.starts_with('<') {
            let len = rest.find('<').unwrap_or(rest.len());
            items.push(Item { start: pos, end: pos + len, kind: ItemKind::Text });
            pos += len;
            continue;
        }
        if let Some((name, attrs, open_len)) = block_open(rest) {
            match element_len(rest, &name) {
                Some((len, close_at)) => {
                    let inner = &rest[open_len..close_at];
                    items.push(Item { start: pos, end: pos + len, kind: ItemKind::Block { attrs, name, inner } });
                    pos += len;
                }
                None => {
                    items.push(Item { start: pos, end: pos + open_len, kind: ItemKind::Open { attrs } });
                    pos += open_len;
                }
            }
            continue;
        }
        if let Some((name, _, _, _)) = emphasis_open(rest) {
            if let Some((len, _)) = element_len(rest, &name) {
                items.push(Item { start: pos, end: pos + len, kind: ItemKind::Emph });
                pos += len;
                continue;
            }
        }
        let Some(gt) = rest.find('>') else { break };
        items.push(Item { start: pos, end: pos + gt + 1, kind: ItemKind::Tag });
        pos += gt + 1;
    }
    items
}

/// A one-word centred heading: the letters of Psalm 119 («Bet», «Guímel»). Not «Selah».
fn one_word_heading(t: &str) -> bool {
    let n = t.chars().count();
    (2..=8).contains(&n)
        && t.chars().all(char::is_alphabetic)
        && t.chars().next().is_some_and(char::is_uppercase)
        && !["selah", "amén", "amen", "aleluya"].contains(&t.to_lowercase().as_str())
}

/// The lines of a block (`<br>` breaks them), as shown.
fn block_lines(s: &str) -> Vec<String> {
    RE_BR
        .split(s)
        .map(|piece| visible(piece).split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Titles in bold paragraphs at the END of the verse: they belong to the next verse
/// (reina_valera_1960.bbli, rv60a.bbli: `…con fe? </p><p>&nbsp;</p><p style="text-align:center;
/// font-weight:bold;">El pacto de Dios con Abraham</p><p>&nbsp;`; the DHH 2002 .bbli:
/// `…vosotros.</p><p>&nbsp;</p><p><b><i>Sal y luz del mundo</i></b>`).
fn html_next(raw: &str, after: usize) -> Vec<Span> {
    let items = html_items(raw, after);
    let noise = |it: &Item| match &it.kind {
        ItemKind::Block { inner, .. } => !has_visible(inner),
        ItemKind::Open { .. } | ItemKind::Tag => true,
        ItemKind::Text | ItemKind::Emph => !has_visible(&raw[it.start..it.end]),
    };
    // (start, end, centred, lines), from the last one
    let mut found: Vec<(usize, usize, bool, Vec<String>)> = Vec::new();
    let mut i = items.len();
    // Where the first title (or its open paragraph) sits among the items.
    let mut first_item = items.len();
    loop {
        while i > 0 && noise(&items[i - 1]) {
            i -= 1;
        }
        if i == 0 {
            break;
        }
        let it = &items[i - 1];
        match &it.kind {
            ItemKind::Block { attrs, name, inner } => {
                let (b, _) = emphasis_inside(inner);
                let bold = name.starts_with('h') || RE_BOLD_STYLE.is_match(attrs) || (b && all_emphasized(inner));
                if !bold {
                    break;
                }
                found.push((it.start, it.end, attrs.contains("center"), block_lines(inner)));
                i -= 1;
                first_item = i;
            }
            ItemKind::Emph => {
                // In a paragraph of its own: the open <p> right before it.
                let (b, _) = emphasis_inside(&raw[it.start..it.end]);
                let mut j = i - 1;
                while j > 0 && matches!(items[j - 1].kind, ItemKind::Text) && noise(&items[j - 1]) {
                    j -= 1;
                }
                let Some(ItemKind::Open { attrs }) = j.checked_sub(1).map(|k| &items[k].kind) else { break };
                if !b {
                    break;
                }
                found.push((it.start, it.end, attrs.contains("center"), block_lines(&raw[it.start..it.end])));
                i = j - 1;
                first_item = i;
            }
            _ => break,
        }
    }
    found.reverse();
    let Some(first) = found.first().map(|f| f.0) else { return Vec::new() };
    // The verse comes first and ends a sentence (not «…tiene escrito este nombre:» before an
    // inscription in capitals), then an empty paragraph sets the title apart.
    let verse = visible(&raw[after..first]);
    let verse = verse.trim_end();
    let blank = items[i..first_item].iter().any(|it| matches!(it.kind, ItemKind::Block { .. }));
    if !has_visible(verse) || verse.ends_with([':', ',', ';']) || !blank {
        return Vec::new();
    }
    for (k, (_, _, centred, lines)) in found.iter().enumerate() {
        for (n, l) in lines.iter().enumerate() {
            let ok = next_line(l)
                || (*centred && one_word_heading(l))
                // under the title: its parallel passage, «(Dt. 1.9-18)»
                || ((k > 0 || n > 0) && RE_PARALLEL_REF.is_match(l));
            if !ok {
                return Vec::new();
            }
        }
    }
    found.into_iter().map(|(start, end, _, _)| Span { start, end, kind: Kind::Next }).collect()
}

// ----- RTF -----

/// RTF words that only break, reset or centre a paragraph (`\pard` before `\par`).
const RTF_LAYOUT: [&str; 6] = ["\\pard", "\\par", "\\qc", "\\ql", "\\qr", "\\qj"];

/// Whitespace, `\par` breaks and paragraph layout at `pos` (Reina Valera 1960.bblx opens a
/// title with ` \pard  \pard\par \qc {\par \b\cf16 Dios llama a Abram}`).
fn skip_rtf_breaks(raw: &str, mut pos: usize) -> usize {
    'next: loop {
        let rest = &raw[pos..];
        let t = rest.trim_start();
        pos += rest.len() - t.len();
        for w in RTF_LAYOUT {
            if let Some(r) = t.strip_prefix(w) {
                if !r.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                    pos += w.len();
                    continue 'next;
                }
            }
        }
        return pos;
    }
}

/// `s` without the paragraph layout and spaces at its end (`keep_par`: stop at a `\par`).
fn trim_rtf_layout_end(s: &str, keep_par: bool) -> &str {
    let mut s = s.trim_end();
    'next: loop {
        for w in RTF_LAYOUT {
            if keep_par && w == "\\par" {
                continue;
            }
            if let Some(t) = s.strip_suffix(w) {
                s = t.trim_end();
                continue 'next;
            }
        }
        return s;
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
/// `…Mesías.{\cf2\super [h]}\par \par {\b\i Nacimento de Jesús}`; Reina Valera 1960.bblx
/// `…con fe? \par  \pard\par \qc {\par \b\cf16 El pacto de Dios con Abraham}\par\pard`, its
/// parallel passage in a group of its own).
fn rtf_next(raw: &str, after: usize) -> Vec<Span> {
    let mut end = raw.len();
    // (start, end, centred), from the last one
    let mut groups: Vec<(usize, usize, bool)> = Vec::new();
    loop {
        let s = trim_rtf_layout_end(&raw[..end], false);
        if !s.ends_with('}') {
            break;
        }
        let Some(open) = s.rfind('{') else { break };
        let g = &s[open..];
        if g[1..g.len() - 1].contains('}') || !rtf_title_group(g) {
            break;
        }
        let pre = &raw[..open];
        let layout = &pre[trim_rtf_layout_end(pre, false).len()..];
        groups.push((open, s.len(), g.contains("\\qc") || layout.contains("\\qc")));
        end = open;
    }
    groups.reverse();
    let Some(first) = groups.first().map(|g| g.0) else { return Vec::new() };
    // A blank line before them (two breaks), and before that the verse, ending a sentence.
    let before = trim_rtf_layout_end(&raw[..first], true);
    let blank = before
        .strip_suffix("\\par")
        .is_some_and(|b| trim_rtf_layout_end(b, true).ends_with("\\par"));
    let verse = visible(&raw[after.min(first)..first]);
    let verse = verse.trim_end();
    if first < after || !blank || !has_visible(verse) || verse.ends_with([':', ',', ';']) {
        return Vec::new();
    }
    for (k, &(a, b, centred)) in groups.iter().enumerate() {
        let g = &raw[a..b];
        let lines = rtf_lines(g);
        let ok = rtf_lines_ok(g, next_line)
            || (k == 0 && centred && lines.len() == 1 && one_word_heading(&lines[0]))
            || (k > 0 && !lines.is_empty() && lines.iter().all(|l| RE_PARALLEL_REF.is_match(l)));
        if !ok {
            return Vec::new();
        }
    }
    groups.into_iter().map(|(start, end, _)| Span { start, end, kind: Kind::Next }).collect()
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
        && !t.split_whitespace().last().is_some_and(|w| w.eq_ignore_ascii_case("selah"))
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

// ----- asterisks (rv1960.bbli) -----
//
// « * *   Dios llama a Abram*  Pero Jehová…», « * *   LIBRO I * El justo y los pecadores*  …»,
// « **  Arrepentimiento…* Al músico principal. Salmo de David…*\r\n Ten piedad…», and at the end
// of a verse «…con fe? * *   El pacto de Dios con Abraham*». A single «*» inside a verse is
// its line break (poetry) and other Bibles use «*» for notes, so only the «* *» opener counts.

static RE_STAR_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\*[ \t]*\*").unwrap());
static RE_STAR_NEXT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\*)[ \t]*(\*)[ \t]*([^*\r\n]+?)[ \t]*(\*)(?:[ \t]*([^*\r\n]+?)[ \t]*(\*))?[ \t]*$").unwrap()
});

/// A title doesn't end on these (a verse cut by note marks: «… conozcamos ſu * modeﬅia»).
fn ends_on_small_word(t: &str) -> bool {
    const SMALL: [&str; 24] = [
        "y", "e", "o", "u", "ni", "de", "del", "la", "el", "los", "las", "lo", "en", "a", "al", "que", "con",
        "por", "para", "su", "sus", "ſu", "un", "una",
    ];
    t.split_whitespace().last().is_some_and(|w| SMALL.contains(&w.to_lowercase().as_str()))
}

/// A psalm's heading under its title: «Al músico principal. Salmo de David, cuando…».
fn psalm_heading(t: &str) -> bool {
    const FIRST: [&str; 15] = [
        "Salmo", "Al", "Oración", "Masquil", "Mictam", "Cántico", "Canción", "Sigaión", "Himno", "Plegaria",
        "Poema", "De", "Para", "Alabanza", "Testimonio",
    ];
    t.chars().count() <= 250
        && t.ends_with('.')
        && t.split_whitespace().next().is_some_and(|w| FIRST.contains(&w.trim_end_matches([',', '.', ';'])))
}

fn star_lead(raw: &str, blanks: &mut Vec<usize>) -> Vec<Span> {
    let lead = raw.len() - raw.trim_start().len();
    let Some(m) = RE_STAR_OPEN.find(&raw[lead..]) else { return Vec::new() };
    let mut stars = vec![lead, lead + m.end() - 1];
    let mut pos = lead + m.end();
    let mut spans: Vec<Span> = Vec::new();
    // The asterisk before this piece stood apart from the one before it («LIBRO I * El justo y
    // los pecadores*»): a second title. A closing «Sion*» is followed by the verse.
    let mut spaced = false;
    loop {
        let rest = &raw[pos..];
        let t = rest.trim_start_matches([' ', '\t', '\u{a0}']);
        pos += rest.len() - t.len();
        if t.starts_with('*') {
            stars.push(pos);
            pos += 1;
            continue;
        }
        let Some(rel) = t.find('*') else { break };
        let piece = t[..rel].trim_end();
        if piece.is_empty() || piece.contains(['\r', '\n']) {
            break;
        }
        let kind = if spans.is_empty() {
            (lead_line(piece) && !ends_on_small_word(piece)).then_some(Kind::Lead)
        } else if spaced && next_line(piece) && !ends_on_small_word(piece) {
            Some(Kind::Lead)
        } else if RE_PARALLEL_REF.is_match(piece) || psalm_heading(piece) {
            Some(Kind::LeadMinor)
        } else {
            None
        };
        let Some(kind) = kind else { break };
        spans.push(Span { start: pos, end: pos + piece.len(), kind });
        spaced = piece.len() < rel;
        pos += rel;
    }
    // The verse follows, and starts a sentence.
    let verse = raw[pos..].trim_start();
    if spans.is_empty() || !has_visible(verse) || verse.chars().next().is_some_and(char::is_lowercase) {
        return Vec::new();
    }
    blanks.extend(stars);
    spans
}

fn star_next(raw: &str, after: usize, blanks: &mut Vec<usize>) -> Vec<Span> {
    let body = raw.trim_end();
    if !body.ends_with('*') {
        return Vec::new();
    }
    let from = after.min(body.len());
    let Some(c) = RE_STAR_NEXT.captures(&body[from..]) else { return Vec::new() };
    let at = |i: usize| c.get(i).map(|m| (from + m.start(), from + m.end()));
    let (Some(s1), Some(s2), Some((ta, tb)), Some(s4)) = (at(1), at(2), at(3), at(4)) else {
        return Vec::new();
    };
    let title = &raw[ta..tb];
    if !lead_line(title) || ends_on_small_word(title) || !has_visible(&raw[from..s1.0]) {
        return Vec::new();
    }
    let mut spans = vec![Span { start: ta, end: tb, kind: Kind::Next }];
    let mut stars = vec![s1.0, s2.0, s4.0];
    if let (Some((ra, rb)), Some(s6)) = (at(5), at(6)) {
        if !RE_PARALLEL_REF.is_match(&raw[ra..rb]) {
            return Vec::new();
        }
        spans.push(Span { start: ra, end: rb, kind: Kind::Next });
        stars.push(s6.0);
    }
    blanks.extend(stars);
    spans
}

/// The page's words (app.js `tokenizeWords`): runs of Hebrew, of Greek, or of Latin letters and
/// digits. Marks and the Strong's lookup count these.
#[cfg(test)]
pub(crate) fn page_words(text: &str) -> Vec<String> {
    fn class(c: char) -> u8 {
        let u = c as u32;
        if (0x0590..=0x05FF).contains(&u) || (0xFB1D..=0xFB4F).contains(&u) {
            1
        } else if (0x0370..=0x03FF).contains(&u) || (0x1F00..=0x1FFF).contains(&u) {
            2
        } else if c.is_ascii_alphanumeric() || "ÁÉÍÓÚÜÑáéíóúüñ".contains(c) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esword::text::clean_text;

    /// (kind, text) of every marked span after cleaning, as the page will see it.
    fn spans_shown(raw: &str) -> Vec<(char, String)> {
        let t = find_titles(raw, true);
        let shown = clean_text(&t.mark(raw));
        let mut out = Vec::new();
        let mut open: Option<char> = None;
        let mut cur = String::new();
        for c in shown.chars() {
            match c {
                HEAD_OPEN | HEAD_MINOR | HEAD_NEXT => {
                    open = Some(c);
                    cur.clear();
                }
                HEAD_CLOSE => {
                    if let Some(k) = open.take() {
                        out.push((k, cur.split_whitespace().collect::<Vec<_>>().join(" ")));
                    }
                }
                _ if open.is_some() => cur.push(c),
                _ => {}
            }
        }
        out
    }

    fn same_words(raw: &str) {
        let t = find_titles(raw, true);
        let marked = clean_text(&t.mark(raw));
        let stripped: String = marked.chars().filter(|c| !matches!(*c as u32, 0xE005..=0xE008)).collect();
        assert_eq!(page_words(&stripped), page_words(&clean_text(raw)), "words moved: {raw:?}");
    }

    #[test]
    fn plain_text_titles_before_and_after() {
        let gal31 = " \r\n\r\nEl Espíritu se recibe por la fe\r\n ¡Oh gálatas insensatos! ¿quién os fascinó?\r\n";
        assert_eq!(spans_shown(gal31), [(HEAD_OPEN, "El Espíritu se recibe por la fe".into())]);
        let gal35 = " Aquel, pues, ¿lo hace por las obras de la ley, o por el oír con fe? \r\n\r\nEl pacto de Dios con Abraham\r\n";
        assert_eq!(spans_shown(gal35), [(HEAD_NEXT, "El pacto de Dios con Abraham".into())]);
        let ex1812 = " Y tomó Jetro, suegro de Moisés, holocaustos delante de Dios. 

Nombramiento de jueces
(Dt. 1.9-18)
";
        assert_eq!(
            spans_shown(ex1812),
            [(HEAD_NEXT, "Nombramiento de jueces".into()), (HEAD_NEXT, "(Dt. 1.9-18)".into())]
        );
        same_words(ex1812);
        let sam41 = " 

Los filisteos capturan el arca
 Y Samuel habló a todo Israel. 
Por aquel tiempo salió Israel a encontrar en batalla a los filisteos.
";
        assert_eq!(spans_shown(sam41), [(HEAD_OPEN, "Los filisteos capturan el arca".into())]);
        same_words(sam41);
        let ps119_9 = " \r\nBet\r\n¿Con qué limpiará el joven su camino? \r\nCon guardar tu palabra.\r\n";
        assert_eq!(spans_shown(ps119_9), [(HEAD_OPEN, "Bet".into())]);
        let ps119_1 = " \r\n\r\nExcelencias de la ley de Dios\r\n\r\nAlef\r\nBienaventurados los perfectos de camino, \r\nLos que andan en la ley de Jehová.\r\n";
        assert_eq!(
            spans_shown(ps119_1),
            [(HEAD_OPEN, "Excelencias de la ley de Dios".into()), (HEAD_OPEN, "Alef".into())]
        );
        let ps23_1 = " \r\n\r\nJehová es mi pastor\r\nSalmo de David.\r\nJehová es mi pastor; nada me faltará.\r\n";
        assert_eq!(
            spans_shown(ps23_1),
            [(HEAD_OPEN, "Jehová es mi pastor".into()), (HEAD_OPEN, "Salmo de David.".into())]
        );
        for raw in [gal31, gal35, ps119_9, ps119_1, ps23_1] {
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
        assert!(spans_shown(" Higaion. Selah. Jehová se ha hecho conocer en el juicio que ejecutó. \r\n\r\nHigaion. Selah\r\n").is_empty());
        // HTML: «Selah» right-aligned AFTER the verse stays in the verse.
        assert!(spans_shown("Muchos dicen de mi alma:</p> No hay salvación para él en Elohim.</p> <p align=\"right\"><i>Selah</i></p>").is_empty());
        // A verse that is only a title-like line is no title.
        assert!(spans_shown("<p align=\"center\"><b>Solo esto</b></p>").is_empty());
    }

    #[test]
    fn html_titles() {
        let rvr = "<p align=\"center\"><span style=\"font-weight:bold;\">El pacto de Dios con Abraham</span></p> Así Abraham creyó a Dios, y le fue contado por justicia.<span style=\"color:#804DB3;\"><sup>a</sup></span>";
        assert_eq!(spans_shown(rvr), [(HEAD_OPEN, "El pacto de Dios con Abraham".into())]);
        let dhh = "<p align=\"center\"><b>2. Dios nos hace libres por la fe (3.1—5.12)</b><br/><i><b>La experiencia cristiana del Espíritu</b></i></p> ¡Gálatas, duros para entender!";
        let got = spans_shown(dhh);
        assert_eq!(got.len(), 1);
        assert!(got[0].1.starts_with("2. Dios nos hace libres") && got[0].1.ends_with("del Espíritu"), "{got:?}");
        let rv1960_ps = "&nbsp;</p><p style=\"text-align:center;\"><i>Salmo de David.</i><br /> Jehová es mi pastor; nada me faltará.";
        assert_eq!(spans_shown(rv1960_ps), [(HEAD_MINOR, "Salmo de David.".into())]);
        let btih = "<p style=\"text-align:center;font-weight:bold;\">II.— SALVADOS POR LA GRACIA</p><p style=\"text-align:center;font-weight:bold;\">Salvados por la fe</p> Gálatas, ¿cómo son tan insensatos?";
        assert_eq!(spans_shown(btih).len(), 2);
        // Italic poetry (RVR1960x, rv60) is no title; an italic psalm title before plain text is.
        assert!(spans_shown("<span style=\"font-style:italic;\">Si siete veces será vengado Caín,</span></p><span style=\"font-style:italic;\">Lamec en verdad setenta veces siete lo será.</span>").is_empty());
        assert!(spans_shown("<i>Bienaventurados los perfectos de camino,</i> </p><p lang=\"en-US\" style=\"\"> <i>Los que andan en la ley de Jehová.</i>").is_empty());
        assert!(spans_shown("<i>Bendito el varón que confía en Jehová.</i><br/><i>Y cuya confianza es Jehová.</i>").is_empty());
        assert!(spans_shown("<span style=\"font-style:italic;\">Pozo, el cual cavaron los señores.</span></p><span style=\"font-style:italic;\">Lo cavaron los príncipes del pueblo,</span></p>Del desierto vinieron a Matana,").is_empty());
        let gen51 = "<p align=\"center\"><span style=\"font-weight:bold;\">Los descendientes de Adán</span></p><p align=\"center\">(1 Cr.1.1–4)</p> Este es el libro de las generaciones de Adán.";
        assert_eq!(spans_shown(gen51), [(HEAD_OPEN, "Los descendientes de Adán".into()), (HEAD_MINOR, "(1 Cr.1.1–4)".into())]);
        same_words(gen51);
        // A bold first word inside the verse is not a title.
        assert!(spans_shown("<b>Jehová</b> es mi pastor; nada me faltará.").is_empty());
        for raw in [rvr, dhh, rv1960_ps, btih] {
            same_words(raw);
        }
    }

    #[test]
    fn rtf_titles_before_and_after() {
        let gal = " \\par {\\b 2. Dios nos hace libres por la fe}\\par {\\b\\i La experiencia cristiana del Espíritu}\\par ¡Gálatas insensatos!, ¿quién os embrujó?{\\cf2\\super [a]}\r ";
        assert_eq!(
            spans_shown(gal),
            [(HEAD_OPEN, "2. Dios nos hace libres por la fe".into()), (HEAD_OPEN, "La experiencia cristiana del Espíritu".into())]
        );
        let mat = "hasta el nacimiento del Mesías.{\\cf2\\super [h]}\\par \\par {\\b\\i Nacimento de Jesús}";
        assert_eq!(spans_shown(mat), [(HEAD_NEXT, "Nacimento de Jesús".into())]);
        same_words(gal);
        same_words(mat);
        // RV1960 .bblx: centred bold groups with the break inside, before and after the verse.
        let gal31 = " \\par \\par {\\qc \\b El Espíritu se recibe por la fe\\par}  ¡Oh gálatas insensatos! ¿quién os fascinó?\r ";
        assert_eq!(spans_shown(gal31), [(HEAD_OPEN, "El Espíritu se recibe por la fe".into())]);
        let gal35 = " Aquel, pues, ¿lo hace por las obras de la ley, o por el oír con fe? \\par \\par {\\qc \\b El pacto de Dios con Abraham\\par}\r ";
        assert_eq!(spans_shown(gal35), [(HEAD_NEXT, "El pacto de Dios con Abraham".into())]);
        let ex1812 = " Y tomó Jetro, suegro de Moisés, holocaustos delante de Dios. \\par \\par {\\qc \\b Nombramiento de jueces\\par (Dt. 1.9-18)\\par}\r ";
        let got = spans_shown(ex1812);
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(got[0].0 == HEAD_NEXT && got[0].1 == "Nombramiento de jueces (Dt. 1.9-18)", "{got:?}");
        same_words(ex1812);
        let gal36 = " Así Abraham creyó a Dios, y le fue contado por justicia.{\\super\\cf6 (A)}\r ";
        assert!(spans_shown(gal36).is_empty());
        for raw in [gal31, gal35, gal36] {
            same_words(raw);
        }
    }

    #[test]
    fn html_titles_after_the_verse() {
        // reina_valera_1960.bbli
        let gal35 = "Aquel, pues, que os suministra el Espíritu, ¿lo hace por las obras de la ley, o por el oír con fe? </p><p lang=\"en-US\" style=\"\">&nbsp;</p><p lang=\"en-US\" style=\"text-align:center;\">&nbsp;</p><p lang=\"en-US\" style=\"text-align:center;color:#C0C0C0;font-weight:bold;\">El pacto de Dios con Abraham</p><p lang=\"en-US\" style=\"\">&nbsp;";
        assert_eq!(spans_shown(gal35), [(HEAD_NEXT, "El pacto de Dios con Abraham".into())]);
        // rv60a.bbli: a title and its parallel passage; a letter of Psalm 119.
        let ex1812 = "Y tomó Jetro, suegro de Moisés, holocaustos delante de Dios. </p><p lang=\"en-US\" style=\"\">&nbsp;</p><p lang=\"en-US\" style=\"text-align:center;font-weight:bold;\">Nombramiento de jueces</p><p lang=\"en-US\" style=\"text-align:center;font-weight:bold;\">(Dt. 1.9-18)</p><p lang=\"en-US\" style=\"\">&nbsp;";
        assert_eq!(
            spans_shown(ex1812),
            [(HEAD_NEXT, "Nombramiento de jueces".into()), (HEAD_NEXT, "(Dt. 1.9-18)".into())]
        );
        let ps1198 = "Tus estatutos guardaré; </p><p lang=\"en-US\" style=\"\"> No me dejes enteramente. </p><p lang=\"en-US\" style=\"\">&nbsp;</p><p lang=\"en-US\" style=\"text-align:center;font-weight:bold;\">Bet</p><p lang=\"en-US\" style=\"\">&nbsp;";
        assert_eq!(spans_shown(ps1198), [(HEAD_NEXT, "Bet".into())]);
        // DHH 2002 .bbli: bold italics in a paragraph left open.
        let mat512 = "Así persiguieron también a los profetas que vivieron antes que vosotros.<span style=\"color:#2E78C2;\"><sup>[q]</sup></span></p><p lang=\"en-US\" style=\"\">&nbsp;</p><p lang=\"en-US\" style=\"\"><b><i>Sal y luz del mundo</i></b>";
        assert_eq!(spans_shown(mat512), [(HEAD_NEXT, "Sal y luz del mundo".into())]);
        // A title before the verse, none after it.
        let gal31 = "</p><p lang=\"en-US\" style=\"text-align:center;\">&nbsp;</p><p lang=\"en-US\" style=\"text-align:center;color:#C0C0C0;font-weight:bold;\">El Espíritu se recibe por la fe</p><p lang=\"en-US\" style=\"\">&nbsp;</p><p lang=\"en-US\" style=\"\"> ¡Oh gálatas insensatos! ¿quién os fascinó? </p><p lang=\"en-US\" style=\"\">&nbsp;";
        assert_eq!(spans_shown(gal31), [(HEAD_OPEN, "El Espíritu se recibe por la fe".into())]);
        // Not titles: «Selah» set apart, bold words inside the verse, italics only.
        assert!(spans_shown("No hay para él salvación en Dios. </p><p style=\"text-align:center;font-weight:bold;\">Selah</p>").is_empty());
        assert!(spans_shown("<p>Y dijo Dios: <b>Sea la luz</b></p>").is_empty());
        assert!(spans_shown("Y dijo Dios: </p><p><b>Sea la luz</b> y fue la luz.").is_empty());
        assert!(spans_shown("los profetas.</p><p lang=\"en-US\" style=\"\"><i>La sal y la luz</i>").is_empty());
        // An inscription closing the verse («…este nombre:») and a bold line with no blank
        // line before it stay in the verse.
        assert!(spans_shown("Y en su vestidura y en su muslo tiene escrito este nombre: </p><p>&nbsp;</p><p style=\"text-align:center;font-weight:bold;\">REY DE REYES Y SEÑOR DE SEÑORES</p>").is_empty());
        assert!(spans_shown("¿lo hace por el oír con fe? </p><p style=\"text-align:center;font-weight:bold;\">El pacto de Dios con Abraham</p>").is_empty());
        for raw in [gal35, ex1812, ps1198, mat512, gal31] {
            same_words(raw);
        }
    }

    #[test]
    fn rtf_centred_paragraph_titles() {
        // Reina Valera 1960.bblx: `\qc` before the group, the break inside it.
        let gn121 = " \\pard  \\pard\\par \\qc {\\par \\b\\cf16 Dios llama a Abram}\\par\\pard \\par \\par  Pero Jehová había dicho a Abram: Vete de tu tierra.{\\super\\cf6 (A)} \\par\r ";
        assert_eq!(spans_shown(gn121), [(HEAD_OPEN, "Dios llama a Abram".into())]);
        let gal35 = " Aquel, pues, ¿lo hace por las obras de la ley, o por el oír con fe? \\par  \\pard\\par \\qc {\\par \\b\\cf16 El pacto de Dios con Abraham}\\par\\pard\r ";
        assert_eq!(spans_shown(gal35), [(HEAD_NEXT, "El pacto de Dios con Abraham".into())]);
        let ex1812 = " Y tomó Jetro, suegro de Moisés, holocaustos delante de Dios. \\par  \\pard\\par \\qc {\\par \\b\\cf16 Nombramiento de jueces}\\par\\pard \\pard\\par \\qc {\\par \\b\\cf16 (Dt. 1.9-18)}\\par\\pard\r ";
        assert_eq!(
            spans_shown(ex1812),
            [(HEAD_NEXT, "Nombramiento de jueces".into()), (HEAD_NEXT, "(Dt. 1.9-18)".into())]
        );
        let ps1198 = " Tus estatutos guardaré; \\par  No me dejes enteramente. \\par  \\pard\\par \\qc {\\par \\b\\cf16 Bet}\\par\\pard\r ";
        assert_eq!(spans_shown(ps1198), [(HEAD_NEXT, "Bet".into())]);
        // A chapter's last verse that ends in breaks only; an inscription after «:».
        assert!(spans_shown(" y murió Taré en Harán.  \\par  \\par \\par\r ").is_empty());
        assert!(spans_shown(" tiene escrito este nombre: \\par \\par {\\qc \\b REY DE REYES Y SEÑOR DE SEÑORES\\par}\r ").is_empty());
        for raw in [gn121, gal35, ex1812, ps1198] {
            same_words(raw);
        }
    }

    #[test]
    fn asterisk_titles() {
        // rv1960.bbli
        let gal31 = " * *   El Espíritu se recibe por la fe*  ¡Oh gálatas insensatos! ¿quién os fascinó?\r ";
        assert_eq!(spans_shown(gal31), [(HEAD_OPEN, "El Espíritu se recibe por la fe".into())]);
        let gal35 = " Aquel, pues, ¿lo hace por las obras de la ley, o por el oír con fe? * *   El pacto de Dios con Abraham*\r ";
        assert_eq!(spans_shown(gal35), [(HEAD_NEXT, "El pacto de Dios con Abraham".into())]);
        let ex1812 = " Y tomó Jetro, suegro de Moisés, holocaustos delante de Dios. * *   Nombramiento de jueces* (Dt. 1.9-18)*\r ";
        assert_eq!(
            spans_shown(ex1812),
            [(HEAD_NEXT, "Nombramiento de jueces".into()), (HEAD_NEXT, "(Dt. 1.9-18)".into())]
        );
        let ps1198 = " Tus estatutos guardaré; *  No me dejes enteramente. * *   Bet*\r ";
        assert_eq!(spans_shown(ps1198), [(HEAD_NEXT, "Bet".into())]);
        let ps11 = " * *   LIBRO I * El justo y los pecadores*  Bienaventurado el varón que no anduvo en consejo de malos, *  Ni estuvo en camino de pecadores, *  Ni en silla de escarnecedores se ha sentado;\r ";
        assert_eq!(
            spans_shown(ps11),
            [(HEAD_OPEN, "LIBRO I".into()), (HEAD_OPEN, "El justo y los pecadores".into())]
        );
        // The first line of the psalm after a title is not a second title.
        let ps481 = " * *   Hermosura y gloria de Sion*  Grande es Jehová, y digno de ser en gran manera alabado *  En la ciudad de nuestro Dios, en su monte santo.\r ";
        assert_eq!(spans_shown(ps481), [(HEAD_OPEN, "Hermosura y gloria de Sion".into())]);
        let ps571 = " **  Plegaria pidiendo ser librado de los perseguidores* (Sal. 108.1-5)* Al músico principal; sobre No destruyas. Mictam de David, cuando huyó de delante de Saúl a la cueva.* Ten misericordia de mí, oh Dios, ten misericordia de mí; *  Porque en ti ha confiado mi alma,\r ";
        assert_eq!(
            spans_shown(ps571),
            [
                (HEAD_OPEN, "Plegaria pidiendo ser librado de los perseguidores".into()),
                (HEAD_MINOR, "(Sal. 108.1-5)".into()),
                (HEAD_MINOR, "Al músico principal; sobre No destruyas. Mictam de David, cuando huyó de delante de Saúl a la cueva.".into()),
            ]
        );
        let ps511 = " **  Arrepentimiento, y plegaria pidiendo purificación* Al músico principal. Salmo de David, cuando después que se llegó a Betsabé, vino a él Natán el profeta.*\r\n Ten piedad de mí, oh Dios, conforme a tu misericordia; *  Conforme a la multitud de tus piedades borra mis rebeliones.\r ";
        assert_eq!(spans_shown(ps511).len(), 2);
        // The asterisks around a title go; the poetry breaks stay.
        let shown = clean_text(&find_titles(ps481, true).mark(ps481));
        assert_eq!(shown.matches('*').count(), 1, "{shown:?}");
        // Not titles: a note mark (NTV), a verse cut by note marks (Oso), poetry.
        assert!(spans_shown("* Joás* tenía siete años cuando comenzó a reinar.").is_empty());
        assert!(spans_shown("* * Examinemoſlo con affrenta, y con tormẽto, paraq̃ conozcamos ſu * modeﬅia, y prouemos ſu paciencia. \r\n").is_empty());
        assert!(spans_shown(" La salvación es de Jehová; *  Sobre tu pueblo sea tu bendición.  Selah\r ").is_empty());
        for raw in [gal31, gal35, ex1812, ps1198, ps11, ps481, ps571, ps511] {
            same_words(raw);
        }
    }

    /// Every installed Bible, a spread of chapters: words never move, how often each module
    /// fires, and RV1960-T / RVR1960x checked verse by verse against the untitled RVR1960.
    /// `cargo test --release --lib -- --ignored title_sweep --nocapture`.
    #[test]
    #[ignore]
    fn title_sweep() {
        let dir = std::path::Path::new(ESWORD);
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let chapters: [(i32, i32); 12] = [(1, 1), (1, 2), (19, 3), (19, 23), (19, 119), (23, 53), (40, 5), (42, 15), (45, 8), (48, 3), (49, 2), (66, 21)];
        let mut moved = 0usize;
        let mut busy = Vec::new();
        let mut last_next = Vec::new();
        let mut modules = 0usize;
        let mut firing = 0usize;
        for e in rd.flatten() {
            let p = e.path();
            if !matches!(p.extension().and_then(|x| x.to_str()), Some("bbli" | "bblx")) {
                continue;
            }
            let Ok(conn) = rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { continue };
            modules += 1;
            let (mut verses, mut leads, mut nexts) = (0usize, 0usize, 0usize);
            for (b, c) in chapters {
                let Ok(mut st) = conn.prepare("SELECT Verse, Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 ORDER BY Verse") else { continue };
                let rows: Vec<(i32, String)> = st
                    .query_map([b, c], |r| Ok((r.get::<_, i32>(0)?, r.get::<_, String>(1).unwrap_or_default())))
                    .map(|it| it.flatten().collect())
                    .unwrap_or_default();
                let n = rows.len();
                for (i, (_, raw)) in rows.iter().enumerate() {
                    verses += 1;
                    let t = find_titles(raw, true);
                    if t.is_empty() {
                        continue;
                    }
                    let marked = clean_text(&t.mark(raw));
                    let stripped: String = marked.chars().filter(|c| !matches!(*c as u32, 0xE005..=0xE008)).collect();
                    if page_words(&stripped) != page_words(&clean_text(raw)) {
                        moved += 1;
                        eprintln!("MOVED {:?} {b}:{c}: {raw:?}", p.file_name().unwrap());
                    }
                    if t.spans.iter().any(|s| s.kind != Kind::Next) {
                        leads += 1;
                    }
                    if t.spans.iter().any(|s| s.kind == Kind::Next) {
                        nexts += 1;
                        if i + 1 == n {
                            last_next.push(format!("{} {b}:{c}", p.file_name().unwrap().to_string_lossy()));
                        }
                    }
                }
            }
            if leads + nexts > 0 {
                firing += 1;
            }
            if verses > 0 && (leads as f64 / verses as f64) > 0.2 {
                busy.push(format!("{} {leads}/{verses}", p.file_name().unwrap().to_string_lossy()));
            }
        }
        eprintln!("{modules} Bibles, {firing} with titles; words moved in {moved} verses");
        eprintln!("over 20% of verses with titles before them: {busy:#?}");
        eprintln!("chapters whose last verse carries the next chapter's title: {} {:?}", last_next.len(), last_next.iter().take(8).collect::<Vec<_>>());

        // Oracle: titles taken out of RV1960-T / RVR1960x must leave the RVR1960 verse.
        // Words folded for comparing two editions: lower case, no accents, no reference
        // digits / «Cr» notes; a note letter glued on («serpientea») still matches.
        let fold = |w: &str| -> String {
            w.to_lowercase().chars().map(|c| match c {
                'á' => 'a', 'é' => 'e', 'í' => 'i', 'ó' => 'o', 'ú' | 'ü' => 'u', _ => c,
            }).collect()
        };
        let words_of = |s: &str| -> Vec<String> {
            page_words(s)
                .into_iter()
                .map(|w| fold(&w))
                .filter(|w| w.chars().count() > 1 && !w.chars().all(|c| c.is_ascii_digit()) && w != "cr")
                .collect()
        };
        let eqish = |a: &String, b: &String| a == b
            || (a.starts_with(b.as_str()) && a.len() <= b.len() + 2)
            || (b.starts_with(a.as_str()) && b.len() <= a.len() + 2);
        let without_titles = |marked: &str| -> String {
            let mut out = String::new();
            let mut inside = false;
            for c in marked.chars() {
                match c as u32 {
                    0xE005 | 0xE007 | 0xE008 => inside = true,
                    0xE006 => inside = false,
                    _ if !inside => out.push(c),
                    _ => {}
                }
            }
            out
        };
        let open = |f: &str| rusqlite::Connection::open_with_flags(dir.join(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok();
        let Some(plain) = open("00RVR1960.bbli") else { return };
        for titled in [
            "rv1960-t_reina_valera_1960_con_ttulos_y_notas.bbli",
            "00RVR1960x.bbli",
            "Biblia Reina Valera 1960.bblx",
            "rv1960.bbli",
            "Reina Valera 1960.bblx",
            "reina_valera_1960.bbli",
            "rv60a.bbli",
            "biblia_reina_valera_1960.bbli",
        ] {
            let Some(conn) = open(titled) else { continue };
            let mut st = conn.prepare("SELECT Book, Chapter, Verse, Scripture FROM Bible").unwrap();
            let mut checked = 0usize;
            let mut bad = Vec::new();
            let mut fired = 0usize;
            for row in st.query_map([], |r| Ok((r.get::<_, i32>(0)?, r.get::<_, i32>(1)?, r.get::<_, i32>(2)?, r.get::<_, String>(3).unwrap_or_default()))).unwrap().flatten() {
                let (b, c, v, raw) = row;
                let Ok(other) = plain.query_row("SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3", [b, c, v], |r| r.get::<_, String>(0)) else { continue };
                let t = find_titles(&raw, true);
                if !t.is_empty() {
                    fired += 1;
                }
                let mine = words_of(&without_titles(&clean_text(&t.mark(&raw))));
                let theirs = words_of(&clean_text(&other));
                checked += 1;
                if mine.len() < 3 || theirs.len() < 3 {
                    continue;
                }
                let head_ok = (0..3).all(|i| eqish(&mine[i], &theirs[i]));
                let tail_ok = mine.iter().rev().take(2).any(|w| theirs.iter().rev().take(4).any(|x| eqish(x, w)));
                if !head_ok || !tail_ok {
                    bad.push(format!("{b}:{c}:{v} mine={:?}…{:?} theirs={:?}…{:?}", &mine[..3], mine.iter().rev().take(3).collect::<Vec<_>>(), &theirs[..3], theirs.iter().rev().take(3).collect::<Vec<_>>()));
                }
            }
            eprintln!("{titled}: {checked} verses, titles in {fired}, {} differ from RVR1960", bad.len());
            // Psalm titles are verse text in the untitled RVR1960: expected there.
            let other: Vec<&String> = bad.iter().filter(|b| !(b.starts_with("19:") && b.split(' ').next().is_some_and(|r| r.ends_with(":1")))).collect();
            eprintln!("   besides psalm titles: {}", other.len());
            for b in other.iter().take(16) {
                eprintln!("   {b}");
            }
        }
    }

    /// EVERY verse of every installed Bible: where titles for the next verse are found (HTML,
    /// RTF, asterisks, plain text), with examples, and words never move.
    /// `cargo test --release --lib -- --ignored next_title_walk --nocapture`.
    #[test]
    #[ignore]
    fn next_title_walk() {
        let dir = std::path::Path::new(ESWORD);
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut moved = 0usize;
        let mut report = Vec::new();
        let mut short = std::collections::BTreeMap::<String, usize>::new();
        for e in rd.flatten() {
            let p = e.path();
            if !matches!(p.extension().and_then(|x| x.to_str()), Some("bbli" | "bblx")) {
                continue;
            }
            let Ok(conn) = rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { continue };
            let Ok(mut st) = conn.prepare("SELECT Book, Chapter, Verse, Scripture FROM Bible") else { continue };
            let Ok(rows) = st.query_map([], |r| {
                Ok((r.get::<_, i32>(0)?, r.get::<_, i32>(1)?, r.get::<_, i32>(2)?, r.get::<_, String>(3).unwrap_or_default()))
            }) else {
                continue;
            };
            // verses with a next-verse title: html, rtf, asterisks, plain text
            let mut counts = [0usize; 4];
            let mut examples = Vec::new();
            for (b, c, v, raw) in rows.flatten() {
                let t = find_titles(&raw, true);
                if t.is_empty() {
                    continue;
                }
                let marked = clean_text(&t.mark(&raw));
                let stripped: String = marked.chars().filter(|c| !matches!(*c as u32, 0xE005..=0xE008)).collect();
                if page_words(&stripped) != page_words(&clean_text(&raw)) {
                    moved += 1;
                    eprintln!("MOVED {:?} {b}:{c}:{v}: {raw:?}", p.file_name().unwrap());
                }
                let next: Vec<String> = t
                    .spans
                    .iter()
                    .filter(|s| s.kind == Kind::Next)
                    .map(|s| visible(&raw[s.start..s.end]).split_whitespace().collect::<Vec<_>>().join(" "))
                    .collect();
                if next.is_empty() {
                    continue;
                }
                let k = if raw.contains('<') {
                    0
                } else if raw.contains("{\\") {
                    1
                } else if !t.blanks.is_empty() {
                    2
                } else {
                    3
                };
                counts[k] += 1;
                for n in &next {
                    if n.split_whitespace().count() <= 2 {
                        *short.entry(n.clone()).or_default() += 1;
                    }
                }
                if examples.len() < 4 && (examples.is_empty() || v % 7 == 0) {
                    examples.push(format!("{b}:{c}:{v} {next:?}"));
                }
            }
            if counts.iter().any(|&n| n > 0) {
                report.push(format!(
                    "{:>5} html {:>5} rtf {:>5} star {:>5} text | {}\n      {}",
                    counts[0],
                    counts[1],
                    counts[2],
                    counts[3],
                    p.file_name().unwrap().to_string_lossy(),
                    examples.join("\n      ")
                ));
            }
        }
        report.sort();
        for r in &report {
            eprintln!("{r}");
        }
        eprintln!("short next-verse titles (1-2 words): {short:?}");
        eprintln!("{} Bibles with next-verse titles; words moved in {moved} verses", report.len());
        assert_eq!(moved, 0);
    }

    /// What marking costs a chapter load: `cargo test --release --lib -- --ignored
    /// chapter_cost --nocapture` (Psalm 119, 176 verses).
    #[test]
    #[ignore]
    fn chapter_cost() {
        for f in ["rv1960-t_reina_valera_1960_con_ttulos_y_notas.bbli", "00RVR1960x.bbli", "rv1960_reina_valera_1960.bbli", "00RVR1960.bbli"] {
            let p = std::path::Path::new(ESWORD).join(f);
            if !p.exists() {
                continue;
            }
            let path = p.to_string_lossy().to_string();
            let _ = crate::esword::get_chapter(&path, 19, 119);
            let t = std::time::Instant::now();
            for _ in 0..20 {
                let _ = crate::esword::get_chapter(&path, 19, 119);
            }
            let per = t.elapsed().as_secs_f64() * 1e3 / 20.0;
            let raws: Vec<String> = {
                let conn = rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
                let mut st = conn.prepare("SELECT Scripture FROM Bible WHERE Book=19 AND Chapter=119").unwrap();
                let v = st.query_map([], |r| r.get::<_, String>(0)).unwrap().flatten().collect();
                v
            };
            let t2 = std::time::Instant::now();
            for _ in 0..20 {
                for r in &raws {
                    std::hint::black_box(find_titles(r, true));
                }
            }
            let find = t2.elapsed().as_secs_f64() * 1e3 / 20.0;
            eprintln!("{f:<55} get_chapter Ps 119: {per:.2} ms (finding titles: {find:.2} ms)");
        }
    }

    /// Whole Bibles: how many chapters end on a verse that carries the next chapter's title.
    #[test]
    #[ignore]
    fn chapter_end_titles() {
        for f in [
            "rv1960-t_reina_valera_1960_con_ttulos_y_notas.bbli",
            "Biblia Reina Valera 1960.bblx",
            "Biblia Dios Habla Hoy Latinoamericana con Deuterocanonicos (2002).bblx",
            "AB-ntv_nueva_traduccin_viviente.bbli",
            "Bb-pdt_-_la_palabra_de_dios_para_todos_2008.bbli",
            "rv1960.bbli",
            "Reina Valera 1960.bblx",
            "reina_valera_1960.bbli",
            "rv60a.bbli",
            "biblia_dios_habla_hoy_latinoamericana_con_deuterocannicos_2002.bbli",
        ] {
            let p = std::path::Path::new(ESWORD).join(f);
            let Ok(conn) = rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { continue };
            let mut st = conn
                .prepare("SELECT b.Book, b.Chapter, b.Verse, b.Scripture FROM Bible b JOIN (SELECT Book, Chapter, max(Verse) AS V FROM Bible GROUP BY Book, Chapter) m ON b.Book = m.Book AND b.Chapter = m.Chapter AND b.Verse = m.V")
                .unwrap();
            let mut hits = Vec::new();
            let mut n = 0;
            for row in st.query_map([], |r| Ok((r.get::<_, i32>(0)?, r.get::<_, i32>(1)?, r.get::<_, i32>(2)?, r.get::<_, String>(3).unwrap_or_default()))).unwrap().flatten() {
                n += 1;
                let (b, c, v, raw) = row;
                let t = find_titles(&raw, true);
                if t.spans.iter().any(|s| s.kind == Kind::Next) {
                    hits.push(format!("{b}:{c}:{v}"));
                }
            }
            eprintln!("{f}: {} of {n} chapters end on a next-chapter title {:?}", hits.len(), hits.iter().take(10).collect::<Vec<_>>());
        }
    }

    // ----- real modules (skipped when e-Sword isn't installed) -----

    const ESWORD: &str = r"C:\Program Files (x86)\e-Sword";

    fn verse(file: &str, b: i32, c: i32, v: i32) -> Option<String> {
        let p = std::path::Path::new(ESWORD).join(file);
        if !p.exists() {
            eprintln!("skip: {file}");
            return None;
        }
        let conn = rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        conn.query_row(
            "SELECT Scripture FROM Bible WHERE Book=?1 AND Chapter=?2 AND Verse=?3",
            [b, c, v],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    fn check(file: &str, b: i32, c: i32, v: i32, want: &[(char, &str)]) {
        let Some(raw) = verse(file, b, c, v) else { return };
        let got = spans_shown(&raw);
        let want: Vec<(char, String)> = want.iter().map(|(k, t)| (*k, t.to_string())).collect();
        assert_eq!(got, want, "{file} {b}:{c}:{v} raw {raw:?}");
        same_words(&raw);
    }

    #[test]
    fn real_bibles_fire_where_titles_are() {
        let t = "rv1960-t_reina_valera_1960_con_ttulos_y_notas.bbli";
        check(t, 48, 3, 1, &[(HEAD_OPEN, "El Espíritu se recibe por la fe")]);
        check(t, 48, 3, 5, &[(HEAD_NEXT, "El pacto de Dios con Abraham")]);
        check(t, 1, 2, 3, &[(HEAD_NEXT, "El hombre en el huerto del Edén")]);
        check(t, 19, 119, 9, &[(HEAD_OPEN, "Bet")]);
        check(t, 19, 119, 1, &[(HEAD_OPEN, "Excelencias de la ley de Dios"), (HEAD_OPEN, "Alef")]);
        check(t, 19, 23, 1, &[(HEAD_OPEN, "Jehová es mi pastor"), (HEAD_OPEN, "Salmo de David.")]);
        check(t, 19, 23, 2, &[]);
        check("00RVR1960x.bbli", 48, 3, 6, &[(HEAD_OPEN, "El pacto de Dios con Abraham")]);
        check("rv1960_reina_valera_1960.bbli", 19, 23, 1, &[(HEAD_MINOR, "Salmo de David.")]);
        check("rv1960_reina_valera_1960.bbli", 48, 3, 6, &[]);
        let bblx = "Biblia Reina Valera 1960.bblx";
        check(bblx, 48, 3, 1, &[(HEAD_OPEN, "El Espíritu se recibe por la fe")]);
        check(bblx, 48, 3, 5, &[(HEAD_NEXT, "El pacto de Dios con Abraham")]);
        check(bblx, 1, 2, 3, &[(HEAD_NEXT, "El hombre en el huerto del Edén")]);
        check(bblx, 48, 3, 6, &[]);
        check("AB-Junemann Septuaginta en espanol.bbli", 1, 1, 1, &[(HEAD_OPEN, "Creación (Reseña sintética)")]);
        check("11_Kadosh Israelita Mesianica.bbli", 1, 1, 1, &[(HEAD_OPEN, "Parashah 1: Bereshit (En el principio)"), (HEAD_OPEN, "1:1-6:8")]);
        if let Some(raw) = verse("01-DHH-D.bbli", 48, 3, 1) {
            assert_eq!(spans_shown(&raw).len(), 1, "{raw:?}");
        }
        if let Some(raw) = verse("Biblia Dios Habla Hoy Latinoamericana con Deuterocanonicos (2002).bblx", 48, 3, 1) {
            assert_eq!(spans_shown(&raw).len(), 2, "{raw:?}");
        }
        // Titles at the end of the previous verse in HTML, RTF and asterisk layouts.
        check("rv1960.bbli", 48, 3, 1, &[(HEAD_OPEN, "El Espíritu se recibe por la fe")]);
        check("rv1960.bbli", 48, 3, 5, &[(HEAD_NEXT, "El pacto de Dios con Abraham")]);
        check("rv1960.bbli", 48, 3, 6, &[]);
        check("Reina Valera 1960.bblx", 1, 12, 1, &[(HEAD_OPEN, "Dios llama a Abram")]);
        check("Reina Valera 1960.bblx", 48, 3, 5, &[(HEAD_NEXT, "El pacto de Dios con Abraham")]);
        check("reina_valera_1960.bbli", 48, 3, 5, &[(HEAD_NEXT, "El pacto de Dios con Abraham")]);
        check("rv60a.bbli", 19, 119, 8, &[(HEAD_NEXT, "Bet")]);
        check("biblia_dios_habla_hoy_latinoamericana_con_deuterocannicos_2002.bbli", 40, 5, 12, &[(HEAD_NEXT, "Sal y luz del mundo")]);
        if let Some(raw) = verse("00RVR1960+.bbli", 19, 23, 1) {
            let got = spans_shown(&raw);
            assert!(got.len() == 1 && got[0].1.starts_with("Salmo"), "{got:?}");
            same_words(&raw);
        }
    }
}
