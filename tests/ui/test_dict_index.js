// Diccionario topic index, e-Sword style (user choice 2026-11-11). Beside the article the module's
// topics are listed alphabetically: a tap jumps to the exact topic (highlighted) or marks where
// the word would go («▸ pierda»), a click opens exactly that topic, «Volver» returns to the word,
// «Anteriores…/Siguientes…» load more, typing in «Buscar…» moves the index without searching,
// ↓/↑ walk the topics, and Comentario has no index. Since 2026-11-18 the Léxico has its own
// (ported from the Windows build): Strong's rows with their headword, «Griego» / «Hebreo». The backend's index is faked here
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

scenario("Léxico has its own index (the Lexicon table); Comentario has none; back in Diccionario it returns", () => {
  setup("pierda");
  let mark = H.calls.length;
  setWorkspace("lexico");
  settle();
  assert(!nav().classList.contains("hidden"), "shown in Léxico (2026-11-18, from the Windows build)");
  assert($id("view-study").classList.contains("has-index"), "two columns");
  const c = indexCalls(mark).pop();
  eq([c.args.modulePath, c.args.lexicon], [STRONGES, true], "the Léxico's module, its Lexicon table");
  eq(nav().querySelector(".dict-index-list").getAttribute("aria-label"), "Temas del léxico", "labelled for the Léxico");
  setWorkspace("comentario");
  settle();
  assert(nav().classList.contains("hidden"), "hidden in Comentario");
  assert(!$id("view-study").classList.contains("has-index"), "one column");
  mark = H.calls.length;
  setWorkspace("diccionario");
  settle();
  assert(!nav().classList.contains("hidden"), "shown again");
  eq(indexCalls(mark).pop().args.lexicon, undefined, "the Diccionario's index never asks for the Lexicon table");
}, { allowMissingFixtures: true });

// —— Léxico index: Strong's rows with their headword, «Griego» / «Hebreo» ——
const LEX_ROWS = [
  ...Array.from({ length: 200 }, (_, i) => ({ topic: `G${i + 1}`, lemma: i === 25 ? "ἀγάπη" : `λ${i}` })),
  ...Array.from({ length: 200 }, (_, i) => ({ topic: `H${i + 1}`, lemma: i === 0 ? "אָב" : "" })),
].map((r, i) => ({ ...r, rowid: 5000 + i }));
const HEBREW_START = 200;
function fakeLexIndex(a) {
  const total = LEX_ROWS.length;
  let focus = null;
  let exact = false;
  if (a.query) {
    focus = LEX_ROWS.findIndex((r) => r.topic.toLowerCase() === String(a.query).toLowerCase());
    exact = focus >= 0;
    if (focus < 0) focus = 0;
  }
  const center = a.anchor ?? focus ?? 0;
  const start = Math.max(0, center - (a.before ?? 40));
  const end = Math.min(total, center + (a.after ?? 80));
  const rows = LEX_ROWS.slice(start, end).map((r, i) => {
    const row = { pos: start + i, rowid: r.rowid, topic: r.topic };
    if (r.lemma) row.lemma = r.lemma;
    return row;
  });
  return { ok: { total, start, focus, exact, rows, strongKeyed: true, greekStart: 0, hebrewStart: HEBREW_START } };
}
function setupLex(code) {
  H.overrides.resolve_word = (a) =>
    a.word === code
      ? { ok: { word: code, normalized: code, strongs: [code], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: false, interlinearHits: [], matchedHit: null, matchKind: "" } }
      : undefined;
  H.overrides.lookup_lexicon = (a) => (a.strong === code ? { ok: { topic: code, html: `<p>Definición de ${code}</p>`, plain: `Definición de ${code}`, matchKind: "exact" } } : { ok: null });
  H.overrides.dictionary_index = (a) => (a.lexicon ? fakeLexIndex(a) : fakeIndex(a));
  H.overrides.get_dictionary_entry = (a) => {
    const r = LEX_ROWS.find((x) => x.rowid === a.rowid);
    return { ok: r && a.lexicon ? { topic: r.topic, html: `<p>Artículo de ${r.topic}</p>`, plain: `Artículo de ${r.topic}`, matchKind: "exact" } : null };
  };
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "lexico", modulesByKind: SAVED } } });
  $id("dict-query").value = code;
  clickEl($id("btn-dict-search"));
  settle();
}

scenario("Léxico: a Strong's row shows its Greek/Hebrew headword; the code's row is the focus", () => {
  setupLex("G26");
  const r = rows().find((x) => x.dataset.topic === "G26");
  assert(r, "G26 listed");
  assert(r.classList.contains("focus"), "focus on the code shown");
  eq(r.querySelector(".dict-index-lemma").textContent, "ἀγάπη", "headword beside the code");
  eq(r.querySelector(".dict-index-lemma").getAttribute("dir"), "auto", "isolated (Hebrew reads right to left)");
  const plain = rows().find((x) => x.dataset.topic === "G1");
  eq(plain.getAttribute("title"), "G1 λ0", "tooltip names both");
}, { allowMissingFixtures: true });

