// Accuracy round 2026-11-11 (UI half of the Strong bridge). A verb has no article of its own
// (amó, creó); the backend reaches the concept's article through the word's Strong family
// (G25 → AMOR) and tags it matchKind "strong". The Diccionario view sends the resolved codes
// and the lexicons so the backend can do that, labels those articles «relacionado · G25», and
// a word with no Strong keeps the old call exactly. The side pane labels them the same way.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: "" };
const AMOR = { topic: "AMOR", html: "<p>Afecto profundo…</p>", plain: "Afecto profundo…", matchKind: "strong" };

function resolveAs(word, strongs, dictionary = []) {
  H.overrides.resolve_word = (a) =>
    a.word === word
      ? { ok: { word, normalized: word, strongs, hebrew: [], greek: [], dictionary, lexicon: [], interlinearUsed: true, interlinearHits: [], matchedHit: null, matchKind: strongs.length ? "tagged" : "" } }
      : undefined;
}
function searchWord(word) {
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "diccionario", modulesByKind: SAVED } } });
  $id("dict-query").value = word;
  clickEl($id("btn-dict-search"));
  settle();
}
const lookups = (q) => H.calls.filter((c) => c.cmd === "lookup_dictionary" && c.args.query === q);
const content = () => $id("study-content");

scenario("«amó» (G25): the view sends strongs + lexPaths and labels AMOR «relacionado · G25»", () => {
  resolveAs("amó", ["G25"]);
  H.overrides.lookup_dictionary = (a) => (a.query === "amó" ? { ok: a.strongs ? [AMOR] : [] } : undefined);
  searchWord("amó");
  const call = lookups("amó").pop();
  assert(call, "lookup_dictionary called for «amó»");
  eq(call.args.strongs, ["G25"], "resolved Strong sent");
  assert(Array.isArray(call.args.lexPaths) && call.args.lexPaths.length > 0, "lexicons sent");
  const topic = content().querySelector("article.study-entry .topic");
  assert(topic, "AMOR article shown");
  includes(topic.textContent, "AMOR", "topic");
  includes(topic.textContent, "relacionado · G25", "labelled as related, with the code");
  excludes(studyText(), "Sin resultados", "the concept article answers the tap");
}, { allowMissingFixtures: true });

scenario("A word with no Strong keeps the legacy call (no strongs / lexPaths keys)", () => {
  resolveAs("Nicodemo", []);
  H.overrides.lookup_dictionary = (a) => (a.query === "Nicodemo" ? { ok: [] } : undefined);
  searchWord("Nicodemo");
  const call = lookups("Nicodemo").pop();
  assert(call, "lookup_dictionary called");
  eq(call.args.strongs, undefined, "no strongs key");
  eq(call.args.lexPaths, undefined, "no lexPaths key");
}, { allowMissingFixtures: true });

scenario("An exact article is never labelled (only matchKind «strong» is)", () => {
  resolveAs("mundo", ["G2889"]);
  H.overrides.lookup_dictionary = (a) =>
    a.query === "mundo" ? { ok: [{ topic: "MUNDO", html: "<p>…</p>", plain: "…", matchKind: "exact" }] } : undefined;
  searchWord("mundo");
  const topic = content().querySelector("article.study-entry .topic");
  eq(topic.textContent.trim(), "MUNDO", "no label on an exact match");
}, { allowMissingFixtures: true });

scenario("Side pane: a bridged entry from another dictionary carries the same label", () => {
  const other = { modulePath: `${LIB_DIR}/03 Nuevo Diccionario Biblico Vila Escuain.dcti`, moduleAbbr: "Vila-Escuain", topic: "Amor", html: "<p>…</p>", plain: "Amor de Dios…", matchKind: "strong" };
  resolveAs("amó", ["G25"], [other]);
  H.overrides.lookup_dictionary = (a) => (a.query === "amó" ? { ok: [AMOR] } : undefined);
  searchWord("amó");
  const meta = $id("link-panel-inline").querySelector('article.mini-entry[data-kind="dict"] .mini-meta');
  assert(meta, "side pane lists the other dictionary's article");
  includes(meta.textContent, "Vila-Escuain · Amor", "module and topic");
  includes(meta.textContent, "relacionado · G25", "labelled");
}, { allowMissingFixtures: true });
