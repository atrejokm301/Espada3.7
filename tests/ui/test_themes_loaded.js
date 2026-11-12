// ISS-34 — index.html has no inline script (CSP script-src 'self'); themes.js defines the theme
// list before app.v7.js runs; a CSP meta tag covers `cargo tauri dev`.

scenario("index.html: no inline script content; themes.js loads before app.v7.js", () => {
  const html = readFile(`${UI_DIR}/index.html`);
  const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)];
  assert(scripts.length >= 2, "scripts found");
  for (const m of scripts) {
    assert(/\ssrc=/.test(m[1]), `inline <script> without src: ${m[0].slice(0, 80)}`);
    eq(m[2].trim(), "", "script tag has no inline body");
  }
  const srcs = scripts.map((m) => (m[1].match(/src="([^"?]+)/) || [])[1]);
  assert(srcs.indexOf("themes.js") >= 0 && srcs.indexOf("themes.js") < srcs.indexOf("app.v7.js"), `order: ${srcs.join(", ")}`);
});

scenario("CSP meta: script-src 'self' without 'unsafe-inline'; styles, images, IPC still allowed", () => {
  const html = readFile(`${UI_DIR}/index.html`);
  const m = html.match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/);
  assert(m, "CSP meta present");
  const csp = Object.fromEntries(m[1].split(";").map((d) => d.trim().split(/\s+/)).map(([k, ...v]) => [k, v]));
  eq(csp["script-src"], ["'self'"], "script-src");
  assert(csp["style-src"].includes("'unsafe-inline'"), "inline style attributes still allowed");
  assert(csp["img-src"].includes("data:"), "data: images (CSS chevrons)");
  assert(csp["connect-src"].includes("ipc:") && csp["connect-src"].includes("http://ipc.localhost"), "Tauri IPC");
  eq(csp["object-src"], ["'none'"], "no plugins");
  eq(csp["base-uri"], ["'none'"], "no <base> hijack");
});

scenario("The app boots with the theme list from themes.js (9 themes in the picker)", () => {
  boot({ storage: {} });
  assert(Array.isArray(window.ESPADA_THEMES) && window.ESPADA_THEMES.length === 9, "ESPADA_THEMES defined");
  eq(THEMES, window.ESPADA_THEMES, "app uses the themes.js list");
  clickEl($id("btn-theme"));
  eq($id("theme-grid").querySelectorAll(".theme-card").length, 9, "picker lists 9 themes");
  clickEl($id("theme-grid").querySelector('.theme-card[data-theme="warm-paper"]'));
  settle();
  eq(document.body.dataset.theme, "warm-paper", "theme applied");
});
