// ISS-20 (5) + ISS-35: Paralela words are tappable, highlightable, and a tap resolves against
// the column's own Bible. Baseline before the fix: 0 .word spans in Paralela (raw v.html).

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NVI = `${LIB_DIR}/01NVI.bbli`;
const RVRP = `${LIB_DIR}/00RVR1960+.bbli`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const PAR = { mode: "paralela", parallel1: NVI, parallel2: RVRP };

function cell(verse, module) {
  return $id("bible-content").querySelector(`td.verse[data-verse="${verse}"][data-module="${module}"]`);
}

scenario("Paralela Juan 3: every column is tokenized (≥700 .word spans), inside span.verse-body", () => {
  boot({ storage: { nav: NAV, parallel: PAR } });
  const root = $id("bible-content");
  assert(root.querySelector(".parallel-table"), "parallel table rendered");
  const words = root.querySelectorAll(".parallel-table .word");
  assert(words.length >= 700, `expected ≥700 .word spans, got ${words.length}`);
  assert(words.every((w) => w.closest(".verse-body")), "every word sits inside .verse-body (highlight CSS applies)");
  for (const p of [RVR, NVI, RVRP]) {
    const c = cell(16, p);
    assert(c, `column cell for ${p.split("/").pop()} carries data-module`);
    assert(c.querySelectorAll(".word").length > 10, `${p.split("/").pop()} v16 has words`);
  }
  // Strong codes of the '+' column stay links, never words.
  const plus = cell(16, RVRP);
  assert(plus.querySelectorAll(".strong-ref").length > 0, "strong-ref spans kept in the RVR1960+ column");
  eq(plus.querySelectorAll(".strong-ref .word").length, 0, "no word inside a strong-ref");
  eq(words.filter((w) => /^[HG]\d+$/.test(w.textContent)).length, 0, "no Strong code is a .word");
});

scenario("Tapping a word in the NVI column resolves it with that column's Bible and verse", () => {
  boot({ storage: { nav: NAV, parallel: PAR } });
  const c = cell(17, NVI);
  const w = c.querySelectorAll(".word").find((x) => x.textContent === "mundo");
  assert(w, "NVI 3:17 has «mundo»");
  const mark = H.calls.length;
  clickEl(w);
  settle();
  const rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, "one resolve_word");
  eq(rw[0].args.word, "mundo", "word");
  eq(rw[0].args.biblePath, NVI, "biblePath is the tapped column's module, not the sidebar Bible");
  eq(rw[0].args.verse, 17, "verse of the tapped row");
  eq(rw[0].args.occurrence, 0, "first «mundo» of NVI 3:17 (counted within the cell)");
  eq(state.workspace, "diccionario", "a word tap still switches to Diccionario (user decision)");
  assert(w.classList.contains("word-selected"), "tapped word highlighted");
  eq(state.verse, 17, "tapped row becomes the current verse");
});

scenario("Same word, second column occurrence: counted per cell, not across the row", () => {
  boot({ storage: { nav: NAV, parallel: PAR } });
  const c = cell(17, RVR);
  const hits = c.querySelectorAll(".word").filter((x) => x.textContent === "mundo");
  assert(hits.length >= 2, "RVR1960 3:17 has «mundo» at least twice");
  const mark = H.calls.length;
  clickEl(hits[1]);
  settle();
  const rw = callsSince(mark, "resolve_word")[0];
  eq(rw.args.biblePath, RVR, "first column = sidebar Bible");
  eq(rw.args.occurrence, 1, "second «mundo» in its own cell");
});

scenario("Keep working: the verse-number cell still selects the verse; missing cells stay inert", () => {
  boot({ storage: { nav: NAV, parallel: PAR } });
  const num = $id("bible-content").querySelector('td.vnum[data-verse="18"]');
  const mark = H.calls.length;
  clickEl(num);
  settle();
  eq(state.verse, 18, "verse selected");
  eq(callsSince(mark, "resolve_word").length, 0, "no word lookup");
  eq(frag("—"), 0, "an em dash alone yields no word");
  function frag(html) {
    const el = document.createElement("div");
    el.innerHTML = tokenizeWords(html);
    return el.querySelectorAll(".word").length;
  }
});
