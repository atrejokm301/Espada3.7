// ISS-02 (UI half): the tap says WHICH of the same words in the verse it was (occurrence),
// typed searches say they are typed, and «no Strong in the interlinear» gets its own message.
// Before the fix only the word text was sent, so the three «que» of Jn 3:16 always got the
// same answer (γάρ G1063), and an empty result read «No se encontró definición… Pruebe otro módulo».

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: "" };

function tapResolve(el) {
  const mark = H.calls.length;
  clickEl(el);
  settle();
  const rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, "one resolve_word per tap");
  return rw[0].args;
}

scenario("Jn 3:16: the 1st, 2nd and 3rd «que» send occurrence 0, 1, 2", () => {
  boot({ storage: { nav: NAV, study: { modulesByKind: SAVED } } });
  for (const n of [0, 1, 2]) {
    const args = tapResolve(wordEl("que", 16, n));
    eq(args.word, "que", "word");
    eq(args.occurrence, n, `occurrence of «que» #${n + 1}`);
    eq(args.biblePath, RVR, "biblePath sent (ISS-35)");
    eq(args.typed, undefined, "a tap is not typed");
    eq(args.taggedStrongs, undefined, "untagged Bible: no taggedStrongs");
  }
});

scenario("Case and accents don't split the count; other words are not counted", () => {
  boot({ storage: { nav: NAV, study: { modulesByKind: SAVED } } });
  H.overrides.get_bible_chapter = (a) =>
    a.modulePath === RVR && a.chapter === 3
      ? { ok: { book: 43, chapter: 3, module: RVR, abbreviation: "RVR1960", title: "", verses: [
          { verse: 16, html: "Que el que oye diga: Ven; y QUÉ es eso que", plain: "" },
        ] } }
      : undefined;
  reloadChapter();
  settle();
  const last = $id("bible-content").querySelectorAll(".verse .word").filter((w) => normalizeLoose(w.textContent) === "que");
  eq(last.length, 4, "four forms of que");
  H.overrides.resolve_word = (a) => ({ ok: { word: a.word, normalized: "que", strongs: [], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: false, interlinearHits: [], matchedHit: null } });
  eq(tapResolve(last[3]).occurrence, 3, "«que» after Que/que/QUÉ is occurrence 3");
}, { allowMissingFixtures: true });

scenario("Morphology codes, footnotes and Strong links are never counted", () => {
  boot({ storage: { nav: NAV, study: { modulesByKind: SAVED } } });
  H.overrides.get_bible_chapter = (a) =>
    a.modulePath === RVR && a.chapter === 3
      ? { ok: { book: 43, chapter: 3, module: RVR, abbreviation: "RVR1960", title: "", verses: [
          { verse: 16, html: 'a <span class="morph">a</span> <sup class="fn">a</sup> <span class="strong-ref" data-strong="H853">H853</span> y a', plain: "" },
        ] } }
      : undefined;
  reloadChapter();
  settle();
  const as = $id("bible-content").querySelectorAll(".verse .word").filter((w) => w.textContent === "a");
  eq(as.length, 2, "only the two real «a» are words");
  H.overrides.resolve_word = (a) => ({ ok: { word: a.word, normalized: "a", strongs: [], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: false, interlinearHits: [], matchedHit: null } });
  eq(tapResolve(as[1]).occurrence, 1, "second real «a»");
}, { allowMissingFixtures: true });

// Review round 1: RVR1960x («con títulos y notas») puts the section title inside the verse.
// Today's backend sends it as plain <strong>…</strong><br> (recorded: Jn 3:16 = "<strong>De tal
// manera amó Dios al mundo</strong><br>Porque de tal manera amó…"), which the UI cannot tell from
// in-verse bold. Contract for WP2: wrap such titles in <span class="vtitle">; their words are
// then skipped by the count, because the interlinear the backend aligns with has no titles.
function titledVerse16(html) {
  H.overrides.get_bible_chapter = (a) =>
    a.modulePath === RVR && a.chapter === 3
      ? { ok: { book: 43, chapter: 3, module: RVR, abbreviation: "RVR1960", title: "", verses: [{ verse: 16, html, plain: "" }] } }
      : undefined;
  reloadChapter();
  settle();
  H.overrides.resolve_word = (a) => ({ ok: { word: a.word, normalized: a.word, strongs: [], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: false, interlinearHits: [], matchedHit: null } });
}
scenario("A .vtitle section title inside the verse is not counted: verse-body «amó» is occurrence 0", () => {
  boot({ storage: { nav: NAV, study: { modulesByKind: SAVED } } });
  titledVerse16('<span class="vtitle"><strong>De tal manera amó Dios al mundo</strong></span><br>Porque de tal manera amó Dios al mundo, que ha dado; amó');
  eq(wordEl("amó", 16, 0).closest(".vtitle") != null, true, "fixture: first «amó» is the title's");
  eq(tapResolve(wordEl("amó", 16, 1)).occurrence, 0, "first «amó» of the verse text");
  eq(tapResolve(wordEl("amó", 16, 2)).occurrence, 1, "second «amó» of the verse text");
  eq(tapResolve(wordEl("Dios", 16, 1)).occurrence, 0, "«Dios» of the verse text");
}, { allowMissingFixtures: true });

