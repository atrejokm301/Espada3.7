// ISS-23 (user decision 2) — the WHOLE fallback chain, not only its first link: when the
// preferred module is missing from the folder, the next one in the user's list is used.
//   Bible: 00RVR1960 → «RV1960 Reina Valera 1960» → any Bible
//   Comentario: Matthew Henry → MacArthur → Mundo Hispano → Plenitud
//   Diccionario: Nelson → Vila-Escuain → Certeza → Mundo Hispano
//   Léxico: STRONGes → Strongspa → StrongPLUS → Multiléxico → BDB-Thayer; AHLB last
// Automatic picks are never saved. Folders are the recorded library minus some files (by name).

const NAV = { book: 43, chapter: 3, verse: 16 };
const libList = () => H.fixture.responses[fixtureKey("list_modules", { modulesDir: LIB_DIR })].ok;

const matches = (m, d) => (typeof d === "function" ? d(m) : d.test(m.filename));
/**
 * Boot with the library minus every module matching one of `drop` (filename RegExp or
 * predicate on the module); `mark(m)` may return fields to override (e.g. {encrypted:true}).
 */
function bootWithout(drop, { ws = "biblia", study = {}, mark = () => null } = {}) {
  H.overrides.list_modules = (a) =>
    a.modulesDir === LIB_DIR
      ? {
          ok: libList()
            .filter((m) => !drop.some((d) => matches(m, d)))
            .map((m) => Object.assign({}, m, mark(m) || {})),
        }
      : undefined;
  // The pick is what is under test: content replies are neutral (nothing recorded for subsets).
  H.overrides.probe_modules_content = (a) => ({ ok: a.paths.map((p) => ({ path: p, has: false })) });
  H.overrides.resolve_word = (a) => ({ ok: { word: a.word, normalized: a.word, strongs: ["G25"], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: true, interlinearHits: [], matchedHit: null } });
  H.overrides.lookup_dictionary = () => ({ ok: [] });
  H.overrides.lookup_lexicon = () => ({ ok: null });
  H.overrides.lookup_lexicon_multi = () => ({ ok: [] });
  H.overrides.get_commentary = () => ({ ok: [] });
  boot({ storage: { nav: NAV, study: Object.assign({ workspace: ws, selectedWord: "amó", lastStrong: "G25" }, study) } });
}
const fileOf = (p) => (moduleByPath(p) || {}).filename || "";
const tab = (ws) => {
  clickEl(document.querySelector(`.ws-tab[data-ws="${ws}"]`));
  settle();
};
const savedKinds = () => (storedStudy() || {}).modulesByKind || {};
function expectPick(ws, re, why) {
  const mark = H.calls.length;
  tab(ws);
  assert(re.test(fileOf(state.studyModule)), `${why}: got «${fileOf(state.studyModule)}»`);
  eq(savedKinds()[ws] || "", "", `${ws}: automatic pick not saved`);
  return mark;
}

const CMH = /matthew henry|^cmh\.cmti$/i;
const MACARTHUR = /macarthur/i;
const MUNDO_HISPANO_C = /mundo_?hispano|mundohispano/i;
const NELSON = /^nelson\.dcti$/i;
const VILA = /vila.?escua/i;
const CERTEZA = /certeza/i;
const STRONGES = /STRONGes\.lexi$/;
const STRONGSPA = /^strongspa\.lexi$/i;
const STRONGPLUS = /strongsplus\.lexi$/i;
const MULTILEX = /^multil/i;

scenario("Comentario: no Matthew Henry → MacArthur", () => {
  bootWithout([CMH]);
  expectPick("comentario", MACARTHUR, "MacArthur");
});

scenario("Comentario: no Matthew Henry, no MacArthur → Mundo Hispano", () => {
  bootWithout([CMH, MACARTHUR]);
  expectPick("comentario", MUNDO_HISPANO_C, "Mundo Hispano");
});

scenario("Comentario: none of MH/MacArthur/Mundo Hispano → Plenitud", () => {
  bootWithout([CMH, MACARTHUR, MUNDO_HISPANO_C]);
  expectPick("comentario", /plenitud/i, "Plenitud");
});

scenario("Comentario: Matthew Henry encrypted → skipped for MacArthur (an unreadable default is useless)", () => {
  bootWithout([], { mark: (m) => (CMH.test(m.filename) ? { encrypted: true } : null) });
  expectPick("comentario", MACARTHUR, "MacArthur");
});

