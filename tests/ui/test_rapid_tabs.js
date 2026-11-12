// Rapid tab switching (ISS-01 / ISS-09 / ISS-21 / ISS-11 prerequisite): whatever order the
// replies come back in, the screen ends on the LAST tab the user chose, rendered with that
// tab's own module, and no tab's module choice is touched by another tab's late reply.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: CMH };
const returning = (study) => ({ nav: NAV, study: Object.assign({ modulesByKind: SAVED }, study) });
const tabEl = (ws) => document.querySelector(`.ws-tab[data-ws="${ws}"]`);
const optionPaths = () => $id("sel-study-module").options.map((o) => o.value);
const candidate = (ws) => (p) => studyCandidates(ws).some((m) => m.path === p);

/** Let everything run except the tap's resolve_word, which stays in flight. */
function holdResolve() {
  settleHolding((c) => c.cmd === "resolve_word");
  assert(H.pending.some((c) => c.cmd === "resolve_word"), "the tapped word is still resolving");
  eq(state.workspace, "biblia", "nothing switched yet");
}

function assertSettledOn(ws, module, bannerPrefix) {
  eq(state.workspace, ws, "last tab clicked wins");
  eq(tabEl(ws).getAttribute("aria-selected"), "true", "tab marked selected");
  eq(state.studyModule, module, `${ws} renders with its own module`);
  const opts = optionPaths();
  assert(opts.length && opts.every(candidate(ws)), `dropdown lists only ${ws} candidates`);
  eq($id("sel-study-module").value, module, "dropdown shows the module in use");
  includes(studyText(), bannerPrefix, "the tab's own view is on screen");
  excludes(studyText(), "Cargando…", "no stuck loading state");
  eq($id("study-content").getAttribute("aria-busy"), null, "not left busy");
  eq($id("study-content").querySelector(".empty-state.error"), null, "no error block");
  eq(storedStudy().modulesByKind, SAVED, "no tab's saved choice was rewritten");
}

scenario("Diccionario → Léxico → Comentario → Diccionario in a burst, replies reversed → Diccionario/Nelson", () => {
  boot({ storage: returning({ workspace: "biblia", selectedWord: "amó", wordOrigin: { book: 43, chapter: 3, verse: 16, biblePath: RVR } }) });
  H.delivery = "hold";
  for (const ws of ["diccionario", "lexico", "comentario", "diccionario"]) clickEl(tabEl(ws));
  settle({ order: "reverse" });
  assertSettledOn("diccionario", NELSON, "Diccionario · NELSON-test · «amó»");
});

scenario("Burst ending on Léxico, replies reversed → Léxico/STRONGes shows G25, nothing from Diccionario", () => {
  boot({ storage: returning({ workspace: "biblia", selectedWord: "amó", lastStrong: "G25", wordOrigin: { book: 43, chapter: 3, verse: 16, biblePath: RVR } }) });
  H.delivery = "hold";
  for (const ws of ["comentario", "diccionario", "lexico"]) clickEl(tabEl(ws));
  settle({ order: "reverse" });
  assertSettledOn("lexico", STRONGES, "Léxico · STRONGes");
  excludes(studyText(), "Diccionario ·", "no dictionary banner leaked into Léxico");
  excludes(studyText(), "Comentario ·", "no commentary banner leaked into Léxico");
});

scenario("Burst ending on Biblia → no late study render; the next Diccionario visit is correct", () => {
  boot({ storage: returning({ workspace: "biblia", selectedWord: "amó", wordOrigin: { book: 43, chapter: 3, verse: 16, biblePath: RVR } }) });
  H.delivery = "hold";
  clickEl(tabEl("diccionario"));
  clickEl(tabEl("lexico"));
  clickEl(tabEl("biblia"));
  const mark = H.writes.length;
  settle({ order: "reverse" });
  eq(state.workspace, "biblia", "still on Biblia");
  assert($id("view-study").classList.contains("hidden"), "study view hidden");
  eq(writesSince(mark).length, 0, "late Diccionario/Léxico replies wrote nothing into the study pane");
  H.delivery = "fifo";
  clickEl(tabEl("diccionario"));
  settle();
  assertSettledOn("diccionario", NELSON, "Diccionario · NELSON-test · «amó»");
});

