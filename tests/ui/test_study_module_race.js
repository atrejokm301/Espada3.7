// ISS-01 — the study tabs must render with a module of THEIR kind, chosen synchronously.
// Failed before the fix: first tap «No hay módulos…», Léxico/Diccionario looked up the
// commentary/previous-tab module, 'Otros comentarios' listed the primary, late probes wrote.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NAV_JN316 = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const isKind = (p, kind) => moduleKind(p) === kind;
const lexCandidate = (p) => {
  const m = moduleByPath(p);
  return !!m && (m.kind === "lexicon" || (m.kind === "dictionary" && /strong|lexic|vine|thayer|bdb|hebrew|hebreo|griego|greek/i.test(`${m.abbreviation} ${m.title} ${m.filename}`)));
};
const noModulesShown = () => H.writes.some((w) => w.id === "study-content" && /No hay módulos de este tipo/.test(w.html));

scenario("S1 fresh launch on Biblia, tap «amó» → dictionary lookup, never «No hay módulos»", () => {
  boot({ storage: { nav: NAV_JN316 } });
  eq(state.workspace, "biblia", "starts on Biblia");
  const mark = H.calls.length;
  clickEl(wordEl("amó", 16));
  settle();
  eq(state.workspace, "diccionario", "word tap switches to Diccionario (user decision: keep)");
  const looks = callsSince(mark, "lookup_dictionary");
  assert(looks.length >= 1, "a dictionary lookup was sent");
  for (const c of looks) assert(isKind(c.args.modulePath, "dictionary"), `lookup_dictionary got a ${moduleKind(c.args.modulePath) || "missing"} module: ${c.args.modulePath}`);
  assert(!noModulesShown(), "«No hay módulos de este tipo» was rendered");
  includes(studyText(), "Diccionario ·", "dictionary view rendered");
  assert(isKind(state.studyModule, "dictionary"), "studyModule is a dictionary");
});

scenario("S2 Comentario → Biblia → tap word → a dictionary is used", () => {
  boot({ storage: { nav: NAV_JN316, study: { workspace: "comentario" } } });
  assert(isKind(state.studyModule, "commentary"), "Comentario uses a commentary");
  clickEl(document.querySelector('.ws-tab[data-ws="biblia"]'));
  settle();
  const mark = H.calls.length;
  clickEl(wordEl("mundo", 16));
  settle();
  const looks = callsSince(mark, "lookup_dictionary");
  assert(looks.length >= 1, "lookup_dictionary sent");
  for (const c of looks) assert(isKind(c.args.modulePath, "dictionary"), `dictionary lookup used ${moduleKind(c.args.modulePath)} module`);
  eq(callsSince(mark, "lookup_lexicon").length, 0, "no lexicon lookups on a dictionary tap");
  excludes(studyText(), "Sin resultados en CMH", "not searching the commentary");
});

scenario("S3 Comentario → Léxico → a lexicon candidate is used", () => {
  boot({ storage: { nav: NAV_JN316, study: { workspace: "comentario", selectedWord: "amó", lastStrong: "G25" } } });
  const mark = H.calls.length;
  clickEl(document.querySelector('.ws-tab[data-ws="lexico"]'));
  settle();
  const looks = callsSince(mark, "lookup_lexicon");
  assert(looks.length >= 1, "lookup_lexicon sent");
  for (const c of looks) assert(lexCandidate(c.args.modulePath), `lookup_lexicon used a non-lexicon module: ${c.args.modulePath}`);
  assert(lexCandidate(state.studyModule), "studyModule is a lexicon candidate");
  assert(lexCandidate($id("sel-study-module").value), "dropdown shows a lexicon candidate");
});

