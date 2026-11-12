// ISS-46 — after a folder switch the selected word is resolved again (once) against the NEW
// module list; choices that are not in the new list are dropped; nothing points at the old list.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const IRV = `${LIB_DIR}/00Interlineal-iRV 1960+.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const VILA = `${LIB_DIR}/03 Nuevo Diccionario Biblico Vila Escuain.dcti`;
const F1 = "/tmp/espada-F1";
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
function switchTo(dir, keep) {
  const all = state.modules;
  H.overrides.list_modules = (a) => (a.modulesDir === dir ? { ok: all.filter((m) => keep.includes(m.path)) } : undefined);
  H.overrides.find_interlinear = (a) => (a.modulesDir === dir ? { ok: keep.includes(IRV) ? IRV : null } : undefined);
  H.overrides.resolve_word = (a) => ({
    ok: {
      word: a.word, normalized: a.word, strongs: ["G25"], hebrew: [], greek: [], interlinearUsed: true, interlinearHits: [], matchedHit: null,
      dictionary: a.dictPaths.map((p) => ({ modulePath: p, moduleAbbr: "D", topic: "AMOR", html: "amor", plain: "amor" })),
      lexicon: a.lexPaths.map((p) => ({ modulePath: p, moduleAbbr: "L", topic: "G25", html: "agapao", plain: "agapao" })),
    },
  });
  const mark = H.calls.length;
  $id("modules-path").value = dir;
  clickEl($id("btn-apply-path"));
  settle();
  return mark;
}

scenario("Switch with a selected word → exactly one resolve_word, all its paths in the new list", () => {
  boot({ storage: { nav: NAV, study: { workspace: "biblia", selectedWord: "amó", wordOrigin: { book: 43, chapter: 3, verse: 16, biblePath: RVR }, modulesByKind: { diccionario: VILA, lexico: STRONGES } } } });
  assert(state.wordLinks, "word resolved in the first folder");
  const keep = [RVR, IRV, NELSON, STRONGES, CMH];
  const mark = switchTo(F1, keep);
  eq(state.modulesDir, F1, "switched");
  const rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, "one re-resolve");
  for (const p of rw[0].args.dictPaths.concat(rw[0].args.lexPaths)) assert(keep.includes(p), `old-folder path sent: ${p}`);
  eq(rw[0].args.interlinearPath, IRV, "new folder's interlinear");
  const wl = state.wordLinks;
  for (const e of (wl.dictionary || []).concat(wl.lexicon || [])) assert(keep.includes(e.modulePath), `stale path in wordLinks: ${e.modulePath}`);
  eq(state.modulesByKind.diccionario, "", "a choice outside the new list is dropped");
  eq(state.modulesByKind.lexico, STRONGES, "a choice still in the list is kept");
  for (const card of $id("link-panel-inline").querySelectorAll(".mini-entry")) assert(keep.includes(card.dataset.path), `side card from the old folder: ${card.dataset.path}`);
});

scenario("Switch without a selected word → no resolve_word", () => {
  boot({ storage: { nav: NAV, study: { workspace: "biblia" } } });
  const mark = switchTo(F1, [RVR, NELSON]);
  eq(callsSince(mark, "resolve_word").length, 0, "nothing to resolve");
  eq(state.wordLinks, null, "no stale links");
  eq(state.lastStrong, "", "no stale Strong");
});

scenario("Edge: the word's Bible is not in the new folder → re-resolved without that biblePath", () => {
  const LBLA = `${LIB_DIR}/00LBLA-NT+.bbli`;
  boot({ storage: { nav: NAV, study: { workspace: "biblia", selectedWord: "amó", wordOrigin: { book: 43, chapter: 3, verse: 16, biblePath: LBLA } } } });
  const mark = switchTo(F1, [RVR, IRV, NELSON]);
  const rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, "one re-resolve");
  assert(rw[0].args.biblePath !== LBLA, "old folder's Bible not sent");
});
