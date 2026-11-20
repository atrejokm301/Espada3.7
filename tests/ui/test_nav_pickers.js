// Navigation pickers (2026-11-19): the Libro tile opens Génesis…Apocalipsis as tiles in two
// sections (Antiguo Testamento 1–39, Nuevo Testamento 40–66); a book shows its chapter tiles;
// a chapter navigates. Biblia / Paralela / módulo / fuente are dropdown menus that drive the
// hidden native selects (still the source of truth). The buttons always show what the selects hold.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NVI = `${LIB_DIR}/01NVI.bbli`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const at = (book, chapter, verse, biblePath = RVR) => ({ book, chapter, verse, biblePath });
const onBiblia = (nav = at(43, 3, 16), extra = {}) => Object.assign({ nav, study: { workspace: "biblia" } }, extra);
const key = (el, opts) => dispatch(el, makeEvent("keydown", opts));
const input = (el, value) => {
  el.value = value;
  return dispatch(el, makeEvent("input"));
};
const panelOpen = () => !$id("panel-books").classList.contains("hidden");
const tiles = (root = $id("books-body")) => root.querySelectorAll(".book-tile");
const visibleBooks = () => tiles().filter((t) => !t.classList.contains("hidden")).map((t) => Number(t.dataset.book));
const menu = () => $id("glass-menu");
const menuOpen = () => !menu().classList.contains("hidden");
const trigger = (selId) => document.querySelector(`[data-menu-for="${selId}"]`);
const optionText = (selId, value) => $id(selId).options.find((o) => o.value === value).textContent.trim();
const menuItemFor = (selId, value) => {
  const i = $id(selId).options.findIndex((o) => o.value === value);
  assert(i >= 0, `option ${value} in ${selId}`);
  return menu().querySelector(`.gm-item[data-i="${i}"]`);
};
const chapterCalls = (mark) => callsSince(mark, "get_bible_chapter");

scenario("Libro tile shows the current book and testament; the panel lists Génesis…Apocalipsis split AT 39 / NT 27", () => {
  boot({ storage: onBiblia() });
  eq($id("book-pick-name").textContent, "Juan", "book on the tile");
  eq($id("book-testament").textContent, "NT", "testament badge");
  clickEl($id("btn-book-pick"));
  assert(panelOpen(), "books panel open");
  eq($id("panel-books").dataset.view, "books", "books view");
  const ot = tiles($id("books-ot"));
  const nt = tiles($id("books-nt"));
  eq(ot.length, 39, "39 AT books");
  eq(nt.length, 27, "27 NT books");
  eq(ot.map((t) => Number(t.dataset.book)), Array.from({ length: 39 }, (_, i) => i + 1), "AT in canonical order");
  eq(nt.map((t) => Number(t.dataset.book)), Array.from({ length: 27 }, (_, i) => i + 40), "NT in canonical order");
  // Small pills show a short label; the full name is the accessible name and the tooltip.
  eq([ot[0], ot[38], nt[0], nt[26]].map((t) => t.getAttribute("aria-label")), ["Génesis", "Malaquías", "Mateo", "Apocalipsis"], "full names, Génesis…Apocalipsis");
  eq([ot[0], ot[38], nt[0], nt[26]].map((t) => t.querySelector(".bt-name").textContent), ["Gén", "Mal", "Mat", "Ap"], "short labels");
  includes(ot[0].getAttribute("title"), "Génesis · 50 capítulos", "tooltip: name and chapters");
  const shorts = tiles().map((t) => t.querySelector(".bt-name").textContent);
  eq(new Set(shorts).size, 66, "every short label is different");
  includes($id("books-ot").textContent, "Antiguo Testamento", "AT heading");
  includes($id("books-nt").textContent, "Nuevo Testamento", "NT heading");
  const active = tiles().filter((t) => t.classList.contains("active"));
  eq(active.map((t) => Number(t.dataset.book)), [43], "only Juan is marked current");
  eq(active[0].getAttribute("aria-current"), "true", "current tile announced");
  assert($id("books-nt").contains(active[0]), "Juan is in the NT section");
  assert(document.activeElement === $id("book-filter"), "typing filters right away");
});

