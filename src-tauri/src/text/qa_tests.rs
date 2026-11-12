//! WP2 QA-stage tests: edge cases and regressions the plan's test_plan calls for that the
//! implementation tests did not cover, plus cheap property checks (synthetic and on the
//! real library, read-only). Library tests skip with a visible message when it is absent.

use super::gate_tests::{cell, library_dir, open_ro, visible};
use super::*;

/// Every internal marker bracket in the output must come from a literal one in the input.
fn assert_no_marker_leak(raw: &str, html: &str) {
    let lit = raw.matches(['⟦', '⟧']).count();
    let out = html.matches(['⟦', '⟧']).count();
    assert!(out <= lit, "internal marker leaked: {raw:?} -> {html}");
    for name in ["⟦REF", "⟦STRONG", "⟦W:", "⟦/W", "⟦MORPH", "⟦WOC", "⟦FN", "⟦TD", "⟦IMG"] {
        if !raw.contains(name) {
            assert!(!html.contains(name), "marker {name} leaked: {raw:?} -> {html}");
        }
    }
}

// ---------------------------------------------------------------- ISS-24 RTF whitelist

/// TA.bbli has 92 verses with `\par\par…`: the delimiter of the first word used to swallow
/// the backslash of the next one, so every second `\par` stayed visible.
#[test]
fn qa_iss24_consecutive_rtf_control_words_all_convert() {
    assert_eq!(clean_esword_text("A\\par\\par B"), "A<br>B");
    assert_eq!(clean_esword_text("nación.\\par\\par\\par\\par 4. Entonces"), "nación.<br>4. Entonces");
    assert_eq!(clean_esword_text("x \\ldblquote\\emdash\\rdblquote  y"), "x “—” y");
    assert_eq!(clean_esword_text("a\\line\\tab b"), "a<br>b");
}

#[test]
fn qa_iss24_rtf_word_followed_by_lowercase_letter_is_not_a_control_word() {
    assert_eq!(clean_esword_text("\\parte y \\linea \\bible"), "\\parte y \\linea \\bible");
    // a known word right before an unknown one: only the known one converts
    assert_eq!(clean_esword_text("A\\par\\pard B"), "A<br>\\pard B");
}

#[test]
fn qa_iss12_rtf_fragment_bold_toggle_keeps_rtf_space_rules() {
    // `{\i…}` marks a headerless fragment, so `\b … \b0` become bold; RTF eats exactly one
    // space after a control word.
    let h = clean_esword_text("{\\i a} texto \\b negrita\\b0  normal");
    assert_eq!(h, "<em>a</em> texto <strong>negrita</strong> normal");
    // outside a fragment `\b`/`\i` are dropped, never shown
    assert_eq!(clean_esword_text("texto \\b negrita\\b0  normal"), "texto negrita normal");
}

// ---------------------------------------------------------------- ISS-36 own tags

