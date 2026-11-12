// ISS-45 — hovering a Strong link previews the user's lexicon (Léxico choice, then Spanish
// Strong lexicons; English AHLB last), with a 'lemma · transliteration — glosses' first line.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const AHLB = `${LIB_DIR}/04AHLB.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
function hover(code) {
  const el = document.createElement("span");
  el.className = "strong-ref";
  el.dataset.strong = code;
  el.textContent = code;
  $id("study-content").appendChild(el);
  const mark = H.calls.length;
  dispatch(el, makeEvent("mouseover"));
  settle();
  return { call: callsSince(mark, "lookup_lexicon_multi")[0], tip: $id("tooltip") };
}

scenario("Saved Léxico = STRONGes → hovering H430 asks STRONGes first; Spanish text, not AHLB", () => {
  boot({ storage: { nav: NAV, study: { modulesByKind: { lexico: STRONGES } } } });
  const { call, tip } = hover("H430");
  assert(call, "lookup_lexicon_multi sent");
  eq(call.args.modulePaths[0], STRONGES, "chosen lexicon first");
  assert(!tip.classList.contains("hidden"), "tooltip shown");
  excludes(tip.textContent, "AHLB#", "not the English AHLB entry");
  includes(tip.querySelector(".tip-lemma").textContent, "אֱלֹהִים", "Hebrew lemma line");
});

scenario("Nothing saved → a Spanish Strong lexicon comes before AHLB, which is last", () => {
  boot({ storage: { nav: NAV } });
  const { call } = hover("H430");
  const paths = call.args.modulePaths;
  const abbr = (p) => (moduleByPath(p) || {}).abbreviation;
  eq(abbr(paths[0]), "STRONGes", "STRONGes first");
  assert(paths.indexOf(STRONGES) < paths.indexOf(AHLB), "STRONGes before AHLB");
  eq(abbr(paths[paths.length - 1]), "AHLB", "AHLB last");
  const order = state.modules.filter((m) => m.kind === "lexicon").map((m) => m.path);
  eq(order[0], AHLB, "state.modules keeps the scan order (not re-sorted)");
});

scenario("A saved lexicon that no longer exists is skipped", () => {
  const gone = `${LIB_DIR}/no-existe.lexi`;
  boot({ storage: { nav: NAV, study: { modulesByKind: { lexico: gone } } } });
  const { call } = hover("G26");
  assert(!call.args.modulePaths.includes(gone), "missing module not sent");
  eq((moduleByPath(call.args.modulePaths[0]) || {}).abbreviation, "STRONGes", "preferred first");
});

scenario("First line 'ἀγάπη · agápe — ágape, amado, amor' (lemma and transliteration separated)", () => {
  boot({ storage: { nav: NAV, study: { modulesByKind: { lexico: STRONGES } } } });
  const { tip } = hover("G26");
  eq(tip.querySelector(".tip-lemma").textContent, "ἀγάπη · agápe — ágape, amado, amor", "lemma line");
  excludes(tip.textContent, "ἀγάπηagápe", "no run-together lemma");
});

scenario("Edge: an entry that does not start with Greek/Hebrew is shown as plain text", () => {
  boot({ storage: { nav: NAV } });
  H.overrides.lookup_lexicon_multi = () => ({ ok: { topic: "G26", html: "Amor", plain: "Amor fraternal y divino." } });
  const { tip } = hover("G26");
  eq(tip.querySelector(".tip-lemma"), null, "no lemma line");
  includes(tip.textContent, "Amor fraternal y divino.", "plain text");
});

scenario("Error path: lookup fails → only the code, nothing thrown", () => {
  boot({ storage: { nav: NAV } });
  H.overrides.lookup_lexicon_multi = () => ({ err: "No se pudo abrir módulo" });
  const { tip } = hover("G26");
  eq(tip.textContent.trim(), "G26", "code only");
});
