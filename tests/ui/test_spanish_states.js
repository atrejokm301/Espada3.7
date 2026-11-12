// ISS-26 (+ empty states) — every failure the reader sees is explained in Spanish; the raw
// backend text (often English SQLite) is only a small detail line. A failed probe or an empty
// result is never presented as an error, and an error is never presented as «Sin resultados».

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: CMH };
const on = (ws, extra = {}) => ({ nav: NAV, study: Object.assign({ workspace: ws, modulesByKind: SAVED }, extra) });
const SPANISH_HEADLINES = [
  "Formato de módulo no compatible.",
  "El módulo ya no está en la carpeta. Pulse «Actualizar lista de módulos» en Ajustes.",
  "El archivo no es un módulo válido.",
  "No se pudo leer este módulo.",
];
const ENGLISH = /\b(the|file|database|unable|such|table|column|error|failed|not found|locked|malformed)\b/i;
const errIn = (id) => $id(id).querySelector(".empty-state.error");
const headline = (box) => (box.querySelector(".error-title") || box).textContent.replace("⚠", "").trim();

scenario("Unit: real backend/SQLite messages map to a Spanish headline; nothing English in the headline", () => {
  boot({ storage: on("biblia") });
  const cases = [
    ["no such table: Dictionary", 0],
    ["no such column: Definition", 0],
    ["No se pudo abrir módulo: unable to open database file: /x/Nelson.dcti", 1],
    ["No such file or directory (os error 2)", 1],
    ["file is not a database", 2],
    ["file is encrypted or is not a database", 2],
    ["database disk image is malformed", 3],
    ["database is locked", 3],
    ["", 3],
  ];
  for (const [raw, want] of cases) {
    const { title, detail } = friendlyError(raw);
    eq(title, SPANISH_HEADLINES[want], `headline for «${raw}»`);
    assert(!ENGLISH.test(title), `English in headline for «${raw}»`);
    if (raw) eq(detail, raw, `raw text kept as detail for «${raw}»`);
  }
  eq(friendlyError(new Error("no such table: X")).title, SPANISH_HEADLINES[0], "Error objects use their message");
});

scenario("Diccionario: 'file is encrypted or is not a database' → «El archivo no es un módulo válido.»", () => {
  boot({ storage: on("diccionario", { selectedWord: "amor" }) });
  H.overrides.lookup_dictionary = () => ({ err: "file is encrypted or is not a database" });
  $id("dict-query").value = "gracia";
  clickEl($id("btn-dict-search"));
  settle();
  const box = errIn("study-content");
  assert(box, "error block");
  eq(headline(box), "El archivo no es un módulo válido.", "Spanish headline");
  excludes(studyText(), "Sin resultados", "not an empty result");
});

scenario("Edge: a very long raw error is cut to a short detail line; the headline stays Spanish", () => {
  boot({ storage: on("diccionario", { selectedWord: "amor" }) });
  const raw = "no such column: " + "x".repeat(2000);
  H.overrides.lookup_dictionary = () => ({ err: raw });
  $id("dict-query").value = "gracia";
  clickEl($id("btn-dict-search"));
  settle();
  const box = errIn("study-content");
  eq(headline(box), "Formato de módulo no compatible.", "headline");
  lte(box.querySelector(".error-detail").textContent.length, 240, "detail length");
});

scenario("Léxico: failure, then «Reintentar» after the module is back → the definition replaces the error", () => {
  boot({ storage: on("lexico", { selectedWord: "amó", lastStrong: "G25" }) });
  H.overrides.lookup_lexicon = () => ({ err: "unable to open database file" });
  $id("dict-query").value = "G26";
  clickEl($id("btn-dict-search"));
  settle();
  const box = errIn("study-content");
  assert(box, "error block");
  delete H.overrides.lookup_lexicon;
  clickEl(box.querySelector('[data-error-act="retry"]'));
  settle();
  eq(errIn("study-content"), null, "error gone");
  includes(studyText(), "G26", "definition shown");
  excludes(studyText(), "No se encontró definición", "the entry exists");
});

scenario("Léxico: a Strong code with no entry in the module → Spanish empty message, no error block", () => {
  boot({ storage: on("lexico", { selectedWord: "amó", lastStrong: "G25" }) });
  H.overrides.lookup_lexicon = () => ({ ok: null });
  $id("dict-query").value = "G26";
  clickEl($id("btn-dict-search"));
  settle();
  eq(errIn("study-content"), null, "an empty result is not an error");
  includes(studyText(), "No se encontró definición en", "Spanish empty message");
});

scenario("A rejected content probe is not an error: the view renders, the dropdown just has no ✦", () => {
  boot({ storage: on("diccionario", { selectedWord: "amó" }) });
  H.overrides.probe_modules_content = () => ({ err: "database is locked" });
  $id("dict-query").value = "gracia";
  clickEl($id("btn-dict-search"));
  settle();
  eq(errIn("study-content"), null, "no error block for a probe");
  includes(studyText(), "Diccionario · NELSON-test · «gracia»", "the dictionary rendered");
  const opts = $id("sel-study-module").options;
  assert(opts.length > 1, "candidates still listed");
  assert(opts.every((o) => !o.textContent.startsWith("✦")), "no content marks without a probe");
  eq(state.studyModule, NELSON, "saved choice kept");
  eq(H.uncaught.length, 0, "nothing thrown");
});

scenario("Bible chapter cannot be read → Spanish error in the reading pane; «Reintentar» reloads it", () => {
  boot({ storage: on("biblia") });
  H.overrides.get_bible_chapter = () => ({ err: "No se pudo abrir módulo: unable to open database file: /x/00RVR1960.bbli" });
  clickEl(document.querySelector('.ws-tab[data-ws="biblia"]'));
  changeSelect($id("sel-chapter"), "4");
  settle();
  const box = errIn("bible-content");
  assert(box, "error block in the reading pane");
  eq(box.getAttribute("role"), "alert", "announced");
  assert(SPANISH_HEADLINES.includes(headline(box)), `Spanish headline, got «${headline(box)}»`);
  includes(box.querySelector(".error-detail").textContent, "unable to open", "raw text only as detail");
  delete H.overrides.get_bible_chapter;
  const mark = H.calls.length;
  clickEl(box.querySelector('[data-error-act="retry"]'));
  settle();
  eq(callsSince(mark, "get_bible_chapter").length, 1, "retried");
  assert($id("bible-content").querySelectorAll(".verse").length > 10, "chapter shown");
  eq(errIn("bible-content"), null, "error gone");
});

scenario("Bible error «Elegir otro módulo» focuses the Bible selector", () => {
  boot({ storage: on("biblia") });
  H.overrides.get_bible_chapter = () => ({ err: "no such table: Bible" });
  changeSelect($id("sel-chapter"), "4");
  settle();
  clickEl(errIn("bible-content").querySelector('[data-error-act="choose"]'));
  assert(document.activeElement === $id("sel-bible"), "Bible selector focused");
});

scenario("Word lookup fails → side pane says it in Spanish (raw text only as detail)", () => {
  boot({ storage: on("biblia") });
  H.overrides.resolve_word = () => ({ err: "No se pudo abrir módulo: unable to open database file: /x/iRV.bbli" });
  clickEl(wordEl("amó", 16));
  settle();
  const box = errIn("link-panel-inline");
  assert(box, "error block in the side pane");
  assert(SPANISH_HEADLINES.includes(headline(box)), `Spanish headline, got «${headline(box)}»`);
  includes(box.textContent, "unable to open database file", "detail kept");
  excludes(studyText(), "Cargando…", "main pane not stuck");
});
