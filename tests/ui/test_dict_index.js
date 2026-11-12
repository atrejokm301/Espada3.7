// Diccionario topic index, e-Sword style (user choice 2026-11-11). Beside the article the module's
// topics are listed alphabetically: a tap jumps to the exact topic (highlighted) or marks where
// the word would go («▸ pierda»), a click opens exactly that topic, «Volver» returns to the word,
// «Anteriores…/Siguientes…» load more, typing in «Buscar…» moves the index without searching,
// ↓/↑ walk the topics, and Léxico / Comentario have no index. The backend's index is faked here
// with the same order rule (case and accents off, ñ after n).

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: "" };

const ikey = (s) => String(s).trim().toLowerCase().replace(/ñ/g, "n~").normalize("NFD").replace(/\p{M}/gu, "");
const TOPICS = [
  "ABOGADO", "ABRAM", "AMÓN", "AMOR", "ANZUELO", "AÑO", "NUBE", "ÑU", "OVEJA", "PIEDRA",
  "PIEDRAS PRECIOSAS", "PILATO, PONCIO", "PINO", "ZARZA",
  ...Array.from({ length: 300 }, (_, i) => `TEMA ${String(i).padStart(3, "0")}`),
]
  .sort((a, b) => (ikey(a) < ikey(b) ? -1 : ikey(a) > ikey(b) ? 1 : 0))
  .map((topic, i) => ({ topic, rowid: 1000 + i }));

function fakeIndex(a) {
  const total = TOPICS.length;
  let focus = null;
  let exact = false;
  if (a.query) {
    const k = ikey(a.query);
    focus = TOPICS.findIndex((t) => ikey(t.topic) >= k);
    if (focus < 0) focus = total;
    exact = focus < total && ikey(TOPICS[focus].topic) === k;
  }
  const center = a.anchor ?? focus ?? 0;
  const start = Math.max(0, center - (a.before ?? 40));
  const end = Math.min(total, center + (a.after ?? 80));
  const rows = TOPICS.slice(start, end).map((t, i) => ({ pos: start + i, rowid: t.rowid, topic: t.topic }));
  return { ok: { total, start, focus, exact, rows } };
}
const topicOf = (rowid) => (TOPICS.find((t) => t.rowid === rowid) || {}).topic;

function setup(word, entries = []) {
  H.overrides.resolve_word = (a) =>
    a.word === word
      ? { ok: { word, normalized: word, strongs: [], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: false, interlinearHits: [], matchedHit: null, matchKind: "" } }
      : undefined;
  H.overrides.lookup_dictionary = (a) => (a.query === word ? { ok: entries } : undefined);
  H.overrides.dictionary_index = fakeIndex;
  H.overrides.get_dictionary_entry = (a) => {
    const topic = topicOf(a.rowid);
    return { ok: topic ? { topic, html: `<p>Artículo de ${topic}</p>`, plain: `Artículo de ${topic}`, matchKind: "exact" } : null };
  };
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "diccionario", modulesByKind: SAVED } } });
  $id("dict-query").value = word;
  clickEl($id("btn-dict-search"));
  settle();
}
const nav = () => $id("dict-index");
const rows = () => nav().querySelectorAll(".dict-index-row");
const row = (topic) => rows().find((r) => r.textContent === topic);
const indexCalls = (mark = 0) => H.calls.slice(mark).filter((c) => c.cmd === "dictionary_index");
const content = () => $id("study-content");
/** Topic of the row right after the «▸ word» marker (where the word would go). */
function rowAfterMarker() {
  const kids = nav().querySelector(".dict-index-list").children;
  const i = kids.findIndex((k) => k.classList.contains("dict-index-marker"));
  return i >= 0 && kids[i + 1] ? kids[i + 1].textContent : null;
}

scenario("«pierda»: the index opens beside the article and marks where the word would go", () => {
  setup("pierda");
  assert(!nav().classList.contains("hidden"), "index visible in Diccionario");
  assert($id("view-study").classList.contains("has-index"), "two-column layout");
  const c = indexCalls().pop();
  eq([c.args.modulePath, c.args.query, c.args.before, c.args.after], [NELSON, "pierda", 40, 80], "window around the word");
  includes(nav().querySelector(".dict-index-marker").textContent, "▸ pierda", "marker");
  eq(rowAfterMarker(), "PILATO, PONCIO", "between PIEDRAS PRECIOSAS and PILATO");
  eq(rows().filter((r) => r.getAttribute("aria-selected") === "true").length, 0, "nothing selected: not a topic");
  includes(studyText(), "El índice de la izquierda marca dónde iría", "the article says how to continue");
  includes(nav().textContent, `Índice · ${TOPICS.length.toLocaleString("es")} temas`, "count");
}, { allowMissingFixtures: true });

scenario("«amor» (a topic): its row is highlighted and selected, no marker", () => {
  setup("amor", [{ topic: "AMOR", html: "<p>Afecto…</p>", plain: "Afecto…", matchKind: "exact" }]);
  const r = row("AMOR");
  assert(r && r.classList.contains("focus"), "focus row");
  eq(r.getAttribute("aria-selected"), "true", "selected");
  eq(nav().querySelector(".dict-index-marker"), null, "no marker for an exact topic");
}, { allowMissingFixtures: true });

