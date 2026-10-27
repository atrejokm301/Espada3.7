/**
 * Espada — reading theme helpers (Tauri / web)
 *
 * Prefer token themes (data-theme="warm-paper") over filter:sepia() on <body>.
 * Full-page filters force extra compositing and tint chrome/buttons.
 *
 * Persistence key matches app.js: localStorage "espada.theme"
 */
(function (global) {
  const STORAGE_KEY = "espada.theme";
  const DEFAULT_THEME = "true-dark";

  /**
   * Apply a theme id to <html> + <body> and persist.
   * @param {string} id
   * @param {{ persist?: boolean }} [opts]
   */
  function applyReadingTheme(id, opts) {
    const persist = opts?.persist !== false;
    const theme = id || DEFAULT_THEME;
    document.documentElement.dataset.theme = theme;
    document.body.dataset.theme = theme;
    if (persist) {
      try {
        localStorage.setItem(STORAGE_KEY, theme);
      } catch (_) {
        /* private mode */
      }
    }
    // Optional: tell assistive tech / form controls about scheme
    const dark = theme === "true-dark" || theme === "frosted-glass";
    document.documentElement.style.colorScheme = dark ? "dark" : "light";
    return theme;
  }

  function loadSavedReadingTheme() {
    let saved = DEFAULT_THEME;
    try {
      saved = localStorage.getItem(STORAGE_KEY) || DEFAULT_THEME;
    } catch (_) {
      /* ignore */
    }
    return applyReadingTheme(saved, { persist: false });
  }

  /**
   * OPTIONAL micro-warmth on a single element (reader pane only).
   * Uses a cheap overlay, NOT filter on the whole tree.
   * Prefer the warm-paper token theme instead when possible.
   * @param {HTMLElement | null} el
   * @param {boolean} on
   */
  function setReaderWarmOverlay(el, on) {
    if (!el) return;
    el.style.boxShadow = on
      ? "inset 0 0 120px rgba(180, 140, 90, 0.08)"
      : "";
    el.style.backgroundColor = on ? "var(--reader-bg)" : "";
  }

  global.EspadaReadingTheme = {
    STORAGE_KEY,
    applyReadingTheme,
    loadSavedReadingTheme,
    setReaderWarmOverlay,
  };
})(typeof window !== "undefined" ? window : globalThis);