scenario("Book tile → chapter tiles → chapter: Romanos 1 is shown; selects, tile, badge and history follow", () => {
  boot({ storage: onBiblia() });
  clickEl($id("btn-book-pick"));
  const mark = H.calls.length;
  clickEl($id("books-body").querySelector('.book-tile[data-book="45"]'));
  settle();
  eq($id("panel-books").dataset.view, "chapters", "chapter view");
  eq($id("books-title").textContent, "Romanos", "panel title");
  eq($id("books-body").querySelectorAll(".num-tile").length, 16, "16 chapter tiles");
  eq(state.book, 43, "a book tile alone does not navigate");
  eq(chapterCalls(mark).length, 0, "no chapter loaded yet");
  clickEl($id("books-body").querySelector('.num-tile[data-ch="1"]'));
  settle();
  eq([state.book, state.chapter, state.verse], [45, 1, 1], "Romanos 1:1");
  assert(!panelOpen(), "panel closed");
  assert($id("overlay").classList.contains("hidden"), "overlay closed");
  eq($id("sel-book").value, "45", "book select follows");
  eq($id("sel-chapter").value, "1", "chapter select follows");
  eq($id("book-pick-name").textContent, "Romanos", "book tile");
  eq($id("book-testament").textContent, "NT", "badge");
  eq($id("btn-chapter-pick").textContent, "1", "chapter bar");
  eq(state.history[0].label, "Romanos 1:1", "history entry");
  const calls = chapterCalls(mark);
  eq(calls.length, 1, "one chapter load");
  eq([calls[0].args.book, calls[0].args.chapter], [45, 1], "for Romanos 1");
  includes($id("bible-content").textContent, "Pablo, siervo de Jesucristo", "Romanos text on screen");
});

scenario("An AT book: Josué 2 → badge AT; at Génesis 1 the ‹ chapter button is disabled", () => {
  boot({ storage: onBiblia() });
  clickEl($id("btn-book-pick"));
  clickEl($id("books-body").querySelector('.book-tile[data-book="6"]'));
  clickEl($id("books-body").querySelector('.num-tile[data-ch="2"]'));
  settle();
  eq([state.book, state.chapter], [6, 2], "Josué 2");
  eq($id("book-testament").textContent, "AT", "badge AT");
  eq($id("book-pick-name").textContent, "Josué", "book tile");
  assert(!$id("btn-prev-ch").disabled, "‹ enabled away from Gn 1");
  clickEl($id("btn-book-pick"));
  clickEl($id("books-body").querySelector('.book-tile[data-book="1"]'));
  clickEl($id("books-body").querySelector('.num-tile[data-ch="1"]'));
  settle();
  eq([state.book, state.chapter], [1, 1], "Génesis 1");
  assert($id("btn-prev-ch").disabled, "‹ disabled at Gn 1");
  assert(!$id("btn-next-ch").disabled, "› enabled");
});

scenario("Chapter number opens the current book's chapters; the current chapter just closes, another navigates", () => {
  boot({ storage: onBiblia() });
  eq($id("btn-chapter-pick").textContent, "3", "chapter bar shows 3");
  clickEl($id("btn-chapter-pick"));
  assert(panelOpen(), "panel open");
  eq($id("panel-books").dataset.view, "chapters", "straight to chapters");
  eq($id("books-title").textContent, "Juan", "of the current book");
  eq($id("books-body").querySelectorAll(".num-tile").length, 21, "Juan has 21");
  const cur = $id("books-body").querySelector(".num-tile.active");
  eq(cur && cur.dataset.ch, "3", "current chapter marked");
  assert(document.activeElement === cur, "focus on the current chapter");
  let mark = H.calls.length;
  clickEl(cur);
  settle();
  assert(!panelOpen(), "closed");
  eq(chapterCalls(mark).length, 0, "same chapter: no reload");
  eq(state.verse, 16, "verse kept");
  clickEl($id("btn-chapter-pick"));
  mark = H.calls.length;
  clickEl($id("books-body").querySelector('.num-tile[data-ch="4"]'));
  settle();
  eq([state.book, state.chapter, state.verse], [43, 4, 1], "Juan 4:1");
  eq(chapterCalls(mark).length, 1, "one load");
  eq($id("btn-chapter-pick").textContent, "4", "chapter bar follows");
});

