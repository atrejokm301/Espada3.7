// ISS-27 — commentary headers are Spanish only: «V.16», «CAP. 3», «LIBRO» (no «· VERSE»).

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const ALL = { verse: true, chapter: true, book: true };

scenario("Verse, chapter and book headers carry no English scope word", () => {
  boot({ storage: { nav: NAV, study: { workspace: "comentario", filters: ALL, modulesByKind: { comentario: CMH } } } });
  const key = fixtureKey("get_commentary", { modulePath: CMH, book: 43, chapter: 3, verse: 16, filters: ALL });
  const rec = H.fixture.responses[key];
  assert(rec && rec.ok && rec.ok.length, "fixture with entries");
  const scopes = new Set(rec.ok.map((e) => e.scope));
  assert(scopes.size >= 2, `several scopes in the fixture (${[...scopes].join(", ")})`);
  const labels = $id("study-content").querySelectorAll(".scope-label").map((e) => e.textContent.trim());
  eq(labels, rec.ok.map((e) => e.label), "header = the module's own Spanish label");
  for (const l of labels) assert(!/\b(verse|chapter|book)\b/i.test(l), `English scope word in «${l}»`);
});

scenario("Edge: an entry without a label falls back to a Spanish scope name", () => {
  boot({ storage: { nav: NAV, study: { workspace: "comentario", modulesByKind: { comentario: CMH } } } });
  H.overrides.get_commentary = (a) => (a.modulePath === CMH ? { ok: [{ scope: "chapter", label: "", html: "Nota", plain: "Nota" }] } : undefined);
  clickEl($id("btn-verse-pick"));
  settle();
  clickEl($id("verse-grid").querySelector('button[data-v="17"]'));
  settle();
  eq($id("study-content").querySelectorAll(".scope-label").map((e) => e.textContent.trim()), ["Capítulo"], "Spanish fallback label");
});
