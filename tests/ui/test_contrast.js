// ISS-33 — WCAG AA (≥ 4.5:1) for text tokens in every theme except Frosted Glass (its contrast
// depends on the desktop behind the window), measured on the reader, glass and card surfaces.
// Also: tappable words look tappable; the module selector has an accessible name.

function parseColor(v) {
  v = String(v).trim();
  let m = v.match(/^#([0-9a-f]{6})$/i);
  if (m) return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16)).concat(1);
  m = v.match(/^rgba?\(([^)]*)\)$/);
  if (m) {
    const p = m[1].split(",").map(Number);
    return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1];
  }
  if (v === "transparent") return [0, 0, 0, 0];
  return null;
}
const over = (fg, bg) => [0, 1, 2].map((i) => fg[i] * fg[3] + bg[i] * (1 - fg[3])).concat(1);
function lum(c) {
  const ch = (x) => ((x /= 255) <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4));
  return 0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2]);
}
const ratio = (a, b) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
};
function themes() {
  const css = readFile(`${UI_DIR}/styles.v7.css`);
  const rootBlock = css.slice(css.indexOf(":root"), css.indexOf("}", css.indexOf(":root")));
  const vars = (block) => Object.fromEntries([...block.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
  const root = vars(rootBlock);
  const out = {};
  for (const m of css.matchAll(/body\[data-theme="([\w-]+)"\]\s*\{([^}]*)\}/g)) {
    if (!m[2].includes("--bg-base")) continue;
    out[m[1]] = Object.assign({ "--link": "var(--accent)" }, root, vars(m[2]));
  }
  return { css, out };
}

scenario("Every theme (except Frosted Glass): text, dim, muted, words of Christ and links ≥ 4.5:1", () => {
  const { out } = themes();
  eq(Object.keys(out).length, 9, "nine themes parsed");
  const fails = [];
  for (const [name, v] of Object.entries(out)) {
    if (name === "frosted-glass") continue;
    const resolve = (tok) => {
      let x = v[tok];
      for (let i = 0; i < 3 && /^var\(/.test(x); i++) x = v[x.slice(4, -1).trim()];
      return parseColor(x);
    };
    let base = resolve("--bg-base");
    const reader = over(resolve("--reader-bg"), base);
    const glass = over(resolve("--glass"), base);
    const card = over(resolve("--glass-soft"), reader);
    for (const tok of ["--text", "--text-dim", "--text-muted", "--woc", "--link"]) {
      const c = resolve(tok);
      assert(c, `${name} ${tok} parsed`);
      const worst = Math.min(...[reader, glass, card].map((bg) => ratio(over(c, bg), bg)));
      if (worst < 4.5) fails.push(`${name} ${tok} ${worst.toFixed(2)}`);
    }
  }
  eq(fails, [], "contrast failures");
});

scenario("Muted text stays quieter than dim text (hierarchy kept)", () => {
  const { out } = themes();
  for (const [name, v] of Object.entries(out)) {
    const a = (tok) => (parseColor(v[tok]) || [0, 0, 0, 1])[3];
    assert(a("--text-muted") <= a("--text-dim"), `${name}: muted ${a("--text-muted")} > dim ${a("--text-dim")}`);
  }
});

scenario("Tappable words: pointer cursor + hover tint; links use the --link token", () => {
  const { css } = themes();
  assert(/\.verse-body \.word \{[^}]*cursor:\s*pointer/.test(css), ".word has cursor:pointer");
  assert(/\.verse-body \.word:hover \{[^}]*background/.test(css), ".word has a hover background");
  assert(/\.bible-ref,\s*\.strong-ref,\s*\.hebrew-run,\s*\.greek-run \{[^}]*color:\s*var\(--link\)/.test(css), "links use --link");
  assert(/body \{\s*--link:\s*var\(--accent\);/.test(css), "--link defaults to the theme accent on body");
});

scenario("The study module selector and the verse stepper have accessible names", () => {
  boot({ storage: {} });
  eq($id("sel-study-module").getAttribute("aria-label"), "Módulo de estudio", "selector label");
  eq($id("btn-verse-prev").getAttribute("aria-label"), "Versículo anterior", "‹ label");
  eq($id("btn-verse-next").getAttribute("aria-label"), "Versículo siguiente", "› label");
});