scenario("Back from the chapter tiles returns to the 66 books; nothing is navigated", () => {
  boot({ storage: onBiblia() });
  clickEl($id("btn-book-pick"));
  clickEl($id("books-body").querySelector('.book-tile[data-book="45"]'));
  assert(!$id("btn-books-back").classList.contains("hidden"), "back shown in chapter view");
  assert($id("books-tools").classList.contains("hidden"), "search hidden in chapter view");
  const mark = H.calls.length;
  clickEl($id("btn-books-back"));
  settle();
  eq($id("panel-books").dataset.view, "books", "books view again");
  eq(tiles().length, 66, "all 66");
  assert($id("btn-books-back").classList.contains("hidden"), "back hidden");
  assert(!$id("books-tools").classList.contains("hidden"), "search shown");
  eq(state.book, 43, "still Juan");
  eq(H.calls.length - mark, 0, "no IPC");
});

scenario("Filter: accent- and space-insensitive, by name or abbreviation; no match → message; Enter opens the first match", () => {
  boot({ storage: onBiblia() });
  clickEl($id("btn-book-pick"));
  const f = $id("book-filter");
  input(f, "genesis");
  eq(visibleBooks(), [1], "«genesis» → Génesis");
  assert($id("books-nt").classList.contains("hidden"), "empty NT section hidden");
  input(f, "juan");
  eq(visibleBooks(), [43, 62, 63, 64], "«juan» → Juan, 1–3 Juan");
  assert($id("books-ot").classList.contains("hidden"), "empty AT section hidden");
  eq($id("books-nt").querySelector(".testament-count").textContent, "4 de 27", "count says what is left");
  input(f, "1 sam");
  eq(visibleBooks(), [9], "«1 sam» → 1 Samuel (short label)");
  input(f, "sant");
  eq(visibleBooks(), [59], "«sant» → Santiago");
  input(f, "1 CO");
  eq(visibleBooks(), [46], "«1 CO» → 1 Corintios");
  input(f, "Ap");
  eq(visibleBooks(), [66], "abbreviation «Ap»");
  input(f, "xyz");
  eq(visibleBooks(), [], "no match");
  assert(!$id("books-empty").classList.contains("hidden"), "«Ningún libro coincide» shown");
  input(f, "");
  eq(visibleBooks().length, 66, "cleared → all");
  eq($id("books-nt").querySelector(".testament-count").textContent, "27 libros", "full count back");
  assert($id("books-empty").classList.contains("hidden"), "message hidden again");
  input(f, "rom");
  key(f, { key: "Enter" });
  settle();
  eq($id("panel-books").dataset.view, "chapters", "Enter → chapters");
  eq($id("books-title").textContent, "Romanos", "of the first match");
  key(document.body, { key: "Escape" });
  assert(!panelOpen(), "Esc closes");
  clickEl($id("btn-book-pick"));
  eq(f.value, "", "filter cleared on reopen");
  eq(visibleBooks().length, 66, "all books on reopen");
});

scenario("Single-chapter book (Judas) goes straight to Judas 1, no chapter step", () => {
  H.overrides.get_bible_chapter = (a) =>
    a.book === 65
      ? { ok: { abbreviation: "RVR1960", book: "65", chapter: "1", module: a.modulePath, title: "Reina Valera", verses: [{ verse: 1, html: "Judas, siervo de Jesucristo", plain: "Judas, siervo de Jesucristo" }] } }
      : undefined;
  boot({ storage: onBiblia() });
  clickEl($id("btn-book-pick"));
  const mark = H.calls.length;
  clickEl($id("books-body").querySelector('.book-tile[data-book="65"]'));
  settle();
  eq([state.book, state.chapter, state.verse], [65, 1, 1], "Judas 1:1");
  assert(!panelOpen(), "panel closed, no chapter grid");
  eq(chapterCalls(mark).map((c) => [c.args.book, c.args.chapter]), [[65, 1]], "Judas 1 loaded");
  eq($id("book-pick-name").textContent, "Judas", "book tile");
}, { allowMissingFixtures: true });