scenario("Léxico: «Hebreo» and «Griego» jump to where each testament's codes start", () => {
  setupLex("G26");
  const jumps = nav().querySelectorAll(".dict-index-jump");
  eq(jumps.map((j) => j.textContent.trim()), ["Griego", "Hebreo"], "two jump buttons");
  let mark = H.calls.length;
  clickEl(nav().querySelector('.dict-index-jump[data-jump="hebrew"]'));
  settle();
  let c = indexCalls(mark)[0];
  eq([c.args.anchor, c.args.before, c.args.lexicon, c.args.query], [HEBREW_START, 0, true, null], "a window from H1");
  eq(rows()[0].dataset.topic, "H1", "the list starts at H1");
  eq(rows()[0].querySelector(".dict-index-lemma").textContent, "אָב", "Hebrew headword");
  eq(nav().querySelector(".dict-index-marker"), null, "a jump is not a search: no marker");
  mark = H.calls.length;
  clickEl(nav().querySelector('.dict-index-jump[data-jump="greek"]'));
  settle();
  c = indexCalls(mark)[0];
  eq(c.args.anchor, 0, "back to G1");
  eq(rows()[0].dataset.topic, "G1", "the list starts at G1");
}, { allowMissingFixtures: true });

scenario("Léxico: a click opens that number from the Lexicon table; «Volver» returns to the code", () => {
  setupLex("G26");
  clickEl(nav().querySelector('.dict-index-jump[data-jump="hebrew"]'));
  settle();
  const mark = H.calls.length;
  clickEl(rows().find((x) => x.dataset.topic === "H1"));
  settle();
  const g = H.calls.slice(mark).filter((c) => c.cmd === "get_dictionary_entry");
  eq(g.length, 1, "one article fetch");
  eq([g[0].args.rowid, g[0].args.lexicon], [LEX_ROWS[HEBREW_START].rowid, true], "by rowid, from the Lexicon table");
  includes(content().querySelector(".study-banner").textContent, "Léxico", "Léxico banner");
  includes(content().querySelector(".study-banner").textContent, "H1", "names the topic, not the headword glued to it");
  includes(studyText(), "Artículo de H1", "its article");
  const back = $id("btn-dict-back");
  includes(back.textContent, "Volver a «G26»", "back to the code");
  const m2 = H.calls.length;
  clickEl(back);
  settle();
  assert(H.calls.slice(m2).some((c) => c.cmd === "lookup_lexicon" && c.args.strong === "G26"), "G26 looked up again");
}, { allowMissingFixtures: true });

scenario("Léxico: typing moves its index (Lexicon table) without searching", () => {
  setupLex("G26");
  const mark = H.calls.length;
  $id("dict-query").value = "H1";
  dispatch($id("dict-query"), makeEvent("input"));
  settle();
  const c = indexCalls(mark);
  eq(c.length, 1, "one index request (debounced)");
  eq([c[0].args.query, c[0].args.lexicon], ["H1", true], "for what is typed, in the Léxico's table");
  eq(H.calls.slice(mark).filter((x) => x.cmd === "resolve_word" || x.cmd === "lookup_lexicon").length, 0, "no search");
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

scenario("Switching Diccionario → Léxico never shows the Diccionario's index in the Léxico, even when the Léxico fails", () => {
  setup("pierda");
  assert(state.dictIndex && !state.dictIndex.lexicon, "fixture: the Diccionario's index is up");
  H.overrides.dictionary_index = (a) => (a.lexicon ? { err: "database is locked" } : fakeIndex(a));
  setWorkspace("lexico");
  settle();
  assert(!state.dictIndex || state.dictIndex.lexicon, "no Diccionario index state in the Léxico");
  excludes(nav().textContent, "PIEDRA", "none of the Diccionario's topics on screen");
}, { allowMissingFixtures: true });

scenario("While the Léxico's index loads, none of the Diccionario's rows are on screen or clickable (review 2026-11-18)", () => {
  setup("pierda");
  assert(state.dictIndex && !state.dictIndex.lexicon, "fixture: the Diccionario's index is up");
  H.overrides.dictionary_index = (a) => (a.lexicon ? fakeLexIndex(a) : fakeIndex(a));
  H.overrides.lookup_lexicon = () => ({ ok: null });
  H.delivery = "hold";
  setWorkspace("lexico");
  const isLexIndex = (e) => e.cmd === "dictionary_index" && e.args.lexicon === true;
  settleHolding(isLexIndex);
  assert(H.pending.some(isLexIndex), "fixture: the Léxico's index is still loading");
  excludes(nav().textContent, "PIEDRA", "no Diccionario topic on screen");
  eq(rows().length, 0, "no row to click");
  includes(nav().textContent, "Cargando", "a loading header instead");
  H.delivery = "fifo";
  release();
  settle();
  assert(rows().some((r) => r.dataset.topic === "G1"), "then the Léxico's own rows");
}, { allowMissingFixtures: true });
