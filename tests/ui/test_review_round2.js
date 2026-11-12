// WP4 review round 2 — regression tests for the nonblocking findings that were fixed:
//  1. fresh install, first Bible tap: no «Sin resultados» flash from the sync pick before the
//     content-first switch (Diccionario waits, bounded, for the probe);
//  2. Comentario with nothing saved: the preferred list (Matthew Henry …) ranks before content;
//  3. lookup_lexicon replies for another code (HEAD LIKE fallback, G26 → G260) are dropped;
//  4. changing the Bible on Comentario updates the verse text under the banner;
//  5. «Actualizar lista» keeps a session-only fallback folder session-only (ISS-31);
//  6. bracketed supplied words ([es], [yo]) are tappable; footnote markers are not;
//  7. old-WebKit fallbacks for color-mix() / :focus-visible; 8. cache-buster bumped.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const LBLA = `${LIB_DIR}/00LBLA-NT+.bbli`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const GONE = "/Volumes/Desconectado/modulos";
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const tab = (ws) => {
  clickEl(document.querySelector(`.ws-tab[data-ws="${ws}"]`));
  settle();
};
const abbrOf = (p) => (moduleByPath(p) || {}).abbreviation || "";

// ── 1. first-tap flash ──────────────────────────────────────────────────────────────
scenario("Fresh install, first tap «amó»: one dictionary render, never one for a module that is then replaced", () => {
  boot({ storage: { nav: NAV } });
  const mark = H.calls.length;
  const w = H.writes.length;
  clickEl(wordEl("amó", 16));
  settle();
  eq(state.workspace, "diccionario", "word tap opens Diccionario");
  const finalAbbr = abbrOf(state.studyModule);
  const renders = writesSince(w, "study-content").filter((x) => /Diccionario ·/.test(x.html));
  eq(renders.length, 1, "one dictionary render");
  includes(renders[0].html, `Diccionario · ${finalAbbr}`, "it is the module that stays");
  // HEAD backend: the probe moves the pick off Nelson (no entry) → a «Sin resultados» there was the flash.
  // WP3 backend: nothing has «amó» (verb forms dropped) → Nelson's empty result is the honest answer.
  const sinRes = writesSince(w, "study-content").filter((x) => /Sin resultados/.test(x.html));
  assert(sinRes.every((x) => x.html.includes(`Diccionario · ${finalAbbr}`)), "no empty-result message from a replaced module");
  const probe = callsSince(mark, "probe_modules_content");
  const look = callsSince(mark, "lookup_dictionary");
  eq(look.length, 1, "the dictionary is looked up once (no lookup in a module that is then replaced)");
  assert(probe.length >= 1 && look[0].issuedAt >= probe[0].doneAt, "the lookup waits for the content probe");
  eq(look[0].args.modulePath, state.studyModule, "the lookup used the module that is shown");
  eq(((storedStudy() || {}).modulesByKind || {}).diccionario || "", "", "automatic pick not saved");
});

scenario("Probe slower than the wait → the sync pick is looked up after the bound (never stuck on «Cargando…»)", () => {
  boot({ storage: { nav: NAV } });
  H.LAT.probe_modules_content = 60000; // the harness backend is serial: later replies queue behind it
  const mark = H.calls.length;
  clickEl(wordEl("amó", 16));
  advance(5000); // well past FIRST_PROBE_WAIT_MS, long before the probe reply
  const probe = callsSince(mark, "probe_modules_content");
  const look = callsSince(mark, "lookup_dictionary");
  assert(probe.length && !probe[0].doneAt, "probe still running");
  eq(look.length, 1, "the lookup was issued without the probe");
  assert(look[0].issuedAt - probe[0].issuedAt <= FIRST_PROBE_WAIT_MS + 1, "after at most the bounded wait");
  settle();
  includes(studyText(), "Diccionario ·", "a dictionary view is shown in the end");
  excludes(studyText(), "Cargando…", "not left loading");
});

scenario("Saved dictionary → no wait: the lookup is issued before the probe answers", () => {
  const nelson = `${LIB_DIR}/Nelson.dcti`;
  boot({ storage: { nav: NAV, study: { modulesByKind: { diccionario: nelson } } } });
  const mark = H.calls.length;
  clickEl(wordEl("amó", 16));
  settle();
  const look = callsSince(mark, "lookup_dictionary");
  const probe = callsSince(mark, "probe_modules_content");
  assert(look.length >= 1, "looked up");
  eq(look[0].args.modulePath, nelson, "saved module used");
  assert(!probe.length || look[0].issuedAt < probe[0].doneAt, "lookup not delayed by the probe");
});

