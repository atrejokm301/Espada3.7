// Section titles of combined modules (2026-11-18, ported from the Windows build): the main panel
// asks the backend for them (`sections: true`) — Léxico, Diccionario, a topic from the Índice and
// the Comentario — and the backend wraps those lines in <span class="sec-title">. Side panes,
// probes and tooltips never ask (learning a module's titles reads a sample of it once).

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const MULTI = `${LIB_DIR}/multilxico_strong-chvez-tuggy-vine-swanson.lexi`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const MH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const G67_HTML =
  '<span class="sec-title"><strong>Diccionario Strong</strong></span><br>Ἀγρίππας Agríppas<br><span class="sec-title"><strong>Diccionario Tuggy</strong></span><br>Agripa. Véase <span class="bible-ref">Hch 25:13</span>';

const callsOf = (cmd, mark = 0) => H.calls.slice(mark).filter((c) => c.cmd === cmd);

scenario("Léxico: the main lookup asks for section titles and shows them as headings", () => {
  H.overrides.resolve_word = (a) =>
    a.word === "G67"
      ? { ok: { word: "G67", normalized: "g67", strongs: ["G67"], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: false, interlinearHits: [], matchedHit: null, matchKind: "" } }
      : undefined;
  H.overrides.lookup_lexicon = (a) => ({ ok: a.strong === "G67" ? { topic: "G67", html: G67_HTML, plain: "x", matchKind: "exact" } : null });
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "lexico", modulesByKind: { lexico: MULTI, diccionario: "", comentario: "" } } } });
  $id("dict-query").value = "G67";
  clickEl($id("btn-dict-search"));
  settle();
  const main = callsOf("lookup_lexicon").filter((c) => c.args.modulePath === MULTI);
  assert(main.length >= 1, "looked up in the Multiléxico");
  assert(main.every((c) => c.args.sections === true), "the main panel asks for section titles");
  const heads = $id("study-content").querySelectorAll(".study-body .sec-title").map((h) => h.textContent.trim());
  eq(heads, ["Diccionario Strong", "Diccionario Tuggy"], "drawn as section headings");
  assert(!callsOf("lookup_lexicon_multi").some((c) => c.args.sections), "side lookups never ask");
}, { allowMissingFixtures: true });

scenario("Diccionario: the main lookup asks; the Strong bridge arguments are unchanged", () => {
  H.overrides.resolve_word = (a) =>
    a.word === "amor"
      ? { ok: { word: "amor", normalized: "amor", strongs: ["G26"], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: true, interlinearHits: [], matchedHit: null, matchKind: "" } }
      : undefined;
  H.overrides.lookup_dictionary = () => ({ ok: [{ topic: "AMOR", html: '<span class="sec-title"><strong>ARQUEOLÓGICAS</strong></span><br>texto', plain: "texto", matchKind: "exact" }] });
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "diccionario", modulesByKind: { diccionario: NELSON, lexico: "", comentario: "" } } } });
  $id("dict-query").value = "amor";
  clickEl($id("btn-dict-search"));
  settle();
  const c = callsOf("lookup_dictionary").filter((x) => x.args.modulePath === NELSON).pop();
  eq([c.args.query, c.args.sections], ["amor", true], "main lookup with titles");
  eq(c.args.strongs, ["G26"], "bridge arguments still sent");
  eq($id("study-content").querySelector(".sec-title").textContent.trim(), "ARQUEOLÓGICAS", "heading kept by the link pass");
}, { allowMissingFixtures: true });

scenario("Comentario: the chosen commentary asks for titles, «Otros comentarios» never do", () => {
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "comentario", modulesByKind: { comentario: MH, diccionario: "", lexico: "" } } } });
  settle();
  const calls = callsOf("get_commentary");
  const main = calls.filter((c) => c.args.modulePath === MH);
  const others = calls.filter((c) => c.args.modulePath !== MH);
  assert(main.length >= 1 && main.every((c) => c.args.sections === true), "main commentary with titles");
  assert(others.length >= 1, "fixture: the side pane read other commentaries");
  assert(others.every((c) => c.args.sections === undefined), "side pane never learns titles");
}, { allowMissingFixtures: true });