scenario("Diccionario: no Nelson → Vila-Escuain, and the first lookup uses it", () => {
  bootWithout([NELSON]);
  const mark = expectPick("diccionario", VILA, "Vila-Escuain");
  const first = callsSince(mark, "lookup_dictionary")[0];
  assert(first && first.args.modulePath === state.studyModule, "the first lookup uses the pick (no other module first)");
});

scenario("Diccionario: no Nelson, no Vila-Escuain → Certeza", () => {
  bootWithout([NELSON, VILA]);
  expectPick("diccionario", CERTEZA, "Certeza");
});

scenario("Diccionario: no Nelson/Vila/Certeza → Mundo Hispano", () => {
  bootWithout([NELSON, VILA, CERTEZA]);
  expectPick("diccionario", /mundo ?hispano/i, "Mundo Hispano");
});

scenario("Léxico chain: STRONGes → Strongspa → StrongPLUS → Multiléxico → BDB-Thayer", () => {
  const chain = [
    [[], STRONGES],
    [[STRONGES], STRONGSPA],
    [[STRONGES, STRONGSPA], STRONGPLUS],
    [[STRONGES, STRONGSPA, STRONGPLUS], MULTILEX],
    [[STRONGES, STRONGSPA, STRONGPLUS, MULTILEX], /BDB-THAYERes/i],
  ];
  // One folder per step: re-boot is not possible in one process, so switch folders.
  bootWithout([]);
  let step = 0;
  for (const [drop, want] of chain) {
    const dir = `/tmp/espada-lex-${step++}`;
    H.overrides.list_modules = (a) => ({ ok: libList().filter((m) => !drop.some((re) => re.test(m.filename))) });
    H.overrides.find_interlinear = () => ({ ok: null });
    $id("modules-path").value = dir;
    clickEl($id("btn-apply-path"));
    settle();
    tab("biblia");
    expectPick("lexico", want, `step ${step}: ${want}`);
  }
}, { allowMissingFixtures: true }); // the step folders' chapters are not recorded

scenario("Léxico: no Spanish Strong lexicon at all → a neutral one, never AHLB while another exists", () => {
  bootWithout([STRONGES, STRONGSPA, STRONGPLUS, MULTILEX, /BDB-THAYERes/i]);
  tab("lexico");
  assert(!/AHLB/i.test(fileOf(state.studyModule)), `AHLB picked: ${fileOf(state.studyModule)}`);
});

scenario("Edge: AHLB is the only lexicon → it is used (better than nothing)", () => {
  const otherLexicon = (m) =>
    (m.kind === "lexicon" && m.filename !== "04AHLB.lexi") ||
    (m.kind === "dictionary" && /strong|lexic|vine|thayer|bdb|hebrew|hebreo|griego|greek|ahlb|ancient hebrew/i.test(`${m.abbreviation} ${m.title} ${m.filename}`));
  bootWithout([otherLexicon]);
  eq(studyCandidates("lexico").map((m) => m.filename), ["04AHLB.lexi"], "only AHLB offered");
  tab("lexico");
  eq(fileOf(state.studyModule), "04AHLB.lexi", "the only lexicon");
});

scenario("Bible: no 00RVR1960 → «RV1960 Reina Valera 1960»", () => {
  bootWithout([/^00RVR1960\.bbli$/]);
  eq(fileOf(state.biblePath), "RV1960 Reina Valera 1960.bbli", "second RV1960 file");
  eq($id("sel-bible").value, state.biblePath, "dropdown shows it");
}, { allowMissingFixtures: true }); // that Bible's chapter is not recorded

scenario("Edge: no RV1960 Bible at all → still some Bible selected (never an empty reader)", () => {
  bootWithout([/^00RVR1960\.bbli$/, /^RV1960 Reina Valera 1960\.bbli$/, (m) => m.kind === "bible" && /^RVR?1960$/i.test(String(m.abbreviation).trim())]);
  assert(state.biblePath && moduleKind(state.biblePath) === "bible", `a Bible is selected, got «${state.biblePath}»`);
  eq($id("sel-bible").value, state.biblePath, "dropdown shows it");
  excludes($id("bible-content").textContent, "Seleccione una carpeta de módulos y una Biblia.", "not the no-Bible message");
}, { allowMissingFixtures: true });
