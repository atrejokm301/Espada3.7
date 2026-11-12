// ISS-18 — step verse by verse while reading a commentary: ‹ v.16 › buttons, ←/→ and Alt+↑/↓
// (never in text fields), chapter edges, verse text under the banner, debounced side pane.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const at = (book, chapter, verse) => ({ book, chapter, verse, biblePath: RVR });
const onComentario = (nav, study = {}) => ({ nav, study: Object.assign({ workspace: "comentario", modulesByKind: { comentario: CMH } }, study) });
const primaryCalls = (mark) => callsSince(mark, "get_commentary").filter((c) => c.args.modulePath === CMH);
const key = (el, opts) => dispatch(el, makeEvent("keydown", opts));
const banner = () => ($id("study-content").querySelector(".study-banner") || { textContent: "" }).textContent;

scenario("› at Jn 3:16 → commentary for 3:17, banner and stepper say 3:17, no history or re-resolve", () => {
  boot({ storage: onComentario(at(43, 3, 16), { selectedWord: "amó", wordOrigin: { book: 43, chapter: 3, verse: 16 } }) });
  assert(!$id("verse-stepper").classList.contains("hidden"), "stepper visible on Comentario");
  eq($id("verse-stepper-label").textContent, "v.16", "label");
  const hist = JSON.stringify(state.history);
  const mark = H.calls.length;
  clickEl($id("btn-verse-next"));
  settle();
  eq([state.book, state.chapter, state.verse], [43, 3, 17], "moved one verse");
  const calls = primaryCalls(mark);
  assert(calls.length >= 1 && calls.every((c) => c.args.verse === 17), "main commentary fetched for 3:17");
  eq(banner(), "Comentario · Juan 3:17", "banner");
  eq($id("verse-stepper-label").textContent, "v.17", "stepper label");
  eq(callsSince(mark, "resolve_word").length, 0, "the selected word is not re-resolved");
  eq(callsSince(mark, "get_bible_chapter").length, 0, "same chapter: no chapter reload");
  eq(JSON.stringify(state.history), hist, "no history entry per step");
  eq(JSON.parse(localStorage.getItem(LS_KEYS.nav)).verse, 17, "position persisted");
  eq([...state.selectedVerses], [17], "Biblia selection follows");
});

scenario("Verse text appears under the banner and follows the step", () => {
  boot({ storage: onComentario(at(43, 3, 16)) });
  includes($id("study-content").querySelector(".study-verse-text").textContent, "Porque de tal manera amó Dios al mundo", "3:16 text");
  clickEl($id("btn-verse-next"));
  settle();
  includes($id("study-content").querySelector(".study-verse-text").textContent, "Porque no envió Dios a su Hijo", "3:17 text");
});

scenario("› at Jn 3:36 → Jn 4:1; ‹ at 4:1 → 3:36 (previous chapter's LAST verse)", () => {
  boot({ storage: onComentario(at(43, 3, 36)) });
  clickEl($id("btn-verse-next"));
  settle();
  eq([state.chapter, state.verse], [4, 1], "next chapter v.1");
  eq(banner(), "Comentario · Juan 4:1", "banner 4:1");
  eq($id("sel-chapter").value, "4", "chapter select follows");
  clickEl($id("btn-verse-prev"));
  settle();
  eq([state.chapter, state.verse], [3, 36], "back to the last verse of chapter 3");
  eq(banner(), "Comentario · Juan 3:36", "banner 3:36");
});

scenario("Keys: → and Alt+↓ step forward, ← and Alt+↑ back (focus on the page)", () => {
  boot({ storage: onComentario(at(43, 3, 16)) });
  key(document.body, { key: "ArrowRight" });
  settle();
  eq(state.verse, 17, "→");
  key(document.body, { key: "ArrowDown", altKey: true });
  settle();
  eq(state.verse, 18, "Alt+↓");
  key(document.body, { key: "ArrowUp", altKey: true });
  settle();
  eq(state.verse, 17, "Alt+↑");
  key(document.body, { key: "ArrowLeft" });
  settle();
  eq(state.verse, 16, "←");
  key(document.body, { key: "ArrowDown" });
  settle();
  eq(state.verse, 16, "plain ↓ keeps scrolling, no step");
});

scenario("Alt+↓ / → in the search box, the notes or a select do nothing", () => {
  boot({ storage: onComentario(at(43, 3, 16)) });
  const mark = H.calls.length;
  for (const id of ["dict-query", "notes-side", "sel-study-module"]) {
    key($id(id), { key: "ArrowDown", altKey: true });
    key($id(id), { key: "ArrowRight" });
  }
  settle();
  eq(state.verse, 16, "no step from a field");
  eq(primaryCalls(mark).length, 0, "no commentary request");
});