// ── 2. commentary ranking ──────────────────────────────────────────────────────────
function bootCmtProbe(hasFn) {
  H.overrides.probe_modules_content = (a) => ({ ok: a.paths.map((p) => ({ path: p, has: hasFn(p) })) });
  boot({ storage: { nav: NAV } });
  tab("comentario");
}
scenario("Comentario, nothing saved: Matthew Henry without a verse note still beats Adam Clarke with one", () => {
  bootCmtProbe((p) => /clarke|jfb|jamieson/i.test(p));
  eq(state.studyModule, CMH, `Matthew Henry kept (got ${abbrOf(state.studyModule)})`);
  const first = $id("sel-study-module").options[0].value;
  eq(first, CMH, "Matthew Henry first in the dropdown");
});

scenario("Comentario, nothing saved: content still orders inside the preferred group", () => {
  const mac = () => findModule(/macarthur/i, "commentary");
  bootCmtProbe((p) => /macarthur/i.test(p));
  assert(mac(), "MacArthur in the library");
  eq(state.studyModule, mac().path, "MacArthur (preferred, with a note) before Matthew Henry (preferred, none)");
});

scenario("Diccionario keeps content-first: a non-preferred dictionary with content still wins", () => {
  const items = studyRankProbe();
  assert(items.has, "a module with content is ranked first");
});
function studyRankProbe() {
  boot({ storage: { nav: NAV } });
  const list = studyCandidates("diccionario");
  const odd = list.find((m) => preferenceRank(m, "dictionary") === 100);
  assert(odd, "a neutral dictionary exists");
  const ranked = rankStudyModules(list, new Map([[odd.path, true]]), "dictionary");
  return { has: ranked[0].path === odd.path };
}

// ── 3. lexicon neighbour guard ─────────────────────────────────────────────────────
const lexEntry = (topic) => ({ topic, html: `<p>entrada ${topic}</p>`, plain: `entrada ${topic}` });
scenario("Léxico: a G260 reply to a G26 lookup is not shown as the G26 definition", () => {
  H.overrides.lookup_lexicon = (a) => (a.strong === "G26" ? { ok: lexEntry("G260") } : undefined);
  boot({ storage: { nav: NAV, study: { workspace: "lexico", selectedWord: "G26", lastStrong: "G26", modulesByKind: { lexico: STRONGES } } } });
  settle();
  excludes(studyText(), "entrada G260", "neighbour entry dropped");
  includes(studyText(), "No se encontró definición", "honest «not found» in this module");
});

scenario("Léxico: leading zeros and a combining accent on the topic still match (G0026, G2624́)", () => {
  boot({});
  assert(lexEntryMatches(lexEntry("G0026"), "G26"), "G0026 = G26");
  assert(lexEntryMatches(lexEntry("G2624́"), "G2624"), "VGNT accent ignored");
  assert(!lexEntryMatches(lexEntry("G260"), "G26"), "G260 ≠ G26");
  assert(lexEntryMatches(lexEntry("Amós habla…"), "amó"), "word queries are not filtered");
  assert(!lexEntryMatches(null, "G26"), "null stays null");
});

scenario("Commentary card: a neighbour reply from the chosen lexicon is not shown for the tapped code", () => {
  H.overrides.lookup_lexicon = (a) => (a.modulePath === STRONGES && a.strong === "G26" ? { ok: lexEntry("G260") } : undefined);
  boot({ storage: { nav: NAV, study: { workspace: "comentario", modulesByKind: { comentario: CMH, lexico: STRONGES } } } });
  const content = $id("study-content");
  content.innerHTML += `<div class="study-body">${linkifyHtml("véase G26", { book: 43, chapter: 3 })}</div>`;
  clickEl(content.querySelector('.strong-ref[data-strong="G26"]'));
  settle();
  const card = $id("link-panel-inline").querySelector(".cmt-word-card");
  assert(card, "card shown");
  excludes(card.textContent, "entrada G260", "neighbour dropped");
  eq(state.cmtWordEntry, null, "no chosen-lexicon entry kept");
});

// ── 4. Bible change on Comentario ──────────────────────────────────────────────────
scenario("Comentario open, sidebar Bible changed → the verse under the banner is the new Bible's", () => {
  boot({ storage: { nav: NAV, study: { workspace: "comentario", modulesByKind: { comentario: CMH } } } });
  const before = $id("study-content").querySelector(".study-verse-text").textContent;
  const mark = H.calls.length;
  changeSelect($id("sel-bible"), LBLA);
  settle();
  const lbla = state.chapterData.verses.find((v) => v.verse === 16).plain;
  assert(lbla && lbla !== before, "the two Bibles word Jn 3:16 differently");
  eq($id("study-content").querySelector(".study-verse-text").textContent, lbla, "verse text follows the Bible");
  eq(callsSince(mark, "get_commentary").filter((c) => c.args.modulePath === CMH).length, 0, "the commentary is not refetched");
  includes(studyText(), "Comentario · Juan 3:16", "banner kept");
});

scenario("Bible changed on Biblia → the Comentario pane is not touched", () => {
  boot({ storage: { nav: NAV } });
  const w = H.writes.length;
  changeSelect($id("sel-bible"), LBLA);
  settle();
  eq(writesSince(w, "study-content").length, 0, "no study render");
});

