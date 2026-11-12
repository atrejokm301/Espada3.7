// ISS-19 — Greek/Hebrew runs in study text: one tappable span per word (combining marks never
// cut it), ≥2 base letters, Greek data-lookup folded (NFD, no marks, lowercase, ς→σ).

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
function spans(html, cls) {
  const box = document.createElement("div");
  box.innerHTML = linkifyHtml(html, { book: 43, chapter: 3 });
  return box.querySelectorAll(`.${cls}`).map((s) => ({ text: s.textContent, lookup: s.dataset.lookup }));
}
const bootPlain = () => boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });

scenario("Decoded 'ουτω' with a combining rough breathing → ONE span, data-lookup 'ουτω'", () => {
  bootPlain();
  const word = "οὑτω"; // ο υ ̔ τ ω (entity-decoded form, as in Clarke)
  eq(spans(`así ${word} dice`, "greek-run"), [{ text: word, lookup: "ουτω" }], "one run");
});

scenario("Precomposed 'ἀγάπη', 'οὑτω', 'λόγος' → one span each, folded lookups", () => {
  bootPlain();
  eq(spans("ἀγάπη, οὑτω y λόγος.", "greek-run"), [
    { text: "ἀγάπη", lookup: "αγαπη" },
    { text: "οὑτω", lookup: "ουτω" },
    { text: "λόγος", lookup: "λογοσ" },
  ], "runs");
});

scenario("Edge: one-letter words and stray marks are not links; punctuation stays outside", () => {
  bootPlain();
  eq(spans("ὁ y ἡ", "greek-run"), [], "single letters are not words");
  eq(spans("͂σσα", "greek-run"), [{ text: "σσα", lookup: "σσα" }], "a leading combining mark is left as text");
  eq(spans("λόγος· καί", "greek-run").map((x) => x.text), ["λόγος", "καί"], "ano teleia not inside the word");
});

scenario("Hebrew: one span per pointed word; data-lookup keeps the points (backend matches them)", () => {
  bootPlain();
  eq(spans("dice אֱלֹהִים aquí", "hebrew-run"), [{ text: "אֱלֹהִים", lookup: "אֱלֹהִים" }], "pointed word");
  eq(spans("א", "hebrew-run"), [], "one letter is not a word");
});

scenario("Real recorded commentary text: no Greek run is cut at a combining mark", () => {
  bootPlain();
  let runs = 0;
  for (const [k, v] of Object.entries(H.fixture.responses)) {
    if (!k.startsWith("get_commentary|") || !v.ok) continue;
    for (const e of v.ok) {
      const out = linkifyHtml(e.html, { book: 43, chapter: 3 });
      const re = /<span class="greek-run word-like" data-lookup="[^"]*">([^<]*)<\/span>/g;
      let m;
      while ((m = re.exec(out))) {
        runs++;
        const after = out[m.index + m[0].length] || "";
        assert(!/\p{M}/u.test(after), `run «${m[1]}» is followed by a combining mark (split word)`);
        assert(baseLetterCount(m[1]) >= 2, `run «${m[1]}» has < 2 letters`);
      }
    }
  }
  assert(runs > 0, "the recorded commentaries contain Greek runs");
});
