// ISS-05 (UI half): once the backend re-escapes text (WP2), verse html contains &amp; / &lt; / &gt;.
// tokenizeWords must keep entities atomic: never split them, never make them tappable words.
// Before the fix 'S&amp;EÑOR' became <span>S&amp</span>;<span>EÑOR</span> → «S&;EÑOR» on screen.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;

// Verbatim copy of the pre-fix tokenizer (app.v7.js @ HEAD 6748155) — the reference for
// "entity-free text tokenizes exactly as before".
function tokenizeWordsBefore(html) {
  const parts = html.split(/(<[^>]+>)/);
  let out = "";
  let w = 0;
  for (const part of parts) {
    if (!part) continue;
    if (part.startsWith("<")) {
      out += part;
      continue;
    }
    out += part.replace(/([^\s.,;:!?¡¿()«»""''—–\-]+)/g, (word) => {
      const id = w++;
      return `<span class="word" data-word="${id}">${word}</span>`;
    });
  }
  return out;
}
function visible(html) {
  const el = document.createElement("div");
  el.innerHTML = html;
  return el.textContent;
}
function words(html) {
  const el = document.createElement("div");
  el.innerHTML = html;
  return el.querySelectorAll(".word").map((w) => w.textContent);
}
function spanTexts(html) {
  return [...html.matchAll(/<span class="word" data-word="\d+">([^<]*)<\/span>/g)].map((m) => m[1]);
}

scenario("'al S&amp;EÑOR.' — no span ends in '&amp', no bare ';', text preserved", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  const src = "Cantad al S&amp;EÑOR.";
  const out = tokenizeWords(src);
  for (const t of spanTexts(out)) {
    assert(!/&[#A-Za-z0-9]*$/.test(t), `span ends inside an entity: «${t}»`);
    assert(!/^;/.test(t) && t !== ";", `span starts with a bare ';': «${t}»`);
    assert(!t.includes("&amp;"), `&amp; tapped as part of a word: «${t}»`);
  }
  eq(visible(out), visible(src), "visible text unchanged");
  excludes(visible(out), "&;", "no «&;» artefact");
  eq(words(out), ["Cantad", "al", "S", "EÑOR"], "tappable words");
});

scenario("'&lt; Prendieron' gives exactly one word, «Prendieron»", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  const out = tokenizeWords("&lt; Prendieron");
  eq(words(out), ["Prendieron"], "words");
  eq(visible(out), "< Prendieron", "visible text");
  const angled = tokenizeWords("este temor &lt;servil&gt; a Dios");
  eq(words(angled), ["este", "temor", "servil", "a", "Dios"], "angle-quoted word stays tappable");
  eq(visible(angled), "este temor <servil> a Dios", "angle quotes visible");
});

scenario("Letter entities stay inside their word; numeric and hex forms too", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  eq(words(tokenizeWords("Espa&ntilde;a y Jos&#233; y Mois&#xE9;s")), ["España", "y", "José", "y", "Moisés"], "letter entities");
  eq(words(tokenizeWords("uno&nbsp;dos &mdash; tres&hellip;")), ["uno", "dos", "tres"], "punctuation entities separate words");
  eq(words(tokenizeWords("a &amp;amp; b")), ["a", "amp", "b"], "double-escaped text: the entity itself is never split");
  const odd = tokenizeWords("x &unknown y & z &#0; w");
  eq(visible(odd), visible("x &unknown y & z &#0; w"), "malformed / bare '&' text preserved");
});

scenario("Entity-free text tokenizes exactly as before (whole RV1960 Juan 3)", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  const ch = state.chapterData;
  assert(ch && ch.verses && ch.verses.length >= 36, "recorded RV1960 Jn 3 loaded");
  let n = 0;
  for (const v of ch.verses) {
    if (/&(?:#\d+|#x[0-9a-f]+|[a-z][a-z0-9]*);/i.test(v.html)) continue;
    eq(tokenizeWords(v.html), tokenizeWordsBefore(v.html), `Jn 3:${v.verse} tokenization changed`);
    n++;
  }
  assert(n >= 36, "compared every verse");
  const v16 = $id("bible-content").querySelector('.verse[data-verse="16"] .verse-body');
  includes(v16.textContent, "Porque de tal manera amó Dios al mundo", "Jn 3:16 rendered");
});

scenario("Tags are still passed through untouched", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  const src = '<span class="woc">Yo soy</span> el <span class="strong-ref" data-strong="G25">G25</span> &amp; fin';
  const out = tokenizeWords(src);
  includes(out, '<span class="woc">', "woc span kept");
  includes(out, 'data-strong="G25"', "strong-ref kept");
  eq(visible(out), visible(src), "visible text unchanged");
});

scenario("linkifyHtml passes through only real tags; refs between stray '<' '>' are linked", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  const stray = linkifyHtml("si a < b, ver Juan 3:16 > c");
  includes(stray, 'class="bible-ref"', "reference linked although surrounded by stray brackets");
  eq(visible(stray), "si a < b, ver Juan 3:16 > c", "visible text unchanged");
  const ent = linkifyHtml("temor &lt;servil&gt; a Dios; G26 &amp; Juan 3:16");
  eq(visible(ent), "temor <servil> a Dios; G26 & Juan 3:16", "entities untouched");
  includes(ent, 'data-strong="G26"', "Strong linked next to an entity");
  includes(ent, 'data-chapter="3" data-verse="16"', "reference linked after an entity");
  const tags = linkifyHtml('<b>Juan 3:16</b><!-- nota --><span class="woc">G26</span>');
  includes(tags, "<b>", "real tag kept");
  includes(tags, "<!-- nota -->", "comment kept verbatim");
  includes(tags, 'data-strong="G26"', "text inside real tags still linkified");
});