scenario("A click opens exactly that topic; «Volver a «pierda»» returns to the word's results", () => {
  setup("pierda");
  const mark = H.calls.length;
  clickEl(row("PIEDRA"));
  settle();
  const g = H.calls.slice(mark).filter((c) => c.cmd === "get_dictionary_entry");
  eq(g.length, 1, "one article fetch");
  eq(g[0].args.rowid, TOPICS.find((t) => t.topic === "PIEDRA").rowid, "by its rowid");
  includes(content().querySelector(".study-banner").textContent, "PIEDRA", "banner names the topic");
  includes(studyText(), "Artículo de PIEDRA", "its article");
  eq(row("PIEDRA").getAttribute("aria-selected"), "true", "row selected");
  const back = $id("btn-dict-back");
  includes(back.textContent, "Volver a «pierda»", "back button");
  const m2 = H.calls.length;
  clickEl(back);
  settle();
  assert(H.calls.slice(m2).some((c) => c.cmd === "lookup_dictionary" && c.args.query === "pierda"), "the word is looked up again");
  includes(content().querySelector(".study-banner").textContent, "«pierda»", "back on the word");
}, { allowMissingFixtures: true });

scenario("«Siguientes…» appends the next window and «Anteriores…» the previous one", () => {
  setup("tema 200"); // deep in the list: the window starts after topic 0
  assert(Number(rows()[0].dataset.pos) > 0, "fixture: window does not start at 0");
  const n0 = rows().length;
  const start0 = Number(rows()[0].dataset.pos);
  const end0 = Number(rows()[n0 - 1].dataset.pos) + 1;
  let mark = H.calls.length;
  clickEl(nav().querySelector('.dict-index-more[data-dir="next"]'));
  settle();
  let c = indexCalls(mark)[0];
  eq([c.args.anchor, c.args.before, c.args.after], [end0, 0, 120], "next window from the end");
  assert(rows().length > n0, "rows appended");
  eq(Number(rows()[0].dataset.pos), start0, "beginning kept");
  mark = H.calls.length;
  const prev = nav().querySelector('.dict-index-more[data-dir="prev"]');
  assert(prev, "«Anteriores…» shown when the window does not start at 0");
  clickEl(prev);
  settle();
  c = indexCalls(mark)[0];
  eq(c.args.anchor, Math.max(0, start0 - 120), "previous window");
  assert(Number(rows()[0].dataset.pos) < start0, "rows prepended");
}, { allowMissingFixtures: true });

scenario("Typing in «Buscar…» moves the index without searching the word", () => {
  setup("pierda");
  const mark = H.calls.length;
  $id("dict-query").value = "zar";
  dispatch($id("dict-query"), makeEvent("input"));
  settle();
  const c = indexCalls(mark);
  eq(c.length, 1, "one index request (debounced)");
  eq(c[0].args.query, "zar", "for what is typed");
  eq(H.calls.slice(mark).filter((x) => x.cmd === "resolve_word" || x.cmd === "lookup_dictionary").length, 0, "no search");
  eq(rowAfterMarker(), "ZARZA", "the index jumps to ZARZA");
}, { allowMissingFixtures: true });

scenario("↓ on a topic focuses the next one and opens it (like e-Sword)", () => {
  setup("pierda");
  const r = row("PIEDRA");
  r.focus();
  const mark = H.calls.length;
  dispatch(r, makeEvent("keydown", { key: "ArrowDown" }));
  settle();
  const next = TOPICS[TOPICS.findIndex((t) => t.topic === "PIEDRA") + 1];
  eq(document.activeElement.textContent, next.topic, "focus moved");
  const g = H.calls.slice(mark).filter((c) => c.cmd === "get_dictionary_entry");
  eq(g.length, 1, "opened once");
  eq(g[0].args.rowid, next.rowid, "the next topic");
}, { allowMissingFixtures: true });

scenario("Léxico has no index; back in Diccionario it returns", () => {
  setup("pierda");
  setWorkspace("lexico");
  settle();
  assert(nav().classList.contains("hidden"), "hidden in Léxico");
  assert(!$id("view-study").classList.contains("has-index"), "one column");
  setWorkspace("diccionario");
  settle();
  assert(!nav().classList.contains("hidden"), "shown again");
}, { allowMissingFixtures: true });

scenario("An index failure is a short Spanish message; the article still renders", () => {
  setup("pierda");
  H.overrides.dictionary_index = () => ({ err: "no such table: Dictionary" });
  $id("dict-query").value = "pierda";
  clickEl($id("btn-dict-search"));
  settle();
  includes(nav().textContent, "Formato de módulo no compatible.", "Spanish headline");
  includes(content().querySelector(".study-banner").textContent, "«pierda»", "the word's view is still there");
}, { allowMissingFixtures: true });
