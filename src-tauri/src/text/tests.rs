//! WP2 unit tests for the text cleaner (no module library needed).
//! Each test names the audit issue it guards; most fail on the pre-WP2 cleaner (`legacy`).

use super::gate_tests::visible;
use super::*;

const GN_12_3_RVR1960P: &str = "Bendeciré <tvm>C</tvm> <tvm>Vbw1-S-C</tvm> <num>H1288</num> a <tvm>VbR-MPC</tvm> <tvm>RS2MS</tvm> <num>H1288</num> <num>H859</num> los que te bendijeren, y <tvm>C</tvm> a <tvm>VbR-MSC</tvm> <tvm>RS2MS</tvm> <num>H7043</num> <num>H859</num> los que te maldijeren maldeciré <tvm>VaI1-S</tvm> <num>H779</num>; y <tvm>C</tvm> serán <tvm>Vdp3-P</tvm> <num>H1288</num> benditas en <tvm>P</tvm> ti <tvm>RS2MS</tvm> <num>H859</num> todas <tvm>NC-SC</tvm> <num>H3605</num> las <tvm>NCFPC</tvm> <num>H4940</num> familias de la <tvm>A</tvm> tierra <tvm>NCFSA</tvm> <num>H127</num>.";
const GN_1_1_RVR1960P: &str = "En <tvm>P</tvm> el principio <tvm>NCFSA</tvm> <num>H7225</num> creó <tvm>VaP3MS</tvm> <num>H1254</num> Dios <tvm>NCMPA</tvm> <num>H430</num> • <tvm>PO</tvm> <num>H853</num> los <tvm>A</tvm> cielos <tvm>NCMPA</tvm> <num>H8064</num> y <tvm>C</tvm> <tvm>PO</tvm> <num>H853</num> la <tvm>A</tvm> tierra <tvm>NC-SA</tvm> <num>H776</num>.";

fn strongs_of<'a>(units: &'a [OwnTagUnit], word: &str) -> Option<&'a Vec<String>> {
    units.iter().find(|u| u.word == word).map(|u| &u.strongs)
}

// ---------------------------------------------------------------- ISS-36

#[test]
fn iss36_own_tag_units_gn_12_3_tierra_has_h127_familias_untagged() {
    let u = own_tag_units(GN_12_3_RVR1960P);
    assert_eq!(strongs_of(&u, "tierra"), Some(&vec!["H127".to_string()]), "{u:?}");
    assert_eq!(strongs_of(&u, "familias"), None, "{u:?}");
    // mid-phrase layout ('a <tvm> <tvm> <num><num>') stays untagged by design
    assert_eq!(strongs_of(&u, "a"), None, "{u:?}");
    assert_eq!(strongs_of(&u, "maldeciré"), Some(&vec!["H779".to_string()]));
}

#[test]
fn iss36_own_tag_units_gn_1_1_principio_creo_dios() {
    let u = own_tag_units(GN_1_1_RVR1960P);
    assert_eq!(strongs_of(&u, "principio"), Some(&vec!["H7225".to_string()]));
    assert_eq!(strongs_of(&u, "creó"), Some(&vec!["H1254".to_string()]));
    assert_eq!(strongs_of(&u, "Dios"), Some(&vec!["H430".to_string()]));
    // '• <tvm>PO</tvm> <num>H853</num>' and 'y <tvm>C</tvm> <tvm>PO</tvm>' never attach
    assert_eq!(strongs_of(&u, "y"), None);
    assert!(!u.iter().any(|x| x.strongs.contains(&"H853".to_string())), "{u:?}");
}

#[test]
fn iss36_multi_num_group_keeps_all_codes_first_is_primary() {
    let u = own_tag_units("los que te bendijeren <tvm>VbR-MPC</tvm> <num>H1288</num> <num>h859</num>, y");
    assert_eq!(
        strongs_of(&u, "bendijeren"),
        Some(&vec!["H1288".to_string(), "H859".to_string()])
    );
}

