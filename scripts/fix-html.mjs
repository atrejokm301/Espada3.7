import { readFileSync, writeFileSync, existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

/**
 * Produce a self-contained index.html:
 * - CSS inline
 * - app.js as classic IIFE, with </script> escaped, try/catch wrapper
 * No external asset loads (WebView/Tauri custom protocol is unreliable for them).
 */
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");
const htmlPath = join(dist, "index.html");
const jsPath = join(dist, "assets", "app.js");
const cssPath = join(dist, "assets", "style.css");

if (!existsSync(htmlPath) || !existsSync(jsPath)) {
  console.error("Missing dist files");
  process.exit(1);
}

let html = readFileSync(htmlPath, "utf8");
let js = readFileSync(jsPath, "utf8");
const css = existsSync(cssPath) ? readFileSync(cssPath, "utf8") : "";

// Escape sequences that break HTML script embedding
js = js.replace(/<\/script/gi, "<\\/script");

html = html.replace(/<link[^>]*href="[^"]*style\.css"[^>]*>\s*/gi, "");
html = html.replace(/<script[^>]*src="[^"]*app\.js"[^>]*>\s*<\/script>\s*/gi, "");
html = html.replace(/\s+crossorigin(="[^"]*")?/g, "");

if (css && html.includes("</head>")) {
  html = html.replace(
    "</head>",
    `<style id="adc-css">\n${css.replace(/<\/style/gi, "<\\/style")}\n</style>\n</head>`,
  );
}

const payload = `
<script>
(function(){
  function msg(t, err){
    try {
      var m = document.getElementById("boot-msg");
      if (m) { m.textContent = t; if (err) m.style.color = "#f87171"; }
      if (window.__adcSetBoot) window.__adcSetBoot(t);
    } catch(e) {}
  }
  msg("Ejecutando bundle embebido…");
  try {
${js}
    msg("Bundle ejecutado · montando React…");
  } catch (e) {
    msg("Error en bundle: " + (e && e.message ? e.message : e), true);
    console.error(e);
  }
})();
</script>
<script>
setTimeout(function(){
  var boot = document.getElementById("boot");
  if (boot) {
    var m = document.getElementById("boot-msg");
    if (m && m.style.color !== "rgb(248, 113, 113)") {
      m.style.color = "#f87171";
      m.textContent = "Bundle corrió pero la UI no reemplazó #boot. Error de React.";
    }
  }
}, 4000);
</script>
`;

if (html.includes("</body>")) {
  html = html.replace("</body>", payload + "\n</body>");
} else {
  html += payload;
}

writeFileSync(htmlPath, html);
console.log(`OK: self-contained index.html (${Math.round(html.length / 1024)} KB)`);
