// Devocional tab (2026-11-18, ported from the Windows build): e-Sword devotionals (.devi) with one
// reading per calendar day. Opens on today's date; ‹ › and ← → step a day (across months and the
// year's end, with 29 de febrero); «Hoy» comes back; the right pane lists the folder's other
// devotionals for the same day. The library on this Mac has none, so the modules are faked.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const MANANTIALES = `${LIB_DIR}/manantiales.devi`;
const SPURGEON = `${LIB_DIR}/spurgeon.devi`;
const MONTHS = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];
const mod = (path, abbreviation, title, kind) => ({
  path, filename: path.split("/").pop(), title, abbreviation, kind, encrypted: false, hasStrongs: false, version: 1,
});
const BIBLE = mod(RVR, "RVR1960", "Reina Valera 1960", "bible");
const DEVS = [mod(MANANTIALES, "MED", "Manantiales en el desierto", "devotional"), mod(SPURGEON, "SPU", "Lecturas matutinas", "devotional")];

function setupDev({ devs = DEVS, readings = (a) => ({ html: `<p>Lectura de ${a.modulePath.split("/").pop()} para ${a.day}/${a.month}. Ver <b>Jn 3:16</b></p>` }) } = {}) {
  H.overrides.list_modules = () => ({ ok: [BIBLE, ...devs] });
  H.overrides.find_interlinear = () => ({ ok: null });
  H.overrides.get_devotional = (a) => {
    const r = readings(a);
    return r && r.err ? { err: r.err } : { ok: r ? { month: a.month, day: a.day, html: r.html, title: "", abbreviation: "" } : null };
  };
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "biblia", modulesByKind: { devocional: MANANTIALES } } } });
  clickEl(document.querySelector('.ws-tab[data-ws="devocional"]'));
  settle();
}
const devCalls = (mark = 0) => H.calls.slice(mark).filter((c) => c.cmd === "get_devotional");
const label = () => $id("dev-stepper-label").textContent;
const today = () => {
  const d = new Date();
  return { month: d.getMonth() + 1, day: d.getDate() };
};
const labelOf = (d) => `${d.day} de ${MONTHS[d.month - 1]}`;

scenario("The tab opens on today's reading of the chosen devotional", () => {
  setupDev();
  const t = today();
  const c = devCalls().pop();
  eq([c.args.modulePath, c.args.month, c.args.day], [MANANTIALES, t.month, t.day], "today's reading");
  eq(label(), labelOf(t), "stepper shows the date");
  assert(!$id("dev-stepper").classList.contains("hidden"), "day stepper visible");
  assert($id("verse-stepper").classList.contains("hidden"), "no verse stepper");
  assert($id("dict-search-row").classList.contains("hidden"), "no search box");
  assert($id("dict-index").classList.contains("hidden"), "no index");
  includes($id("study-content").querySelector(".study-banner").textContent, `Devocional · MED · ${labelOf(t)}`, "banner");
  includes(studyText(), "Lectura de manantiales.devi", "the reading");
  assert($id("study-content").querySelector(".bible-ref"), "references in the reading are links");
  assert($id("btn-dev-today").disabled, "«Hoy» disabled on today");
}, { allowMissingFixtures: true });

scenario("‹ › step a day across months, the year's end and 29 de febrero", () => {
  setupDev();
  const go = (d) => setDevotionalDate(d) || settle();
  go({ month: 1, day: 1 });
  clickEl($id("btn-dev-prev"));
  settle();
  eq(label(), "31 de diciembre", "back over the year's end");
  clickEl($id("btn-dev-next"));
  settle();
  eq(label(), "1 de enero", "forward over the year's end");
  go({ month: 2, day: 28 });
  clickEl($id("btn-dev-next"));
  settle();
  eq(label(), "29 de febrero", "devotionals carry the 29th");
  clickEl($id("btn-dev-next"));
  settle();
  eq(label(), "1 de marzo", "then March");
  const c = devCalls().pop();
  eq([c.args.month, c.args.day], [3, 1], "the reading asked for that day");
  assert(!$id("btn-dev-today").disabled, "«Hoy» enabled away from today");
  clickEl($id("btn-dev-today"));
  settle();
  eq(label(), labelOf(today()), "«Hoy» comes back");
}, { allowMissingFixtures: true });

