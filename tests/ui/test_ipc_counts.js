// ISS-21 (duplicate handlers / render token) and ISS-09 (one cached probe per query change).
// Baselines before the fix: a G25 chip click = 4 resolve_word, 5 probes, 6 renders; a word tap =
// 2 probes; Buscar = 2 probes + 2 lookups; returning-user startup on Diccionario = 5 probes.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: "" };
const returning = (study) => ({ nav: NAV, study: Object.assign({ modulesByKind: SAVED }, study) });
const count = (mark, cmd) => callsSince(mark, cmd).length;
const probeCount = (mark) => count(mark, "probe_modules_content");

scenario("G25 chip in Diccionario → ≤1 resolve_word, ≤1 probe, ≤2 renders", () => {
  boot({ storage: returning({ workspace: "diccionario", selectedWord: "amó" }) });
  const chip = $id("study-content").querySelector('.sync-bar .chip-strong[data-strong="G25"]');
  assert(chip, "Strong chip G25 rendered in the dictionary sync bar");
  const mark = H.calls.length;
  const w = H.writes.length;
  clickEl(chip);
  settle();
  eq(state.workspace, "lexico", "chip opens Léxico");
  lte(count(mark, "resolve_word"), 1, "resolve_word calls");
  lte(probeCount(mark), 1, "probe calls");
  lte(writesSince(w).length, 2, "#study-content writes");
  includes(studyText(), "Léxico ·", "Léxico rendered");
  assert(callsSince(mark, "lookup_lexicon").some((c) => c.args.strong === "G25" && c.args.modulePath === STRONGES), "G25 looked up in the saved lexicon");
});

scenario("'Ir a Léxico' from Diccionario → 0 resolve, ≤1 probe, ≤2 renders", () => {
  boot({ storage: returning({ workspace: "diccionario", selectedWord: "amó" }) });
  const mark = H.calls.length;
  const w = H.writes.length;
  clickEl($id("btn-jump-lex-from-dict"));
  settle();
  eq(state.workspace, "lexico", "on Léxico");
  eq(count(mark, "resolve_word"), 0, "no re-resolve: the result already holds G25");
  lte(probeCount(mark), 1, "probe calls");
  lte(writesSince(w).length, 2, "#study-content writes");
  excludes(studyText(), "No se encontró definición", "definition found");
});

scenario(".original-chip in the Léxico card → exactly 1 resolve_word and ≤2 renders", () => {
  boot({ storage: returning({ workspace: "lexico", selectedWord: "amó", lastStrong: "G25" }) });
  const chip = $id("study-content").querySelector(".original-chip[data-lookup]");
  assert(chip, "an original-language chip is rendered (interlinear hits of Jn 3:16)");
  const mark = H.calls.length;
  const w = H.writes.length;
  clickEl(chip);
  settle();
  eq(count(mark, "resolve_word"), 1, "resolve_word calls");
  lte(writesSince(w).length, 2, "#study-content writes");
  eq(state.selectedWord, chip.dataset.lookup, "the chip's word is the new query");
});

scenario("Buscar in Diccionario → 1 resolve, 1 probe, 1 lookup", () => {
  boot({ storage: returning({ workspace: "diccionario", selectedWord: "amó" }) });
  $id("dict-query").value = "gracia";
  const mark = H.calls.length;
  clickEl($id("btn-dict-search"));
  settle();
  eq(count(mark, "resolve_word"), 1, "resolve_word calls");
  eq(count(mark, "lookup_dictionary"), 1, "lookup_dictionary calls");
  eq(probeCount(mark), 1, "probe calls");
  includes(studyText(), "«gracia»", "renders the new query");
});

scenario("Buscar again with the same query → inline spinner, no blanking, 0 probes", () => {
  boot({ storage: returning({ workspace: "diccionario", selectedWord: "amó" }) });
  const before = studyText();
  includes(before, "«amó»", "dictionary rendered for amó");
  H.delivery = "hold";
  const mark = H.calls.length;
  const w = H.writes.length;
  clickEl($id("btn-dict-search"));
  drainMicrotasks();
  releaseWhere((c) => c.cmd === "resolve_word"); // word resolved; the re-render is now waiting on its lookup
  assert(H.pending.some((c) => c.cmd === "lookup_dictionary"), "dictionary lookup in flight");
  const content = $id("study-content");
  assert(content.classList.contains("is-refreshing"), "pane shows the inline refresh state");
  eq(content.getAttribute("aria-busy"), "true", "aria-busy while refreshing");
  eq(writesSince(w).length, 0, "pane not blanked to «Cargando…»");
  settle({ order: "fifo" });
  assert(!content.classList.contains("is-refreshing"), "refresh state cleared");
  assert(content.getAttribute("aria-busy") !== "true", "aria-busy cleared");
  eq(probeCount(mark), 0, "cached probe reused");
  includes(studyText(), "«amó»", "same content after refresh");
});