scenario("Edge: get_books fails → the 66 fallback books are still split by number (1–39 / 40–66)", () => {
  H.overrides.get_books = () => ({ err: "boom" });
  boot({ storage: onBiblia() });
  clickEl($id("btn-book-pick"));
  eq(tiles($id("books-ot")).length, 39, "AT");
  eq(tiles($id("books-nt")).length, 27, "NT");
  eq(Number(tiles($id("books-nt"))[0].dataset.book), 40, "NT starts at 40");
}, { allowMissingFixtures: true });

scenario("Biblia dropdown: every Bible listed, ✓ on the current; picking NVI loads it and the button follows", () => {
  boot({ storage: onBiblia() });
  const btn = trigger("sel-bible");
  eq(btn.querySelector(".pk-main").textContent, optionText("sel-bible", RVR), "button shows RVR1960");
  clickEl(btn);
  assert(menuOpen(), "menu open");
  eq(menu().dataset.for, "sel-bible", "for the Bible select");
  eq(btn.getAttribute("aria-expanded"), "true", "expanded");
  eq(menu().querySelectorAll(".gm-item").length, $id("sel-bible").options.length, "one row per Bible");
  const cur = menu().querySelectorAll(".gm-item.selected");
  eq(cur.length, 1, "one current row");
  assert(cur[0] === menuItemFor("sel-bible", RVR), "the current one is RVR1960");
  includes(cur[0].querySelector(".gm-check").textContent, "✓", "check mark");
  assert(menu().contains(document.activeElement), "focus inside the menu");
  const mark = H.calls.length;
  clickEl(menuItemFor("sel-bible", NVI));
  settle();
  eq(state.biblePath, NVI, "state");
  eq($id("sel-bible").value, NVI, "native select follows");
  eq(btn.querySelector(".pk-main").textContent, optionText("sel-bible", NVI), "button shows NVI");
  assert(!menuOpen(), "menu closed");
  eq(btn.getAttribute("aria-expanded"), "false", "collapsed");
  assert(document.activeElement === btn, "focus back on the button");
  eq(chapterCalls(mark).filter((c) => c.args.modulePath === NVI).length, 1, "NVI chapter loaded");
  eq(JSON.parse(localStorage.getItem(LS_KEYS.nav)).biblePath, NVI, "persisted");
  const mark2 = H.calls.length;
  clickEl(btn);
  clickEl(menuItemFor("sel-bible", NVI));
  settle();
  eq(chapterCalls(mark2).length, 0, "same Bible again: no reload");
});

scenario("Menu keys: filter + Enter picks the first match; ↓ moves through rows; Esc closes and refocuses the button", () => {
  boot({ storage: onBiblia() });
  const btn = trigger("sel-bible");
  clickEl(btn);
  const filter = $id("gm-filter");
  assert(filter, "the Bible list (more than 8) has a filter");
  {
    input(filter, "nvi");
    const shown = menu().querySelectorAll(".gm-item").filter((it) => !it.classList.contains("hidden"));
    assert(shown.length >= 1 && shown.length < $id("sel-bible").options.length, "the list is filtered");
    assert(shown.includes(menuItemFor("sel-bible", NVI)), "NVI among the matches");
    const want = $id("sel-bible").options[Number(shown[0].dataset.i)].value;
    key(filter, { key: "Enter" });
    settle();
    eq(state.biblePath, want, "Enter picks the first match");
    assert(!menuOpen(), "closed after Enter");
    clickEl(btn);
  }
  const first = document.activeElement;
  key(first, { key: "ArrowDown" });
  assert(document.activeElement !== first && menu().contains(document.activeElement), "↓ moves focus to a row");
  key(document.activeElement, { key: "Escape" });
  assert(!menuOpen(), "Esc closes");
  assert(document.activeElement === btn, "focus back on the button");
}, { allowMissingFixtures: true }); // the first «nvi» match (NVI.bbli) has no recorded chapter

