// ISS-11 prerequisite (WP5 async commands): replies arriving in REVERSE order must not show
// stale content. Guards: interconnectSeq (resolve), renderSeq (study pane), probe token + tab.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const CLARKE = `${LIB_DIR}/clarke.cmti`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: CLARKE };
const returning = (study) => ({ nav: NAV, study: Object.assign({ modulesByKind: SAVED }, study) });
const lexCandidate = (p) => studyCandidates("lexico").some((m) => m.path === p);

scenario("Taps A then B, replies reversed → B is shown everywhere", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  H.delivery = "hold";
  clickEl(wordEl("amó", 16));
  clickEl(wordEl("mundo", 16));
  settle({ order: "reverse" });
  eq(state.workspace, "diccionario", "on Diccionario");
  eq(state.selectedWord, "mundo", "selected word");
  eq(state.wordLinks && state.wordLinks.word, "mundo", "word links belong to B");
  includes(studyText(), "«mundo»", "dictionary shows B");
  excludes(studyText(), "«amó»", "no trace of A in the main pane");
  eq($id("dict-query").value, "mundo", "search box follows B");
  eq($id("ws-ctx-word").textContent, "mundo", "context bar follows B");
});

scenario("Taps A then B, A's dictionary reply arrives last → still B", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  H.delivery = "hold";
  clickEl(wordEl("amó", 16));
  // A resolves and starts rendering; its lookup stays held.
  settleHolding((c) => c.cmd === "lookup_dictionary");
  assert(H.pending.some((c) => c.cmd === "lookup_dictionary" && c.args.query === "amó"), "A's lookup in flight");
  clickEl(wordEl("mundo", 16));
  settle({ order: "reverse" });
  includes(studyText(), "«mundo»", "B rendered");
  excludes(studyText(), "«amó»", "A's late reply did not overwrite B");
});

scenario("Tab switch mid-probe → the late probe writes nothing into the new tab", () => {
  boot({ storage: returning({ workspace: "diccionario", selectedWord: "amó" }) });
  // Force a new dictionary probe (new query), keep it held.
  H.delivery = "hold";
  $id("dict-query").value = "gracia";
  clickEl($id("btn-dict-search"));
  settleHolding((c) => c.cmd === "probe_modules_content" && c.args.kind === "dictionary");
  assert(H.pending.some((c) => c.cmd === "probe_modules_content" && c.args.kind === "dictionary"), "dictionary probe in flight");
  clickEl(document.querySelector('.ws-tab[data-ws="lexico"]'));
  settle({ order: "reverse" });
  eq(state.workspace, "lexico", "on Léxico");
  eq(state.studyModule, STRONGES, "Léxico module unchanged");
  const opts = $id("sel-study-module").options.map((o) => o.value);
  assert(opts.length && opts.every(lexCandidate), "dropdown lists only Léxico candidates");
  eq(storedStudy().modulesByKind.diccionario, NELSON, "Diccionario choice untouched");
  eq(storedStudy().modulesByKind.lexico, STRONGES, "Léxico choice untouched");
  includes(studyText(), "Léxico ·", "Léxico rendered");
});

scenario("5 fast verse steps in Comentario, replies reversed → banner and content agree", () => {
  boot({ storage: returning({ workspace: "comentario" }) });
  eq(state.studyModule, CLARKE, "saved commentary");
  clickEl($id("btn-verse-pick"));
  settle();
  H.delivery = "hold";
  const isPrimary = (c) => c.cmd === "get_commentary" && c.args.modulePath === CLARKE;
  for (const v of [17, 18, 19, 20, 21]) {
    clickEl($id("verse-grid").querySelector(`button[data-v="${v}"]`));
    settleHolding(isPrimary); // everything else answers; the main commentary replies pile up
  }
  assert(H.pending.filter(isPrimary).length >= 2, "several main-commentary replies are in flight");
  settle({ order: "reverse" });
  eq(state.verse, 21, "current verse");
  includes(studyText(), "Comentario · Juan 3:21", "banner shows the current verse");
  const expected = H.fixture.responses[fixtureKey("get_commentary", { modulePath: CLARKE, book: 43, chapter: 3, verse: 21, filters: state.filters })];
  assert(expected && expected.ok, "fixture for 3:21");
  const labels = $id("study-content").querySelectorAll(".scope-label").map((e) => e.textContent);
  eq(labels, expected.ok.map((e) => e.label), "entries are the 3:21 entries (label only, ISS-27)");
});
