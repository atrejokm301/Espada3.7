// ISS-15 — 'Otros comentarios': built from the modules the probe says have a note (not the
// first 10 alphabetically), after the main commentary, without the primary or copies of it,
// Spanish/preferred first, «Mostrar todos (N)», and refreshed by the filter / module handlers.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const CMH_TWIN = `${LIB_DIR}/cmh.cmti`; // byte-identical copy under another file name
const BCS = `${LIB_DIR}/Biblia Comentada.cmti`;
const CLARKE = `${LIB_DIR}/clarke.cmti`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const onComentario = (primary, nav = NAV) => ({ nav, study: { workspace: "comentario", modulesByKind: { comentario: primary } } });
const paneCards = () => $id("link-panel-inline").querySelectorAll(".mini-entry");
const panePaths = () => paneCards().map((c) => c.dataset.path);
const paneText = () => $id("link-panel-inline").textContent.replace(/\s+/g, " ").trim();
const cmtCalls = (mark) => callsSince(mark, "get_commentary");
const probeAt = (verse, filters) =>
  H.fixture.responses[fixtureKey("probe_modules_content", {
    kind: "commentary",
    paths: studyCandidates("comentario").map((m) => m.path),
    book: 43, chapter: 3, verse, query: "", filters,
  })];

scenario("The chosen commentary is requested before any 'Otros' commentary", () => {
  boot({ storage: onComentario(CMH) });
  const calls = H.calls.filter((c) => c.cmd === "get_commentary");
  assert(calls.length > 1, "main + side commentaries requested");
  eq(calls[0].args.modulePath, CMH, "first get_commentary is the main commentary");
  includes(studyText(), "Comentario · Juan 3:16", "main commentary rendered");
});

scenario("The primary (and its byte-identical twin) never appear; no two cards are the same", () => {
  boot({ storage: onComentario(CMH) });
  const paths = panePaths();
  assert(paths.length >= 3, `several other commentaries listed (got ${paths.length})`);
  assert(!paths.includes(CMH), "primary listed in 'Otros comentarios'");
  assert(!paths.includes(CMH_TWIN), "the primary's identical copy cmh.cmti is listed");
  const bodies = paneCards().map((c) => c.querySelector(".mini-body").textContent);
  eq(new Set(bodies).size, bodies.length, "every card shows different text");
});

scenario("Candidates come from the probe (has=true), Spanish/preferred first, never only A–B", () => {
  boot({ storage: onComentario(CMH) });
  const probe = probeAt(16, state.filters);
  assert(probe && probe.ok, "probe fixture for Jn 3:16");
  const has = new Set(probe.ok.filter((r) => r.has).map((r) => r.path));
  for (const p of panePaths()) assert(has.has(p), `listed ${p} although the probe says it has no note`);
  const asked = H.calls.filter((c) => c.cmd === "get_commentary" && c.args.modulePath !== CMH).map((c) => c.args.modulePath);
  for (const p of asked) assert(has.has(p), `fetched ${p} which has no note here`);
  const abbrs = paneCards().map((c) => moduleByPath(c.dataset.path).abbreviation);
  assert(abbrs.some((a) => /MacArthur|BE-MH|Mundo Hispano|Plenitud/.test(a)), `preferred Spanish commentaries listed (${abbrs.join(", ")})`);
  assert(!/^(Adam Clarke|Biblical Illustrator|Expositor's Bible)$/.test(abbrs[0]), `English module first: ${abbrs[0]}`);
  assert(abbrs.some((a) => !/^[A-B]/.test(a)), "not only the alphabetically first modules");
});

scenario("«Mostrar todos (N)» lists every other commentary with a note", () => {
  boot({ storage: onComentario(CMH) });
  const btn = $id("link-panel-inline").querySelector(".cmt-show-all");
  assert(btn, "«Mostrar todos» offered when more than 10 have a note");
  const n = Number((btn.textContent.match(/\((\d+)\)/) || [])[1]);
  assert(n > 10, `N counts all candidates (${btn.textContent})`);
  const before = paneCards().length;
  clickEl(btn);
  settle();
  assert(paneCards().length > before, `more cards after «Mostrar todos» (${before} → ${paneCards().length})`);
  assert(!$id("link-panel-inline").querySelector(".cmt-show-all"), "button gone once everything is shown");
  eq(state.workspace, "comentario", "still on Comentario (the click is not a card click)");
  eq(state.studyModule, CMH, "primary unchanged");
});

scenario("A filter toggle re-runs the side pane with the new filters", () => {
  boot({ storage: onComentario(CMH) });
  const mark = H.calls.length;
  const chk = $id("flt-chapter");
  chk.checked = true;
  dispatch(chk, makeEvent("change"));
  settle();
  const side = cmtCalls(mark).filter((c) => c.args.modulePath !== CMH);
  assert(side.length >= 1, "side pane refetched after the filter change");
  for (const c of side) eq(c.args.filters, { verse: true, chapter: true, book: false }, "side pane uses the new filters");
  assert(!panePaths().includes(CMH), "primary still excluded");
});

scenario("Choosing another main commentary removes it from the pane and refreshes", () => {
  boot({ storage: onComentario(CMH) });
  const target = panePaths()[0];
  assert(target, "a side commentary to pick");
  changeSelect($id("sel-study-module"), target);
  settle();
  eq(state.studyModule, target, "new primary");
  assert(!panePaths().includes(target), "the new primary is not listed in 'Otros'");
});

scenario("BCS primary at Jos 2:17 still finds the other commentaries (not «Ningún otro…»)", () => {
  boot({ storage: onComentario(BCS, { book: 6, chapter: 2, verse: 17, biblePath: RVR }) });
  eq(state.studyModule, BCS, "BCS is the primary");
  excludes(paneText(), "Ningún otro comentario", "false empty message");
  assert(paneCards().length >= 1, "other commentaries listed");
});

scenario("Edge: no other commentary has a note → the empty message", () => {
  boot({ storage: onComentario(CLARKE) });
  H.overrides.get_commentary = (a) => (a.modulePath === CLARKE ? undefined : { ok: [] });
  clickEl($id("btn-verse-pick"));
  settle();
  clickEl($id("verse-grid").querySelector('button[data-v="17"]'));
  settle();
  includes(paneText(), "Ningún otro comentario tiene notas para Jn 3:17", "empty message");
  eq(paneCards().length, 0, "no cards");
});

scenario("Edge: an unreadable side module is skipped quietly (no error card)", () => {
  boot({ storage: onComentario(CMH) });
  const first = panePaths()[0];
  H.overrides.get_commentary = (a) => (a.modulePath === first ? { err: "no such table: VerseCommentary" } : undefined);
  const chk = $id("flt-chapter");
  chk.checked = true;
  dispatch(chk, makeEvent("change"));
  settle();
  assert(!panePaths().includes(first), "failing module not listed");
  excludes(paneText(), "no such table", "raw error not shown in the side pane");
  assert(paneCards().length >= 1, "the others still render");
});