scenario("Side-pane Strong chip still navigates to Léxico", () => {
  boot({ storage: returning({ workspace: "diccionario", selectedWord: "amó" }) });
  const chip = $id("link-panel-inline").querySelector('.chip-strong[data-strong="G25"]');
  assert(chip, "side pane shows the G25 chip");
  const mark = H.calls.length;
  clickEl(chip);
  settle();
  eq(state.workspace, "lexico", "navigated to Léxico");
  lte(count(mark, "resolve_word"), 1, "resolve_word calls");
  assert(callsSince(mark, "lookup_lexicon").some((c) => c.args.strong === "G25"), "G25 looked up");
});

scenario("Word tap from Biblia → 1 resolve, ≤1 probe, 1 dictionary lookup sent before the probe", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  const mark = H.calls.length;
  clickEl(wordEl("amó", 16));
  settle();
  eq(count(mark, "resolve_word"), 1, "resolve_word calls");
  eq(count(mark, "lookup_dictionary"), 1, "lookup_dictionary calls");
  lte(probeCount(mark), 1, "probe calls");
  const look = callsSince(mark, "lookup_dictionary")[0];
  const probe = callsSince(mark, "probe_modules_content")[0];
  assert(!probe || look.seq < probe.seq, "the main lookup is sent before the probe");
});

scenario("Same word tapped twice → 0 probes the second time", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  clickEl(wordEl("amó", 16));
  settle();
  const mark = H.calls.length;
  clickEl(wordEl("amó", 16));
  settle();
  eq(probeCount(mark), 0, "second tap reuses the cached probe");
  eq(count(mark, "lookup_dictionary"), 1, "one lookup");
});

scenario("Tab switch → ≤1 probe", () => {
  boot({ storage: returning({ workspace: "diccionario", selectedWord: "amó" }) });
  const mark = H.calls.length;
  clickEl(document.querySelector('.ws-tab[data-ws="comentario"]'));
  settle();
  lte(probeCount(mark), 1, "probe calls on Diccionario → Comentario");
  const mark2 = H.calls.length;
  clickEl(document.querySelector('.ws-tab[data-ws="lexico"]'));
  settle();
  lte(probeCount(mark2), 1, "probe calls on Comentario → Léxico");
});

scenario("Returning-user startup on Diccionario → ≤1 probe, 1 resolve, 1 lookup", () => {
  boot({ storage: returning({ workspace: "diccionario", selectedWord: "amó" }) });
  lte(probeCount(0), 1, "probes at startup");
  eq(count(0, "resolve_word"), 1, "resolve_word at startup");
  eq(count(0, "lookup_dictionary"), 1, "lookup_dictionary at startup");
  includes(studyText(), "Diccionario · NELSON", "saved dictionary rendered");
});

scenario("Probe cache is bounded (LRU of 50)", () => {
  boot({ storage: returning({ workspace: "diccionario", selectedWord: "amó" }) });
  for (let i = 0; i < 60; i++) probeCachePut(`dictionary|w${i}`, new Map());
  lte(state.probeCache.size, 50, "cache size");
  assert(!state.probeCache.has("dictionary|w0") && state.probeCache.has("dictionary|w59"), "oldest entries evicted first");
});

scenario("Word lookups on Comentario do not refetch the verse's other commentaries", () => {
  boot({ storage: returning({ workspace: "comentario", selectedWord: "amó" }) });
  const content = $id("study-content");
  content.innerHTML += `<div class="study-body">${linkifyHtml("ἀγάπη")}</div>`;
  const mark = H.calls.length;
  clickEl(content.querySelector(".greek-run"));
  settle();
  eq(count(mark, "resolve_word"), 1, "the Greek word is resolved once");
  const primary = state.studyModule;
  const others = callsSince(mark, "get_commentary").filter((c) => c.args.modulePath !== primary);
  eq(others.length, 0, "side pane (other commentaries) not refetched for a word");
});
