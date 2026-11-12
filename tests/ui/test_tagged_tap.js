// ISS-36 + ISS-35 (UI half). Strong-tagged Bibles carry each word's own Strong (WP2/WP3 render it
// as <span class="word" data-strong="H127">tierra</span>). The tap must keep that span, send its
// codes as taggedStrongs plus the Bible on screen (biblePath), and an older backend that ignores
// those args must not override the word's own tag with its interlinear guess.
// Chapters come from fixtures/tagged.json, recorded from the in-progress worktree backend
// (load_bible_chapter → clean_bible_verse_tagged). resolve_word replies are the HEAD backend's.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const RVRP = `${LIB_DIR}/00RVR1960+.bbli`;
const LBLAP = `${LIB_DIR}/10LBLA+.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const SAVED = { diccionario: NELSON, lexico: STRONGES, comentario: "" };
const TAGGED = JSON.parse(readFile(`${FIXTURE_DIR}/tagged.json`)).responses;

function useTaggedChapters() {
  H.overrides.get_bible_chapter = (a) => {
    const rec = TAGGED[`get_bible_chapter|${JSON.stringify([a.modulePath, a.book, a.chapter])}`];
    return rec || undefined;
  };
}
function bootAt(biblePath, book, chapter, verse, study = {}, fixture) {
  useTaggedChapters();
  boot({ fixture, storage: { nav: { book, chapter, verse, biblePath }, study: Object.assign({ modulesByKind: SAVED }, study) } });
}
function visibleVerse(v) {
  return $id("bible-content").querySelector(`.verse[data-verse="${v}"] .verse-body`).textContent;
}

scenario("RVR1960+ Gn 12:3: tapping «tierra» sends taggedStrongs ['H127'] and biblePath", () => {
  bootAt(RVRP, 1, 12, 3);
  const tierra = wordEl("tierra", 3);
  eq(tierra.dataset.strong, "H127", "the backend's tagged span is the tappable word");
  const mark = H.calls.length;
  clickEl(tierra);
  settle();
  const rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, "one resolve_word");
  eq(rw[0].args.taggedStrongs, ["H127"], "taggedStrongs");
  eq(rw[0].args.biblePath, RVRP, "biblePath = the Bible on screen (ISS-35)");
  eq(rw[0].args.occurrence, 0, "occurrence");
  eq(state.lastStrong, "H127", "the word's own tag is the current Strong, whatever the interlinear guessed");
  eq(state.wordLinks.strongs[0], "H127", "own tag first in the result");
  const bar = $id("study-content").querySelector(".sync-bar .chip-strong");
  eq(bar && bar.dataset.strong, "H127", "Diccionario's first Strong chip");
});

scenario("Untagged word in the same verse («familias») sends no taggedStrongs", () => {
  bootAt(RVRP, 1, 12, 3);
  const familias = wordEl("familias", 3);
  eq(familias.dataset.strong, undefined, "untagged");
  const mark = H.calls.length;
  clickEl(familias);
  settle();
  const rw = callsSince(mark, "resolve_word")[0];
  eq(rw.args.taggedStrongs, undefined, "no taggedStrongs");
  eq(rw.args.biblePath, RVRP, "biblePath still sent");
});

scenario("Visible text: morphology codes hidden from words, every Strong code stays a link", () => {
  bootAt(RVRP, 1, 1, 1);
  const verse = $id("bible-content").querySelector('.verse[data-verse="1"] .verse-body');
  const words = verse.querySelectorAll(".word").map((w) => w.textContent);
  eq(words.slice(0, 5), ["En", "el", "principio", "creó", "Dios"], "first words");
  for (const code of ["P", "NCFSA", "VaP3MS", "NCMPA"]) assert(!words.includes(code), `«${code}» is not a word`);
  eq(verse.querySelectorAll(".morph .word, .strong-ref .word, .word .word").length, 0, "nothing re-wrapped");
  const rec = TAGGED[`get_bible_chapter|${JSON.stringify([RVRP, 1, 1])}`].ok.verses[0].html;
  const raw = document.createElement("div");
  raw.innerHTML = rec;
  eq(visibleVerse(1), raw.textContent, "tokenizing never changes the visible text");
});

scenario("LBLA+ 1 Sa 16:7 «SEÑOR» → H3068 in Léxico even when the backend guesses 'no'", () => {
  bootAt(LBLAP, 9, 16, 7, {}, "head"); // the HEAD backend ignores taggedStrongs and guesses
  const senor = wordEl("SEÑOR", 7);
  eq(senor.dataset.strong, "H3068", "tag");
  clickEl(senor);
  settle();
  const raw = H.calls.filter((c) => c.cmd === "resolve_word").pop().result.ok;
  assert((raw.strongs || [])[0] !== "H3068", `fixture: the HEAD backend's own guess is not H3068 (${raw.strongs})`);
  eq(state.lastStrong, "H3068", "own tag wins");
  setWorkspace("lexico");
  settle();
  const topics = $id("study-content").querySelectorAll(".study-entry .topic").map((t) => t.textContent);
  eq(topics, ["H3068"], "one definition: H3068");
  const card = $id("study-content").querySelector(".lex-strong-btn");
  eq(card && card.dataset.strong, "H3068", "card Strong");
});

