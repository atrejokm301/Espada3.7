// ISS-43 (UI half): the Léxico box must not promise Greek/Hebrew input until the backend reports
// a lemma index (new response field lemmaIndexUsed). With it, a typed Greek word renders the
// definition of the code the index returned; several candidates are offered as chips.
// Baseline: the placeholder always said «hebreo, griego o lema», and typing ἀγάπη gave
// «No se encontró definición».

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: "" };
const onLexico = () => ({ nav: NAV, study: { workspace: "lexico", modulesByKind: SAVED } });

function lemmaReply(word, strongs, extra = {}) {
  return {
    ok: Object.assign(
      { word, normalized: word, strongs, hebrew: [], greek: [word], dictionary: [], lexicon: [], interlinearUsed: true,
        interlinearHits: [], matchedHit: null, matchKind: "lemma", related: [], lemmaIndexUsed: true },
      extra
    ),
  };
}
const topics = () => $id("study-content").querySelectorAll(".study-entry .topic").map((t) => t.textContent);

scenario("Older backend (no lemmaIndexUsed field): the placeholder does not promise Greek/Hebrew", () => {
  boot(Object.assign({ fixture: "head" }, { storage: onLexico() }));
  const ph = $id("dict-query").placeholder;
  assert(!/griego|hebreo|griega|hebrea/i.test(ph), `placeholder «${ph}»`);
  includes(ph, "G26", "still offers Strong numbers");
  $id("dict-query").value = "amor";
  clickEl($id("btn-dict-search"));
  settle();
  assert(!/griego|hebreo|griega|hebrea/i.test($id("dict-query").placeholder), "still no promise after a HEAD reply");
});

scenario("Typed ἀγάπη with a lemma index → G26 defined from STRONGes; placeholder now offers Greek/Hebrew", () => {
  boot({ storage: onLexico() });
  H.overrides.resolve_word = (a) => (a.word === "ἀγάπη" ? lemmaReply("ἀγάπη", ["G26"]) : undefined);
  $id("dict-query").value = "ἀγάπη";
  const mark = H.calls.length;
  clickEl($id("btn-dict-search"));
  settle();
  const rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, "one resolve_word");
  eq(rw[0].args.typed, true, "typed flag sent");
  eq(state.lastStrong, "G26", "single candidate becomes the current Strong");
  eq(topics(), ["G26"], "G26 definition rendered (real STRONGes entry)");
  excludes(studyText(), "No se encontró definición", "no empty state");
  includes($id("dict-query").placeholder, "griega o hebrea", "placeholder follows the backend capability");
});

scenario("Several lemma candidates → chips to choose from, no definition guessed", () => {
  boot({ storage: onLexico() });
  H.overrides.resolve_word = (a) => (a.word === "אהבה" ? lemmaReply("אהבה", ["H157", "H160"], { greek: [], hebrew: ["אהבה"] }) : undefined);
  $id("dict-query").value = "אהבה";
  clickEl($id("btn-dict-search"));
  settle();
  eq(state.lastStrong, "", "no Strong picked for the user");
  eq(topics(), [], "no definition guessed");
  const chips = $id("study-content").querySelectorAll(".sync-bar .chip-strong").map((c) => c.dataset.strong);
  eq(chips, ["H157", "H160"], "candidates as chips");
  includes(studyText(), "Posibles números Strong", "chip label");
  includes(studyText(), "varios números Strong", "explains the choice");
});

scenario("lemmaIndexUsed:false is still a capable backend: placeholder switches, no ambiguity rule", () => {
  boot({ storage: onLexico() });
  H.overrides.resolve_word = (a) => (a.word === "amor" ? lemmaReply("amor", ["G26", "G25"], { lemmaIndexUsed: false }) : undefined);
  $id("dict-query").value = "amor";
  clickEl($id("btn-dict-search"));
  settle();
  eq(state.lastStrong, "G26", "first code is current (not a lemma-index result)");
  includes($id("dict-query").placeholder, "griega o hebrea", "capability detected from the field's presence");
});

scenario("Newer backend snapshot (wp3), no mocks: typed ἀγάπη resolves through the real lemma index", () => {
  boot({ fixture: "wp3", storage: onLexico() });
  $id("dict-query").value = "ἀγάπη";
  clickEl($id("btn-dict-search"));
  settle();
  const raw = H.calls.filter((c) => c.cmd === "resolve_word").pop().result.ok;
  eq(raw.lemmaIndexUsed, true, "fixture: the real reply used the lemma index");
  assert(raw.strongs.includes("G26"), `fixture: G26 among ${raw.strongs}`);
  includes($id("dict-query").placeholder, "griega o hebrea", "placeholder offers Greek/Hebrew");
  if (raw.strongs.length === 1) eq(topics(), ["G26"], "single candidate defined");
  else includes(studyText(), "Posibles números Strong", "several candidates offered as chips");
});