scenario("Ping-pong Diccionario ↔ Léxico ×3 before any reply → ≤1 probe per kind, 1 lookup render each at the end", () => {
  boot({ storage: returning({ workspace: "biblia", selectedWord: "amó", lastStrong: "G25", wordOrigin: { book: 43, chapter: 3, verse: 16, biblePath: RVR } }) });
  const mark = H.calls.length;
  H.delivery = "hold";
  for (let i = 0; i < 3; i++) {
    clickEl(tabEl("diccionario"));
    clickEl(tabEl("lexico"));
  }
  settle({ order: "fifo" });
  const probes = callsSince(mark, "probe_modules_content");
  lte(probes.filter((c) => c.args.kind === "dictionary").length, 1, "dictionary probes (cached / shared)");
  lte(probes.filter((c) => c.args.kind === "lexicon").length, 1, "lexicon probes (cached / shared)");
  eq(callsSince(mark, "resolve_word").length, 0, "no re-resolve: the word was already resolved");
  assertSettledOn("lexico", STRONGES, "Léxico · STRONGes");
});

// —— Tab chosen while a tapped word is still resolving (≈0.7 s on the real backend) ——

scenario("Tap «amó», then Léxico before resolve_word answers → stays on Léxico (not yanked to Diccionario)", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  H.delivery = "hold";
  clickEl(wordEl("amó", 16));
  holdResolve();
  clickEl(tabEl("lexico"));
  settle({ order: "fifo" });
  eq(state.workspace, "lexico", "the tab the user chose afterwards wins");
  eq(state.selectedWord, "amó", "tapped word selected");
  eq(state.lastStrong, "G25", "its Strong");
  includes(studyText(), "Léxico · STRONGes", "Léxico rendered");
  eq(storedStudy().workspace, "lexico", "persisted tab is the user's");
});

scenario("Tap «amó», then Comentario before resolve_word answers → stays on Comentario", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  H.delivery = "hold";
  clickEl(wordEl("amó", 16));
  holdResolve();
  clickEl(tabEl("comentario"));
  settle({ order: "fifo" });
  eq(state.workspace, "comentario", "the tab the user chose afterwards wins");
  eq(state.studyModule, CMH, "Comentario module");
  includes(studyText(), "Comentario · Juan 3:16", "commentary on screen");
  eq(state.selectedWord, "amó", "the tap still selected the word");
});

scenario("Control: tap «amó» with no tab change → Diccionario (user decision 1 unchanged)", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  H.delivery = "hold";
  clickEl(wordEl("amó", 16));
  settle({ order: "fifo" });
  eq(state.workspace, "diccionario", "word tap opens Diccionario");
  includes(studyText(), "Diccionario · NELSON-test · «amó»", "dictionary rendered");
});

scenario("Tap the 2nd «que», switch to Léxico mid-resolve → every resolve_word keeps the tap context", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  H.delivery = "hold";
  const mark = H.calls.length;
  clickEl(wordEl("que", 16, 1));
  holdResolve();
  clickEl(tabEl("lexico"));
  settle({ order: "fifo" });
  const rw = callsSince(mark, "resolve_word");
  assert(rw.length >= 1, "resolved");
  for (const c of rw) {
    eq(c.args.word, "que", "word");
    eq(c.args.occurrence, 1, "occurrence of the tapped «que» (ISS-02)");
    eq(c.args.biblePath, RVR, "Bible it was read in (ISS-35)");
  }
  lte(rw.length, 1, "the in-flight tap is not resolved a second time");
  eq(state.workspace, "lexico", "on Léxico");
}, { allowMissingFixtures: true }); // asserts the resolve args only; on the newer backend the 2nd «que» is G5620, whose Léxico page is not recorded

scenario("Edge: Léxico waits on tap A, user then searches «gracia» → «gracia» wins; A is not resolved again", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  H.delivery = "hold";
  const mark = H.calls.length;
  clickEl(wordEl("amó", 16));
  holdResolve();
  clickEl(tabEl("lexico"));
  settleHolding((c) => c.cmd === "resolve_word" && c.args.word === "amó");
  $id("dict-query").value = "gracia";
  clickEl($id("btn-dict-search"));
  settle({ order: "reverse" });
  eq(state.selectedWord, "gracia", "the newer search owns the selection");
  eq(state.workspace, "lexico", "still on Léxico");
  eq(callsSince(mark, "resolve_word").filter((c) => c.args.word === "amó").length, 1, "A resolved once only");
  excludes($id("ws-ctx-word").textContent, "amó", "context bar is not A");
});

scenario("Failure: the tap's resolve_word rejects while Léxico waits → Léxico renders, no stuck «Cargando…», no retry storm", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  H.delivery = "hold";
  const mark = H.calls.length;
  H.overrides.resolve_word = () => ({ err: "unable to open database file" });
  clickEl(wordEl("amó", 16));
  holdResolve();
  clickEl(tabEl("lexico"));
  settle({ order: "fifo" });
  eq(state.workspace, "lexico", "on Léxico");
  eq(callsSince(mark, "resolve_word").length, 1, "one resolve, not re-sent by the waiting view");
  excludes(studyText(), "Cargando…", "the view finished");
  assert(studyText().length > 0, "something rendered");
});