scenario("Esc inside a panel closes only the dropdown first, then the panel", () => {
  boot({ storage: onBiblia() });
  clickEl($id("btn-crossrefs"));
  settle();
  assert(!$id("panel-crossrefs").classList.contains("hidden"), "cross-references open");
  clickEl(trigger("sel-crossref-source"));
  assert(menuOpen(), "source menu open");
  key(document.activeElement, { key: "Escape" });
  assert(!menuOpen(), "menu closed");
  assert(!$id("panel-crossrefs").classList.contains("hidden"), "panel still open");
  key(document.body, { key: "Escape" });
  assert($id("panel-crossrefs").classList.contains("hidden"), "second Esc closes the panel");
});

scenario("Comentario: with a dropdown open, ←/→ never step the verse", () => {
  boot({ storage: { nav: at(43, 3, 16), study: { workspace: "comentario", modulesByKind: { comentario: CMH } } } });
  clickEl(trigger("sel-study-module"));
  assert(menuOpen(), "module menu open");
  key(document.body, { key: "ArrowRight" });
  settle();
  eq(state.verse, 16, "→ while open: no verse step");
  key(document.body, { key: "ArrowDown", altKey: true });
  settle();
  eq(state.verse, 16, "Alt+↓ while open: no verse step");
  key(document.body, { key: "Escape" });
  assert(!menuOpen(), "closed");
  key(document.body, { key: "ArrowRight" });
  settle();
  eq(state.verse, 17, "with the menu closed → steps again");
}, { allowMissingFixtures: true });

scenario("Code-driven changes repaint the buttons: a history jump to another Bible updates the Biblia button", () => {
  const hist = [{ book: 43, chapter: 3, verse: 16, biblePath: NVI, label: "Juan 3:16", ts: 1 }];
  boot({ storage: onBiblia(at(43, 4, 1), { history: hist }) });
  eq($id("book-pick-name").textContent, "Juan", "book");
  eq($id("btn-chapter-pick").textContent, "4", "chapter 4");
  clickEl($id("btn-history"));
  clickEl($id("history-list").querySelector('.history-item[data-idx="0"]'));
  settle();
  eq([state.chapter, state.verse], [3, 16], "Juan 3:16");
  eq(state.biblePath, NVI, "NVI");
  eq(trigger("sel-bible").querySelector(".pk-main").textContent, optionText("sel-bible", NVI), "Biblia button repainted");
  eq($id("btn-chapter-pick").textContent, "3", "chapter bar repainted");
});

scenario("Paralela 1/2 buttons show the saved choices at boot (fillSelect paints them)", () => {
  boot({ storage: onBiblia(at(43, 3, 16), { parallel: { mode: "lectura", parallel1: NVI, parallel2: "" } }) });
  eq(trigger("sel-parallel1").querySelector(".pk-main").textContent, optionText("sel-parallel1", NVI), "P1 = NVI");
  eq(trigger("sel-parallel2").querySelector(".pk-main").textContent, "— Ninguna —", "P2 = none");
});

scenario("Switching to Comentario paints the module button from the module select", () => {
  boot({ storage: onBiblia(at(43, 3, 16), { study: { workspace: "biblia", modulesByKind: { comentario: CMH } } }) });
  clickEl(document.querySelector('.ws-tab[data-ws="comentario"]'));
  settle();
  eq($id("sel-study-module").value, CMH, "saved commentary selected");
  const sel = $id("sel-study-module");
  eq(trigger("sel-study-module").querySelector(".pk-main").textContent, sel.options[sel.selectedIndex].textContent.trim(), "button = selected option");
  includes(trigger("sel-study-module").querySelector(".pk-main").textContent, "Matthew Henry", "shows the commentary");
}, { allowMissingFixtures: true });

