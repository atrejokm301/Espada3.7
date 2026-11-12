// ISS-44 — without a (validated) interlinear: no filename guess, no 'Interlineal' badge or
// header claim, and Léxico explains why a Spanish word has no Strong number.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };

scenario("find_interlinear → null: interlinearPath '' (no filename fallback), header says nothing", () => {
  H.overrides.find_interlinear = () => ({ ok: null });
  boot({ storage: { nav: NAV } });
  eq(state.interlinearPath, "", "no guessed interlinear");
  excludes($id("modules-meta").textContent, "Interlineal detectado", "no false claim");
});

scenario("Léxico, Spanish word, no interlinear → explanation, no badge, Diccionario still works", () => {
  H.overrides.find_interlinear = () => ({ ok: null });
  boot({ storage: { nav: NAV, study: { workspace: "biblia", modulesByKind: { lexico: `${LIB_DIR}/04STRONGes.lexi` } } } });
  clickEl(wordEl("amó", 16));
  settle();
  eq(state.workspace, "diccionario", "Diccionario after a tap");
  includes(studyText(), "Diccionario ·", "dictionary pane filled without an interlinear");
  clickEl(document.querySelector('.ws-tab[data-ws="lexico"]'));
  settle();
  includes(studyText(), "No se encontró un interlineal en la carpeta", "explains");
  excludes(studyText(), "Pruebe otro módulo arriba", "no misleading advice");
  eq($id("study-content").querySelector(".lex-badge"), null, "no Interlineal badge");
});

scenario("Badge only when the interlinear really returned hits", () => {
  boot({ storage: { nav: NAV, study: { workspace: "lexico", selectedWord: "amó", lastStrong: "G25" } } });
  assert(state.wordLinks.interlinearHits.length > 0, "fixture has hits");
  assert($id("study-content").querySelector(".lex-badge"), "badge with hits");
  H.overrides.resolve_word = (a) => ({ ok: { word: a.word, normalized: a.word, strongs: [], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: true, interlinearHits: [], matchedHit: null } });
  $id("dict-query").value = "mundo";
  clickEl($id("btn-dict-search"));
  settle();
  eq($id("study-content").querySelector(".lex-badge"), null, "no badge when the interlinear gave nothing");
});

scenario("A typed Strong code without an interlinear still looks it up (no explanation)", () => {
  H.overrides.find_interlinear = () => ({ ok: null });
  boot({ storage: { nav: NAV, study: { workspace: "lexico" } } });
  $id("dict-query").value = "G26";
  clickEl($id("btn-dict-search"));
  settle();
  excludes(studyText(), "No se encontró un interlineal", "codes do not need the interlinear");
});