// ── 5. ISS-31 refresh ──────────────────────────────────────────────────────────────
scenario("Saved folder unmounted, session fallback, «Actualizar lista» → the saved path is kept", () => {
  H.overrides.refresh_modules = () => ({ ok: null });
  boot({ storage: { modulesDir: GONE, nav: NAV } });
  eq(state.modulesDir, LIB_DIR, "session folder");
  clickEl($id("btn-refresh-modules"));
  settle();
  eq(state.modulesDir, LIB_DIR, "still the session folder");
  eq(localStorage.getItem(LS_KEYS.modulesDir), GONE, "the unmounted drive's path is not overwritten");
});

scenario("Normal folder, «Actualizar lista» → saved as before", () => {
  H.overrides.refresh_modules = () => ({ ok: null });
  boot({ storage: { nav: NAV } });
  localStorage.removeItem(LS_KEYS.modulesDir);
  clickEl($id("btn-refresh-modules"));
  settle();
  eq(localStorage.getItem(LS_KEYS.modulesDir), LIB_DIR, "saved");
});

// ── 6. bracketed supplied words ────────────────────────────────────────────────────
scenario("Supplied words in brackets are tappable; footnote markers and codes are not", () => {
  boot({});
  // NRV2000 / TNM / VIN style supplied words, NVI/PDT/BJ3/TLA-D markers, Greek TR codes.
  const out = tokenizeWords("El nombre [era] Pisón; [yo] [lo] haré<sup>[a]</sup> [12] [6b] [j11] 3779[ADV] [y] [Él]");
  const words = [...out.matchAll(/<span class="word"[^>]*>([^<]*)<\/span>/g)].map((m) => m[1]);
  for (const wd of ["era", "yo", "lo", "Él"]) assert(words.includes(wd), `«${wd}» tappable: ${JSON.stringify(words)}`);
  for (const mk of ["a", "12", "6b", "j11", "ADV", "y"]) assert(!words.includes(mk), `«[${mk}]» stays plain: ${JSON.stringify(words)}`);
  includes(out, "[<span", "the bracket itself stays plain text before the word");
});

// ── 7. old-WebKit CSS fallbacks ────────────────────────────────────────────────────
function cssRules(css) {
  const out = [];
  for (const m of css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}@]+)\{([^{}]*)\}/g)) out.push({ sel: m[1].trim(), body: m[2] });
  return out;
}
scenario("CSS: every :focus-visible ring has a :focus fallback rule; no selector list mixes it in", () => {
  const rules = cssRules(readFile(`${UI_DIR}/styles.v7.css`));
  const fv = rules.filter((r) => r.sel.includes(":focus-visible"));
  assert(fv.length >= 5, "focus rules present");
  for (const r of fv) {
    const parts = r.sel.split(",").map((s) => s.trim());
    // A list with one unknown selector is dropped whole on WebKit < 15.4: only the
    // «hide for pointer focus» rule may contain :focus-visible.
    assert(parts.every((s) => /:focus:not\(:focus-visible\)$/.test(s)), `only :focus:not(:focus-visible) uses it: ${r.sel}`);
    assert(/outline\s*:\s*none/.test(r.body), `${r.sel} hides the ring`);
    for (const s of parts) {
      const base = s.replace(":focus:not(:focus-visible)", ":focus");
      assert(rules.some((x) => x.sel.split(",").map((y) => y.trim()).includes(base) && /outline\s*:\s*2px/.test(x.body)), `${base} draws the ring`);
    }
  }
});

scenario("CSS: the word hover tint and the error box have a plain fallback before color-mix()", () => {
  const rules = cssRules(readFile(`${UI_DIR}/styles.v7.css`));
  for (const sel of [".verse-body .word:hover", ".link-panel-inline .empty-state.error"]) {
    const r = rules.find((x) => x.sel.split(",").map((y) => y.trim()).includes(sel));
    assert(r, `${sel} rule`);
    const decls = r.body.split(";").map((d) => d.trim()).filter((d) => d.startsWith("background:"));
    assert(decls.length >= 2 && !/color-mix/.test(decls[0]) && /color-mix/.test(decls[decls.length - 1]), `${sel}: fallback first, color-mix last: ${JSON.stringify(decls)}`);
  }
});

// ── 8. cache-buster ────────────────────────────────────────────────────────────────
scenario("index.html: one cache-buster token for the three changed files, not the old one", () => {
  const html = readFile(`${UI_DIR}/index.html`);
  const toks = [...html.matchAll(/(styles\.v7\.css|themes\.js|app\.v7\.js)\?v=([\w.-]+)/g)].map((m) => m[2]);
  eq(toks.length, 3, "three versioned assets");
  eq(new Set(toks).size, 1, "same token");
  assert(toks[0] !== "release-crossref1", `bumped (got ${toks[0]})`);
});
