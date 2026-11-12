// ISS-19 — a Greek/Hebrew word or Strong number tapped INSIDE a commentary is answered in the
// side pane (lexicon card + first entry + «Abrir en Léxico»); the commentary is not reloaded,
// keeps its scroll position, and the other commentaries are not refetched.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const onCmt = (study = {}) => ({ nav: NAV, study: Object.assign({ workspace: "comentario", modulesByKind: { comentario: CMH } }, study) });
const card = () => $id("link-panel-inline").querySelector(".cmt-word-card");
function injectGreek(text) {
  const content = $id("study-content");
  content.innerHTML += `<div class="study-body">${linkifyHtml(text, { book: 43, chapter: 3 })}</div>`;
  return content;
}

scenario("Greek word tap: one resolve, no commentary reload, scroll kept, card with a definition", () => {
  boot({ storage: onCmt() });
  const content = injectGreek("el amor (ἀγάπη) de Dios");
  content.scrollTop = 420;
  const mark = H.calls.length;
  const w = H.writes.length;
  clickEl(content.querySelector(".greek-run"));
  settle();
  eq(state.workspace, "comentario", "stays on Comentario");
  eq(callsSince(mark, "resolve_word").map((c) => c.args.word), ["αγαπη"], "one resolve_word with the folded word");
  eq(callsSince(mark, "get_commentary").length, 0, "neither the main nor the other commentaries are refetched");
  eq(writesSince(w, "study-content").length, 0, "#study-content not rewritten");
  eq(content.scrollTop, 420, "scroll position kept");
  assert(card(), "word card in the side pane");
  assert($id("link-panel-inline").querySelectorAll(".mini-entry[data-kind=\"cmt\"]").length > 0, "other commentaries still listed below");
  assert(card().querySelector(".cmt-open-lex"), "«Abrir en Léxico»");
  assert(content.querySelector(".greek-run").classList.contains("word-selected"), "tapped word marked");
});

scenario("Strong number in a commentary: card shows the chosen lexicon's entry first", () => {
  boot({ storage: onCmt({ modulesByKind: { comentario: CMH, lexico: STRONGES } }) });
  const content = injectGreek("véase G26");
  const mark = H.calls.length;
  clickEl(content.querySelector('.strong-ref[data-strong="G26"]'));
  settle();
  eq(state.workspace, "comentario", "stays on Comentario");
  eq(callsSince(mark, "get_commentary").length, 0, "no commentary refetch");
  const meta = card().querySelector(".cmt-word-entry .mini-meta");
  assert(meta, `an entry is shown: ${card().textContent.slice(0, 200)}`);
  includes(meta.textContent, "STRONGes", "the saved Léxico module's entry comes first");
  includes(meta.textContent, "G26", "entry for G26");
});

scenario("«Abrir en Léxico» opens the Léxico tab on that Strong; «Cerrar» removes the card", () => {
  boot({ storage: onCmt({ modulesByKind: { comentario: CMH, lexico: STRONGES } }) });
  clickEl(injectGreek("véase G26").querySelector(".strong-ref"));
  settle();
  clickEl(card().querySelector(".cmt-close-word"));
  settle();
  eq(card(), null, "card closed");
  clickEl($id("study-content").querySelector(".strong-ref"));
  settle();
  const mark = H.calls.length;
  clickEl(card().querySelector(".cmt-open-lex"));
  settle();
  eq(state.workspace, "lexico", "Léxico");
  assert(callsSince(mark, "lookup_lexicon").some((c) => c.args.modulePath === STRONGES && c.args.strong === "G26"), "G26 in STRONGes");
});

scenario("Two fast taps, replies reversed → the card shows the second word", () => {
  boot({ storage: onCmt() });
  const content = injectGreek("ἀγάπη y λόγος");
  H.delivery = "hold";
  const runs = content.querySelectorAll(".greek-run");
  clickEl(runs[0]);
  clickEl(runs[1]);
  settle({ order: "reverse" });
  eq(card().querySelector(".ctx-word-title").textContent, "λόγος", "second word shown, as written");
});

// Review round 2: the title showed the folded lookup key («λογοσ»: no accent, medial σ at the
// end), which reads as a misspelling. The tapped text is shown; the folded key is only looked up.
scenario("Card title is the word as written (λόγος), the lookup uses the folded key (λογοσ)", () => {
  boot({ storage: onCmt() });
  const content = injectGreek("el λόγος de Dios");
  const run = content.querySelector(".greek-run");
  eq(run.getAttribute("data-lookup"), "λογοσ", "folded lookup key on the run");
  const mark = H.calls.length;
  clickEl(run);
  eq(card().textContent.includes("Buscando «λόγος»"), true, "«Buscando…» shows the word as written");
  settle();
  eq(callsSince(mark, "resolve_word").map((c) => c.args.word), ["λογοσ"], "resolve_word gets the folded key");
  eq(card().querySelector(".ctx-word-title").textContent, "λόγος", "title as written");
  excludes(card().textContent, "λογοσ", "the folded key is never shown");
});

scenario("Error path: resolve_word fails → message in the card, other commentaries kept", () => {
  boot({ storage: onCmt() });
  H.overrides.resolve_word = () => ({ err: "No se pudo abrir módulo" });
  clickEl(injectGreek("ἀγάπη").querySelector(".greek-run"));
  settle();
  includes(card().textContent, "No se pudo buscar", "error shown in the card");
  assert($id("link-panel-inline").querySelectorAll(".mini-entry[data-kind=\"cmt\"]").length > 0, "other commentaries still there");
}, { allowUncaught: true });

scenario("Diccionario keeps its behaviour: a Greek word there re-renders the dictionary view", () => {
  boot({ storage: { nav: NAV, study: { workspace: "diccionario", selectedWord: "amor" } } });
  const content = injectGreek("ἀγάπη");
  clickEl(content.querySelector(".greek-run"));
  settle();
  eq(state.workspace, "diccionario", "still Diccionario");
  eq(state.selectedWord, "αγαπη", "new query");
  includes(studyText(), "Diccionario ·", "dictionary view rendered for the new word");
});
