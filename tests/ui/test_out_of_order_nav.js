// ISS-11 prerequisite (WP5 async commands) for the READING pane: when chapter / book / Bible
// changes are made faster than the backend answers, and the replies come back in reverse
// order, the reader shows the LAST chapter chosen — the text, the location label and the
// stored position agree. (Today's sync backend answers FIFO; WP5 makes this order possible.)

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NVI = `${LIB_DIR}/01NVI.bbli`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const verseText = (v) => {
  const el = $id("bible-content").querySelector(`.verse[data-verse="${v}"]`);
  return el ? el.textContent.replace(/\s+/g, " ") : "";
};
const fixtureVerse = (path, b, c, v) => {
  const r = H.fixture.responses[fixtureKey("get_bible_chapter", { modulePath: path, book: b, chapter: c })];
  assert(r && r.ok, `fixture ${b}:${c} for ${path}`);
  return r.ok.verses.find((x) => x.verse === v);
};
const verseCount = () => $id("bible-content").querySelectorAll(".verse").length;

scenario("Chapter 4 then back to 3 before either reply, replies reversed → Juan 3 is shown", () => {
  boot({ storage: { nav: NAV, study: { workspace: "biblia" } } });
  H.delivery = "hold";
  changeSelect($id("sel-chapter"), "4");
  changeSelect($id("sel-chapter"), "3");
  settle({ order: "reverse" });
  eq(state.chapter, 3, "state");
  eq(verseCount(), H.fixture.responses[fixtureKey("get_bible_chapter", { modulePath: RVR, book: 43, chapter: 3 })].ok.verses.length, "Juan 3's verse count on screen");
  includes(verseText(16), "de tal manera amó Dios al mundo", "Jn 3:16 text");
  eq($id("sel-chapter").value, "3", "chapter dropdown agrees");
});

scenario("Bible NVI then back to RVR1960 quickly, replies reversed → RVR1960 text", () => {
  boot({ storage: { nav: NAV, study: { workspace: "biblia" } } });
  H.delivery = "hold";
  changeSelect($id("sel-bible"), NVI);
  changeSelect($id("sel-bible"), RVR);
  settle({ order: "reverse" });
  eq(state.biblePath, RVR, "state");
  includes(verseText(16), "de tal manera amó Dios al mundo", "RVR1960 wording");
  const nvi16 = fixtureVerse(NVI, 43, 3, 16);
  excludes(verseText(16), nvi16.html.replace(/<[^>]+>/g, "").slice(0, 25), "no NVI wording");
});

scenario("Book Romanos then Génesis quickly, replies reversed → Génesis 1 text and label", () => {
  boot({ storage: { nav: NAV, study: { workspace: "biblia" } } });
  H.delivery = "hold";
  changeSelect($id("sel-book"), "45");
  changeSelect($id("sel-book"), "1");
  settle({ order: "reverse" });
  eq([state.book, state.chapter], [1, 1], "state");
  includes(verseText(1), "En el principio creó Dios", "Gn 1:1 text");
  excludes($id("bible-content").textContent, fixtureVerse(RVR, 45, 1, 1).html.replace(/<[^>]+>/g, "").slice(0, 30), "no Romanos text");
});

scenario("Control (today's FIFO order): chapter 4 then 3 → Juan 3", () => {
  boot({ storage: { nav: NAV, study: { workspace: "biblia" } } });
  changeSelect($id("sel-chapter"), "4");
  changeSelect($id("sel-chapter"), "3");
  settle();
  includes(verseText(16), "de tal manera amó Dios al mundo", "Jn 3:16 text");
});

scenario("Paralela: chapter 4 then 3 quickly, replies reversed → the table is Juan 3 in every column", () => {
  const PAR = { mode: "paralela", parallel1: NVI, parallel2: "" };
  boot({ storage: { nav: NAV, parallel: PAR } });
  // NVI Juan 4 is not recorded: a small stand-in chapter keeps the reply shape real.
  H.overrides.get_bible_chapter = (a) =>
    a.modulePath === NVI && a.book === 43 && a.chapter === 4
      ? { ok: { book: 43, chapter: 4, module: NVI, abbreviation: "NVI", title: "", verses: [{ verse: 1, html: "NVI-JUAN-4", plain: "NVI-JUAN-4" }] } }
      : undefined;
  H.delivery = "hold";
  changeSelect($id("sel-chapter"), "4");
  changeSelect($id("sel-chapter"), "3");
  settle({ order: "reverse" });
  const root = $id("bible-content");
  assert(root.querySelector(".parallel-table"), "table rendered");
  excludes(root.textContent, "NVI-JUAN-4", "no Juan 4 cell");
  const rvr16 = root.querySelector(`td.verse[data-verse="16"][data-module="${RVR}"]`);
  assert(rvr16, "Juan 3:16 row");
  includes(rvr16.textContent.replace(/\s+/g, " "), "de tal manera amó Dios al mundo", "RVR1960 Jn 3:16");
});
