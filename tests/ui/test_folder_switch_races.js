// ISS-46 / ISS-09 — folder switch while replies from the OLD folder are still in flight.
// Nothing that belongs to the old module list (probe results, lookups, word links, the old
// folder's own module list) may land on screen or in caches after the user picked another folder.
//
// "Mirror" folders: the same recorded library served from another directory. Every command
// whose args mention the mirror dir is answered from the fixture recorded for LIB_DIR, with the
// paths in the reply rewritten to the mirror (so a reply's origin is visible in its paths).

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: CMH };
const F2 = "/tmp/espada-F2";
const F3 = "/tmp/espada-F3";
const MIRRORED = [
  "list_modules", "find_interlinear", "get_bible_chapter", "resolve_word", "probe_modules_content",
  "lookup_dictionary", "lookup_lexicon", "lookup_lexicon_multi", "get_commentary", "get_passage",
  "get_verse_count", "get_verse_plain", "list_cross_reference_sources", "get_cross_references", "refresh_modules",
];
const swapDir = (v, from, to) => JSON.parse(JSON.stringify(v).split(from).join(to));

const fullList = () => H.fixture.responses[fixtureKey("list_modules", { modulesDir: LIB_DIR })].ok;
const MARK = (dir) => `[[${dir}]]`;

/**
 * Serve `dir` as a copy of the recorded library, minus the modules whose LIB_DIR path is in
 * `drop`. Dictionary replies carry a visible marker MARK(dir) so the screen shows whose reply won.
 */
function mirrorFolder(dir, drop = []) {
  for (const cmd of MIRRORED) {
    const prev = H.overrides[cmd];
    H.overrides[cmd] = (a) => {
      if (!JSON.stringify(a || {}).includes(dir)) return prev ? prev(a) : undefined;
      let libArgs = swapDir(a, dir, LIB_DIR);
      if (cmd === "resolve_word" && drop.length) {
        // Recorded with the full lists: ask for those, keep only the modules this folder has.
        const of = (k) => fullList().filter((m) => m.kind === k && !m.encrypted).map((m) => m.path);
        libArgs = Object.assign({}, libArgs, { dictPaths: of("dictionary"), lexPaths: of("lexicon") });
      }
      if (cmd === "probe_modules_content" && drop.length && !H.fixture.responses[fixtureKey(cmd, libArgs)]) {
        return { ok: a.paths.map((p) => ({ path: p, has: false })) }; // not recorded for this subset
      }
      const saved = H.overrides[cmd];
      delete H.overrides[cmd];
      let r;
      try {
        r = answer(cmd, libArgs);
      } finally {
        H.overrides[cmd] = saved;
      }
      if (r.ok && cmd === "list_modules") r = { ok: r.ok.filter((m) => !drop.includes(m.path)) };
      if (r.ok && cmd === "resolve_word") {
        const keep = (e) => !drop.includes(e.modulePath);
        r = { ok: Object.assign({}, r.ok, { dictionary: (r.ok.dictionary || []).filter(keep), lexicon: (r.ok.lexicon || []).filter(keep) }) };
      }
      if (r.ok && cmd === "lookup_dictionary") r = { ok: r.ok.map((e) => Object.assign({}, e, { html: `${e.html} ${MARK(dir)}`, plain: `${e.plain || ""} ${MARK(dir)}` })) };
      return swapDir(r, LIB_DIR, dir);
    };
  }
}
function applyFolder(dir) {
  $id("modules-path").value = dir;
  clickEl($id("btn-apply-path"));
}
const fromLib = (p) => String(p || "").startsWith(LIB_DIR + "/");
const allPathsIn = (dir, v) => !JSON.stringify(v).includes(LIB_DIR) || dir === LIB_DIR;

scenario("A dictionary probe from the old folder still in flight → the new folder is probed; ✦ marks are the new folder's", () => {
  boot({ storage: { nav: NAV, study: { workspace: "diccionario", selectedWord: "amó", modulesByKind: SAVED } } });
  mirrorFolder(F2);
  H.delivery = "hold";
  $id("dict-query").value = "gracia";
  clickEl($id("btn-dict-search"));
  const oldProbe = (c) => c.cmd === "probe_modules_content" && c.args.kind === "dictionary" && fromLib(c.args.paths[0]);
  settleHolding(oldProbe);
  assert(H.pending.some(oldProbe), "old folder's dictionary probe in flight");
  const mark = H.calls.length;
  applyFolder(F2);
  settleHolding(oldProbe);
  settle({ order: "fifo" });
  eq(state.modulesDir, F2, "switched");
  const newProbes = callsSince(mark, "probe_modules_content").filter((c) => c.args.kind === "dictionary");
  assert(newProbes.length >= 1, "the new folder's dictionaries were probed");
  assert(newProbes.every((c) => c.args.paths.every((p) => p.startsWith(F2 + "/"))), "probe sent with the new paths");
  const opts = $id("sel-study-module").options;
  assert(opts.every((o) => o.value.startsWith(F2 + "/")), "dropdown lists the new folder only");
  assert(opts.some((o) => o.textContent.startsWith("✦")), "content marks present (from the new folder's probe)");
  for (const [k, m] of state.probeCache) assert(allPathsIn(F2, [...m.keys()]), `probe cache entry ${k} holds old-folder paths`);
});

