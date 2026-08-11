import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

// Señales visibles de que el bundle corrió
try {
  document.title = "ADC · montando…";
  // @ts-expect-error boot helper
  window.__adcSetBoot?.("main.tsx: montando React…");
} catch {
  /* ignore */
}

function showFatal(err: unknown) {
  const e = err instanceof Error ? err : new Error(String(err));
  const root = document.getElementById("root");
  if (!root) return;
  root.innerHTML = `
    <div style="padding:24px;font-family:Segoe UI,system-ui,sans-serif;background:#020617;color:#f87171;height:100%;box-sizing:border-box;overflow:auto">
      <h1 style="color:#fbbf24;margin-top:0">Error al iniciar</h1>
      <pre style="color:#e2e8f0;white-space:pre-wrap;font-size:13px">${e.message}\n\n${e.stack ?? ""}</pre>
    </div>
  `;
  document.title = "ADC · error";
}

try {
  const rootEl = document.getElementById("root");
  if (!rootEl) {
    showFatal(new Error("No se encontró #root"));
  } else {
    ReactDOM.createRoot(rootEl).render(
      <React.StrictMode>
        <App />
      </React.StrictMode>,
    );
    document.title = "Espada 3.7 (Windows)";
  }
} catch (err) {
  showFatal(err);
}
