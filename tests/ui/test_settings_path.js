// ISS-31 — no hard-coded '/Users/amed301' path; «Usar «for mac espada»» asks the backend; the
// saved folder is written only after a successful scan, and an init fallback never replaces it.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const GONE = "/Volumes/Desconectado/modulos";

scenario("No 'amed301' anywhere in the UI files", () => {
  for (const f of ["app.v7.js", "index.html", "themes.js"]) {
    let txt = "";
    try {
      txt = readFile(`${UI_DIR}/${f}`);
    } catch {
      continue;
    }
    excludes(txt, "amed301", f);
  }
});

scenario("«Usar «for mac espada»» asks get_default_modules_dir and loads that folder", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  const mark = H.calls.length;
  clickEl($id("btn-use-default-espada"));
  settle();
  eq(callsSince(mark).map((c) => c.cmd).slice(0, 2), ["get_default_modules_dir", "set_modules_path"], "backend default, then set");
  eq(callsSince(mark, "set_modules_path")[0].args.path, LIB_DIR, "the found folder");
  eq(localStorage.getItem(LS_KEYS.modulesDir), LIB_DIR, "saved after a successful scan");
});

scenario("No default folder → a Spanish hint, nothing changes", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  H.overrides.get_default_modules_dir = () => ({ ok: null });
  const before = localStorage.getItem(LS_KEYS.modulesDir);
  const mark = H.calls.length;
  clickEl($id("btn-use-default-espada"));
  settle();
  eq(callsSince(mark, "set_modules_path").length, 0, "no folder applied");
  includes($id("modules-meta").textContent, "No se encontró «for mac espada»", "hint");
  eq(localStorage.getItem(LS_KEYS.modulesDir), before, "saved folder untouched");
});

scenario("Saved folder missing at launch + valid default → 752 modules load; the saved path is kept", () => {
  boot({
    storage: { modulesDir: GONE, nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } },
  });
  eq(state.modules.length, 752, "default folder loaded for the session");
  eq(state.modulesDir, LIB_DIR, "session folder");
  eq(localStorage.getItem(LS_KEYS.modulesDir), GONE, "saved folder NOT overwritten (remount brings it back)");
}, { allowMissingFixtures: false });

scenario("Saved folder missing and no default → Ajustes opens with an explanation", () => {
  H.overrides.get_default_modules_dir = () => ({ ok: null });
  boot({ storage: { modulesDir: GONE } });
  assert(!$id("panel-settings").classList.contains("hidden"), "Ajustes open");
  includes($id("modules-meta").textContent, "No se encontró la carpeta de módulos guardada", "explains");
  eq(localStorage.getItem(LS_KEYS.modulesDir), GONE, "saved folder kept");
});

scenario("A failed list_modules leaves the saved folder and the loaded modules unchanged", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  const n = state.modules.length;
  const saved = localStorage.getItem(LS_KEYS.modulesDir);
  H.overrides.list_modules = (a) => (a.modulesDir === "/tmp/roto" ? { err: "No es una carpeta válida" } : undefined);
  $id("modules-path").value = "/tmp/roto";
  clickEl($id("btn-apply-path"));
  settle();
  eq(localStorage.getItem(LS_KEYS.modulesDir), saved, "LS unchanged");
  eq(state.modules.length, n, "previous modules still usable");
  eq(state.modulesDir, LIB_DIR, "current folder unchanged");
});
