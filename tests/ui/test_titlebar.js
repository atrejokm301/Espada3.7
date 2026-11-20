// Unified titlebar (2026-11-19): the workspace tabs moved from their own full-width row into the
// title row, as one compact pill with a sliding thumb; the «Contexto» chip sits at the right and
// only shows while a word is selected (no empty «Contexto: —» stretch).

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const header = () => document.querySelector("header.titlebar");
const nav = () => document.querySelector(".workspace-tabs");
const chip = () => $id("ws-context");
const tab = (ws) => document.querySelector(`.ws-tab[data-ws="${ws}"]`);

scenario("The tabs live in the titlebar (no separate row) and are not a window-drag area", () => {
  boot({ storage: { nav: NAV, study: { workspace: "biblia" } } });
  assert(header().contains(nav()), "tab bar inside the titlebar");
  eq(nav().getAttribute("role"), "tablist", "still a tablist");
  assert(nav().hasAttribute("data-no-drag"), "clicks on the bar never start a window drag");
  eq(nav().querySelectorAll(".ws-tab").length, 5, "five tabs");
  assert(header().contains(chip()), "context chip in the titlebar too");
  eq(document.querySelectorAll("#app > header").length, 1, "child selector works here (sanity for the next check)");
  eq(document.querySelectorAll("#app > nav").length, 0, "no tab row between the titlebar and the panes");
});

scenario("Context chip: hidden with no word, shows the tapped word, hides again after «Quitar selección»", () => {
  boot({ storage: { nav: NAV, study: { workspace: "biblia" } } });
  assert(chip().classList.contains("hidden"), "no word → no chip (no empty «Contexto: —»)");
  clickEl(wordEl("amó", 16));
  settle();
  assert(!chip().classList.contains("hidden"), "word selected → chip shown");
  eq($id("ws-ctx-word").textContent, "amó", "with the word");
  clickEl($id("btn-clear-word"));
  settle();
  assert(chip().classList.contains("hidden"), "cleared → hidden again");
});

scenario("Sliding thumb: without layout the active tab keeps its own pill; with layout the thumb follows the tab", () => {
  boot({ storage: { nav: NAV, study: { workspace: "biblia" } } });
  // The harness has no layout (no offsetLeft): the bar must still show which tab is active.
  assert(!nav().classList.contains("has-thumb"), "no measurement → no thumb mode");
  assert(tab("biblia").classList.contains("active"), "active tab marked (it draws its own pill)");
  // Give the tabs a layout, then switch: the thumb takes the new tab's place and width.
  [["biblia", 3, 60], ["comentario", 65, 104], ["diccionario", 171, 106], ["lexico", 279, 70], ["devocional", 351, 102]].forEach(([ws, x, w]) => {
    tab(ws).offsetLeft = x;
    tab(ws).offsetWidth = w;
  });
  clickEl(tab("comentario"));
  settle();
  const thumb = nav().querySelector(".ws-thumb");
  assert(nav().classList.contains("has-thumb"), "thumb mode on");
  eq(thumb.style.transform, "translateX(65px)", "under Comentario");
  eq(thumb.style.width, "104px", "as wide as Comentario");
  eq(tab("comentario").getAttribute("aria-selected"), "true", "announced");
  clickEl(tab("lexico"));
  settle();
  eq([thumb.style.transform, thumb.style.width], ["translateX(279px)", "70px"], "follows to Léxico");
}, { allowMissingFixtures: true });