/// A word glued onto a tag name (`<brPalabra`, `</b palabra`) is tag damage, not a word:
/// tagging it moved the marker inside the tag and exposed `&lt;` in the tagged html only.
#[test]
fn qa_iss36_word_glued_to_tag_name_is_not_tagged_and_visible_text_matches() {
    for raw in [
        "<brPalabra <tvm>X</tvm> <num>H1</num> fin",
        "palabra</b <tvm>X</tvm> <num>H1</num> fin",
        "<redYo <tvm>X</tvm> <num>H1</num> fin",
    ] {
        let tagged = clean_bible_verse_tagged(raw);
        let plain = clean_esword_text(raw);
        assert_eq!(visible(&tagged), visible(&plain), "{raw} -> {tagged}");
        assert!(!tagged.contains("&lt;"), "{raw} -> {tagged}");
        assert!(!own_tag_units(raw).iter().any(|u| u.word.starts_with("br") || u.word == "b" || u.word.starts_with("red")), "{raw}");
    }
    // a word after a complete tag is still tagged
    let h = clean_bible_verse_tagged("<p>Palabra <tvm>X</tvm> <num>H1</num>");
    assert!(h.contains(r#"<span class="word" data-strong="H1">Palabra</span>"#), "{h}");
}

#[test]
fn qa_iss36_word_spans_balance_with_red_letters_and_formatting() {
    let raw = "<red>Yo <tvm>RP1-S</tvm> <num>G1473</num> soy</red> el <b>camino</b> <tvm>N</tvm> <num>G3598</num>";
    let h = clean_bible_verse_tagged(raw);
    assert_eq!(visible(&h), visible(&clean_esword_text(raw)));
    assert!(h.contains(r#"<span class="woc"><span class="word" data-strong="G1473">Yo</span>"#), "{h}");
    // '<b>camino</b> <tvm>' — the word is behind a tag: untagged by the whole-word rule
    assert!(!h.contains(r#">camino</span>"#), "{h}");
    assert_eq!(h.matches("<span").count(), h.matches("</span>").count(), "{h}");
}

#[test]
fn qa_iss36_codes_are_normalised_and_hebrew_or_accented_words_are_whole() {
    let u = own_tag_units("dijo JEHOVÁ <tvm>NPMSA</tvm> <num> h3068 </num>: — Él <tvm>R</tvm> <num>H1931</num>");
    assert_eq!(u, vec![
        OwnTagUnit { word: "JEHOVÁ".into(), strongs: vec!["H3068".into()] },
        OwnTagUnit { word: "Él".into(), strongs: vec!["H1931".into()] },
    ]);
    // a number directly before the morph is not a word
    assert!(own_tag_units("12 <tvm>X</tvm> <num>H1</num>").is_empty());
    // a tvm with no <num> after it gives nothing
    assert!(own_tag_units("palabra <tvm>X</tvm> y").is_empty());
}

// ---------------------------------------------------------------- ISS-05 guardrails

/// Strongspa.lexi 'Rimmón': an unclosed `<ref>` runs to a later `</ref>` across a `<num>`;
/// the inner marker broke the REF marker and «⟦REF:1Cr 6:62…» was shown to the user.
#[test]
fn qa_iss05_ref_containing_markup_never_leaks_internal_markers() {
    let raw = "Rimmón o Rimmonó (<ref>1Cr 6:62 [77]); lo mismo que <num>H7416</num> Rimón, de cinco lugares (Jos 19:13</ref>) es";
    let h = clean_esword_text(raw);
    assert_no_marker_leak(raw, &h);
    assert!(h.contains(r#"<span class="strong-ref" data-strong="H7416">H7416</span>"#), "{h}");
    let v = visible(&h);
    assert!(v.contains("1Cr 6:62 [77]") && v.contains("Rimón") && v.contains("Jos 19:13"), "{v}");
    for raw in [
        "<ref>Jn 3:16 <num>G25</num></ref> fin",
        "<ref>Dios <tvm>X</tvm> <num>H430</num> 1:1</ref> fin",
    ] {
        for h in [clean_esword_text(raw), clean_bible_verse_tagged(raw)] {
            assert_no_marker_leak(raw, &h);
            assert!(visible(&h).ends_with("fin"), "{raw} -> {h}");
        }
        assert_eq!(visible(&clean_bible_verse_tagged(raw)), visible(&clean_esword_text(raw)));
    }
    // a ref with plain inner tags still links
    let h = clean_esword_text("<ref>Jn <red>3:16</red></ref>");
    assert!(h.contains(r#"data-book="Jn" data-chapter="3" data-verse="16""#), "{h}");
    // Review round 1: an unterminated tag inside the ref (or a quote / double-escaped tag
    // cutting through a marker) must not eat the marker's ⟧ and the text after it.
    for (raw, tail) in [
        ("<ref>Jn 3:16 <b x</ref> más texto aquí y allá", "más texto aquí y allá"),
        ("<ref>Jn 3:16 <span class=x</ref> resto del artículo", "resto del artículo"),
        ("<ref>Jn 3:16 <a title=\"</ref> texto\">", "texto\">"),
        ("<ref>Jn 3:16 &lt;span class=x</ref> resto &gt; fin", "resto > fin"),
        ("<a title=\"<ref>Jn\" >3:16</ref> fin", "fin"),
        ("<a title=\"<tvm>x\" ></tvm> fin", "fin"),
    ] {
        for h in [clean_esword_text(raw), clean_bible_verse_tagged(raw)] {
            assert_no_marker_leak(raw, &h);
            assert!(!h.contains('⟦') && !h.contains('⟧'), "{raw:?} -> {h}");
            assert!(visible(&h).trim_end().ends_with(tail), "{raw:?} -> {h}");
        }
    }
    // the unterminated fragment still shows as a raw ref label, as the old cleaner did
    let h = clean_esword_text("<ref>Jn 3:16 <b x</ref> más");
    assert!(h.contains("bible-ref-raw") && visible(&h).contains("Jn 3:16 <b x"), "{h}");
    // a whole marker inside a quoted attribute is still stripped with its tag (bdbthayer)
    let h = clean_esword_text(r#"<a href="tw://[self]?<num>H1129</num>" style="x">edificar</a>"#);
    assert_eq!(visible(&h), "edificar", "{h}");
}

/// Review round 1 (DoS): misnested inline openers must not make the balancer's output grow
/// quadratically. The old cleaner was linear; this flood produced 212 MB from 215 kB.
#[test]
fn qa_iss05_misnested_inline_flood_output_stays_linear() {
    let raw = format!(
        "{}{}",
        "<span style=\"font-weight:bold\"><red>".repeat(5000),
        "</red>x".repeat(5000)
    );
    let t = std::time::Instant::now();
    for h in [clean_esword_text(&raw), clean_bible_verse_tagged(&raw)] {
        assert!(h.len() < 4 * raw.len(), "output {} B from {} B input", h.len(), raw.len());
        assert_eq!(visible(&h).matches('x').count(), 5000);
        // still well-formed: every opened inline tag is closed
        assert_eq!(h.matches("<strong>").count(), h.matches("</strong>").count(), "strong");
        assert_eq!(h.matches("<span").count(), h.matches("</span>").count(), "span");
    }
    // generous even for debug builds (the unbounded balancer needed 160 ms in release)
    assert!(t.elapsed() < std::time::Duration::from_secs(10), "{:?}", t.elapsed());
    // A deep stack of unclosed openers plus closers that match nothing stays cheap (only the
    // top of the stack is searched) and linear.
    let raw = format!("{}{}", "<red>".repeat(50_000), "</i>".repeat(50_000));
    let t = std::time::Instant::now();
    let h = clean_esword_text(&raw);
    assert!(t.elapsed() < std::time::Duration::from_secs(10), "{:?}", t.elapsed());
    assert!(h.len() < 8 * raw.len(), "output {} B from {} B input", h.len(), raw.len());
    // ebc.cmti: one `<red>` left open per footnote; formatting inside a deep stack is kept.
    let raw = format!("{}<sup>4</sup> <i>new name</i> fin", "<red>a ".repeat(40));
    let h = clean_esword_text(&raw);
    assert!(h.contains(r#"<sup class="fn">4</sup>"#) && h.contains("<em>new name</em>"), "{h}");
    // A long `⟦W:…⟧` payload under a deep stack also stays linear.
    let w = format!("{}x<tvm>V</tvm> <num>{}</num>", "<i>".repeat(31), "H1 ".repeat(2000));
    let raw = format!("{w}{}", "</i>".repeat(31));
    let h = clean_esword_text_with(&raw, CleanOptions { tagged_words: true, ..Default::default() });
    assert!(h.len() < 4 * raw.len(), "output {} B from {} B input", h.len(), raw.len());
    // ordinary misnesting is still balanced the same way
    assert_eq!(
        clean_esword_text("<b>uno <i>dos</b> tres</i>"),
        "<strong>uno <em>dos</em></strong><em> tres</em>"
    );
}

/// Guard: dropping double-escaped formatting keeps the words around it apart when the
/// module put a space there (esp-port 'apreciar', Onkelos). Without a space the words still
/// join (today's behaviour too); no library row has that shape, so it is left as is.
#[test]
fn qa_iss05_double_escaped_breaks_keep_surrounding_spaces() {
    assert_eq!(visible(&clean_esword_text("taxar. &lt;BR&gt;aprecio")), "taxar. aprecio");
    assert_eq!(visible(&clean_esword_text("}&lt;BR&gt;&nbsp;&lt;BR&gt; x")), "} x");
    assert_eq!(clean_esword_text("&lt;b&gt;x&lt;/b&gt;y"), "xy");
    assert_eq!(clean_esword_text("<p>&lt;new&gt;</p>"), "");
}

#[test]
fn qa_iss05_raw_angle_brackets_used_as_operators_stay_visible() {
    let h = clean_esword_text("3 < 5 y 7 > 2");
    assert_eq!(h, "3 &lt; 5 y 7 &gt; 2");
    assert_eq!(visible(&h), "3 < 5 y 7 > 2");
    assert_eq!(visible(&clean_esword_text("a <- b --> c")), "a <- b --> c");
}

#[test]
fn qa_iss05_empty_and_whitespace_only_inputs() {
    for raw in ["", "   ", "\n\n", "<p></p>", "&nbsp;", "<br><br>", "<ref></ref>", "<tvm></tvm>"] {
        let h = clean_esword_text(raw);
        assert!(visible(&h).is_empty(), "{raw:?} -> {h:?}");
        assert!(!h.contains('⟦'), "{raw:?} -> {h:?}");
    }
}

/// must_not_break: Greek/Hebrew entities decode; `plain` consumers never see raw entities.
#[test]
fn qa_must_not_break_hebrew_entities_and_plain_has_no_entities() {
    let h = clean_esword_text("<heb>&#x05D0;&#1489;&#x05BC;</heb> &aleph; &amp;amp; &quot;x&quot;");
    assert!(h.contains("אבּ"), "{h}");
    let p = to_plain(&h);
    assert_eq!(p, "אבּ ℵ & \"x\"");
    assert!(!p.contains("&amp;") && !p.contains("&quot;"), "{p}");
}

// ---------------------------------------------------------------- ISS-24 to_plain

#[test]
fn qa_iss24_to_plain_table_rows_and_cells_never_glue() {
    assert_eq!(to_plain("<table><tr><td>a</td><td>b</td></tr><tr><th>c</th></tr></table>"), "a b c");
    assert_eq!(to_plain(&clean_esword_text("<table><tr><td>a</td><td>b</td></tr><tr><td>c</td></tr></table>")), "a · b c");
}

// ---------------------------------------------------------------- ISS-20

#[test]
fn qa_iss20_morph_is_case_insensitive_and_escapes_its_text() {
    let h = clean_esword_text("x <TVM>A&amp;B</TVM> <Num>h12</Num>");
    assert!(h.contains(r#"<span class="morph">A&amp;B</span>"#), "{h}");
    assert!(h.contains(r#"<span class="strong-ref" data-strong="H12">H12</span>"#), "{h}");
    assert_eq!(to_plain(&h), "x H12");
    // markup inside a morph code never becomes live
    let h = clean_esword_text("x <tvm>&lt;b onclick=1&gt;</tvm> y");
    assert!(!h.contains("<b") && !h.contains("onclick=1\""), "{h}");
}

// ---------------------------------------------------------------- ISS-25

#[test]
fn qa_iss25_underscore_ranges_and_adjacent_refs() {
    assert_eq!(clean_esword_text("Gén_1:1-3"), "Gén 1:1-3");
    assert_eq!(clean_esword_text("Gén_1:1;Éx_2:2, Núm_3:4"), "Gén 1:1;Éx 2:2, Núm 3:4");
    assert_eq!(clean_esword_text("(Jua_7:50)."), "(Jua 7:50).");
    let h = clean_esword_text("<ref>1Co_13:4</ref>");
    assert!(h.starts_with(r#"<span class="bible-ref" data-book="1Co" data-chapter="13" data-verse="4""#), "{h}");
}

#[test]
fn qa_iss25_cross_chapter_en_dash_and_dotted_book_keep_crossref_shape() {
    let h = clean_esword_text("<ref>Ef. 5:21–6:9</ref>");
    // crossrefs.rs parses class first, then data-book / data-chapter / data-verse
    assert!(
        h.starts_with(r#"<span class="bible-ref" data-book="Ef" data-chapter="5" data-verse="21" data-chapter-end="6" data-verse-end2="9""#),
        "{h}"
    );
    assert!(!h.contains("data-verse-end=\""), "{h}");
    // label text and attributes are escaped exactly once
    let h = clean_esword_text("<ref>Jn &quot;3:16&quot;</ref>");
    assert!(h.contains(r#"data-raw="Jn &quot;3:16&quot;""#) && !h.contains("&amp;quot;"), "{h}");
}

// ---------------------------------------------------------------- ISS-29

#[test]
fn qa_iss29_splice_needs_the_split_shape() {
    // a complete entity before a separate ref is untouched
    let h = clean_esword_text("caf&eacute; <ref>Jn 1:1</ref>");
    assert!(h.starts_with("café <span class=\"bible-ref\" data-book=\"Jn\""), "{h}");
    // upper-case split entity
    let h = clean_esword_text("G&Eacute<ref>;N 1:1</ref>");
    assert!(h.contains(r#"data-book="GÉN""#), "{h}");
    // no letters before the ref after a valid entity: nothing is moved
    let h = clean_esword_text("x &amp; <ref>Jn 1:1</ref>");
    assert!(h.starts_with("x &amp; <span class=\"bible-ref\" data-book=\"Jn\""), "{h}");
}

// ---------------------------------------------------------------- properties

/// Property: for generated verses mixing real own-tag groups with tag damage, the tagged
/// html shows exactly the visible text of the plain html, word spans balance, and no
/// internal marker ever leaks — in both modes.
#[test]
fn qa_property_tagged_visible_text_equals_plain_and_markers_never_leak() {
    const PARTS: &[&str] = &[
        "palabra ", "tierra ", "SEÑOR ", "Él ", "&quot;Yo ", "Se&ntilde;or ", "<tvm>NCFSA</tvm> ",
        "<tvm>C</tvm> ", "<num>H127</num> ", "<num>H859</num>", "<br", "<brPalabra ", "</b ",
        "<b>", "</b>", "<red>", "</red>", "<ref>", "</ref>", "Jn 3:16", "<i>", "</i>", "<", ">",
        "&lt;", "&gt;", "&amp;", "\\par", "\\par ", "<sup>a</sup>", "<p>", "\n", ", ", ". ", "• ",
        "<span style=\"font-weight:bold\">", "</span>", "<tvm>", "</tvm>", "<num>", "</num>",
        "&lt;BR&gt;", "{\\i x}", "Gén_1:1 ", "g&eacut<ref>e;n 1:8</ref> ",
    ];
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for case in 0..400 {
        let n = 2 + (next() % 30) as usize;
        let raw: String = (0..n).map(|_| PARTS[(next() % PARTS.len() as u64) as usize]).collect();
        let plain = clean_esword_text(&raw);
        let tagged = clean_bible_verse_tagged(&raw);
        assert_no_marker_leak(&raw, &plain);
        assert_no_marker_leak(&raw, &tagged);
        assert_eq!(visible(&tagged), visible(&plain), "case {case}: {raw:?}\nT {tagged}\nP {plain}");
        let spans = tagged.matches(r#"class="word""#).count();
        assert!(spans <= own_tag_units(&raw).len(), "case {case}: more word spans than units: {raw:?} -> {tagged}");
        for h in [&plain, &tagged] {
            assert_eq!(h.matches("<span").count(), h.matches("</span>").count(), "case {case}: {raw:?} -> {h}");
            assert_eq!(h.matches("<strong>").count(), h.matches("</strong>").count(), "case {case}: {h}");
            assert_eq!(h.matches("<em>").count(), h.matches("</em>").count(), "case {case}: {h}");
        }
        assert!(!to_plain(&plain).contains('⟦'), "case {case}");
    }
}

// ---------------------------------------------------------------- library (read-only)

fn rows(file: &str, sql: &str) -> Option<Vec<String>> {
    let dir = library_dir()?;
    let path = format!("{dir}/{file}");
    let Some(conn) = open_ro(&path) else {
        eprintln!("SKIP: cannot open {path}");
        return None;
    };
    let mut st = conn.prepare(sql).unwrap_or_else(|e| panic!("{file}: {sql}: {e}"));
    let v: Vec<String> = st.query_map([], |r| cell(r, 0)).unwrap().flatten().collect();
    Some(v)
}

#[test]
fn qa_golden_ta_bbli_consecutive_par_never_visible() {
    let Some(v) = rows("TA.bbli", r"SELECT Scripture FROM Bible WHERE Scripture LIKE '%\par\par%'") else { return };
    assert!(v.len() >= 50, "fixture changed: {} rows", v.len());
    for raw in &v {
        let h = clean_esword_text(raw);
        assert!(!h.contains("\\par"), "{h}");
    }
}

#[test]
fn qa_golden_strongspa_rimmon_has_no_internal_marker() {
    let Some(v) = rows(
        "Strongspa.lexi",
        "SELECT Definition FROM Lexicon WHERE Topic='H7417'",
    ) else {
        return;
    };
    assert_eq!(v.len(), 1, "fixture changed");
    let raw = &v[0];
    assert!(raw.contains("(<ref>1Cr 6:62</num> [77])"), "fixture changed: {raw}");
    let h = clean_esword_text(raw);
    assert_no_marker_leak(raw, &h);
    assert!(!to_plain(&h).contains('⟦'), "{h}");
    assert!(h.contains(r#"data-strong="H7416""#), "{h}");
    assert!(visible(&h).contains("1Cr 6:62 [77]"), "{h}");
}

#[test]
fn qa_golden_esp_port_double_escaped_br_separates_words() {
    let Some(v) = rows(
        "esp-port_diccionario_espaol-portugus.dcti",
        "SELECT Definition FROM Dictionary WHERE Definition LIKE '%taxar. &lt;BR&gt;aprecio%'",
    ) else {
        return;
    };
    assert!(!v.is_empty(), "fixture changed");
    for raw in &v {
        let vis = visible(&clean_esword_text(raw));
        assert!(vis.contains("taxar. aprecio"), "{vis}");
        assert!(!vis.contains("taxar.aprecio"), "{vis}");
    }
}

/// ISS-36 layout primitive for WP3's detector, on the first 20 verses that carry `<num>`.
/// Note for WP3: in these Bibles ~87% of groups attach to a word (mid-phrase groups stay
/// untagged by design), so the ≥90% rule must be computed on `after_tvm`, not `attached`.
#[test]
fn qa_golden_layout_stats_separate_word_tvm_num_bibles_from_others() {
    let sql = "SELECT Scripture FROM Bible WHERE Scripture LIKE '%<num>%' ORDER BY Book, Chapter, Verse LIMIT 20";
    for (f, tagged) in [
        ("00RVR1960+.bbli", true),
        ("10LBLA+.bbli", true),
        ("00LBLA+.bbli", true),
        ("AA-rv1960+_reina_valera_1960_con_strong.bbli", false),
        ("00RVR1960.bbli", false),
    ] {
        let Some(v) = rows(f, sql) else { return };
        let (mut g, mut t, mut a) = (0usize, 0usize, 0usize);
        for raw in &v {
            let s = own_tag_layout_stats(raw);
            g += s.num_groups;
            t += s.after_tvm;
            a += s.attached;
        }
        eprintln!("{f}: verses={} groups={g} after_tvm={t} attached={a}", v.len());
        let ok = g > 0 && t * 10 >= g * 9 && a > 0;
        assert_eq!(ok, tagged, "{f}: groups={g} after_tvm={t} attached={a}");
    }
}

/// Property on real verses: tagged html keeps the plain html's visible text, has exactly
/// one word span per own-tag unit, and never leaks a marker. Full Bible in release builds;
/// every 13th verse in debug builds to keep `cargo test --lib` fast.
fn tagged_bible_property(file: &str) {
    let Some(v) = rows(file, "SELECT Scripture FROM Bible ORDER BY Book, Chapter, Verse") else { return };
    assert!(v.len() > 7000, "{file}: {} verses", v.len());
    let stride = if cfg!(debug_assertions) { 13 } else { 1 };
    let (mut checked, mut units_total) = (0usize, 0usize);
    for raw in v.iter().step_by(stride) {
        let tagged = clean_bible_verse_tagged(raw);
        let plain = clean_esword_text(raw);
        assert_eq!(visible(&tagged), visible(&plain), "{file}: {raw}");
        let units = own_tag_units(raw);
        assert_eq!(tagged.matches(r#"<span class="word""#).count(), units.len(), "{file}: {raw}");
        for u in &units {
            assert!(u.word.chars().all(is_word_char), "{file}: partial word {u:?} in {raw}");
            assert!(!u.strongs.is_empty() && u.strongs.iter().all(|c| RE_STRONG_CODE.is_match(c)), "{u:?}");
        }
        assert_no_marker_leak(raw, &tagged);
        checked += 1;
        units_total += units.len();
    }
    eprintln!("{file}: {checked} verses checked, {units_total} tagged words");
    assert!(units_total > checked, "{file}: expected tagged words");
}

#[test]
fn qa_golden_property_rvr1960_plus_tagged_verses() {
    tagged_bible_property("00RVR1960+.bbli");
}

#[test]
fn qa_golden_property_lbla_plus_tagged_verses() {
    tagged_bible_property("10LBLA+.bbli");
}

#[test]
fn qa_golden_property_lbla_nt_plus_tagged_verses() {
    tagged_bible_property("00LBLA-NT+.bbli");
}