scenario("The old folder's dictionary lookup arrives after the switch → not rendered; the new folder's reply is", () => {
  boot({ storage: { nav: NAV, study: { workspace: "diccionario", selectedWord: "amó", modulesByKind: SAVED } } });
  mirrorFolder(F2);
  H.delivery = "hold";
  $id("dict-query").value = "gracia";
  clickEl($id("btn-dict-search"));
  const oldLookup = (c) => c.cmd === "lookup_dictionary" && fromLib(c.args.modulePath);
  settleHolding(oldLookup);
  assert(H.pending.some(oldLookup), "old Nelson lookup in flight");
  applyFolder(F2);
  settleHolding(oldLookup);
  settle({ order: "reverse" });
  eq(state.studyModule, `${F2}/Nelson.dcti`, "the new folder's Nelson renders");
  includes(studyText(), MARK(F2), "the new folder's reply is on screen");
  for (const el of $id("study-content").querySelectorAll("[data-path]")) assert(!fromLib(el.dataset.path), `old path in the view: ${el.dataset.path}`);
});

scenario("The old folder's resolve_word arrives after the switch → word links hold only new-folder paths", () => {
  boot({ storage: { nav: NAV, study: { workspace: "biblia", modulesByKind: SAVED } } });
  mirrorFolder(F2);
  H.delivery = "hold";
  clickEl(wordEl("amó", 16));
  const oldResolve = (c) => c.cmd === "resolve_word" && fromLib((c.args.dictPaths || [])[0]);
  settleHolding(oldResolve);
  assert(H.pending.some(oldResolve), "old resolve in flight");
  applyFolder(F2);
  settleHolding(oldResolve);
  settle({ order: "reverse" });
  eq(state.modulesDir, F2, "switched");
  assert(state.wordLinks, "the word is resolved again in the new folder");
  assert(allPathsIn(F2, state.wordLinks), "no old-folder path in wordLinks");
  for (const el of $id("link-panel-inline").querySelectorAll("[data-path]")) assert(!fromLib(el.dataset.path), `old path in the side pane: ${el.dataset.path}`);
});

scenario("Two folders chosen quickly, the first folder's list arrives last → the last folder chosen wins", () => {
  boot({ storage: { nav: NAV, study: { workspace: "diccionario", selectedWord: "amó", modulesByKind: SAVED } } });
  mirrorFolder(F2);
  mirrorFolder(F3);
  H.delivery = "hold";
  applyFolder(F2);
  settleHolding((c) => c.cmd === "list_modules" && c.args.modulesDir === F2);
  applyFolder(F3);
  settleHolding((c) => c.cmd === "list_modules" && c.args.modulesDir === F2); // F3 loads completely first
  settle({ order: "fifo" }); // then F2's list finally answers
  eq(state.modulesDir, F3, "folder in use is the one chosen last");
  eq(localStorage.getItem(LS_KEYS.modulesDir), F3, "saved folder is the one chosen last");
  assert(state.modules.length && state.modules.every((m) => m.path.startsWith(F3 + "/")), "module list is F3's");
  assert(state.biblePath.startsWith(F3 + "/"), "Bible from F3");
  assert(state.studyModule.startsWith(F3 + "/"), "study module from F3");
});

scenario("«Actualizar lista de módulos» → cached probes are dropped; the next tab visit probes once", () => {
  boot({ storage: { nav: NAV, study: { workspace: "diccionario", selectedWord: "amó", modulesByKind: SAVED } } });
  H.overrides.refresh_modules = () => ({ ok: null }); // backend cache drop; reply unused
  clickEl(document.querySelector('.ws-tab[data-ws="lexico"]'));
  settle();
  clickEl(document.querySelector('.ws-tab[data-ws="diccionario"]'));
  settle();
  let mark = H.calls.length;
  clickEl(document.querySelector('.ws-tab[data-ws="lexico"]'));
  settle();
  eq(callsSince(mark, "probe_modules_content").length, 0, "control: before the refresh the probe is cached");
  clickEl($id("btn-refresh-modules"));
  settle();
  mark = H.calls.length;
  clickEl(document.querySelector('.ws-tab[data-ws="diccionario"]'));
  settle();
  eq(callsSince(mark, "probe_modules_content").length, 1, "re-probed after the refresh");
  eq(state.studyModule, NELSON, "saved choice kept (still in the list)");
});

scenario("Switch to a folder with no dictionaries while on Diccionario → Spanish empty state, no lookup with an old path", () => {
  boot({ storage: { nav: NAV, study: { workspace: "diccionario", selectedWord: "amó", modulesByKind: SAVED } } });
  const dicts = state.modules.filter((m) => m.kind === "dictionary").map((m) => m.path);
  mirrorFolder(F2, dicts);
  const mark = H.calls.length;
  applyFolder(F2);
  settle();
  eq(state.studyModule, "", "no dictionary to use");
  includes(studyText(), "No hay módulos de este tipo en la carpeta.", "Spanish empty state");
  eq(callsSince(mark, "lookup_dictionary").length, 0, "no dictionary lookup at all");
  for (const c of callsSince(mark)) assert(!JSON.stringify(c.args || {}).includes(LIB_DIR), `${c.cmd} sent an old-folder path`);
});