#[test]
fn iss36_lbla_senor_gets_h3068_and_entity_quote_is_a_boundary() {
    let u = own_tag_units("Pero <tvm>C</tvm> el SEÑOR <tvm>NPMSA</tvm> <num>H3068</num> dijo");
    assert_eq!(strongs_of(&u, "SEÑOR"), Some(&vec!["H3068".to_string()]));
    let u = own_tag_units("&quot;Yo <tvm>RP1-S</tvm> <num>H595</num> soy");
    assert_eq!(strongs_of(&u, "Yo"), Some(&vec!["H595".to_string()]), "{u:?}");
    // a letter entity means we only see the tail of a word: never tag a partial word
    let u = own_tag_units("Se&ntilde;or <tvm>NPMSA</tvm> <num>H3068</num>");
    assert!(u.is_empty(), "{u:?}");
}

#[test]
fn iss36_untagged_and_empty_inputs_give_no_units() {
    assert!(own_tag_units("").is_empty());
    assert!(own_tag_units("En el principio creó Dios los cielos y la tierra.").is_empty());
    // number-before-word layout (AA-rv1960+) is not this layout
    assert!(own_tag_units("<num>H7225</num> principio <num>H1254</num> creó").is_empty());
}

#[test]
fn iss36_layout_stats_count_groups() {
    let st = own_tag_layout_stats(GN_1_1_RVR1960P);
    assert_eq!(st.num_groups, 7);
    assert_eq!(st.after_tvm, 7);
    assert_eq!(st.attached, 5);
    let st = own_tag_layout_stats("<num>H7225</num> principio");
    assert_eq!((st.num_groups, st.after_tvm, st.attached), (1, 0, 0));
}