scenario("← → step days in Devocional (not while typing)", () => {
  setupDev();
  setDevotionalDate({ month: 6, day: 15 });
  settle();
  dispatch(document, makeEvent("keydown", { key: "ArrowRight" }));
  settle();
  eq(label(), "16 de junio", "→ next day");
  dispatch(document, makeEvent("keydown", { key: "ArrowLeft" }));
  dispatch(document, makeEvent("keydown", { key: "ArrowLeft" }));
  settle();
  eq(label(), "14 de junio", "← twice");
}, { allowMissingFixtures: true });

scenario("A day without a reading says so; a failed read is a Spanish error", () => {
  setupDev({ readings: (a) => (a.day === 25 && a.month === 12 ? { err: "database disk image is malformed" } : a.day === 1 ? null : { html: "<p>x</p>" }) });
  setDevotionalDate({ month: 7, day: 1 });
  settle();
  includes(studyText(), "no tiene lectura para el 1 de julio", "no reading");
  setDevotionalDate({ month: 12, day: 25 });
  settle();
  assert($id("study-content").querySelector(".error-state, .empty-state"), "an error block");
  excludes(studyText(), "Lectura de", "no stale reading");
}, { allowMissingFixtures: true });

scenario("The right pane lists the other devotionals; a click makes one primary, same day", () => {
  setupDev();
  setDevotionalDate({ month: 4, day: 10 });
  settle();
  eq($id("analysis-title").textContent, "Otros devocionales", "pane title");
  const others = $id("link-panel-inline").querySelectorAll(".dev-other");
  eq(others.map((b) => b.dataset.path), [SPURGEON], "every other devotional, not the primary");
  includes($id("link-panel-inline").textContent, "Mismo día · 10 de abril", "same day");
  const mark = H.calls.length;
  clickEl(others[0]);
  settle();
  const c = devCalls(mark).pop();
  eq([c.args.modulePath, c.args.month, c.args.day], [SPURGEON, 4, 10], "the other devotional, same day");
  eq(storedStudy().modulesByKind.devocional, SPURGEON, "remembered as the choice");
  includes(studyText(), "Lectura de spurgeon.devi", "its reading");
}, { allowMissingFixtures: true });

scenario("Without devotionals in the folder the tab explains how to add them", () => {
  setupDev({ devs: [] });
  eq(devCalls().length, 0, "nothing to read");
  includes(studyText(), "No hay devocionales en la carpeta de módulos", "empty state");
  includes(studyText(), ".devi", "names the file type");
  includes($id("link-panel-inline").textContent, "Cuando agregue devocionales", "pane explains too");
}, { allowMissingFixtures: true });

scenario("The tab is saved when chosen", () => {
  setupDev();
  eq(storedStudy().workspace, "devocional", "saved");
}, { allowMissingFixtures: true });

scenario("A saved Devocional tab opens again on launch (on today's date)", () => {
  H.overrides.list_modules = () => ({ ok: [BIBLE, ...DEVS] });
  H.overrides.find_interlinear = () => ({ ok: null });
  H.overrides.get_devotional = (a) => ({ ok: { month: a.month, day: a.day, html: "<p>Hoy</p>", title: "", abbreviation: "" } });
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "devocional", modulesByKind: { devocional: SPURGEON } } } });
  eq(state.workspace, "devocional", "restored");
  const c = devCalls().pop();
  eq([c.args.modulePath, c.args.month, c.args.day], [SPURGEON, today().month, today().day], "its saved devotional, today");
}, { allowMissingFixtures: true });
