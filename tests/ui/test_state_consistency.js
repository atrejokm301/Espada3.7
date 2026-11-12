// ISS-03 (UI: only real Strong codes become lastStrong) and ISS-22 (stale verse/word state).

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const RVR_PLUS = `${LIB_DIR}/00RVR1960+.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const DBADV = `${LIB_DIR}/diccionario_bblico_adventistabeta08-04-06.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: "" };
const returning = (study, nav = NAV) => ({ nav, study: Object.assign({ modulesByKind: SAVED }, study) });
const resolves = (mark) => callsSince(mark, "resolve_word");
// Test-local oracle (independent of the app's strongCode): canonical H/G code with value > 0.
const realCode = (s) => {
  const m = String(s || "").trim().match(/^([HG])\s*0*(\d{1,5})$/i);
  return m && Number(m[2]) > 0 ? m[1].toUpperCase() + Number(m[2]) : "";
};
const outlined = () => $id("bible-content").querySelectorAll(".verse.selected, .verse.temp-select").length;

// ── ISS-03 ──────────────────────────────────────────────────────────────
scenario("strongCode accepts only whole H/G codes with a value > 0", () => {
  boot({ storage: { nav: NAV } });
  for (const bad of ["hijo", "Hijo", "gracia", "Génesis", "Gn", "H", "H0", "G0", "G000", "Hijo de Cus (Gn. 10:7)", "Habacuc 1:1-2", "G2624́", "H107119", "H1254a", "", null, undefined]) {
    eq(strongCode(bad), "", `strongCode(${JSON.stringify(bad)})`);
  }
  const good = { "H 430": "H430", H00430: "H430", g26: "G26", h430: "H430", H8804: "H8804", G5656: "G5656", " G25 ": "G25", H0430: "H430" };
  for (const [k, v] of Object.entries(good)) eq(strongCode(k), v, `strongCode(${JSON.stringify(k)})`);
});

scenario("A saved lastStrong 'H0' is ignored at launch", () => {
  boot({ storage: returning({ workspace: "lexico", lastStrong: "H0" }) });
  eq(state.lastStrong, "", "lastStrong");
  assert($id("dict-query").value !== "H0", "search box does not show H0");
  assert(!H.calls.some((c) => c.cmd === "probe_modules_content" && c.args.query === "H0"), "no probe for H0");
  assert(!H.calls.some((c) => c.cmd === "lookup_lexicon" && c.args.strong === "H0"), "no lookup for H0");
  assert((storedStudy().lastStrong || "") !== "H0", "H0 not persisted again");
});

scenario("Tapping «Hijo» never makes a fake H0/G0 the current Strong", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  clickEl(wordEl("Hijo", 16));
  settle();
  const strongs = (state.wordLinks && state.wordLinks.strongs) || [];
  // The current backend (HEAD) returns ['H0','G3588','G5207','H107119',…] here; a fixed one has no H0.
  if (H.fixtureName === "head") eq(strongs[0], "H0", "precondition: HEAD backend puts a fake H0 first for «Hijo»");
  assert(!/^[HG]0*$/.test(state.lastStrong), `fake code kept: ${state.lastStrong}`);
  eq(state.lastStrong, strongs.map(realCode).find(Boolean), "first real code wins");
  assert(!/^[HG]0+$/.test(storedStudy().lastStrong || ""), "fake code not persisted");
  const mark = H.calls.length;
  clickEl(document.querySelector('.ws-tab[data-ws="lexico"]'));
  settle();
  assert($id("dict-query").value !== "H0" && $id("dict-query").value !== "G0", "Léxico box not H0/G0");
  assert(!callsSince(mark, "probe_modules_content").some((c) => /^[HG]0*$/.test(c.args.query)), "probe query is not a fake code");
});

scenario("Clicking a fake 'H0' chip does not set lastStrong", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  $id("ws-ctx-strongs").innerHTML = `<button type="button" class="chip-strong" data-strong="H0">H0</button>`;
  clickEl($id("ws-ctx-strongs").querySelector(".chip-strong"));
  settle();
  eq(state.lastStrong, "", "lastStrong");
});

// ── ISS-22 ──────────────────────────────────────────────────────────────
scenario("Tap «amó», then next chapter → no re-resolve, G25 kept, card still says Juan 3:16", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  clickEl(wordEl("amó", 16));
  settle();
  eq(state.lastStrong, "G25", "amó → G25");
  const mark = H.calls.length;
  clickEl($id("btn-next-ch"));
  settle();
  eq([state.book, state.chapter, state.verse], [43, 4, 1], "moved to Juan 4:1");
  eq(resolves(mark).length, 0, "navigation does not re-resolve the old word in the new verse");
  eq(state.lastStrong, "G25", "lastStrong kept");
  clickEl(document.querySelector('.ws-tab[data-ws="lexico"]'));
  settle();
  const loc = $id("study-content").querySelector(".lex-card-loc");
  assert(loc, "lexicon card rendered");
  includes(loc.textContent, "Juan 3:16", "card shows the verse the word came from");
  excludes(loc.textContent, "Juan 4:1", "card does not show the current verse");
});

scenario("Relaunch with a remembered word → exactly one resolve, never stuck on «Buscando»", () => {
  boot({ storage: returning({ workspace: "biblia", selectedWord: "amó", lastStrong: "G25", wordOrigin: { book: 43, chapter: 3, verse: 16 } }, { book: 43, chapter: 4, verse: 1, biblePath: RVR }) });
  const rs = resolves(0);
  eq(rs.length, 1, "resolve_word at launch");
  eq([rs[0].args.word, rs[0].args.book, rs[0].args.chapter, rs[0].args.verse], ["amó", 43, 3, 16], "resolved at the stored origin");
  excludes($id("link-panel-inline").textContent, "Buscando", "side pane not stuck");
  includes($id("link-panel-inline").textContent, "G25", "side pane shows the word's Strong");
});