scenario("S4 Strong G26 tapped inside a commentary → answered from a lexicon (side card), Léxico on request", () => {
  boot({ storage: { nav: NAV_JN316, study: { workspace: "comentario" } } });
  const content = $id("study-content");
  assert(isKind(state.studyModule, "commentary"), "commentary primary");
  // A Strong number inside commentary text (linkifyHtml turns «G26» into .strong-ref)
  content.innerHTML += `<div class="study-body">${linkifyHtml("ver G26 aquí")}</div>`;
  const mark = H.calls.length;
  clickEl(content.querySelector('.strong-ref[data-strong="G26"]'));
  settle();
  // ISS-19: Comentario keeps its text; the answer is the side card.
  eq(state.workspace, "comentario", "stays on Comentario");
  eq(state.lastStrong, "G26", "G26 is the current Strong");
  const card = $id("link-panel-inline").querySelector(".cmt-word-card");
  assert(card, "word card in the side pane");
  for (const c of callsSince(mark, "lookup_lexicon")) assert(lexCandidate(c.args.modulePath), `lookup in non-lexicon ${c.args.modulePath}`);
  const m2 = H.calls.length;
  clickEl(card.querySelector(".cmt-open-lex"));
  settle();
  eq(state.workspace, "lexico", "«Abrir en Léxico» switches to Léxico");
  const looks = callsSince(m2, "lookup_lexicon");
  assert(looks.length >= 1, "lookup_lexicon sent");
  for (const c of looks) assert(lexCandidate(c.args.modulePath), `G26 looked up in ${moduleKind(c.args.modulePath)} module ${c.args.modulePath}`);
  assert(looks.some((c) => c.args.strong === "G26"), "G26 itself is looked up");
});

scenario("S5 Diccionario → Comentario: 'Otros comentarios' never lists the primary", () => {
  boot({ storage: { nav: NAV_JN316, study: { workspace: "diccionario", selectedWord: "amor" } } });
  clickEl(document.querySelector('.ws-tab[data-ws="comentario"]'));
  settle();
  const primary = state.studyModule;
  assert(isKind(primary, "commentary"), "primary is a commentary");
  const others = $id("link-panel-inline").querySelectorAll(".mini-entry").map((e) => e.dataset.path);
  assert(others.length > 0, "some other commentaries are listed");
  assert(!others.includes(primary), "the primary commentary is listed in 'Otros comentarios'");
  const firstMain = callsSince(0, "get_commentary").find((c) => c.args.modulePath === primary);
  assert(firstMain, "the primary commentary was fetched");
});

scenario("Edge: a saved module that no longer exists → fallback, nothing persisted", () => {
  const gone = `${LIB_DIR}/Diccionario Borrado.dcti`;
  boot({ storage: { nav: NAV_JN316, study: { workspace: "diccionario", selectedWord: "amor", modulesByKind: { diccionario: gone, comentario: "", lexico: "" } } } });
  assert(isKind(state.studyModule, "dictionary"), "fell back to an existing dictionary");
  const looks = callsSince(0, "lookup_dictionary");
  assert(looks.length && looks.every((c) => c.args.modulePath !== gone), "never queried the missing module");
  eq(storedStudy().modulesByKind.diccionario, gone, "the fallback was not written over the saved choice");
});

scenario("Edge: nothing saved → the automatic pick is never persisted", () => {
  boot({ storage: { nav: NAV_JN316, study: { workspace: "lexico", selectedWord: "amó", lastStrong: "G25" } } });
  assert(lexCandidate(state.studyModule), "a lexicon candidate was picked");
  const saved = storedStudy().modulesByKind || {};
  eq([saved.diccionario || "", saved.lexico || "", saved.comentario || ""], ["", "", ""], "no automatic pick persisted");
});

scenario("Edge: a user-picked Vine ° (dictionary kind) in Léxico is kept", () => {
  const vine = `${LIB_DIR}/05_Vine Diccionario Expositivo con numeros Strong.dcti`;
  boot({ storage: { nav: NAV_JN316, study: { workspace: "lexico", selectedWord: "amó", lastStrong: "G25", modulesByKind: { lexico: vine, diccionario: "", comentario: "" } } } });
  eq(state.studyModule, vine, "Vine stays the Léxico module");
  assert(callsSince(0, "lookup_lexicon").some((c) => c.args.modulePath === vine), "Vine was queried");
  eq(storedStudy().modulesByKind.lexico, vine, "choice still saved");
});

