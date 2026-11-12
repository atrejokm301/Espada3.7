// User decision 3 — cross-references default to TSKe (not NASB's chapter-wide list), the Spanish
// TCB is offered, and only an explicit choice is remembered.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const TSKE = `${LIB_DIR}/tske_v12.cmti`;
const NASB = `${LIB_DIR}/nasb.cmti`;
const TCB = `${LIB_DIR}/02TCB.cmti`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
function openCrPanel() {
  const mark = H.calls.length;
  clickEl($id("btn-crossrefs"));
  settle();
  return mark;
}
const options = () => $id("sel-crossref-source").options.map((o) => o.value);

scenario("Nothing saved → TSKe is selected and asked; the default is not stored as a choice", () => {
  boot({ storage: { nav: NAV } });
  const mark = openCrPanel();
  eq($id("sel-crossref-source").value, TSKE, "TSKe selected");
  const cr = callsSince(mark, "get_cross_references");
  eq(cr.length, 1, "one request");
  eq(cr[0].args.sourcePath, TSKE, "TSKe requested (not NASB)");
  assert($id("crossrefs-list").querySelectorAll(".crossref-row").length > 0, "references listed");
  eq((storedStudy() || {}).crossRefChosen || false, false, "not remembered as the user's choice");
});

scenario("An older version's automatic NASB pick (no 'chosen' flag) → TSKe now", () => {
  boot({ storage: { nav: NAV, study: { crossRefSourcePath: NASB } } });
  openCrPanel();
  eq($id("sel-crossref-source").value, TSKE, "TSKe");
});

scenario("The user picks NASB → remembered and used after a relaunch", () => {
  boot({ storage: { nav: NAV } });
  openCrPanel();
  changeSelect($id("sel-crossref-source"), NASB);
  settle();
  const saved = storedStudy();
  eq([saved.crossRefSourcePath, saved.crossRefChosen], [NASB, true], "explicit choice saved");
});

scenario("Relaunch with an explicit NASB choice → NASB is used", () => {
  boot({ storage: { nav: NAV, study: { crossRefSourcePath: NASB, crossRefChosen: true } } });
  const mark = openCrPanel();
  eq(callsSince(mark, "get_cross_references")[0].args.sourcePath, NASB, "NASB kept after relaunch");
  eq($id("sel-crossref-source").value, NASB, "dropdown");
});

scenario("TCB (Spanish) is offered even when the backend does not list it; it shows the verse's rows", () => {
  boot({ storage: { nav: NAV } });
  openCrPanel();
  const listed = H.fixture.responses[fixtureKey("list_cross_reference_sources", { modulesDir: LIB_DIR })];
  assert(listed && listed.ok && !listed.ok.some((s) => s.path === TCB), "this backend does not list TCB");
  assert(options().includes(TCB), "TCB offered in the dropdown");
  const mark = H.calls.length;
  changeSelect($id("sel-crossref-source"), TCB);
  settle();
  eq(callsSince(mark, "get_cross_references").length, 0, "not sent to a backend that would fall back to another source");
  const gc = callsSince(mark, "get_commentary");
  eq(gc.map((c) => [c.args.modulePath, c.args.verse]), [[TCB, 16]], "TCB read for Jn 3:16");
  const text = $id("crossrefs-list").textContent;
  assert($id("crossrefs-list").querySelectorAll(".bible-ref").length > 0, "references are links");
  excludes(text, "Jesús enseña a Nicodemo", "not the chapter summary row");
  includes($id("crossrefs-hint").textContent, "TCB", "source named");
});

scenario("Clicking a TCB reference navigates there", () => {
  boot({ storage: { nav: NAV } });
  openCrPanel();
  changeSelect($id("sel-crossref-source"), TCB);
  settle();
  const ref = $id("crossrefs-list").querySelector(".bible-ref");
  assert(ref, "a reference");
  clickEl(ref);
  settle();
  assert($id("panel-crossrefs").classList.contains("hidden"), "panel closed");
  assert(!(state.book === 43 && state.chapter === 3 && state.verse === 16), `moved to the reference (${state.book}:${state.chapter}:${state.verse})`);
});

scenario("A newer backend that lists TCB → no duplicate option, TCB read through get_cross_references", () => {
  boot({ storage: { nav: NAV } });
  const base = H.fixture.responses[fixtureKey("list_cross_reference_sources", { modulesDir: LIB_DIR })].ok;
  H.overrides.list_cross_reference_sources = () => ({ ok: base.concat([{ path: TCB, title: "TCB — El Tesoro del Conocimiento Biblico", kind: "commentary", encrypted: false }]) });
  H.overrides.get_cross_references = (a) => (a.sourcePath === TCB ? { ok: { source: "TCB", sourcePath: TCB, sourceKind: "commentary", fromLabel: "Jn 3:16", entries: [], note: "Sin referencias." } } : undefined);
  openCrPanel();
  eq(options().filter((p) => p === TCB).length, 1, "TCB listed once");
  const mark = H.calls.length;
  changeSelect($id("sel-crossref-source"), TCB);
  settle();
  eq(callsSince(mark, "get_cross_references").map((c) => c.args.sourcePath), [TCB], "backend path used");
});