scenario("Relaunch on Léxico with a remembered word → one resolve and the definition", () => {
  boot({ storage: returning({ workspace: "lexico", selectedWord: "amó", lastStrong: "G25", wordOrigin: { book: 43, chapter: 3, verse: 16 } }) });
  eq(resolves(0).length, 1, "resolve_word at launch");
  excludes(studyText(), "No se encontró definición", "definition shown");
});

scenario("Right-click «amó» in v16 with {16,18} selected → looks up in v16", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  clickEl($id("bible-content").querySelector('.verse-num[data-verse="16"]'));
  clickEl($id("bible-content").querySelector('.verse-num[data-verse="18"]'), { shiftKey: true });
  settle();
  eq([...state.selectedVerses].sort(), [16, 18], "two verses selected");
  contextMenu(wordEl("amó", 16));
  const mark = H.calls.length;
  clickEl($id("ctx-menu").querySelector('button[data-act="lookup"]'));
  settle();
  const rs = resolves(mark);
  eq(rs.length, 1, "one lookup");
  eq([rs[0].args.word, rs[0].args.verse], ["amó", 16], "word and its own verse");
  eq(state.lastStrong, "G25", "resolved in the right verse");
});

scenario("Paralela: right-click with no word under the cursor never sends the stale word", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  clickEl(wordEl("amó", 16));
  settle();
  clickEl(document.querySelector('.ws-tab[data-ws="biblia"]'));
  clickEl(document.querySelector('.mode-btn[data-mode="paralela"]'));
  settle();
  const cell = $id("bible-content").querySelector('td.verse[data-verse="17"]');
  assert(cell, "parallel table rendered");
  contextMenu(cell);
  const mark = H.calls.length;
  clickEl($id("ctx-menu").querySelector('button[data-act="lookup"]'));
  settle();
  eq(resolves(mark).length, 0, "nothing looked up (no word, no text selection)");
  H.selectionText = "Nicodemo";
  contextMenu(cell);
  clickEl($id("ctx-menu").querySelector('button[data-act="lookup"]'));
  settle();
  const rs = resolves(mark);
  eq(rs.map((c) => c.args.word), ["Nicodemo"], "the selected text is looked up instead");
});

scenario("Only one verse is outlined after tapping a word in another verse", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  eq(outlined(), 1, "launch: the current verse");
  clickEl(wordEl("mundo", 17));
  settle();
  clickEl(document.querySelector('.ws-tab[data-ws="biblia"]'));
  settle();
  eq(outlined(), 1, "after the tap");
  assert($id("bible-content").querySelector('.verse[data-verse="17"]').classList.contains("selected"), "verse 17 outlined");
  eq([...state.selectedVerses], [17], "state matches what is drawn");
});

scenario("Changing chapter in the dropdown resets the verse selection", () => {
  boot({ storage: returning({ workspace: "biblia" }) });
  clickEl($id("bible-content").querySelector('.verse-num[data-verse="16"]'));
  clickEl($id("bible-content").querySelector('.verse-num[data-verse="18"]'), { shiftKey: true });
  changeSelect($id("sel-chapter"), "4");
  settle();
  eq([state.chapter, state.verse], [4, 1], "Juan 4:1");
  eq([...state.selectedVerses], [1], "selection reset to the new verse");
});

scenario("DBADV side card opens its own entry (lookup_lexicon with the card topic)", () => {
  // HEAD fixture: the newer backend no longer offers DBADV prose cards for «amó» (ISS-04).
  boot({ fixture: "head", storage: returning({ workspace: "biblia" }) });
  clickEl(wordEl("amó", 16));
  settle();
  clickEl(document.querySelector('.ws-tab[data-ws="biblia"]'));
  settle();
  const card = $id("link-panel-inline").querySelectorAll('.mini-entry[data-kind="lex"]').find((m) => m.dataset.path === DBADV);
  assert(card, "a DBADV card is offered for «amó»");
  const topic = card.dataset.topic;
  const mark = H.calls.length;
  clickEl(card);
  settle();
  eq(state.workspace, "lexico", "opens Léxico");
  eq(state.studyModule, DBADV, "DBADV is the module");
  assert(callsSince(mark, "lookup_lexicon").some((c) => c.args.modulePath === DBADV && c.args.strong === topic), "the card's topic is looked up");
  eq(resolves(mark).length, 0, "the topic sentence is not re-resolved as a word");
  excludes(studyText(), "No se encontró definición", "entry shown");
  eq(state.lastStrong, "G25", "Strong context kept");
});

scenario("Tapping a Strong number in another verse makes that verse current", () => {
  boot({ storage: returning({ workspace: "biblia" }, { book: 43, chapter: 3, verse: 16, biblePath: RVR_PLUS }) });
  const ref = $id("bible-content").querySelector('.verse[data-verse="18"] .strong-ref');
  assert(ref, "Strong-tagged Bible renders .strong-ref in v18");
  const mark = H.calls.length;
  clickEl(ref);
  settle();
  eq(state.verse, 18, "verse");
  const rs = resolves(mark);
  assert(rs.length >= 1 && rs.every((c) => c.args.verse === 18), "resolved in v18");
  eq(outlined(), 1, "one outlined verse");
});
