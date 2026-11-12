// ISS-04 (UI half): Léxico shows full definitions only for the current Strong and the matched
// interlinear unit's codes; every other code becomes a «Relacionados» chip; invalid codes
// (H0, G0, H107119) are never shown. Baseline: G25 rendered 4 definition blocks
// (G25 amar + G250 áloe + G251 sal + G252 salado); «Hijo» listed H0 H107119 H2 H102115.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: "" };
const onLexico = (study) => ({ nav: NAV, study: Object.assign({ workspace: "lexico", modulesByKind: SAVED }, study) });
// Scenarios about the HEAD backend's junk codes are pinned to the HEAD fixture; the 'wp3'
// snapshot (in-progress WP2+WP3 worktree backend) covers the newer reply shapes.
const head = (study) => Object.assign({ fixture: "head" }, { storage: onLexico(study) });

const topics = () => $id("study-content").querySelectorAll(".study-entry .topic").map((t) => t.textContent);
const chips = (sel) => $id("study-content").querySelectorAll(`.sync-bar ${sel}`).map((c) => c.dataset.strong);

scenario("G25 (strongs [G25,G250,G251,G252], matched G25) → one Definición block, 3 related chips", () => {
  boot(head({ selectedWord: "G25", lastStrong: "G25" }));
  eq(state.wordLinks.strongs, ["G25", "G250", "G251", "G252"], "fixture: the HEAD backend's neighbour codes");
  eq(topics(), ["G25"], "only G25 is defined");
  eq(
    $id("study-content").querySelectorAll(".sync-bar .chip-strong").filter((c) => !c.classList.contains("chip-secondary")).map((c) => c.dataset.strong),
    ["G25"],
    "main chip"
  );
  eq(chips(".chip-secondary"), ["G250", "G251", "G252"], "related chips");
  includes(studyText(), "Relacionados:", "related label");
  const looked = H.calls.filter((c) => c.cmd === "lookup_lexicon" && c.args.modulePath === STRONGES).map((c) => c.args.strong);
  eq([...new Set(looked)], ["G25"], "no lookups for neighbour codes");
});

scenario("«Hijo» Jn 3:16 (old backend): G3588 + G5207 defined; H0/H107119/H102115 never shown", () => {
  boot(head({}));
  clickEl(wordEl("Hijo", 16));
  settle();
  setWorkspace("lexico");
  settle();
  eq(topics(), ["G3588", "G5207"], "the matched unit 'τὸν υἱὸν' = G3588 + G5207");
  const all = $id("study-content").textContent + $id("ws-ctx-strongs").textContent + $id("link-panel-inline").textContent;
  for (const junk of ["H0", "H107119", "H102115"]) {
    assert(!new RegExp(`\\b${junk}\\b`).test(all), `${junk} is never displayed`);
  }
  const rel = chips(".chip-secondary");
  assert(rel.includes("H2"), "a valid unrelated code (H2) is only a related chip");
  assert(rel.every((c) => strongCode(c) === c && !["G3588", "G5207"].includes(c)), `related chips are valid, non-primary codes: ${rel}`);
});

scenario("Clicking a related chip makes it the current Strong (its definition alone)", () => {
  boot(head({ selectedWord: "G25", lastStrong: "G25" }));
  const chip = $id("study-content").querySelector('.sync-bar .chip-secondary[data-strong="G251"]');
  assert(chip, "G251 related chip");
  clickEl(chip);
  settle();
  eq(state.lastStrong, "G251", "current Strong");
  eq(topics()[0], "G251", "G251 defined first");
  assert(!topics().includes("G25"), "G25 is now only related");
});

scenario("New backend shape: related codes come from `related`, never defined", () => {
  boot({ storage: onLexico({ selectedWord: "amó", lastStrong: "G25" }) });
  const base = H.calls.filter((c) => c.cmd === "resolve_word").pop().result.ok;
  H.overrides.resolve_word = (a) =>
    a.word === "G25"
      ? { ok: Object.assign({}, base, { word: "G25", strongs: ["G25"], related: ["G250", "Amor (G26)"], matchKind: "code", lemmaIndexUsed: false }) }
      : undefined;
  $id("dict-query").value = "G25";
  clickEl($id("btn-dict-search"));
  settle();
  eq(topics(), ["G25"], "one definition");
  eq(chips(".chip-secondary"), ["G250"], "valid related code as chip; prose topic ignored");
});

scenario("Context bar and side pane: main codes first, related ones marked secondary", () => {
  boot(head({ selectedWord: "G25", lastStrong: "G25" }));
  const bar = $id("ws-ctx-strongs").querySelectorAll(".chip-strong");
  eq(bar.map((c) => c.dataset.strong), ["G25", "G250", "G251", "G252"], "context bar order");
  eq(bar.map((c) => c.classList.contains("chip-secondary")), [false, true, true, true], "secondary styling");
});

scenario("Keep working: a Spanish word without codes still looks itself up (Vine-style lemma path)", () => {
  boot({ storage: onLexico({ selectedWord: "Hijo" }) });
  H.overrides.resolve_word = (a) => ({ ok: { word: a.word, normalized: "hijo", strongs: [], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: false, interlinearHits: [], matchedHit: null } });
  const mark = H.calls.length;
  $id("dict-query").value = "Hijo";
  clickEl($id("btn-dict-search"));
  settle();
  assert(callsSince(mark, "lookup_lexicon").some((c) => c.args.strong === "Hijo"), "the word itself is looked up");
  excludes(studyText(), "Sin número Strong en el interlineal", "no interlinear → not the interlinear message");
});

scenario("Newer backend snapshot (wp3): «Hijo» → G5207 defined, the article G3588 only related", () => {
  boot({ fixture: "wp3", storage: onLexico({}) });
  clickEl(wordEl("Hijo", 16));
  settle();
  eq(state.wordLinks.strongs, ["G5207", "G3588"], "fixture: content code first");
  setWorkspace("lexico");
  settle();
  eq(topics(), ["G5207"], "one definition");
  eq(chips(".chip-secondary"), ["G3588"], "article as related chip");
});

scenario("Newer backend snapshot (wp3): G25 → one definition and no related chips", () => {
  boot({ fixture: "wp3", storage: onLexico({ selectedWord: "G25", lastStrong: "G25" }) });
  eq(state.wordLinks.strongs, ["G25"], "fixture: no neighbours");
  eq(topics(), ["G25"], "one definition");
  eq(chips(".chip-secondary"), [], "no related chips");
  excludes(studyText(), "Relacionados", "no related label");
});