scenario("Keys do nothing outside Comentario or with a panel open", () => {
  boot({ storage: onComentario(at(43, 3, 16), { workspace: "biblia" }) });
  key(document.body, { key: "ArrowRight" });
  settle();
  eq(state.verse, 16, "Biblia: no step");
  clickEl(document.querySelector('.ws-tab[data-ws="comentario"]'));
  settle();
  clickEl($id("btn-history"));
  settle();
  key(document.body, { key: "ArrowRight" });
  settle();
  eq(state.verse, 16, "panel open: no step");
});

scenario("Edge: ‹ at Gn 1:1 is disabled and does nothing", () => {
  boot({ storage: onComentario(at(1, 1, 1)) });
  assert($id("btn-verse-prev").disabled, "‹ disabled at Gn 1:1");
  key(document.body, { key: "ArrowLeft" });
  settle();
  eq([state.book, state.chapter, state.verse], [1, 1, 1], "stays at Gn 1:1");
});

scenario("5 rapid steps, replies reversed → content matches the banner; side pane fetched once", () => {
  boot({ storage: onComentario(at(43, 3, 16)) });
  const mark = H.calls.length;
  H.delivery = "hold";
  for (let i = 0; i < 5; i++) clickEl($id("btn-verse-next"));
  settle({ order: "reverse" });
  eq(state.verse, 21, "five steps");
  eq(banner(), "Comentario · Juan 3:21", "banner");
  const rec = H.fixture.responses[fixtureKey("get_commentary", { modulePath: CMH, book: 43, chapter: 3, verse: 21, filters: state.filters })];
  assert(rec && rec.ok && rec.ok.length, "fixture for 3:21");
  eq($id("study-content").querySelectorAll(".scope-label").map((e) => e.textContent), rec.ok.map((e) => e.label), "entries are the 3:21 entries");
  const side = callsSince(mark, "get_commentary").filter((c) => c.args.modulePath !== CMH);
  assert(side.length >= 1, "side pane refreshed");
  assert(side.every((c) => c.args.verse === 21), `side pane only for the final verse (got ${[...new Set(side.map((c) => c.args.verse))].join(",")})`);
});

scenario("Back to Biblia after stepping → the stepped-to verse is selected and scrolled into view", () => {
  boot({ storage: onComentario(at(43, 3, 16)) });
  clickEl($id("btn-verse-next"));
  clickEl($id("btn-verse-next"));
  settle();
  const target = $id("bible-content").querySelector('.verse[data-verse="18"]');
  let scrolled = false;
  target.scrollIntoView = () => (scrolled = true);
  clickEl(document.querySelector('.ws-tab[data-ws="biblia"]'));
  settle();
  assert(target.classList.contains("selected"), "v.18 selected");
  eq($id("bible-content").querySelectorAll(".verse.selected").length, 1, "only one verse selected");
  assert(scrolled, "v.18 scrolled into view");
});

scenario("Unknown verse count (no chapter data, get_verse_count fails) → › does not walk past the end and is disabled", () => {
  boot({ storage: onComentario(at(43, 3, 36)) });
  state.chapterData = null; // as if the chapter never loaded
  H.overrides.get_verse_count = () => ({ err: "no such table: Bible" });
  const next = $id("btn-verse-next");
  assert(!next.disabled, "› enabled before trying");
  const mark = H.calls.length;
  clickEl(next);
  settle();
  eq([state.book, state.chapter, state.verse], [43, 3, 36], "stays at 3:36 (old code went to 3:37)");
  eq(primaryCalls(mark).length, 0, "no commentary request for a verse that may not exist");
  assert(next.disabled, "› disabled while the end is unknown");
  includes(next.title, "No se conoce el número de versículos", "tooltip says why");
  key(document.body, { key: "ArrowRight" });
  settle();
  eq(state.verse, 36, "→ does not step either");
  clickEl($id("btn-verse-prev"));
  settle();
  eq(state.verse, 35, "‹ still works");
  assert(!next.disabled, "moving re-enables ›");
  eq(next.title, "Versículo siguiente (→ o Alt+↓)", "original tooltip restored");
}, { allowMissingFixtures: true });

for (const [label, setup] of [
  ["get_verse_count returns 0", () => (H.overrides.get_verse_count = () => ({ ok: 0 }))],
  ["no Bible selected", () => (state.biblePath = "")],
]) {
  scenario(`Unknown end (${label}) → no step past the end, › disabled`, () => {
    boot({ storage: onComentario(at(43, 3, 36)) });
    state.chapterData = null;
    setup();
    clickEl($id("btn-verse-next"));
    settle();
    eq(state.verse, 36, "stays");
    assert($id("btn-verse-next").disabled, "› disabled");
  }, { allowMissingFixtures: true });
}
