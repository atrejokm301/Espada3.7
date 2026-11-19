// Section titles inside Bible text (2026-11-18, ported from the Windows build). The backend cuts
// them out of the verse and sends them beside it (`titles`: lead / minor / next). The page draws
// them above their verse, outside its selection box; a `next` title (stored at the end of the
// verse before) above the following verse, or after the chapter's last verse. Title words are
// tappable but never counted in a tap's occurrence. Chapters recorded before titles existed have
// no `titles` field and render as before.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NAV = { book: 48, chapter: 3, verse: 5, biblePath: RVR };

const CHAPTER = {
  book: 48,
  chapter: 3,
  module: RVR,
  abbreviation: "RVR1960",
  title: "Reina-Valera 1960",
  verses: [
    { verse: 4, html: "¿Tantas cosas habéis padecido en vano?", plain: "¿Tantas cosas habéis padecido en vano?" },
    {
      verse: 5,
      html: "Aquel, pues, ¿lo hace por las obras de la ley?",
      plain: "Aquel, pues, ¿lo hace por las obras de la ley?",
      titles: [{ kind: "next", html: "El pacto de Dios con Abraham" }],
    },
    {
      verse: 6,
      html: "Así Abraham creyó a Dios.",
      plain: "Así Abraham creyó a Dios.",
      titles: [{ kind: "lead", html: "<strong>Abraham y la fe</strong>" }],
    },
    {
      verse: 7,
      html: "Sabed, por tanto, que los que son de fe, éstos son hijos de Abraham.",
      plain: "Sabed, por tanto, que los que son de fe, éstos son hijos de Abraham.",
      titles: [{ kind: "next", html: "Nombramiento de jueces" }, { kind: "next", html: "(Dt. 1.9-18)" }],
    },
  ],
};

function setupTitles(chapter = CHAPTER) {
  H.overrides.get_bible_chapter = (a) => (a.book === 48 && a.chapter === 3 ? { ok: { ...chapter, module: a.modulePath } } : undefined);
  H.overrides.resolve_word = (a) => ({
    ok: { word: a.word, normalized: a.word.toLowerCase(), strongs: [], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: false, interlinearHits: [], matchedHit: null, matchKind: "" },
  });
  boot({ fixture: "head", storage: { nav: NAV, study: { workspace: "biblia" } } });
  settle();
}
const root = () => $id("bible-content");
const blockBefore = (vn) => {
  const kids = root().children;
  const i = kids.findIndex((k) => k.classList.contains("verse") && k.dataset.verse === String(vn));
  return i > 0 && kids[i - 1].classList.contains("vtitles") ? kids[i - 1] : null;
};
const lines = (block) => (block ? block.querySelectorAll(".vtitle").map((l) => l.textContent.trim()) : []);

scenario("Titles sit above their verse, outside it; a «next» title above the following verse", () => {
  setupTitles();
  const v6 = blockBefore(6);
  eq(lines(v6), ["El pacto de Dios con Abraham", "Abraham y la fe"], "verse 5's next title, then verse 6's own");
  eq(v6.dataset.verse, "6", "the block belongs to verse 6");
  eq(blockBefore(5), null, "no heading above verse 5 (its title is for verse 6)");
  excludes(root().querySelector('.verse[data-verse="5"]').textContent, "pacto", "not in verse 5's text");
  excludes(root().querySelector('.verse[data-verse="6"]').textContent, "Abraham y la fe", "not in verse 6's box");
  eq(v6.querySelectorAll(".vtitle")[0].getAttribute("role"), "heading", "a heading for VoiceOver");
}, { allowMissingFixtures: true });

scenario("The last verse's «next» title is shown after the chapter; a parallel passage is lighter", () => {
  setupTitles();
  const kids = root().children;
  const last = kids[kids.length - 1];
  assert(last.classList.contains("vtitles"), "titles after the last verse");
  eq(lines(last), ["Nombramiento de jueces", "(Dt. 1.9-18)"], "both lines");
  assert(last.querySelectorAll(".vtitle")[1].classList.contains("minor"), "«(Dt. 1.9-18)» lighter");
  assert(!last.querySelectorAll(".vtitle")[0].classList.contains("minor"), "the title itself is not");
}, { allowMissingFixtures: true });

scenario("A title word is tappable: it selects its verse and is never counted in an occurrence", () => {
  setupTitles();
  const title = blockBefore(6).querySelectorAll(".vtitle")[1];
  const w = title.querySelectorAll(".word").find((x) => x.textContent === "Abraham");
  assert(w, "title words are words");
  let mark = H.calls.length;
  clickEl(w);
  settle();
  let r = H.calls.slice(mark).filter((c) => c.cmd === "resolve_word").pop();
  eq([r.args.word, r.args.verse, r.args.occurrence], ["Abraham", 6, 0], "verse 6, first occurrence");
  eq(state.verse, 6, "verse 6 selected");
  assert(root().querySelector('.verse[data-verse="6"]').classList.contains("selected"), "the verse box, not the title");
  // «Abraham» in verse 6's own text: the title's «Abraham» does not count before it.
  mark = H.calls.length;
  clickEl(wordEl("Abraham", 6));
  settle();
  r = H.calls.slice(mark).filter((c) => c.cmd === "resolve_word").pop();
  eq(r.args.occurrence, 0, "first «Abraham» of the verse itself");
}, { allowMissingFixtures: true });

scenario("«next» titles are headings, not words (they belong to the verse that follows)", () => {
  setupTitles();
  const next = blockBefore(6).querySelectorAll(".vtitle")[0];
  assert(next.classList.contains("next"), "marked next");
  eq(next.querySelectorAll(".word").length, 0, "not tappable");
}, { allowMissingFixtures: true });

scenario("A chapter without titles (recorded before them) renders exactly as before", () => {
  const plain = { ...CHAPTER, verses: CHAPTER.verses.map(({ verse, html, plain }) => ({ verse, html, plain })) };
  setupTitles(plain);
  eq(root().querySelectorAll(".vtitles").length, 0, "no title blocks");
  eq(root().querySelectorAll(".verse").length, 4, "every verse");
}, { allowMissingFixtures: true });

scenario("Paralela: each column's titles stay in its own cell", () => {
  H.overrides.get_bible_chapter = (a) => (a.book === 48 && a.chapter === 3 ? { ok: { ...CHAPTER, module: a.modulePath } } : undefined);
  boot({
    fixture: "head",
    storage: { nav: NAV, study: { workspace: "biblia" }, parallel: { mode: "paralela", parallel1: `${LIB_DIR}/00LBLA.bbli`, parallel2: "" } },
  });
  settle();
  if (state.mode !== "paralela") {
    clickEl(document.querySelector('.mode-btn[data-mode="paralela"]'));
    settle();
  }
  eq(state.mode, "paralela", "Paralela on");
  const cell = root().querySelector('tr[data-verse="6"] td.verse');
  assert(cell, "verse 6 row");
  eq(cell.querySelectorAll(".vtitle").map((l) => l.textContent.trim()), ["Abraham y la fe"], "its own title, at the top");
  const c5 = root().querySelector('tr[data-verse="5"] td.verse');
  eq(c5.querySelectorAll(".vtitle.next").map((l) => l.textContent.trim()), ["El pacto de Dios con Abraham"], "a next title at the end of its cell");
}, { allowMissingFixtures: true });
