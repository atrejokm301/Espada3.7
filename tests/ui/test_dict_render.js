// ISS-08 (UI half) + user decision (USER-ANSWERS.md): Diccionario shows exact, headword and
// lemma (singular) matches only. Verb-form expansions (cree → CREER, repudia → REPUDIAR) and
// multi-word 'contains' hits are dropped — never listed, no «Temas relacionados» section — and
// with nothing left it says «Sin resultados en …». A missing matchKind (older backend) = exact.
// Fixture: the real Nelson reply for «fe» (FE + 39 prefix/substring junk at HEAD), re-labelled
// with the match kinds the new backend sends, plus real worktree-backend replies for «cree».

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: "" };

function recorded(cmd, args) {
  const r = H.fixture.responses[fixtureKey(cmd, args)];
  if (!r) H.missing.push(fixtureKey(cmd, args));
  return r ? JSON.parse(JSON.stringify(r.ok)) : [];
}
/** Serve Nelson's real entries for «fe», labelled by `label(entry, index)`. */
function labelledFe(label) {
  H.overrides.lookup_dictionary = (a) => {
    if (a.query !== "fe") return undefined;
    return { ok: recorded("lookup_dictionary", a).map((e, i) => Object.assign(e, label(e, i))) };
  };
}
function searchFe() {
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "diccionario", modulesByKind: SAVED } } });
  $id("dict-query").value = "fe";
  clickEl($id("btn-dict-search"));
  settle();
}
const content = () => $id("study-content");
const fullTopics = () => content().querySelectorAll("article.study-entry .topic").map((t) => t.textContent);

scenario("«fe»: exact FE in full; the 39 'contains' hits are dropped, no «Temas relacionados»", () => {
  labelledFe((e, i) => ({ matchKind: i === 0 ? "exact" : "contains" }));
  searchFe();
  const reply = H.calls.filter((c) => c.cmd === "lookup_dictionary" && c.args.query === "fe").pop().result.ok;
  assert(reply.length >= 10, `fixture: Nelson returns many entries for «fe» (${reply.length})`);
  eq(fullTopics(), [reply[0].topic], "only the exact article");
  excludes(studyText(), "Temas relacionados", "no related section");
  for (const e of reply.slice(1)) excludes(studyText(), e.topic, "a 'contains' topic is not listed");
  eq(content().querySelectorAll("button").filter((b) => b.dataset.topic).length, 0, "no topic buttons");
  excludes(studyText(), "Sin resultados", "an exact article exists");
});

scenario("Only verb forms (and contains) → «Sin resultados en Nelson», no titles", () => {
  labelledFe((e, i) => ({ matchKind: i % 2 ? "verbform" : "contains" }));
  searchFe();
  eq(fullTopics().length, 0, "no article");
  includes(studyText(), "Sin resultados en", "the existing no-result message");
  includes(studyText(), "«fe»", "names the query");
  excludes(studyText(), "Temas relacionados", "no related section");
  excludes(studyText(), "Sin artículo exacto", "no half-result message");
});

scenario("snake_case match_kind and any case are honoured too", () => {
  labelledFe((e, i) => (i === 0 ? { match_kind: "exact" } : { match_kind: i % 2 ? "VerbForm" : "CONTAINS" }));
  searchFe();
  eq(fullTopics().length, 1, "only the exact entry");
});

scenario("exact / headword / lemma / unknown kinds render in full; verbform/contains in the same reply are dropped", () => {
  labelledFe((e, i) => ({ matchKind: ["exact", "headword", "lemma", "futuro", "verbform"][i] || "contains" }));
  searchFe();
  const reply = H.calls.filter((c) => c.cmd === "lookup_dictionary" && c.args.query === "fe").pop().result.ok;
  eq(fullTopics(), reply.slice(0, 4).map((e) => e.topic), "exact + headword + lemma + unknown, in reply order");
});

scenario("A 'phrase' kind (multi-word headword, WP3 contract) is dropped like verb forms", () => {
  labelledFe((e, i) => ({ matchKind: i === 0 ? "exact" : "phrase" }));
  searchFe();
  eq(fullTopics().length, 1, "only the exact entry");
});

scenario("Keep working: no matchKind (older backend) = every entry in full, as today", () => {
  searchFe();
  const n = H.calls.filter((c) => c.cmd === "lookup_dictionary" && c.args.query === "fe").pop().result.ok.length;
  eq(fullTopics().length, n, "all entries in full");
  excludes(studyText(), "Temas relacionados", "no related section");
});

scenario("Real worktree backend: tapping «cree» (Jn 3:16) in Nelson never shows the verb-form CREER", () => {
  boot({ fixture: "worktree", storage: { nav: NAV, study: { workspace: "biblia", modulesByKind: SAVED } } });
  clickEl(wordEl("cree", 16));
  settle();
  eq(state.workspace, "diccionario", "word tap switches to Diccionario (user decision 1)");
  const look = H.calls.filter((c) => c.cmd === "lookup_dictionary" && c.args.modulePath === NELSON && c.args.query === "cree");
  assert(look.length >= 1, "Nelson asked for «cree»");
  const reply = look.pop().result.ok || [];
  assert(reply.some((e) => e.topic === "CREER" && e.matchKind === "verbform"), `fixture: the planned backend offers CREER as verbform (${JSON.stringify(reply.map((e) => [e.topic, e.matchKind]))})`);
  eq(fullTopics().filter((t) => t === "CREER").length, 0, "CREER not shown as an article");
  excludes(studyText(), "CREER", "CREER not listed anywhere");
  excludes(studyText(), "Temas relacionados", "no related section");
  if (!reply.some((e) => !["verbform", "contains"].includes(e.matchKind))) includes(studyText(), "Sin resultados en", "no-result message");
});

scenario("Newer backend snapshot (wp3): «fe» in Nelson is the FE article only, marked exact", () => {
  boot({ fixture: "wp3", storage: { nav: NAV, study: { workspace: "diccionario", modulesByKind: SAVED } } });
  $id("dict-query").value = "fe";
  clickEl($id("btn-dict-search"));
  settle();
  const reply = H.calls.filter((c) => c.cmd === "lookup_dictionary" && c.args.query === "fe").pop().result.ok;
  assert(reply.every((e) => e.matchKind), "fixture: every entry carries matchKind");
  eq(fullTopics().length, reply.filter((e) => ["exact", "headword", "lemma"].includes(e.matchKind)).length, "full = exact/headword/lemma");
  assert(fullTopics().length >= 1, "FE shown");
  excludes(studyText(), "Sin artículo exacto", "exact article exists");
});
