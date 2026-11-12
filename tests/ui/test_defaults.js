// ISS-23 (user decision 2) — defaults when nothing valid is saved: Bible RVR1960; commentary
// Matthew Henry → MacArthur → Mundo Hispano → Plenitud; dictionary Nelson → Vila-Escuain →
// Certeza → Mundo Hispano; lexicon STRONGes → Strongspa → StrongPLUS → Multiléxico → BDB-Thayer,
// AHLB last. Automatic picks are never written to modulesByKind; saved choices always win.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const CJ = `${LIB_DIR}/biblia_corona_de_jerusaln.bbli`;
const CLARKE = `${LIB_DIR}/clarke.cmti`;
const abbrOf = (p) => (moduleByPath(p) || {}).abbreviation || "";
const tab = (ws) => {
  clickEl(document.querySelector(`.ws-tab[data-ws="${ws}"]`));
  settle();
};
const savedKinds = () => (storedStudy() || {}).modulesByKind || {};

scenario("Empty storage → RVR1960 (00RVR1960.bbli), not CJ", () => {
  boot({ storage: {} });
  eq(state.biblePath, RVR, "sidebar Bible");
  eq($id("sel-bible").value, RVR, "dropdown shows it");
});

scenario("Empty storage → Comentario opens Matthew Henry (not Adam Clarke); nothing persisted", () => {
  boot({ storage: {} });
  tab("comentario");
  eq(abbrOf(state.studyModule), "CMH", "Matthew Henry");
  assert(state.studyModule !== CLARKE, "not Adam Clarke");
  includes(studyText(), "Comentario · ", "rendered");
  eq(savedKinds().comentario || "", "", "automatic pick not saved");
});

scenario("Empty storage, tap «fe» → a preferred Spanish dictionary (never the 1909 Catholic one)", () => {
  boot({ storage: { nav: { book: 45, chapter: 1, verse: 17, biblePath: RVR } } });
  clickEl(wordEl("fe", 17));
  settle();
  eq(state.workspace, "diccionario", "word tap opens Diccionario (user decision 1)");
  const abbr = abbrOf(state.studyModule);
  assert(/NELSON|Vila-Escuain|Certeza|Mundo Hispano/i.test(abbr), `preferred dictionary, got ${abbr}`);
  assert(!/catholic/i.test((moduleByPath(state.studyModule) || {}).title || ""), "not the Catholic Dictionary");
  eq(savedKinds().diccionario || "", "", "automatic pick not saved");
});

scenario("Empty storage → Léxico uses STRONGes; AHLB is last in its group of the dropdown", () => {
  boot({ storage: { study: { selectedWord: "amó", lastStrong: "G25" } } });
  tab("lexico");
  eq(abbrOf(state.studyModule), "STRONGes", "STRONGes");
  const opts = $id("sel-study-module").options.map((o) => ({ path: o.value, star: o.textContent.startsWith("✦") }));
  const ahlb = opts.findIndex((o) => /AHLB/.test(abbrOf(o.path)));
  assert(ahlb >= 0, "AHLB offered");
  const sameGroup = opts.filter((o) => o.star === opts[ahlb].star);
  assert(/AHLB/.test(abbrOf(sameGroup[sameGroup.length - 1].path)) || /AHLB/.test(abbrOf(sameGroup[sameGroup.length - 2].path)), "AHLB sorted last");
  eq(savedKinds().lexico || "", "", "automatic pick not saved");
});

scenario("Saved choices win: CJ Bible and Adam Clarke are kept", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: CJ }, study: { workspace: "comentario", modulesByKind: { comentario: CLARKE } } } });
  eq(state.biblePath, CJ, "saved Bible kept");
  eq(state.studyModule, CLARKE, "saved commentary kept");
  eq(savedKinds().comentario, CLARKE, "still saved");
});

scenario("Missing saved paths → preferred fallbacks, the saved values are not overwritten", () => {
  const goneBible = `${LIB_DIR}/no-existe.bbli`;
  const goneCmt = `${LIB_DIR}/no-existe.cmti`;
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: goneBible }, study: { workspace: "comentario", modulesByKind: { comentario: goneCmt } } } });
  eq(state.biblePath, RVR, "Bible fallback = RVR1960");
  eq(abbrOf(state.studyModule), "CMH", "commentary fallback = Matthew Henry");
  eq(savedKinds().comentario, goneCmt, "saved commentary left as the user had it");
});

scenario("Preference table order (unit): MH > MacArthur > Mundo Hispano > Plenitud > others > English", () => {
  boot({ storage: {} });
  const rank = (re) => preferenceRank(findModule(re, "commentary"), "commentary");
  assert(rank(/Comentario Matthew Henry/) < rank(/02BE_Macarthur/), "MH before MacArthur");
  assert(rank(/02BE_Macarthur/) < rank(/comentario_bblico_mundo_hispano/), "MacArthur before Mundo Hispano");
  assert(rank(/comentario_bblico_mundo_hispano/) < rank(/02PLENITUD/), "Mundo Hispano before Plenitud");
  assert(rank(/02PLENITUD/) < rank(/barclay/), "preferred before neutral");
  assert(rank(/barclay/) < rank(/clarke\.cmti/), "Spanish neutral before English");
  const lrank = (re) => preferenceRank(findModule(re), "lexicon");
  assert(lrank(/04STRONGes/) < lrank(/Strongspa/) && lrank(/Strongspa/) < lrank(/StrongsPlus/), "STRONGes > Strongspa > StrongPLUS");
  assert(lrank(/StrongsPlus/) < lrank(/multil/) && lrank(/multil/) < lrank(/04BDB-THAYERes/), "> Multiléxico > BDB-Thayer");
  eq(lrank(/04AHLB/), 200, "AHLB deprioritized");
  const drank = (re) => preferenceRank(findModule(re, "dictionary"), "dictionary");
  assert(drank(/Nelson/) < drank(/Vila Escuain/) && drank(/Vila Escuain/) < drank(/Certeza/) && drank(/Certeza/) < drank(/Mundohispano/), "Nelson > Vila-Escuain > Certeza > Mundo Hispano");
});
