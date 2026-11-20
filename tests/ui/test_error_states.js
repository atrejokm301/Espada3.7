// ISS-26 — a failed lookup is shown as an error (Spanish headline, module, detail, «Reintentar»,
// «Elegir otro módulo»), never as «Sin resultados» / «No se encontró definición».

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: CMH };
const on = (ws, extra = {}) => ({ nav: NAV, study: Object.assign({ workspace: ws, modulesByKind: SAVED }, extra) });
const errBox = () => $id("study-content").querySelector(".empty-state.error");

scenario("Diccionario: a rejected lookup → error block, not «Sin resultados»; «Reintentar» re-runs it", () => {
  boot({ storage: on("diccionario", { selectedWord: "amor" }) });
  H.overrides.lookup_dictionary = () => ({ err: "No se pudo abrir módulo: unable to open database file: /x/Nelson.dcti" });
  $id("dict-query").value = "gracia";
  clickEl($id("btn-dict-search"));
  settle();
  const box = errBox();
  assert(box, "error block rendered");
  eq(box.getAttribute("role"), "alert", "announced");
  includes(box.textContent, "El módulo ya no está en la carpeta", "Spanish headline");
  includes(box.textContent, "NELSON", "names the module");
  excludes(studyText(), "Sin resultados", "not presented as an empty result");
  H.overrides.lookup_dictionary = () => ({ ok: [] });
  const mark = H.calls.length;
  clickEl(box.querySelector('[data-error-act="retry"]'));
  settle();
  eq(callsSince(mark, "lookup_dictionary").length, 1, "retry looked up again");
  includes(studyText(), "Sin resultados", "now a genuine empty result");
  eq(errBox(), null, "error cleared");
});

scenario("Diccionario: an empty array is «Sin resultados» (no error)", () => {
  boot({ storage: on("diccionario", { selectedWord: "amor" }) });
  H.overrides.lookup_dictionary = () => ({ ok: [] });
  $id("dict-query").value = "zzzz";
  clickEl($id("btn-dict-search"));
  settle();
  includes(studyText(), "Sin resultados", "empty result");
  eq(errBox(), null, "no error block");
});

scenario("Léxico: every lookup rejected → error block, not «No se encontró definición»", () => {
  boot({ storage: on("lexico", { selectedWord: "amó", lastStrong: "G25" }) });
  H.overrides.lookup_lexicon = () => ({ err: "no such column: Definition" });
  $id("dict-query").value = "G26";
  clickEl($id("btn-dict-search"));
  settle();
  assert(errBox(), "error block");
  includes(errBox().textContent, "Formato de módulo no compatible", "Spanish headline");
  excludes(studyText(), "No se encontró definición", "not presented as missing");
});

scenario("Comentario: get_commentary rejected → error block; «Elegir otro módulo» opens the module list", () => {
  boot({ storage: on("comentario") });
  H.overrides.get_commentary = (a) => (a.modulePath === CMH ? { err: "no such table: VerseCommentary" } : undefined);
  clickEl($id("btn-verse-next"));
  settle();
  assert(errBox(), "error block");
  includes(errBox().textContent, "Formato de módulo no compatible", "headline");
  includes(errBox().textContent, "no such table: VerseCommentary", "raw detail kept small");
  clickEl(errBox().querySelector('[data-error-act="choose"]'));
  // The native select is hidden now (focusing it showed nothing): the module dropdown opens instead.
  eq($id("glass-menu").dataset.for, "sel-study-module", "module list open");
  assert(!$id("glass-menu").classList.contains("hidden"), "menu visible");
  assert($id("glass-menu").contains(document.activeElement), "focus inside the list");
});

scenario("Edge: unknown error text → generic Spanish headline, escaped detail", () => {
  boot({ storage: on("diccionario", { selectedWord: "amor" }) });
  H.overrides.lookup_dictionary = () => ({ err: "<b>boom</b>" });
  $id("dict-query").value = "fe";
  clickEl($id("btn-dict-search"));
  settle();
  includes(errBox().textContent, "No se pudo leer este módulo.", "generic headline");
  includes(errBox().querySelector(".error-detail").textContent, "<b>boom</b>", "detail shown as text");
  eq(errBox().querySelector("b"), null, "no markup injected");
});