scenario("Two Bibles with the same name and title are told apart by file name in the menu", () => {
  boot({ storage: onBiblia() });
  clickEl(trigger("sel-bible"));
  const rows = menu().querySelectorAll(".gm-item").map((it) => it.querySelectorAll(".gm-main, .gm-sub").map((x) => x.textContent).join("|"));
  const opts = $id("sel-bible").options.map((o) => `${o.textContent.trim()}|${o.getAttribute("title") || ""}`);
  const dupOpts = opts.filter((x, i) => opts.indexOf(x) !== i);
  assert(dupOpts.length > 0, "this library has same-name Bibles (precondition)");
  eq(new Set(rows).size, rows.length, "every row reads differently");
  const i = opts.indexOf(dupOpts[0]);
  const file = menu().querySelector(`.gm-item[data-i="${i}"] .gm-file`);
  assert(file, "a file line on a duplicate");
  eq(file.textContent, $id("sel-bible").options[i].value.split("/").pop(), "it is the module's file name");
  const uniq = opts.findIndex((x) => opts.indexOf(x) === opts.lastIndexOf(x));
  eq(menu().querySelector(`.gm-item[data-i="${uniq}"] .gm-file`), null, "no file line on a unique Bible");
});

scenario("Race: the module list is re-sorted while its menu is open → the row clicked is the module chosen", () => {
  boot({ storage: { nav: at(43, 3, 16), study: { workspace: "comentario", modulesByKind: { comentario: CMH } } } });
  const sel = $id("sel-study-module");
  assert(sel.options.length >= 2, "at least two commentaries");
  clickEl(trigger("sel-study-module"));
  const other = sel.options.find((o) => o.value !== sel.value);
  const row = menu().querySelector(`.gm-item[data-v="${other.value}"]`) || menu().querySelectorAll(".gm-item").find((it) => it.getAttribute("data-v") === other.value);
  assert(row, "row for another commentary");
  // A late probe reorders the options under the open menu (reverse them).
  const opts = sel.options.map((o) => o.outerHTML).reverse().join("");
  const keep = sel.value;
  sel.innerHTML = opts;
  sel.value = keep;
  clickEl(row);
  settle();
  eq(state.studyModule, other.value, "the clicked commentary, not whatever now sits at that index");
}, { allowMissingFixtures: true });

scenario("Enter with nothing typed keeps the current Bible (no switch to the first row)", () => {
  boot({ storage: onBiblia() });
  const btn = trigger("sel-bible");
  clickEl(btn);
  assert(document.activeElement === $id("gm-filter"), "focus in the filter");
  const mark = H.calls.length;
  key($id("gm-filter"), { key: "Enter" });
  settle();
  eq(state.biblePath, RVR, "still RVR1960");
  eq(chapterCalls(mark).length, 0, "nothing reloaded");
  assert(!menuOpen(), "menu closed");
  assert(document.activeElement === btn, "focus back on the button");
  eq(JSON.parse(localStorage.getItem(LS_KEYS.nav)).biblePath, RVR, "saved choice unchanged");
});

scenario("Space on a row is not type-ahead; Tab closes and returns to the button", () => {
  boot({ storage: onBiblia() });
  const btn = trigger("sel-bible");
  clickEl(btn);
  key($id("gm-filter"), { key: "ArrowDown" });
  const row = document.activeElement;
  assert(row && row.classList.contains("gm-item"), "a row has focus");
  key(row, { key: " " });
  assert(document.activeElement === row, "Space leaves focus on the same row (WebKit then clicks it)");
  const ev = makeEvent("keydown", { key: "Tab" });
  dispatch(row, ev);
  assert(ev.defaultPrevented, "Tab handled");
  assert(!menuOpen(), "menu closed");
  assert(document.activeElement === btn, "focus on the button, not lost in a removed row");
});

scenario("Second click on the button closes; a press outside closes; a press inside does not", () => {
  boot({ storage: onBiblia() });
  const btn = trigger("sel-bible");
  clickEl(btn);
  assert(menuOpen(), "open");
  clickEl(btn);
  assert(!menuOpen(), "second click closes");
  clickEl(btn);
  dispatch(menu().querySelector(".gm-item"), makeEvent("mousedown"));
  assert(menuOpen(), "press inside keeps it");
  dispatch($id("bible-content"), makeEvent("mousedown"));
  assert(!menuOpen(), "press outside closes");
  clickEl(btn);
  clickEl($id("btn-book-pick"));
  assert(!menuOpen(), "opening a panel closes the menu");
  assert(panelOpen(), "books panel open");
});

