// ISS-33 — the big Hebrew/Greek word on the Léxico card (role=button) works: click, Enter and
// Space each send ONE resolve_word for its Strong code (the single chip path).

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const onLexico = () => ({ nav: NAV, study: { workspace: "lexico", selectedWord: "amó", lastStrong: "G25", modulesByKind: { lexico: STRONGES } } });
const orig = () => $id("study-content").querySelector(".lex-original[data-strong]");

scenario("Click on .lex-original → exactly one resolve_word('G25'), still on Léxico", () => {
  boot({ storage: onLexico() });
  assert(orig(), "lexicon card with the original word");
  eq(orig().dataset.strong, "G25", "card code");
  const mark = H.calls.length;
  clickEl(orig());
  settle();
  eq(callsSince(mark, "resolve_word").map((c) => c.args.word), ["G25"], "one resolve_word for G25");
  eq(state.workspace, "lexico", "Léxico");
  eq(state.lastStrong, "G25", "current Strong");
});

scenario("Enter and Space on the focused word do the same", () => {
  boot({ storage: onLexico() });
  for (const key of ["Enter", " "]) {
    const mark = H.calls.length;
    dispatch(orig(), makeEvent("keydown", { key }));
    settle();
    eq(callsSince(mark, "resolve_word").map((c) => c.args.word), ["G25"], `«${key}» → one resolve_word`);
  }
});

scenario("Other keys do nothing", () => {
  boot({ storage: onLexico() });
  const mark = H.calls.length;
  dispatch(orig(), makeEvent("keydown", { key: "a" }));
  settle();
  eq(callsSince(mark, "resolve_word").length, 0, "no lookup");
});