scenario("Edge: user picks a module in the dropdown → used and persisted", () => {
  boot({ storage: { nav: NAV_JN316, study: { workspace: "diccionario", selectedWord: "amor" } } });
  const nelson = `${LIB_DIR}/Nelson.dcti`;
  const mark = H.calls.length;
  changeSelect($id("sel-study-module"), nelson);
  settle();
  eq(state.studyModule, nelson, "picked module is primary");
  eq(storedStudy().modulesByKind.diccionario, nelson, "user choice persisted");
  assert(callsSince(mark, "lookup_dictionary").every((c) => c.args.modulePath === nelson), "lookups use the picked module");
  eq($id("sel-study-module").value, nelson, "dropdown keeps the user choice after the probe");
});

scenario("Race: probe replies in reverse order never write into another tab", () => {
  const strongs = `${LIB_DIR}/04STRONGes.lexi`;
  const nelson = `${LIB_DIR}/Nelson.dcti`;
  boot({ storage: { nav: NAV_JN316, study: { workspace: "biblia", selectedWord: "amó", lastStrong: "G25", modulesByKind: { diccionario: nelson, lexico: strongs, comentario: "" } } } });
  H.delivery = "hold";
  clickEl(document.querySelector('.ws-tab[data-ws="diccionario"]'));
  clickEl(document.querySelector('.ws-tab[data-ws="lexico"]'));
  settle({ order: "reverse" });
  eq(state.workspace, "lexico", "ends on Léxico");
  eq(state.studyModule, strongs, "Léxico keeps its own module");
  eq(storedStudy().modulesByKind.lexico, strongs, "Léxico choice not overwritten");
  eq(storedStudy().modulesByKind.diccionario, nelson, "Diccionario choice not overwritten");
  assert(lexCandidate($id("sel-study-module").value), "dropdown shows Léxico modules");
  const lexLooks = H.calls.filter((c) => c.cmd === "lookup_lexicon");
  assert(lexLooks.length && lexLooks.every((c) => c.args.modulePath === strongs), "Léxico only queried its module");
});

scenario("Failure: a rejected dictionary lookup is not shown as «No hay módulos» or stuck «Cargando…»", () => {
  boot({ storage: { nav: NAV_JN316 } });
  H.overrides.lookup_dictionary = () => ({ err: "No se pudo abrir módulo: unable to open database file" });
  clickEl(wordEl("amó", 16));
  settle();
  assert(!noModulesShown(), "«No hay módulos» shown for a failed lookup");
  excludes(studyText(), "Cargando…", "stuck on Cargando");
  includes(studyText(), "Diccionario ·", "dictionary view still rendered");
});

scenario("S5b fresh Comentario (nothing saved): after the content-first switch 'Otros' still excludes the primary", () => {
  // Jn 3:7: the alphabetical first commentary (Clarke) has no note there, so the probe switches.
  boot({ storage: { nav: Object.assign({}, NAV_JN316, { verse: 7 }), study: { workspace: "comentario" } } });
  const primary = state.studyModule;
  assert(isKind(primary, "commentary"), "primary is a commentary");
  assert(primary !== `${LIB_DIR}/clarke.cmti`, "content-first switch happened (Clarke has no Jn 3:7 note)");
  const others = $id("link-panel-inline").querySelectorAll(".mini-entry").map((e) => e.dataset.path);
  assert(others.length > 0, "other commentaries listed");
  assert(!others.includes(primary), `'Otros comentarios' lists the primary ${primary}`);
  includes(studyText(), "Comentario · Juan 3:7", "primary rendered");
  eq((storedStudy().modulesByKind || {}).comentario || "", "", "automatic pick not persisted");
});