scenario("Bold words that are not a .vtitle still count (no guessing from <strong>)", () => {
  boot({ storage: { nav: NAV, study: { modulesByKind: SAVED } } });
  titledVerse16("<strong>amó</strong> y amó");
  eq(tapResolve(wordEl("amó", 16, 1)).occurrence, 1, "bold «amó» counted");
}, { allowMissingFixtures: true });

scenario("Right-click 'Buscar palabra' on the 2nd «que» sends occurrence 1", () => {
  boot({ storage: { nav: NAV, study: { modulesByKind: SAVED } } });
  const que = wordEl("que", 16, 1);
  contextMenu(que);
  settle();
  const mark = H.calls.length;
  clickEl($id("ctx-menu").querySelector('button[data-act="lookup"]'));
  settle();
  const rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, "one resolve_word");
  eq(rw[0].args.occurrence, 1, "occurrence");
  eq(rw[0].args.verse, 16, "verse");
});

scenario("Buscar / Enter sends typed:true and no tap context", () => {
  boot({ storage: { nav: NAV, study: { workspace: "lexico", modulesByKind: SAVED } } });
  $id("dict-query").value = "amor";
  let mark = H.calls.length;
  clickEl($id("btn-dict-search"));
  settle();
  let rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, "one resolve_word");
  eq(rw[0].args.typed, true, "typed flag");
  eq(rw[0].args.occurrence, undefined, "no occurrence for a typed word");
  eq(rw[0].args.taggedStrongs, undefined, "no tags for a typed word");
  // Enter goes through the same button
  $id("dict-query").value = "mundo";
  mark = H.calls.length;
  dispatch($id("dict-query"), makeEvent("keydown", { key: "Enter" }));
  settle();
  rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, "Enter → one resolve_word");
  eq(rw[0].args.typed, true, "Enter is typed too");
  eq(state.wordOrigin.typed, true, "typed is kept with the word (relaunch re-sends it)");
});

scenario("Interlinear read but no Strong for the word → its own message, not «Pruebe otro módulo»", () => {
  // The reply is the real HEAD reply for «Nicodemo» at Jn 3:17 (interlinearUsed, strongs []),
  // served for a tapped word of that verse.
  boot({ storage: { nav: Object.assign({}, NAV, { verse: 17 }), study: { workspace: "lexico", modulesByKind: SAVED } } });
  const recorded = Object.entries(H.fixture.responses).find(
    ([k]) => k.startsWith('resolve_word|["Nicodemo"') && k.endsWith(",43,3,17]")
  );
  assert(recorded, "recorded empty reply present");
  H.overrides.resolve_word = (a) => ({ ok: Object.assign({}, recorded[1].ok, { word: a.word }) });
  clickEl(wordEl("condenar", 17));
  settle();
  eq(state.workspace, "diccionario", "tap → Diccionario (user decision)");
  eq(state.wordLinks.interlinearUsed, true, "fixture: interlinear consulted");
  eq((state.wordLinks.strongs || []).length, 0, "fixture: no Strong");
  setWorkspace("lexico");
  settle();
  includes(studyText(), "Sin número Strong en el interlineal para esta palabra (traducción distinta)", "empty-Strong message");
  excludes(studyText(), "No se encontró definición", "not the generic message");
});

scenario("Edge: invalid codes only (old backend 'G0' for «gracia») count as no Strong", () => {
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "lexico", modulesByKind: SAVED } } });
  $id("dict-query").value = "gracia";
  clickEl($id("btn-dict-search"));
  settle();
  eq(state.wordLinks.strongs, ["G0"], "fixture: the old backend returns G0");
  excludes(studyText(), "G0", "G0 is never shown");
  includes(studyText(), "Sin número Strong para «gracia» en el interlineal de este versículo", "typed variant of the message");
});

scenario("Failure: resolve_word rejects → error shown in the side pane, no crash", () => {
  boot({ storage: { nav: NAV, study: { modulesByKind: SAVED } } });
  H.overrides.resolve_word = () => ({ err: "base de datos bloqueada" });
  clickEl(wordEl("que", 16, 1));
  settle();
  includes($id("link-panel-inline").textContent, "base de datos bloqueada", "error surfaced");
}, { allowMissingFixtures: true });