scenario("A rebuilt select under an open menu redraws the rows and keeps the filter text", () => {
  boot({ storage: onBiblia() });
  clickEl(trigger("sel-bible"));
  input($id("gm-filter"), "nvi");
  const bibles = modulesOf("bible").slice().reverse();
  fillSelect($id("sel-bible"), bibles, "path", (m) => m.abbreviation, state.biblePath);
  assert(menuOpen(), "still open");
  eq($id("gm-filter").value, "nvi", "filter text kept");
  eq(menu().querySelectorAll(".gm-item")[0].getAttribute("data-v"), bibles[0].path, "rows follow the new order");
  const shown = menu().querySelectorAll(".gm-item").filter((it) => !it.classList.contains("hidden"));
  assert(shown.length >= 1 && shown.length < bibles.length, "filter re-applied");
  assert(menu().classList.contains("gm-redraw"), "a redraw does not replay the open animation");
  key($id("gm-filter"), { key: "ArrowDown" });
  key(document.activeElement, { key: "ArrowDown" });
  const focusedV = document.activeElement.getAttribute("data-v");
  fillSelect($id("sel-bible"), bibles.slice().reverse(), "path", (m) => m.abbreviation, state.biblePath);
  eq(document.activeElement && document.activeElement.getAttribute("data-v"), focusedV, "keyboard focus stays on the same Bible");
  key(document.activeElement, { key: "Escape" });
  clickEl(trigger("sel-bible"));
  assert(!menu().classList.contains("gm-redraw"), "a fresh open animates again");
});

scenario("Paralela 1 picked from its menu: state, saved, button", () => {
  boot({ storage: onBiblia() });
  clickEl(document.querySelector('.mode-btn[data-mode="paralela"]'));
  settle();
  eq(document.querySelector(".mode-toggle").getAttribute("data-mode"), "paralela", "toggle carries the mode (sliding highlight)");
  clickEl(trigger("sel-parallel1"));
  clickEl(menuItemFor("sel-parallel1", NVI));
  settle();
  eq(state.parallel1, NVI, "state");
  eq(JSON.parse(localStorage.getItem(LS_KEYS.parallel)).parallel1, NVI, "saved");
  eq(trigger("sel-parallel1").querySelector(".pk-main").textContent, optionText("sel-parallel1", NVI), "button");
  includes(trigger("sel-parallel1").getAttribute("aria-label"), "Paralela 1: ", "announces label and value");
}, { allowMissingFixtures: true });

scenario("Apocalipsis 22: › is disabled; the dropdown gives focus back to the Libro pill", () => {
  H.overrides.get_bible_chapter = (a) =>
    a.book === 66 ? { ok: { abbreviation: "RVR1960", book: "66", chapter: "22", module: a.modulePath, title: "", verses: [{ verse: 1, html: "Después", plain: "Después" }] } } : undefined;
  boot({ storage: onBiblia() });
  clickEl($id("btn-book-pick"));
  clickEl($id("books-body").querySelector('.book-tile[data-book="66"]'));
  clickEl($id("books-body").querySelector('.num-tile[data-ch="22"]'));
  settle();
  eq([state.book, state.chapter], [66, 22], "Apocalipsis 22");
  assert($id("btn-next-ch").disabled, "› disabled at the last chapter");
  assert(!$id("btn-prev-ch").disabled, "‹ enabled");
  assert(document.activeElement === $id("btn-book-pick"), "focus back on the Libro button");
}, { allowMissingFixtures: true });

