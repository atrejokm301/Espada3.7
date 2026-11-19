// «ES» line of the Léxico (2026-11-18, ported from the Windows build): a Strong's number's
// Reina-Valera renderings from a Spanish Strong's dictionary. The sources are the installed
// Spanish Strong's dictionaries by title (Strong ° first, then STRONGes, then Strong SPA; the
// lexicon in use first when it is one of them); the row is filled after the card renders and
// also supplies the original word and its pronunciation when the card has none.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const STRONG_DEG = `${LIB_DIR}/01 Diccionario strong.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };

const G26 = { code: "G26", spanish: "ágape, amado, amor", translit: "agápe", original: "ἀγάπη", source: "Strong °", module: "01 Diccionario strong.lexi" };

function setupLexCode(code, { gloss = (a) => ({ ok: a.code === "G26" ? G26 : null }), lexico = STRONGES, original = "" } = {}) {
  H.overrides.resolve_word = (a) =>
    a.word === code
      ? {
          ok: {
            word: code, normalized: code, strongs: [code], hebrew: [], greek: original ? [original] : [], dictionary: [], lexicon: [],
            interlinearUsed: false, interlinearHits: [], matchedHit: null, matchKind: "",
          },
        }
      : undefined;
  H.overrides.lookup_lexicon = (a) => ({ ok: a.strong === code ? { topic: code, html: `<p>Definición de ${code}</p>`, plain: `Definición de ${code}`, matchKind: "exact" } : null });
  H.overrides.strong_gloss = gloss;
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "lexico", modulesByKind: { lexico: lexico, diccionario: "", comentario: "" } } } });
  $id("dict-query").value = code;
  clickEl($id("btn-dict-search"));
  settle();
}
const card = () => $id("study-content").querySelector(".lex-card");
const esRow = () => $id("study-content").querySelector(".lex-es-row");
const glossCalls = (mark = 0) => H.calls.slice(mark).filter((c) => c.cmd === "strong_gloss");

scenario("A typed number gets its Spanish renderings, with the dictionary named", () => {
  setupLexCode("G26");
  const row = esRow();
  assert(row, "ES row in the card");
  eq(row.querySelector(".lex-es-label").textContent, "ES", "label");
  eq(row.querySelector(".lex-es-val").textContent, "ágape, amado, amor", "renderings");
  includes(row.querySelector(".lex-es-src").textContent, "Strong °", "source named");
  assert(!row.querySelector(".lex-es-val").classList.contains("es-pending"), "no longer pending");
  const c = glossCalls()[0];
  eq(c.args.code, "G26", "asked for G26");
  eq(c.args.modulePaths[0], STRONGES, "the lexicon in use first: it is a Spanish Strong's dictionary");
  assert(c.args.modulePaths.includes(STRONG_DEG), "Strong ° among the sources");
  assert(c.args.modulePaths.length <= 4, "at most 4 sources");
  assert(!c.args.modulePaths.some((p) => /plus|multil/i.test(p)), "never StrongPLUS or the Multiléxico");
}, { allowMissingFixtures: true });

scenario("Without the lexicon in use among them, Strong ° comes first", () => {
  setupLexCode("G26", { lexico: `${LIB_DIR}/04SWANSON.lexi` });
  const c = glossCalls()[0];
  assert(c, "asked");
  eq(c.args.modulePaths[0], STRONG_DEG, "Strong ° first");
}, { allowMissingFixtures: true });

scenario("A card without the original word takes the dictionary's word and pronunciation", () => {
  setupLexCode("G26");
  const orig = card().querySelector(".lex-original");
  eq(orig.textContent.trim(), "ἀγάπη", "original word filled in");
  eq(card().querySelector(".lex-translit").textContent, "agápe", "pronunciation filled in");
}, { allowMissingFixtures: true });

scenario("A number the dictionaries give no list for says so (no endless «Buscando…»)", () => {
  setupLexCode("G3588", { gloss: () => ({ ok: null }) });
  const v = esRow().querySelector(".lex-es-val");
  eq(v.textContent, "(el Diccionario Strong no da traducción para este número)", "explicit");
  assert(v.classList.contains("es-none"), "styled as a note");
}, { allowMissingFixtures: true });

scenario("A failed read is a note, not an error page, and is asked again next time", () => {
  setupLexCode("G26", { gloss: () => ({ err: "database is locked" }) });
  includes(esRow().querySelector(".lex-es-val").textContent, "no da traducción", "note");
  H.overrides.strong_gloss = (a) => ({ ok: G26 });
  const mark = H.calls.length;
  $id("dict-query").value = "G26";
  clickEl($id("btn-dict-search"));
  settle();
  eq(glossCalls(mark).length, 1, "asked again (a failure is not cached)");
  eq(esRow().querySelector(".lex-es-val").textContent, "ágape, amado, amor", "filled this time");
}, { allowMissingFixtures: true });

scenario("A number opened from the Léxico's Índice gets its «ES» line too", () => {
  setupLexCode("G26");
  H.overrides.dictionary_index = (a) =>
    a.lexicon
      ? { ok: { total: 2, start: 0, focus: 0, exact: true, rows: [{ pos: 0, rowid: 7, topic: "G25", lemma: "ἀγαπάω" }, { pos: 1, rowid: 8, topic: "G26", lemma: "ἀγάπη" }], strongKeyed: true, greekStart: 0, hebrewStart: null } }
      : undefined;
  H.overrides.get_dictionary_entry = (a) => ({ ok: a.rowid === 7 ? { topic: "G25", html: "<p>amar</p>", plain: "amar", matchKind: "exact" } : null });
  H.overrides.strong_gloss = (a) => ({ ok: a.code === "G25" ? { ...G26, code: "G25", spanish: "amado, amar", original: "ἀγαπάω", translit: "agapáo" } : G26 });
  $id("dict-query").value = "G26";
  clickEl($id("btn-dict-search"));
  settle();
  clickEl($id("dict-index").querySelector('.dict-index-row[data-topic="G25"]'));
  settle();
  const row = $id("study-content").querySelector(".study-entry .lex-es-row");
  assert(row, "ES row in the opened entry");
  eq(row.querySelector(".lex-es-val").textContent, "amado, amar", "G25's renderings");
}, { allowMissingFixtures: true });
