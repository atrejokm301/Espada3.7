// Review round 1: under prefers-reduced-motion the global rule turns every animation into a
// 0.01ms one; for the infinite .is-refreshing spinner that reads as a jittery/frozen ring. The
// spinner must be explicitly still (animation: none) in a reduced-motion block that comes after
// the spinning rule (same specificity, so source order decides).

function cssBlocks(css) {
  // [{ media, selector, body, at }] for plain rules and rules nested one level in @media.
  const out = [];
  const re = /@media\s*([^{]+)\{((?:[^{}]*\{[^{}]*\})*)\s*\}|([^{}@]+)\{([^{}]*)\}/g;
  for (const m of css.matchAll(re)) {
    if (m[1]) {
      for (const r of m[2].matchAll(/([^{}]+)\{([^{}]*)\}/g)) out.push({ media: m[1].trim(), selector: r[1].trim(), body: r[2], at: m.index });
    } else out.push({ media: "", selector: m[3].trim(), body: m[4], at: m.index });
  }
  return out;
}

scenario("Reduced motion: the refresh spinner is still, and that rule wins by source order", () => {
  const css = readFile(`${UI_DIR}/styles.v7.css`).replace(/\/\*[\s\S]*?\*\//g, "");
  const blocks = cssBlocks(css);
  const sel = ".study-content.is-refreshing::before";
  const spin = blocks.filter((b) => !b.media && b.selector === sel && /animation\s*:[^;]*study-refresh-spin/.test(b.body));
  eq(spin.length, 1, "one spinning rule");
  const still = blocks.filter((b) => /prefers-reduced-motion:\s*reduce/.test(b.media) && b.selector.split(",").map((x) => x.trim()).includes(sel));
  assert(still.length >= 1, "a reduced-motion rule for the spinner exists");
  assert(still.some((b) => /animation\s*:\s*none/.test(b.body) && b.at > spin[0].at), "animation: none, after the spinning rule");
});

scenario("The page still loads with the stylesheet (smoke)", () => {
  boot({});
  assert($id("study-content"), "study pane present");
});