scenario("Second tagged «SEÑOR» in 1 Sa 16:7 counts as occurrence 1", () => {
  bootAt(LBLAP, 9, 16, 7);
  const mark = H.calls.length;
  clickEl(wordEl("SEÑOR", 7, 1));
  settle();
  const rw = callsSince(mark, "resolve_word")[0];
  eq(rw.args.occurrence, 1, "occurrence");
  eq(rw.args.taggedStrongs, ["H3068"], "tags");
});

scenario("Edge: junk in data-strong ('H0 X') sends no taggedStrongs", () => {
  bootAt(RVR, 1, 12, 3);
  H.overrides.get_bible_chapter = (a) =>
    a.modulePath === RVR
      ? { ok: { book: 1, chapter: 12, module: RVR, abbreviation: "RVR1960", title: "", verses: [
          { verse: 3, html: 'de la <span class="word" data-strong="H0 X">tierra</span>.', plain: "" },
        ] } }
      : undefined;
  reloadChapter();
  settle();
  const mark = H.calls.length;
  clickEl(wordEl("tierra", 3));
  settle();
  const rw = callsSince(mark, "resolve_word")[0];
  eq(rw.args.taggedStrongs, undefined, "invalid tags are dropped");
});

scenario("Relaunch re-sends the same tap context (biblePath, occurrence, tags)", () => {
  const origin = { book: 1, chapter: 12, verse: 3, biblePath: RVRP, occurrence: 0, taggedStrongs: ["H127"] };
  bootAt(RVRP, 1, 12, 3, { workspace: "lexico", selectedWord: "tierra", lastStrong: "H127", wordOrigin: origin });
  const rw = H.calls.filter((c) => c.cmd === "resolve_word");
  eq(rw.length, 1, "one resolve at startup");
  eq(rw[0].args.taggedStrongs, ["H127"], "tags re-sent");
  eq(rw[0].args.biblePath, RVRP, "Bible re-sent");
  eq(rw[0].args.occurrence, 0, "occurrence re-sent");
  eq(state.lastStrong, "H127", "still H127 after the relaunch resolve");
  eq(storedStudy().wordOrigin.taggedStrongs, ["H127"], "tags stay persisted");
});

scenario("Newer backend snapshot (wp3): own tag already first → result kept as sent", () => {
  bootAt(LBLAP, 9, 16, 7, {}, "wp3");
  clickEl(wordEl("SEÑOR", 7));
  settle();
  const raw = H.calls.filter((c) => c.cmd === "resolve_word").pop().result.ok;
  eq(raw.strongs[0], "H3068", "fixture: the newer backend already resolves H3068");
  eq(state.wordLinks.strongs, raw.strongs, "strongs untouched");
  eq(state.wordLinks.matchedHit, raw.matchedHit, "matched hit untouched");
  eq(state.lastStrong, "H3068", "current Strong");
});

// Accuracy round 2026-11-11: own tags are stored in Greek order («Dios» = G3588 G2316), so the
// article used to become the current Strong. The content code goes first, the article stays.
function bootSyntheticJn316(html) {
  bootAt(RVRP, 43, 3, 16);
  H.overrides.get_bible_chapter = (a) =>
    a.modulePath === RVRP
      ? { ok: { book: 43, chapter: 3, module: RVRP, abbreviation: "RVR1960+", title: "", verses: [{ verse: 16, html, plain: "" }] } }
      : undefined;
  reloadChapter();
  settle();
}

scenario("«Dios» tagged G3588 G2316 sends G2316 first and makes it the current Strong", () => {
  bootSyntheticJn316('amó <span class="word" data-strong="G3588 G2316">Dios</span> al mundo');
  const mark = H.calls.length;
  clickEl(wordEl("Dios", 16));
  settle();
  const rw = callsSince(mark, "resolve_word")[0];
  eq(rw.args.taggedStrongs, ["G2316", "G3588"], "content code first, article kept second");
  eq(state.lastStrong, "G2316", "θεός, not the article ὁ");
  eq(state.wordLinks.strongs[0], "G2316", "first Strong of the result");
}, { allowMissingFixtures: true });

scenario("Edge: a word tagged only with the article (G3588) keeps it", () => {
  bootSyntheticJn316('<span class="word" data-strong="G3588">aquel</span> que en él cree');
  const mark = H.calls.length;
  clickEl(wordEl("aquel", 16));
  settle();
  eq(callsSince(mark, "resolve_word")[0].args.taggedStrongs, ["G3588"], "alone: unchanged");
}, { allowMissingFixtures: true });
