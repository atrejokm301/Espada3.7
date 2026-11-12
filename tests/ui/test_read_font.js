// Reading font (🎨 panel → «Fuente de lectura»): Sistema, the four EspadaSwift fonts (Google Sans
// Flex, Roboto, Inter, Roboto Flex) and OpenDyslexic / Atkinson / Lexend. The choice is a
// data-read-font attribute on <html> (absent = Sistema), saved in localStorage "espada37.readFont";
// styles.v7.css maps each id to bundled ui/fonts/ faces. Greek + Hebrew always use Libertinus Serif.

const FONT_KEY = "espada37.readFont";
const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NAV = { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } };
const GH = "Libertinus Griego Hebreo"; // Libertinus Serif limited to Greek + Hebrew (unicode-range)

function fontCards() {
  return [...$id("font-grid").querySelectorAll(".font-card")];
}
function card(id) {
  return $id("font-grid").querySelector(`.font-card[data-font="${id}"]`);
}
function readFontAttr() {
  return document.documentElement.getAttribute("data-read-font");
}
function pressed() {
  return fontCards()
    .filter((c) => c.getAttribute("aria-pressed") === "true")
    .map((c) => c.dataset.font);
}
function cssNoComments() {
  return readFile(`${UI_DIR}/styles.v7.css`).replace(/\/\*[\s\S]*?\*\//g, "");
}

scenario("First launch: eight font cards (iOS ones first), «Sistema» selected, no data-read-font", () => {
  boot({ storage: NAV });
  eq(
    fontCards().map((c) => c.dataset.font),
    ["system", "googlesans", "roboto", "inter", "robotoflex", "opendyslexic", "atkinson", "lexend"],
    "cards in order"
  );
  // Same labels as EspadaSwift's ReadingFontFamily.label
  for (const [id, label] of [["googlesans", "Google Sans Flex"], ["roboto", "Roboto"], ["inter", "Inter"], ["robotoflex", "Roboto Flex"], ["opendyslexic", "OpenDyslexic"]]) {
    includes(card(id).textContent, label, `${id} label`);
  }
  eq(pressed(), ["system"], "only Sistema pressed");
  assert(card("system").classList.contains("active"), "Sistema active");
  eq(readFontAttr(), null, "no attribute = the app's normal font");
});

scenario("Click OpenDyslexic → <html data-read-font>, saved, only that card pressed", () => {
  boot({ storage: NAV });
  clickEl(card("opendyslexic").querySelector(".font-sample")); // a click on the inner «Aa» bubbles up
  eq(readFontAttr(), "opendyslexic", "attribute set");
  eq(localStorage.getItem(FONT_KEY), "opendyslexic", "saved");
  eq(pressed(), ["opendyslexic"], "pressed state moved");
  assert(!card("system").classList.contains("active"), "Sistema no longer active");
});

scenario("Saved «lexend» is applied at launch, before the user opens 🎨", () => {
  boot({ storage: { ...NAV, [FONT_KEY]: "lexend" } });
  eq(readFontAttr(), "lexend", "applied at init");
  eq(pressed(), ["lexend"], "card shows it");
});

scenario("Unknown saved font (old / hand-edited value) → Sistema, attribute absent", () => {
  boot({ storage: { ...NAV, [FONT_KEY]: "comic-sans\" onload=\"x" } });
  eq(readFontAttr(), null, "never written to the DOM");
  eq(pressed(), ["system"], "Sistema selected");
  eq(localStorage.getItem(FONT_KEY), "system", "bad value replaced");
});

scenario("Back to Sistema removes the attribute (the default look is restored exactly)", () => {
  boot({ storage: { ...NAV, [FONT_KEY]: "atkinson" } });
  eq(readFontAttr(), "atkinson", "precondition");
  clickEl(card("system"));
  eq(readFontAttr(), null, "attribute removed");
  eq(localStorage.getItem(FONT_KEY), "system", "saved");
});

scenario("Font and theme are independent: one never resets the other", () => {
  boot({ storage: { ...NAV, theme: "warm-paper", [FONT_KEY]: "opendyslexic" } });
  eq(document.body.dataset.theme, "warm-paper", "theme restored");
  eq(readFontAttr(), "opendyslexic", "font restored");
  clickEl($id("btn-theme")); // re-renders the theme grid
  clickEl($id("theme-grid").querySelector('.theme-card[data-theme="plain-white"]'));
  eq(readFontAttr(), "opendyslexic", "theme change kept the font");
  clickEl(card("lexend"));
  eq(document.body.dataset.theme, "plain-white", "font change kept the theme");
});

scenario("A click on the grid's gap (no card) changes nothing", () => {
  boot({ storage: { ...NAV, [FONT_KEY]: "lexend" } });
  clickEl($id("font-grid"));
  eq(readFontAttr(), "lexend", "unchanged");
  eq(localStorage.getItem(FONT_KEY), "lexend", "storage unchanged");
});

scenario("localStorage.setItem throwing (storage full) → font still applied, no crash", () => {
  boot({ storage: NAV });
  const orig = localStorage.setItem;
  localStorage.setItem = (k, v) => {
    if (k === FONT_KEY) throw new Error("QuotaExceededError");
    return orig(k, v);
  };
  try {
    clickEl(card("atkinson"));
  } finally {
    localStorage.setItem = orig;
  }
  eq(readFontAttr(), "atkinson", "applied for this session");
  eq(pressed(), ["atkinson"], "card updated");
});

scenario("CSS: every non-system font id has a --font-read rule (Libertinus first), a card sample, and @font-face", () => {
  boot({ storage: NAV }); // loads app.v7.js → READ_FONTS
  const css = cssNoComments();
  const faces = [...css.matchAll(/@font-face\s*\{([^{}]*)\}/g)].map((m) => m[1]);
  const families = new Set(faces.map((b) => (b.match(/font-family\s*:\s*"([^"]+)"/) || [])[1]));
  for (const f of READ_FONTS.filter((x) => x.id !== "system")) {
    const rule = css.match(new RegExp(`html\\[data-read-font="${f.id}"\\]\\s*\\{([^{}]*)\\}`));
    assert(rule, `html[data-read-font="${f.id}"] rule`);
    const decl = (rule[1].match(/--font-read\s*:([^;]*)/) || [])[1] || "";
    const quoted = [...decl.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
    eq(quoted[0], GH, `${f.id}: Greek/Hebrew → Libertinus before the Latin font`);
    const fam = quoted[1];
    assert(fam, `${f.id} sets --font-read to a quoted Latin family`);
    assert(families.has(fam), `${f.id}: an @font-face declares «${fam}»`);
    const sample = css.match(new RegExp(`\\.font-card\\[data-font="${f.id}"\\] \\.font-sample\\s*\\{([^{}]*)\\}`));
    assert(sample && sample[1].includes(`"${fam}"`), `${f.id}: card sample drawn in «${fam}»`);
  }
});

scenario("CSS: the reading panes use --font-read with the app font as fallback", () => {
  const css = cssNoComments();
  const m = [...css.matchAll(/([^{}@]+)\{([^{}]*)\}/g)].find((r) => /font-family\s*:\s*var\(--font-read,\s*var\(--font\)\)/.test(r[2]) && r[1].includes(".bible-content"));
  assert(m, "a rule sets the reading containers' font-family");
  const sels = m[1].split(",").map((s) => s.trim());
  for (const s of [".bible-content", ".study-content", ".link-panel-inline", ".notes-area", ".tooltip"]) {
    assert(sels.includes(s), `${s} follows the reading font`);
  }
  assert(!/^\s*:root\s*\{[^}]*--font-read\s*:/m.test(css), "--font-read is NOT set by default (Sistema = unchanged look)");
});

scenario("Bundled font files exist, are real fonts, and their OFL licenses ship with them", () => {
  const css = cssNoComments();
  const urls = [...css.matchAll(/url\("(fonts\/[^"]+)"\)/g)].map((m) => m[1]);
  eq(urls.length, 20, "20 font faces (13 iOS files + 7 readability faces)");
  for (const u of urls) {
    let bytes;
    try {
      bytes = readFile(`${UI_DIR}/${u}`, "binary");
    } catch (e) {
      throw new AssertionError(`missing ${u}`);
    }
    assert(bytes.length > 20000, `${u} is not a stub (${bytes.length} bytes)`);
    const magic = String.fromCharCode(bytes[0], bytes[1], bytes[2], bytes[3]);
    const ok = u.endsWith(".woff2") ? magic === "wOF2" : bytes[0] === 0 && bytes[1] === 1 && bytes[2] === 0 && bytes[3] === 0;
    assert(ok, `${u} has the right font signature`);
  }
  for (const lic of ["OFL-OpenDyslexic.txt", "OFL-AtkinsonHyperlegibleNext.txt", "OFL-Lexend.txt", "OFL-GoogleSansFlex.txt", "OFL-Inter.txt", "OFL-RobotoFlex.txt", "OFL-LibertinusSerif.txt"]) {
    includes(readFile(`${UI_DIR}/fonts/${lic}`), "SIL Open Font License", lic);
  }
  includes(readFile(`${UI_DIR}/fonts/LICENSE-Roboto-Apache-2.0.txt`), "Apache License", "Roboto (v2.001047) is Apache 2.0");
});

scenario("Greek + Hebrew: Libertinus Serif everywhere (iOS ranges), Latin never drawn with it", () => {
  const css = cssNoComments();
  const face = [...css.matchAll(/@font-face\s*\{([^{}]*)\}/g)].map((m) => m[1]).find((b) => b.includes(`"${GH}"`));
  assert(face, "a Libertinus Griego Hebreo @font-face");
  includes(face, 'url("fonts/LibertinusSerif-Regular.ttf")', "the iOS Libertinus file");
  const range = ((face.match(/unicode-range\s*:([^;]*)/) || [])[1] || "").replace(/\s+/g, "");
  // Exactly EspadaSwift's ScriptText.isHebrewOrGreek blocks.
  eq(range.split(",").sort(), ["U+0370-03FF", "U+0590-05FF", "U+1F00-1FFF", "U+FB1D-FB4F"], "unicode-range");
  // App-wide default (chrome + "Sistema"): Libertinus first in --font.
  const root = css.match(/:root\s*\{[^}]*?--font\s*:\s*([^;]*);/);
  assert(root && root[1].trim().startsWith(`"${GH}"`), "--font starts with Libertinus Griego Hebreo");
  // Original-language words (lexicon lemma, interlinear, Strong tooltip): Libertinus first, not Times.
  for (const sel of [".lex-original", ".ctx-il-orig", ".il-unit .il-o", ".tooltip .tip-orig"]) {
    const esc = sel.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const r = css.match(new RegExp(`(?:^|\\})\\s*${esc}\\s*\\{([^{}]*)\\}`));
    assert(r, `${sel} rule`);
    const fam = (r[1].match(/font-family\s*:\s*([^;]*)/) || [])[1] || "";
    assert(fam.trim().startsWith(`"${GH}"`), `${sel}: Libertinus first (got ${fam.trim()})`);
  }
});