scenario("Render scrolls (reader, study content) never close an open menu; a scroll that moves its button does", () => {
  boot({ storage: onBiblia() });
  clickEl(trigger("sel-bible"));
  dispatch($id("bible-content"), makeEvent("scroll"));
  dispatch($id("study-content"), makeEvent("scroll"));
  assert(menuOpen(), "reader / study scrolls keep it open");
  dispatch(menu().querySelector(".gm-list"), makeEvent("scroll"));
  assert(menuOpen(), "scrolling the list itself keeps it open");
  dispatch(document.querySelector(".sidebar"), makeEvent("scroll"));
  assert(!menuOpen(), "the sidebar (which holds the button) scrolled → closed");
});

scenario("Enter in the empty book filter does nothing (no jump to Génesis)", () => {
  boot({ storage: onBiblia() });
  clickEl($id("btn-book-pick"));
  key($id("book-filter"), { key: "Enter" });
  settle();
  eq($id("panel-books").dataset.view, "books", "still the book tiles");
  eq(state.book, 43, "nowhere navigated");
});

// Dropdowns, not centered screens (2026-11-19): each picker opens right under its control.
const placeAt = (el, left, top, w, h) =>
  (el.getBoundingClientRect = () => ({ left, top, right: left + w, bottom: top + h, width: w, height: h, x: left, y: top }));

scenario("Books drop down under the Libro pill, chapters under the Capítulo bar, verses under the Vers. row; a click outside closes", () => {
  boot({ storage: onBiblia() });
  placeAt($id("btn-book-pick"), 22, 120, 216, 48);
  placeAt(document.querySelector(".stepper-bar"), 22, 210, 150, 40);
  placeAt(document.querySelector(".nav-row"), 22, 186, 216, 64); // the Capítulo / Vers. row
  const books = $id("panel-books");
  clickEl($id("btn-book-pick"));
  assert(books.classList.contains("anchored"), "books: a dropdown");
  assert($id("overlay").classList.contains("clear"), "clear overlay: nothing dims");
  eq([books.style.left, books.style.top, books.style.maxHeight], ["22px", "174px", "714px"], "right under the Libro pill, as tall as the room below");
  clickEl($id("overlay"));
  assert(!panelOpen(), "a click outside closes it");
  clickEl($id("btn-chapter-pick"));
  eq(books.dataset.view, "chapters", "chapter pills");
  assert(books.classList.contains("anchored"), "chapters: a dropdown");
  eq([books.style.left, books.style.top], ["22px", "256px"], "right under the Capítulo bar");
  key(document.body, { key: "Escape" });
  assert(!panelOpen(), "Esc closes it");
  clickEl($id("btn-verse-pick"));
  settle();
  const verses = $id("panel-verse-picker");
  assert(verses.classList.contains("anchored"), "verses: a dropdown");
  eq([verses.style.left, verses.style.top], ["22px", "256px"], "right under the Capítulo / Vers. row, aligned with the others");
  clickEl(verses.querySelector('button[data-v="17"]'));
  settle();
  eq(state.verse, 17, "a verse pill still picks the verse");
});

scenario("No room below → the dropdown opens upward; centered panels (Ajustes) stay centered with their dim overlay", () => {
  boot({ storage: onBiblia() });
  placeAt($id("btn-location-chip"), 500, 820, 200, 40); // window is 900 tall in the harness
  clickEl($id("btn-location-chip"));
  settle();
  const verses = $id("panel-verse-picker");
  assert(verses.classList.contains("anchored"), "the reader's location chip drops the verses down too");
  eq([verses.style.top, verses.style.bottom], ["auto", "86px"], "above the chip, since below there is no room");
  key(document.body, { key: "Escape" });
  clickEl($id("btn-settings"));
  const settings = $id("panel-settings");
  assert(!settings.classList.contains("anchored"), "Ajustes is not a dropdown");
  assert(!$id("overlay").classList.contains("clear"), "its overlay dims as before");
  eq([settings.style.top || "", settings.style.left || ""], ["", ""], "no dropdown position on it");
  key(document.body, { key: "Escape" });
  openPanel("#panel-verse-picker"); // the same panel opened centered after being a dropdown
  assert(!verses.classList.contains("anchored"), "centered again");
  eq([verses.style.top, verses.style.bottom, verses.style.left, verses.style.maxHeight], ["", "", "", ""], "no leftover dropdown position");
});