#[test]
fn iss36_clean_bible_verse_tagged_wraps_word_and_keeps_visible_text() {
    let tagged = clean_bible_verse_tagged(GN_12_3_RVR1960P);
    let plain = clean_esword_text(GN_12_3_RVR1960P);
    assert!(
        tagged.contains(r#"<span class="word" data-strong="H127">tierra</span>"#),
        "{tagged}"
    );
    assert!(!tagged.contains(r#">familias</span>"#), "{tagged}");
    assert_eq!(visible(&tagged), visible(&plain));
    // Strong chips and morph spans are unchanged
    assert!(tagged.contains(r#"<span class="strong-ref" data-strong="H127">H127</span>"#));
    assert!(tagged.contains(r#"<span class="morph">NCFSA</span>"#));
    // untagged clean output never contains word spans
    assert!(!plain.contains(r#"class="word""#));
}

#[test]
fn iss36_tagged_multi_code_attribute() {
    let h = clean_bible_verse_tagged("te bendijeren <tvm>X</tvm> <num>H1288</num> <num>H859</num>");
    assert!(h.contains(r#"<span class="word" data-strong="H1288 H859">bendijeren</span>"#), "{h}");
}

// ---------------------------------------------------------------- ISS-05

#[test]
fn iss05_escaped_angle_text_is_kept_not_deleted() {
    let h = clean_esword_text("La forma &lt;qal&gt; del verbo");
    assert!(h.contains("&lt;qal&gt;"), "{h}");
    assert_eq!(to_plain(&h), "La forma <qal> del verbo");
    let h = clean_esword_text("a &lt; b y c &gt; d");
    assert_eq!(h, "a &lt; b y c &gt; d");
    let h = clean_esword_text("este temor &lt;servil&gt; a Dios");
    assert_eq!(visible(&h), "este temor <servil> a Dios");
    let h = clean_esword_text("&lt;A Psalm of David&gt; Give ear");
    assert!(visible(&h).contains("<A Psalm of David>"), "{h}");
    let h = clean_esword_text("&lt;a Cristo&gt; y");
    assert!(visible(&h).contains("<a Cristo>"), "{h}");
}

#[test]
fn iss05_escaped_markup_never_becomes_live() {
    for raw in [
        "&lt;img src=x onerror=alert(1)&gt;",
        "&lt;form action=x&gt;&lt;button&gt;go&lt;/button&gt;&lt;/form&gt;",
        "&lt;script&gt;alert(1)&lt;/script&gt;",
        "&amp;lt;script&amp;gt;x&amp;lt;/script&amp;gt;",
        "&lt;meta http-equiv=refresh content=0&gt;",
    ] {
        let h = clean_esword_text(raw);
        let lower = h.to_lowercase();
        for bad in ["<img", "<form", "<button", "<script", "<meta"] {
            assert!(!lower.contains(bad), "{raw} -> {h}");
        }
    }
    let h = clean_esword_text("&lt;script&gt;");
    assert_eq!(h, "&lt;script&gt;");
}

#[test]
fn iss05_quote_in_ref_is_escaped_in_data_raw() {
    let h = clean_esword_text(r#"<ref>Jn "3:16" x</ref>"#);
    assert!(h.contains(r#"data-raw="Jn &quot;3:16&quot; x""#), "{h}");
    assert!(!h.contains(r#"data-raw="Jn "3"#), "{h}");
}

#[test]
fn iss05_forged_markers_never_render() {
    for raw in [
        "x ⟦REF:Jn 3:16⟧ y",
        "x &#10214;REF:Jn 3:16&#10215; y",
        "x &#x27E6;STRONG:H1&#x27e7; y",
        "x &LeftDoubleBracket;WOC&RightDoubleBracket; y",
        "x &amp;#10214;REF:Jn 3:16&amp;#10215; y",
    ] {
        let h = clean_esword_text(raw);
        assert!(!h.contains("<span"), "{raw} -> {h}");
        assert!(visible(&h).starts_with("x ") && visible(&h).ends_with(" y"), "{raw} -> {h}");
    }
    // literal double brackets are real text (UBS/Metzger ⟦…⟧) and stay visible
    let h = clean_esword_text("οὐ γὰρ οἴδασιν τί ποιοῦσιν.⟧ {A} y ⟦<b>x</b>⟧");
    assert_eq!(h, "οὐ γὰρ οἴδασιν τί ποιοῦσιν.⟧ {A} y ⟦<strong>x</strong>⟧");
}

#[test]
fn iss05_double_escaped_formatting_is_dropped() {
    let h = clean_esword_text("&lt;i&gt;See: &lt;term&gt;Gheber&lt;/term&gt;&lt;/i&gt;");
    assert_eq!(visible(&h), "See: Gheber");
    assert_eq!(clean_esword_text("<p>&lt;new&gt;</p>"), "");
    assert_eq!(clean_esword_text("x &lt;tw://[self]?G26&gt; y"), "x y");
    let h = clean_esword_text("uno&lt;BR&gt;dos &lt;span class=&quot;x&quot;&gt;tres&lt;/span&gt;");
    assert_eq!(visible(&h), "unodos tres");
}

#[test]
fn iss05_malformed_fragments_are_not_exposed() {
    let h = clean_esword_text("(<span style=“c;<span style=\"x\"><ref>Sal 32:1</ref></span>");
    assert!(!h.contains("&lt;span"), "{h}");
    assert!(h.contains(r#"data-book="Sal""#), "{h}");
    let h = clean_esword_text(
        r#"from the root of <a href="tw://[self]?<num><num>H1129</num></num>" style="color:#4D80B3;">H1129</a> through"#,
    );
    assert!(!h.contains("&lt;a"), "{h}");
    assert!(!h.contains("style="), "{h}");
    assert_eq!(visible(&h), "from the root of H1129 through");
    let h = clean_esword_text("<Lx>Δαυίδ</lx</num> <tvm>N</tvm> <b>de David</b>");
    assert!(!h.contains("&lt;/lx"), "{h}");
    assert!(h.contains("Δαυίδ"), "{h}");
}

#[test]
fn iss05_bare_tag_glued_to_next_tag_is_dropped() {
    // ebc.cmti: '</span></p<p class="indent1">&quot;And the priest'
    let h = clean_esword_text(r#"BLOOD</span></p<p class="indent1">&quot;And the priest"#);
    assert_eq!(visible(&h), "BLOOD \"And the priest");
    assert!(!h.contains("&lt;"), "{h}");
    // 05TS-LSCHAFER: 'style=""<ref>>ua 17:26</ref>' — no stray '>' in the label
    let h = clean_esword_text(r#"a <a href="x">J</a><a href="x" style=""<ref>>ua 17:26</ref></a>."#);
    assert_eq!(visible(&h), "a Jua 17:26.");
}

#[test]
fn iss05_unterminated_fragment_does_not_eat_following_words() {
    // CEVALLOS NT-INT+ Mk 4:8 shape: '<WG1520' must not swallow the next cells.
    let raw = "<td>uno<br><num>G1722>, <WG1520</num></td><td>triakonta<br>treinta</td>";
    let h = clean_esword_text(raw);
    let v = visible(&h);
    assert!(v.contains("triakonta"), "{v}");
    assert!(v.contains("treinta"), "{v}");
    // a stray opener followed by a real tag loses only the fragment
    let v = visible(&clean_esword_text("a <b texto <i>x</i> y"));
    assert!(v.contains('x') && v.ends_with('y'), "{v}");
}

#[test]
fn iss05_entities_decode_until_stable_and_escape_once() {
    let h = clean_esword_text("Trench, &amp;amp; Co.");
    assert_eq!(h, "Trench, &amp; Co.");
    assert_eq!(to_plain(&h), "Trench, & Co.");
    let h = clean_esword_text("cf. &amp;sect; 80");
    assert!(visible(&h).contains('§'), "{h}");
    let h = clean_esword_text("<ref>Gén 1:1 &amp; 3</ref>");
    assert!(!h.contains("&amp;amp;"), "{h}");
    assert!(h.contains(r#"data-raw="Gén 1:1 &amp; 3""#), "{h}");
    let h = clean_esword_text("<ref>Núm 8:12&gt;Cr 14:18</ref>");
    assert!(visible(&h).contains("Núm 8:12>Cr 14:18"), "{h}");
    assert!(!h.contains("&amp;gt;"), "{h}");
}

#[test]
fn iss05_kept_behaviour_refs_strongs_red_greek() {
    let h = clean_esword_text("Véase <ref>Gén 1:1-2</ref> y <num>g26</num> <red>texto</red> &#945;&gamma;&#x3AC;&pi;&eta;");
    assert!(
        h.contains(r#"<span class="bible-ref" data-book="Gén" data-chapter="1" data-verse="1" data-verse-end="2" title="Ver pasaje">Gén 1:1-2</span>"#),
        "{h}"
    );
    assert!(h.contains(r#"<span class="strong-ref" data-strong="G26">G26</span>"#), "{h}");
    assert!(h.contains(r#"<span class="woc">texto</span>"#), "{h}");
    assert!(h.contains("αγάπη"), "{h}");
    // '<ref>2Ki 18:13&mdash;19</ref>' still parses
    let h = clean_esword_text("<ref>2Ki 18:13&mdash;19</ref>");
    assert!(h.contains(r#"data-book="2Ki" data-chapter="18" data-verse="13" data-verse-end="19""#), "{h}");
}

#[test]
fn iss05_inline_tags_are_balanced() {
    // every </span> used to become a stray </strong>
    let h = clean_esword_text(r#"<span style="color:#C0C0C0;">el</span> texto"#);
    assert_eq!(h, "el texto");
    let h = clean_esword_text("<b>abierto sin cerrar");
    assert_eq!(h, "<strong>abierto sin cerrar</strong>");
    let h = clean_esword_text("cerrado</b> sin abrir");
    assert_eq!(h, "cerrado sin abrir");
    let h = clean_esword_text("<b>a <i>b</b> c</i>");
    assert_eq!(h, "<strong>a <em>b</em></strong><em> c</em>");
}

/// Property test: 200 deterministic pseudo-random strings built from hostile fragments
/// must only ever produce allowlisted tags and attributes.
#[test]
fn iss05_property_only_allowlisted_tags_are_emitted() {
    const PARTS: &[&str] = &[
        "<", ">", "&lt;", "&gt;", "&amp;", "&quot;", "\"", "'", "⟦", "⟧", "REF:", "&#10214;",
        "<ref>", "</ref>", "Jn 3:16", "<num>", "H430", "</num>", "<red>", "</red>", "<b>", "</b>",
        "<i>", "</i>", "<span style=\"font-weight:bold\">", "</span>", "<font color=red>", "</font>",
        "<tvm>", "</tvm>", "<sup>", "</sup>", "<script>", "<img src=x onerror=a>", "&lt;img src=x&gt;",
        "<a href=\"javascript:x\">", "</a>", "<br>", "<p>", "\n", " texto ", "ñ", "\\par ", "\\'85",
        "<table><tr><td>", "</td><td>", "<h2>", "</h2>", "&lt;i&gt;", "<WG1520", "</lx", "=\"x\"",
        "onclick=", "<blu>", "</blu>", "<em>", "&#146;", "&eacut<ref>e;n 1:8</ref>",
    ];
    // strict grammar: every attribute is name="value" with no quote, '<' or '>' inside
    static RE_TAG: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"<(/?)([A-Za-z0-9]+)((?:\s[a-z-]+="[^"<>]*")*)>"#).unwrap());
    static RE_ATTR: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\s([A-Za-z-]+)="([^"]*)""#).unwrap());
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for case in 0..200 {
        let n = 3 + (next() % 25) as usize;
        let raw: String = (0..n).map(|_| PARTS[(next() % PARTS.len() as u64) as usize]).collect();
        for tagged in [false, true] {
            let opts = CleanOptions { tagged_words: tagged, image_placeholder: case % 2 == 0, keep_codepage: case % 3 == 0 };
            let h = clean_esword_text_with(&raw, opts);
            assert!(!h.contains('\u{1}') && !h.contains('\u{2}') && !h.contains('\u{3}'), "case {case}: sentinel leaked: {h:?}");
            let mut rest = h.as_str();
            while let Some(i) = rest.find('<') {
                assert!(!rest[..i].contains('>'), "case {case}: raw '>' in text: {raw:?} -> {h}");
                let tail = &rest[i..];
                let m = RE_TAG.find(tail).filter(|m| m.start() == 0);
                let Some(m) = m else {
                    panic!("case {case}: raw '<' in output: {raw:?} -> {h}");
                };
                let caps = RE_TAG.captures(m.as_str()).unwrap();
                let name = caps[2].to_ascii_lowercase();
                assert!(
                    ["span", "em", "strong", "sup", "br"].contains(&name.as_str()),
                    "case {case}: tag <{name}> in {h}"
                );
                for a in RE_ATTR.captures_iter(&caps[3]) {
                    let an = &a[1];
                    assert!(
                        an == "class" || an == "title" || an.starts_with("data-"),
                        "case {case}: attr {an} in {h}"
                    );
                    if an == "class" {
                        assert!(
                            [
                                "woc", "interlinear", "bible-ref", "bible-ref bible-ref-raw",
                                "strong-ref", "morph", "word", "fn"
                            ]
                            .contains(&&a[2]),
                            "case {case}: class {} in {h}",
                            &a[2]
                        );
                    }
                }
                rest = &tail[m.end()..];
            }
            assert!(!rest.contains('>'), "case {case}: raw '>' in text: {raw:?} -> {h}");
        }
    }
}

// ---------------------------------------------------------------- ISS-24

#[test]
fn iss24_rtf_whitelist_on_html_fields() {
    assert_eq!(
        clean_esword_text("derrotó a un\\par destacamento"),
        "derrotó a un<br>destacamento"
    );
    assert_eq!(clean_esword_text("<p>\\parEl hombre"), "El hombre");
    assert_eq!(clean_esword_text("este \\'85 o:"), "este … o:");
    assert_eq!(clean_esword_text("x \\ldblquote y\\rdblquote  z"), "x “y” z");
    // unknown words and lone backslashes are never touched
    assert_eq!(clean_esword_text("\\the end"), "\\the end");
    assert_eq!(clean_esword_text("..\\..\\cdshop \\talmud \\index"), "..\\..\\cdshop \\talmud \\index");
}

#[test]
fn iss24_c1_numeric_entities_map_through_windows_1252() {
    let h = clean_esword_text("God&#146;s &#147;x&#148; a&#151;b x&#133;");
    assert_eq!(visible(&h), "God’s “x” a—b x…");
    assert_eq!(visible(&clean_esword_text("&#x92;")), "’");
}

#[test]
fn iss24_to_plain_never_glues_words() {
    assert_eq!(to_plain("uno<br>dos</p><p>tres"), "uno dos tres");
    assert_eq!(to_plain("<em>principio</em>."), "principio.");
    assert_eq!(to_plain("a</div>b</li>c<hr>d</h2>e"), "a b c d e");
}

#[test]
fn iss24_structure_italic_em_headings_tables() {
    let h = clean_esword_text(r#"<span style="font-style: italic">amado</span> y <em>amor</em>"#);
    assert_eq!(h, "<em>amado</em> y <em>amor</em>");
    let h = clean_esword_text("<h2>AMOR</h2>texto");
    assert_eq!(h, "<strong>AMOR</strong><br>texto");
    let h = clean_esword_text("<table><tr><td>a</td><td>b</td></tr></table>");
    assert_eq!(h, "a · b");
    // empty cells collapse; rows break; no separator at a row edge
    let h = clean_esword_text("<table><tr><td>a</td><td></td><td>&nbsp;</td><td>b</td></tr><tr><td></td><td>c</td></tr></table>");
    assert_eq!(h, "a · b<br>c");
    // OGNTe+ <Tr> is a transliteration tag, not a table row
    let h = clean_esword_text("[<Tr>Biblos</Tr> <grk>Βίβλος</grk>]");
    assert_eq!(h, "[Biblos Βίβλος]");
}

#[test]
fn iss24_data_uri_image_is_fast_and_removed_by_default() {
    let mut raw = String::from("Ver Grafico. <img src=\"data:image/png;base64,");
    raw.push_str(&"QUJD".repeat(15 * 1024 * 1024 / 4));
    raw.push_str("\" width=\"34\" height=\"13\"> fin");
    let t = std::time::Instant::now();
    let h = clean_esword_text(&raw);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    eprintln!("15 MB data URI cleaned in {ms:.1} ms");
    assert_eq!(h, "Ver Grafico. fin");
    // Release budget is 30 ms; unoptimised test builds get a looser bound.
    let budget = if cfg!(debug_assertions) { 600.0 } else { 30.0 };
    assert!(ms < budget, "{ms} ms");
    let h = clean_esword_text_with(&raw, CleanOptions { image_placeholder: true, ..Default::default() });
    assert_eq!(h, "Ver Grafico. [imagen] fin");
    // escaped '&lt;img' stays inert text
    assert!(clean_esword_text("&lt;img src=x&gt;").starts_with("&lt;img"));
}

// ---------------------------------------------------------------- ISS-20

#[test]
fn iss20_tvm_becomes_morph_and_plain_drops_it() {
    let h = clean_esword_text("En <tvm>P</tvm> el principio <tvm>NCFSA</tvm> <num>H7225</num>");
    assert!(h.contains(r#"<span class="morph">P</span>"#), "{h}");
    assert!(h.contains(r#"<span class="morph">NCFSA</span>"#), "{h}");
    assert_eq!(to_plain(&h), "En el principio H7225");
}

#[test]
fn iss20_sup_becomes_fn_and_plain_drops_it() {
    let h = clean_esword_text("nuevo<sup>a</sup> pacto");
    assert_eq!(h, r#"nuevo<sup class="fn">a</sup> pacto"#);
    assert_eq!(to_plain(&h), "nuevo pacto");
}

#[test]
fn iss20_unclosed_tvm_sup_h_never_swallow_following_text() {
    let h = clean_esword_text("a <tvm>X b c d e <b>f</b>");
    assert!(!h.contains("morph"), "{h}");
    assert_eq!(visible(&h), "a X b c d e f");
    let h = clean_esword_text("nota<sup>1 y el resto del texto");
    assert!(!h.contains("<sup"), "{h}");
    assert_eq!(to_plain(&h), "nota1 y el resto del texto");
    let h = clean_esword_text("<h2>Título sin cerrar y todo el artículo");
    assert!(!h.contains("<strong>"), "{h}");
}

#[test]
fn iss20_ognt_greek_words_stay_outside_morph() {
    let raw = "[<Tr>Biblos</Tr> <grk>Βίβλος</grk> <Wn>#1</wn> <TF>biblos</Tf> <num>G976</num> <tvm>N-NSF</tvm> <b>Libro</b>]";
    let h = clean_esword_text(raw);
    assert!(h.contains(r#"<span class="morph">N-NSF</span>"#), "{h}");
    let outside = Regex::new(r#"<span class="morph">[^<]*</span>"#).unwrap().replace_all(&h, "");
    assert!(outside.contains("Βίβλος"), "{h}");
    assert!(outside.contains("Libro"), "{h}");
}

// ---------------------------------------------------------------- ISS-12

#[test]
fn iss12_headerless_rtf_fragment_converts() {
    let raw = r"\par {\cf2\b 1.} {\i Al principio}, como en {\cf11\ul Jua_7:50}). Cf. {\cf11\ul Pro_8:22} espa\'f1ol.\par ";
    let h = clean_esword_text(raw);
    assert!(h.contains("<strong>1.</strong>"), "{h}");
    assert!(h.contains("<em>Al principio</em>"), "{h}");
    assert!(
        h.contains(r#"<span class="bible-ref" data-book="Jua" data-chapter="7" data-verse="50" data-verse-end="50""#),
        "{h}"
    );
    assert!(h.contains("español"), "{h}");
    assert!(!h.contains('\\') && !h.contains('{') && !h.contains('}'), "{h}");
}

#[test]
fn iss12_fonttbl_dropped_and_plain_lines_kept() {
    // headerless fragment path
    let h = clean_esword_text(r"Nota {\fonttbl{\f0 Arial;}{\f1 Symbol;}}{\colortbl;\red0\green0\blue0;}{\i x} y");
    assert_eq!(h, "Nota <em>x</em> y");
    // classic RTF path (starts with '{' and has \f): font names no longer leak
    let t = rtf_to_text(r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}\f0 Hola \'e9l}");
    assert_eq!(t, "Hola él");
    assert_eq!(clean_esword_text("línea1\nlínea2"), "línea1<br>línea2");
    assert_eq!(clean_esword_text("\\the"), "\\the");
}

// ---------------------------------------------------------------- ISS-25

#[test]
fn iss25_underscore_refs_with_numeric_prefix() {
    assert_eq!(clean_esword_text("Cf. 1Co_13:4; 1Cr_6:3"), "Cf. 1Co 13:4; 1Cr 6:3");
    assert_eq!(clean_esword_text("Gén_25:16"), "Gén 25:16");
    assert_eq!(clean_esword_text("snake_case 3:4"), "snake_case 3:4");
    // 21_nombres_de_dios: '<span>&#x05B9;</span><span>xo_15:2</span>' (old converted it
    // only because a stray </strong> sat in between). Its «É» / «ú» went through the Hebrew
    // codepage; since 2026-11-18 the codepage repair gives them back.
    assert_eq!(
        clean_esword_text("<span>&#x05B9;</span><span>xo_15:2</span>; N&#x05EA;m_23:22"),
        "Éxo 15:2; Núm 23:22"
    );
    assert_eq!(
        clean_bible_text("<span>&#x05B9;</span><span>xo_15:2</span>; N&#x05EA;m_23:22"),
        "\u{5B9}xo 15:2; N\u{5EA}m 23:22",
        "Bible text keeps its letters"
    );
    assert_eq!(clean_esword_text("x1Co_13:4"), "x1Co_13:4");
}

#[test]
fn iss25_cross_chapter_ref_parses_with_extra_attributes_last() {
    let h = clean_esword_text("<ref>Jn 3:16-4:2</ref>");
    assert_eq!(
        h,
        r#"<span class="bible-ref" data-book="Jn" data-chapter="3" data-verse="16" data-chapter-end="4" data-verse-end2="2" title="Ver pasaje">Jn 3:16-4:2</span>"#
    );
    assert!(!h.contains("data-verse-end=\""), "crossrefs must keep its v1-only parse: {h}");
    // same chapter written with the chapter twice is a normal range
    let h = clean_esword_text("<ref>Sal 23:1-23:6</ref>");
    assert!(h.contains(r#"data-verse="1" data-verse-end="6""#), "{h}");
    // reversed chapters stay unparsed
    let h = clean_esword_text("<ref>Jn 4:2-3:16</ref>");
    assert!(h.contains("bible-ref-raw"), "{h}");
}

// ---------------------------------------------------------------- ISS-29

#[test]
fn iss29_split_entity_is_spliced_back_into_ref() {
    for (raw, book) in [
        ("g&eacut<ref>e;n 1:8</ref>", "gén"),
        ("&eacute<ref>;xo 3:14</ref>", "éxo"),
        ("n&uacut<ref>e;m 6:24</ref>", "núm"),
        ("g&aacut<ref>e;l 2:20</ref>", "gál"),
        ("am&oacu<ref>te;s 4:10</ref>", "amós"),
    ] {
        let h = clean_esword_text(raw);
        assert!(h.contains(&format!(r#"data-book="{book}""#)), "{raw} -> {h}");
        assert!(!h.contains("e;n") && !h.contains("bible-ref-raw"), "{raw} -> {h}");
    }
    let h = clean_esword_text("A&B <ref>Jn 1:1</ref>");
    assert_eq!(
        h,
        r#"A&amp;B <span class="bible-ref" data-book="Jn" data-chapter="1" data-verse="1" data-verse-end="1" title="Ver pasaje">Jn 1:1</span>"#
    );
    // invalid entity names are left alone
    let h = clean_esword_text("x&zzz<ref>q; 1:1</ref>");
    assert!(!h.contains("data-book=\"xq"), "{h}");
}
