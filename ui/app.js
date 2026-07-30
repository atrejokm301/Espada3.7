(function () {
  "use strict";

  var bootMsg = document.getElementById("boot-msg");
  var bootHint = document.getElementById("boot-hint");
  function setBoot(t, err) {
    window.__ADC_BOOT_STARTED__ = true;
    if (!bootMsg) bootMsg = document.getElementById("boot-msg");
    if (!bootMsg) return;
    bootMsg.textContent = t;
    bootMsg.style.color = err ? "#f87171" : "";
    if (err && bootHint && !bootHint.textContent) {
      bootHint.textContent = "Si el servidor murió: Start-ADC-Stable.bat";
    }
  }

  /** Resolve HTTP API base for adc-api (WinUI inject, Edge app, or same-origin). */
  function getApiBase() {
    if (window.__ADC_API_BASE__) {
      return String(window.__ADC_API_BASE__).replace(/\/$/, "");
    }
    // Served by adc-api itself (http://127.0.0.1:17865/)
    try {
      if (location.protocol === "http:" || location.protocol === "https:") {
        if (
          location.hostname === "127.0.0.1" ||
          location.hostname === "localhost" ||
          location.port === "17865"
        ) {
          return location.origin;
        }
      }
    } catch (e) {}
    return null;
  }

  function getInvoke() {
    // 1) Tauri desktop host
    if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
      return window.__TAURI__.core.invoke.bind(window.__TAURI__.core);
    }
    if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke) {
      return window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
    }
    // 2) HTTP sidecar (WinUI / Edge stable / same-origin)
    var base = getApiBase();
    if (base) {
      return function adcInvoke(cmd, args) {
        var ctrl = typeof AbortController !== "undefined" ? new AbortController() : null;
        var timer = null;
        if (ctrl) {
          timer = setTimeout(function () {
            try { ctrl.abort(); } catch (e) {}
          }, 60000);
        }
        return fetch(base + "/invoke", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ cmd: cmd, args: args || {} }),
          signal: ctrl ? ctrl.signal : undefined
        }).then(function (res) {
          if (timer) clearTimeout(timer);
          return res.json().then(function (body) {
            if (!res.ok) {
              throw new Error((body && body.error) || ("HTTP " + res.status));
            }
            if (body && body.ok === false) {
              throw new Error(body.error || "Error del API");
            }
            return body && Object.prototype.hasOwnProperty.call(body, "result")
              ? body.result
              : body;
          });
        }).catch(function (e) {
          if (timer) clearTimeout(timer);
          throw e;
        });
      };
    }
    return null;
  }

  function invoke(cmd, args) {
    var inv = getInvoke();
    if (!inv) {
      return Promise.reject(new Error(
        "No hay API. Ejecutá: .\\scripts\\run-stable.ps1  (o Tauri / WinUI)."
      ));
    }
    try {
      return Promise.resolve(inv(cmd, args || {})).catch(function (e) {
        var msg = e && e.message ? e.message : String(e);
        if (/Failed to fetch|NetworkError|fetch|abort|AbortError/i.test(msg)) {
          return Promise.reject(new Error(
            "No se pudo contactar adc-api en " + (getApiBase() || "127.0.0.1:17865") +
            ". ¿Se cerró el servidor? Ejecutá .\\scripts\\run-stable.ps1"
          ));
        }
        return Promise.reject(e instanceof Error ? e : new Error(msg));
      });
    } catch (e) {
      return Promise.reject(e instanceof Error ? e : new Error(String(e)));
    }
  }

  function pingApi() {
    var base = getApiBase();
    if (!base) return Promise.resolve(false);
    return fetch(base + "/health", { method: "GET", cache: "no-store" })
      .then(function (r) { return r.ok; })
      .catch(function () { return false; });
  }

  /** Monotonic token so stale chapter loads don't clobber a newer Bible selection. */
  var _chapterLoadGen = 0;

  var state = {
    modules: [],
    books: [],
    bibles: [],
    commentaries: [],
    dictionaries: [],
    lexicons: [],
    bible: null,
    cmt: null,
    dict: null,
    lex: null,
    bookNumber: 1,
    chapter: 1,
    verses: [],
    selectedVerse: null,
    selectedWord: null,      // string or null
    selectedWordIndex: -1,   // which word instance in the verse
    selectedStrongs: [],     // [{ strong, spanish, greek, translit, sourceModule }]
    strongsNote: "",         // resolve status message
    highlightMode: null,     // 'verse' | 'word' | null
    tab: "biblia",
    theme: "true-dark",
    bibleView: "single",     // 'single' | 'parallel'
    parallelPaths: ["", ""], // up to 2 extra bible module paths
    parallelByPath: {},      // path -> { verses: [], title, abbr }
    dictHits: [],
    cmtHits: [],
    lexHits: [],
    contentFlags: {},        // path -> true if has content for current context
    // Commentary isolation filters (like e-Sword level toggles)
    cmtIncludeVerse: true,
    cmtIncludeChapter: false,
    cmtIncludeBook: false
  };

  var CMT_LEVELS_KEY = "adc-bible-cmt-levels-v1";

  var NOTES_KEY = "adc-bible-notes-v1";
  var THEME_KEY = "adc-bible-theme-v1";
  var NAV_KEY = "adc-bible-nav-v1"; // book / chapter / verse / bible path
  var HL_KEY = "adc-bible-highlights-v1"; // semantic style ids, never raw hex
  var PARALLEL_KEY = "adc-bible-parallel-v1";
  var HISTORY_KEY = "adc-bible-history-v1";
  var HISTORY_MAX = 40;
  var WOC_OPEN = "\uE000";
  var WOC_CLOSE = "\uE001";
  /** Named styles only — CSS vars per theme map these to colors */
  var HL_STYLES = ["gold", "sky", "rose", "lime", "violet", "peach"];

  var THEMES = [
    {
      id: "plain-white",
      name: "Plain White",
      desc: "Fondo blanco limpio, texto oscuro muy legible.",
      swatch: "#ffffff",
      swatchText: "#0f172a",
      accent: "#1d4ed8"
    },
    {
      id: "pastel-green",
      name: "Pastel Green",
      desc: "Verde suave, sin cansar la vista.",
      swatch: "#eef8f0",
      swatchText: "#14352a",
      accent: "#0f766e"
    },
    {
      id: "pastel-yellow",
      name: "Pastel Yellow",
      desc: "Crema cálido, ideal para lecturas largas.",
      swatch: "#fff9e8",
      swatchText: "#3b2f0b",
      accent: "#a16207"
    },
    {
      id: "light-blue",
      name: "Light Blue",
      desc: "Azul claro fresco y simple.",
      swatch: "#eef6ff",
      swatchText: "#0c2a4a",
      accent: "#1d4ed8"
    },
    {
      id: "true-dark",
      name: "True Dark Mode",
      desc: "Negro total (#000) con acento azul y tipografía clara.",
      swatch: "#000000",
      swatchText: "#fafafa",
      accent: "#60a5fa"
    },
    {
      id: "fluent-glass",
      name: "Fluent Glass",
      desc: "Translúcido para WinUI 3 (Acrylic/Mica). Deja ver el fondo del escritorio.",
      swatch: "rgba(30,40,55,.55)",
      swatchText: "#e0f2fe",
      accent: "#7dd3fc"
    }
  ];

  function loadNotes() {
    try { return JSON.parse(localStorage.getItem(NOTES_KEY) || "{}"); }
    catch (e) { return {}; }
  }
  function saveNotes(n) { localStorage.setItem(NOTES_KEY, JSON.stringify(n)); }
  var notes = loadNotes();

  /**
   * Permanent highlights store STYLE NAMES only (e.g. "gold"), never hex.
   * Shape: {
   *   verses: { "book|ch|v": "gold" },
   *   words: { "book|ch|v|idx": { style, word } },
   *   underlines: { "book|ch|v": true, "book|ch|v|idx": true }
   * }
   * Theme CSS vars retint marks automatically when data-theme changes.
   */
  function emptyHighlights() {
    return { verses: {}, words: {}, underlines: {} };
  }
  function loadHighlights() {
    try {
      var raw = JSON.parse(localStorage.getItem(HL_KEY) || "{}");
      if (!raw || typeof raw !== "object") return emptyHighlights();
      return {
        verses: raw.verses && typeof raw.verses === "object" ? raw.verses : {},
        words: raw.words && typeof raw.words === "object" ? raw.words : {},
        underlines: raw.underlines && typeof raw.underlines === "object" ? raw.underlines : {}
      };
    } catch (e) {
      return emptyHighlights();
    }
  }
  function saveHighlights() {
    try { localStorage.setItem(HL_KEY, JSON.stringify(highlights)); } catch (e) {}
  }
  var highlights = loadHighlights();

  function isValidHlStyle(s) {
    return HL_STYLES.indexOf(s) >= 0;
  }

  function verseHlKey(book, ch, verse) {
    return book + "|" + ch + "|" + verse;
  }
  function wordHlKey(book, ch, verse, idx) {
    return book + "|" + ch + "|" + verse + "|" + idx;
  }

  /** Key for the current selection (verse or word) used by marks / underline */
  function selectionMarkKey() {
    if (state.selectedVerse == null) return null;
    if (state.highlightMode === "word" && state.selectedWordIndex >= 0) {
      return wordHlKey(state.bookNumber, state.chapter, state.selectedVerse, state.selectedWordIndex);
    }
    return verseHlKey(state.bookNumber, state.chapter, state.selectedVerse);
  }

  function getVerseMarkStyle(verseNum) {
    var k = verseHlKey(state.bookNumber, state.chapter, verseNum);
    var s = highlights.verses[k];
    return isValidHlStyle(s) ? s : null;
  }

  function getWordMarkStyle(verseNum, wordIndex) {
    var k = wordHlKey(state.bookNumber, state.chapter, verseNum, wordIndex);
    var entry = highlights.words[k];
    if (!entry) return null;
    var s = typeof entry === "string" ? entry : entry.style;
    return isValidHlStyle(s) ? s : null;
  }

  function hasUnderlineKey(key) {
    return !!(key && highlights.underlines[key]);
  }

  function getVerseUnderline(verseNum) {
    return hasUnderlineKey(verseHlKey(state.bookNumber, state.chapter, verseNum));
  }

  function getWordUnderline(verseNum, wordIndex) {
    return hasUnderlineKey(wordHlKey(state.bookNumber, state.chapter, verseNum, wordIndex));
  }

  function currentSelectionHasUnderline() {
    return hasUnderlineKey(selectionMarkKey());
  }

  function applyHighlight(styleId) {
    if (!isValidHlStyle(styleId)) return;
    if (state.selectedVerse == null) return;

    if (state.highlightMode === "word" && state.selectedWordIndex >= 0) {
      var wk = wordHlKey(state.bookNumber, state.chapter, state.selectedVerse, state.selectedWordIndex);
      highlights.words[wk] = {
        style: styleId,
        word: state.selectedWord || ""
      };
    } else {
      var vk = verseHlKey(state.bookNumber, state.chapter, state.selectedVerse);
      highlights.verses[vk] = styleId;
    }
    saveHighlights();
    renderVerses(el("search-verse").value);
    updateHlBarUI();
  }

  function clearHighlight() {
    if (state.selectedVerse == null) return;
    if (state.highlightMode === "word" && state.selectedWordIndex >= 0) {
      var wk = wordHlKey(state.bookNumber, state.chapter, state.selectedVerse, state.selectedWordIndex);
      delete highlights.words[wk];
    } else {
      var vk = verseHlKey(state.bookNumber, state.chapter, state.selectedVerse);
      delete highlights.verses[vk];
    }
    saveHighlights();
    renderVerses(el("search-verse").value);
    updateHlBarUI();
  }

  function toggleUnderline() {
    var key = selectionMarkKey();
    if (!key) return;
    if (highlights.underlines[key]) delete highlights.underlines[key];
    else highlights.underlines[key] = true;
    saveHighlights();
    renderVerses(el("search-verse").value);
    updateHlBarUI();
  }

  function clearAllMarksOnSelection() {
    if (state.selectedVerse == null) return;
    if (state.highlightMode === "word" && state.selectedWordIndex >= 0) {
      var wk = wordHlKey(state.bookNumber, state.chapter, state.selectedVerse, state.selectedWordIndex);
      delete highlights.words[wk];
      delete highlights.underlines[wk];
    } else {
      var vk = verseHlKey(state.bookNumber, state.chapter, state.selectedVerse);
      delete highlights.verses[vk];
      delete highlights.underlines[vk];
    }
    saveHighlights();
    renderVerses(el("search-verse").value);
    updateHlBarUI();
  }

  function currentSelectionMarkStyle() {
    if (state.selectedVerse == null) return null;
    if (state.highlightMode === "word" && state.selectedWordIndex >= 0) {
      return getWordMarkStyle(state.selectedVerse, state.selectedWordIndex);
    }
    return getVerseMarkStyle(state.selectedVerse);
  }

  function updateHlBarUI() {
    var bar = el("hl-bar");
    if (!bar) return;
    var hasSel = state.selectedVerse != null;
    var current = currentSelectionMarkStyle();
    var chips = bar.querySelectorAll(".hl-chip[data-style]");
    for (var i = 0; i < chips.length; i++) {
      var chip = chips[i];
      chip.disabled = !hasSel;
      var st = chip.getAttribute("data-style");
      if (hasSel && current === st) chip.classList.add("active");
      else chip.classList.remove("active");
    }
    var clearBtn = el("btn-hl-clear");
    if (clearBtn) {
      clearBtn.disabled = !hasSel || !current;
    }
  }

  function getVersePlainText(verseNum) {
    for (var i = 0; i < state.verses.length; i++) {
      if (state.verses[i].verse === verseNum) {
        return String(state.verses[i].text || "")
          .replace(/\uE000|\uE001/g, "")
          .replace(/<[^>]+>/g, "")
          .replace(/\s+/g, " ")
          .trim();
      }
    }
    return "";
  }

  function selectionCopyPayload() {
    var bm = bookMeta();
    var ref = bm.name + " " + state.chapter;
    if (state.selectedVerse != null) ref += ":" + state.selectedVerse;
    var bible = state.bible
      ? (state.bible.abbreviation || state.bible.filename || "")
      : "";
    if (state.highlightMode === "word" && state.selectedWord) {
      return {
        title: ref + " · «" + state.selectedWord + "»",
        text: "«" + state.selectedWord + "» — " + ref + (bible ? " (" + bible + ")" : "")
      };
    }
    var body = state.selectedVerse != null ? getVersePlainText(state.selectedVerse) : "";
    return {
      title: ref,
      text: (body ? body + "\n\n— " : "") + ref + (bible ? " (" + bible + ")" : "")
    };
  }

  function copyTextToClipboard(text) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      return navigator.clipboard.writeText(text);
    }
    return new Promise(function (resolve, reject) {
      try {
        var ta = document.createElement("textarea");
        ta.value = text;
        ta.style.position = "fixed";
        ta.style.left = "-9999px";
        document.body.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        document.body.removeChild(ta);
        resolve();
      } catch (e) {
        reject(e);
      }
    });
  }

  function shareSelection() {
    var payload = selectionCopyPayload();
    if (navigator.share) {
      return navigator.share({ title: payload.title, text: payload.text }).catch(function () {
        // user cancelled or share failed → fall through to copy
        return copyTextToClipboard(payload.text);
      });
    }
    return copyTextToClipboard(payload.text);
  }

  /* ---------- Soft context menu (right-click) ---------- */
  var ctxState = { open: false, x: 0, y: 0 };

  function closeCtxMenu() {
    var menu = el("ctx-menu");
    if (!menu) return;
    menu.classList.remove("open");
    menu.setAttribute("aria-hidden", "true");
    var sub = el("ctx-hl-sub");
    if (sub) sub.classList.remove("open");
    ctxState.open = false;
  }

  function positionCtxMenu(x, y) {
    var menu = el("ctx-menu");
    if (!menu) return;
    menu.style.left = "0px";
    menu.style.top = "0px";
    menu.classList.add("open");
    // measure after open
    var rect = menu.getBoundingClientRect();
    var pad = 8;
    var left = x;
    var top = y;
    if (left + rect.width > window.innerWidth - pad) {
      left = Math.max(pad, window.innerWidth - rect.width - pad);
    }
    if (top + rect.height > window.innerHeight - pad) {
      top = Math.max(pad, window.innerHeight - rect.height - pad);
    }
    if (left < pad) left = pad;
    if (top < pad) top = pad;
    menu.style.left = left + "px";
    menu.style.top = top + "px";
  }

  function refreshCtxMenuUI() {
    var header = el("ctx-header");
    var payload = selectionCopyPayload();
    if (header) header.textContent = payload.title || "Selección";

    var ulBtn = el("ctx-btn-ul");
    if (ulBtn) {
      if (currentSelectionHasUnderline()) {
        ulBtn.classList.add("active-mark");
        ulBtn.querySelector(".ctx-label").textContent = "Quitar subrayado";
      } else {
        ulBtn.classList.remove("active-mark");
        ulBtn.querySelector(".ctx-label").textContent = "Subrayar";
      }
    }

    var current = currentSelectionMarkStyle();
    var colors = el("ctx-colors");
    if (colors) {
      var chips = colors.querySelectorAll(".hl-chip[data-style]");
      for (var i = 0; i < chips.length; i++) {
        var st = chips[i].getAttribute("data-style");
        if (current === st) chips[i].classList.add("active");
        else chips[i].classList.remove("active");
      }
    }
    var hlBtn = el("ctx-btn-hl");
    if (hlBtn) {
      if (current) hlBtn.classList.add("active-mark");
      else hlBtn.classList.remove("active-mark");
    }
  }

  function openCtxMenu(clientX, clientY) {
    if (state.selectedVerse == null) return;
    refreshCtxMenuUI();
    var menu = el("ctx-menu");
    if (!menu) return;
    menu.setAttribute("aria-hidden", "false");
    positionCtxMenu(clientX, clientY);
    ctxState.open = true;
    ctxState.x = clientX;
    ctxState.y = clientY;
  }

  /**
   * Right-click on verse number / body / word.
   * Selects target first, then opens soft menu (never browser menu).
   */
  function handleVerseContextMenu(ev, verseNum, wordText, wordIndex) {
    ev.preventDefault();
    ev.stopPropagation();
    if (typeof wordIndex === "number" && wordIndex >= 0 && wordText) {
      selectWord(verseNum, wordText, wordIndex);
    } else {
      selectVerse(verseNum, false);
    }
    // open after selection UI updated
    openCtxMenu(ev.clientX, ev.clientY);
  }

  function applyTheme(id) {
    var ok = false;
    for (var i = 0; i < THEMES.length; i++) {
      if (THEMES[i].id === id) { ok = true; break; }
    }
    // Migrate removed themes (e.g. liquid-glass) to True Dark
    if (!ok) id = "true-dark";
    document.documentElement.setAttribute("data-theme", id);
    document.documentElement.removeAttribute("data-glass");
    try { localStorage.setItem(THEME_KEY, id); } catch (e) {}
    state.theme = id;
    renderThemeGrid();
  }

  function defaultThemeId() {
    // Desktop hosts (WinUI / Edge app) get Fluent Glass; pure browser stays True Dark.
    return (window.__ADC_HOST__ === "winui3" || window.__ADC_HOST__ === "edge-app")
      ? "fluent-glass"
      : "true-dark";
  }

  function loadSavedTheme() {
    var id = defaultThemeId();
    try { id = localStorage.getItem(THEME_KEY) || defaultThemeId(); } catch (e) {}
    applyTheme(id);
  }

  function renderThemeGrid() {
    var grid = el("theme-grid");
    if (!grid) return;
    grid.innerHTML = "";
    for (var i = 0; i < THEMES.length; i++) {
      var t = THEMES[i];
      var card = document.createElement("div");
      card.className = "theme-card" + (state.theme === t.id ? " selected" : "");
      card.setAttribute("data-theme-id", t.id);
      card.setAttribute("role", "button");
      card.setAttribute("tabindex", "0");
      card.innerHTML =
        '<div class="theme-swatch" style="background:' + t.swatch + ';color:' + t.swatchText + '">' +
          '<span class="sample-title" style="color:' + t.accent + '">Juan 3:16</span>' +
          '<span class="sample-jesus">“Sígueme”</span>' +
        '</div>' +
        '<div class="theme-card-body">' +
          '<h3></h3><p></p>' +
        '</div>' +
        '<div class="check">✓ Tema activo</div>';
      card.querySelector("h3").textContent = t.name;
      card.querySelector("p").textContent = t.desc;
      (function (themeId) {
        function activate() { applyTheme(themeId); }
        card.onclick = function () { activate(); };
        card.onkeydown = function (ev) {
          if (ev.key === "Enter" || ev.key === " ") {
            ev.preventDefault();
            activate();
          }
        };
      })(t.id);
      grid.appendChild(card);
    }
  }

  // Book aliases for detecting refs in study text (longest first when matching)
  var BOOK_ALIAS_LIST = [
    { n: 1, a: ["genesis", "génesis", "gen", "gn", "gé"] },
    { n: 2, a: ["exodo", "éxodo", "exo", "ex", "éx"] },
    { n: 3, a: ["levitico", "levítico", "lev", "lv"] },
    { n: 4, a: ["numeros", "números", "num", "nm", "núm"] },
    { n: 5, a: ["deuteronomio", "deut", "dt"] },
    { n: 6, a: ["josue", "josué", "jos"] },
    { n: 7, a: ["jueces", "jue", "jdg"] },
    { n: 8, a: ["rut", "rt", "ruth"] },
    { n: 9, a: ["1 samuel", "1samuel", "1 sa", "1sa", "1sam", "i samuel"] },
    { n: 10, a: ["2 samuel", "2samuel", "2 sa", "2sa", "2sam", "ii samuel"] },
    { n: 11, a: ["1 reyes", "1reyes", "1 re", "1re", "1r", "i reyes"] },
    { n: 12, a: ["2 reyes", "2reyes", "2 re", "2re", "2r", "ii reyes"] },
    { n: 13, a: ["1 cronicas", "1 crónicas", "1cronicas", "1cr", "1 cr", "i cronicas"] },
    { n: 14, a: ["2 cronicas", "2 crónicas", "2cronicas", "2cr", "2 cr", "ii cronicas"] },
    { n: 15, a: ["esdras", "esd", "ezra"] },
    { n: 16, a: ["nehemias", "nehemías", "neh"] },
    { n: 17, a: ["ester", "est", "esther"] },
    { n: 18, a: ["job"] },
    { n: 19, a: ["salmos", "salmo", "sal", "sl", "psalm", "psalms", "ps"] },
    { n: 20, a: ["proverbios", "prov", "pr"] },
    { n: 21, a: ["eclesiastes", "eclesiastés", "ecl", "ec", "eccl"] },
    { n: 22, a: ["cantares", "cantar", "cnt", "can", "song"] },
    { n: 23, a: ["isaias", "isaías", "isa", "is"] },
    { n: 24, a: ["jeremias", "jeremías", "jer"] },
    { n: 25, a: ["lamentaciones", "lam"] },
    { n: 26, a: ["ezequiel", "eze", "ez"] },
    { n: 27, a: ["daniel", "dan", "dn"] },
    { n: 28, a: ["oseas", "oseas", "os", "hos"] },
    { n: 29, a: ["joel", "jl"] },
    { n: 30, a: ["amos", "amós", "am"] },
    { n: 31, a: ["abdias", "abdías", "abd"] },
    { n: 32, a: ["jonas", "jonás", "jon"] },
    { n: 33, a: ["miqueas", "miq", "mic"] },
    { n: 34, a: ["nahum", "nahúm", "nah"] },
    { n: 35, a: ["habacuc", "hab"] },
    { n: 36, a: ["sofonias", "sofonías", "sof"] },
    { n: 37, a: ["hageo", "hag"] },
    { n: 38, a: ["zacarias", "zacarías", "zac"] },
    { n: 39, a: ["malaquias", "malaquías", "mal"] },
    { n: 40, a: ["mateo", "mat", "mt"] },
    { n: 41, a: ["marcos", "mar", "mc", "mk"] },
    { n: 42, a: ["lucas", "luc", "lc", "lk"] },
    { n: 43, a: ["juan", "jua", "jn", "john"] },
    { n: 44, a: ["hechos", "hech", "hch", "acts", "act"] },
    { n: 45, a: ["romanos", "rom", "ro", "rm"] },
    { n: 46, a: ["1 corintios", "1corintios", "1 co", "1co", "1cor", "i corintios"] },
    { n: 47, a: ["2 corintios", "2corintios", "2 co", "2co", "2cor", "ii corintios"] },
    { n: 48, a: ["galatas", "gálatas", "gal", "ga"] },
    { n: 49, a: ["efesios", "efe", "ef"] },
    { n: 50, a: ["filipenses", "fil", "php"] },
    { n: 51, a: ["colosenses", "col"] },
    { n: 52, a: ["1 tesalonicenses", "1tesalonicenses", "1 ts", "1ts", "1tes"] },
    { n: 53, a: ["2 tesalonicenses", "2tesalonicenses", "2 ts", "2ts", "2tes"] },
    { n: 54, a: ["1 timoteo", "1timoteo", "1 ti", "1ti", "1tim"] },
    { n: 55, a: ["2 timoteo", "2timoteo", "2 ti", "2ti", "2tim"] },
    { n: 56, a: ["tito", "tit"] },
    { n: 57, a: ["filemon", "filemón", "flm", "flmón"] },
    { n: 58, a: ["hebreos", "heb", "he"] },
    { n: 59, a: ["santiago", "stg", "sant", "james"] },
    { n: 60, a: ["1 pedro", "1pedro", "1 pe", "1pe", "1ped"] },
    { n: 61, a: ["2 pedro", "2pedro", "2 pe", "2pe", "2ped"] },
    { n: 62, a: ["1 juan", "1juan", "1 jn", "1jn", "1jua"] },
    { n: 63, a: ["2 juan", "2juan", "2 jn", "2jn"] },
    { n: 64, a: ["3 juan", "3juan", "3 jn", "3jn"] },
    { n: 65, a: ["judas", "jud", "jude"] },
    { n: 66, a: ["apocalipsis", "apoc", "ap", "rev", "revelation"] }
  ];

  var chapterCache = {}; // key: biblePath|book|chapter -> { verses, title, translation }
  var CHAPTER_CACHE_MAX = 48;
  var lexCache = {}; // key: lexPath|term -> { rows, title }

  function putChapterCache(key, pack) {
    chapterCache[key] = pack;
    var keys = Object.keys(chapterCache);
    if (keys.length <= CHAPTER_CACHE_MAX) return;
    // Drop oldest insertion order keys (JS preserves string key order for non-int keys)
    var drop = keys.length - CHAPTER_CACHE_MAX;
    for (var i = 0; i < drop; i++) delete chapterCache[keys[i]];
  }

  function getChapterCache(key) {
    var pack = chapterCache[key];
    if (!pack || !pack.verses) return null;
    return pack;
  }
  var refHoverTimer = null;
  var refHideTimer = null;

  function saveNavPosition() {
    try {
      localStorage.setItem(NAV_KEY, JSON.stringify({
        bookNumber: state.bookNumber,
        chapter: state.chapter,
        selectedVerse: state.selectedVerse,
        biblePath: state.bible ? state.bible.path : null,
        cmtPath: state.cmt ? state.cmt.path : null,
        dictPath: state.dict ? state.dict.path : null,
        lexPath: state.lex ? state.lex.path : null
      }));
    } catch (e) { /* ignore */ }
    pushHistoryEntry();
  }

  function loadNavPosition() {
    try {
      var raw = localStorage.getItem(NAV_KEY);
      if (!raw) return null;
      return JSON.parse(raw);
    } catch (e) {
      return null;
    }
  }

  /* ---------- Parallel Bibles prefs ---------- */
  function saveParallelPrefs() {
    try {
      localStorage.setItem(PARALLEL_KEY, JSON.stringify({
        bibleView: state.bibleView,
        parallelPaths: state.parallelPaths.slice(0, 2)
      }));
    } catch (e) {}
  }

  function loadParallelPrefs() {
    try {
      var raw = JSON.parse(localStorage.getItem(PARALLEL_KEY) || "{}");
      if (raw.bibleView === "parallel" || raw.bibleView === "single") {
        state.bibleView = raw.bibleView;
      }
      if (Array.isArray(raw.parallelPaths)) {
        state.parallelPaths = [
          String(raw.parallelPaths[0] || ""),
          String(raw.parallelPaths[1] || "")
        ];
      }
    } catch (e) {}
  }

  function bibleByPath(path) {
    if (!path) return null;
    for (var i = 0; i < state.bibles.length; i++) {
      if (state.bibles[i].path === path) return state.bibles[i];
    }
    return null;
  }

  function bibleLabel(mod) {
    if (!mod) return "";
    return mod.abbreviation || mod.filename || mod.title || "Biblia";
  }

  function activeParallelPaths() {
    if (state.bibleView !== "parallel") return [];
    var primary = state.bible ? state.bible.path : "";
    var out = [];
    for (var i = 0; i < state.parallelPaths.length && out.length < 2; i++) {
      var p = state.parallelPaths[i];
      if (!p || p === primary) continue;
      if (out.indexOf(p) >= 0) continue;
      if (!bibleByPath(p)) continue;
      out.push(p);
    }
    return out;
  }

  function fillParallelSelects() {
    var sels = [el("sel-parallel-1"), el("sel-parallel-2")];
    var primary = state.bible ? state.bible.path : "";
    for (var s = 0; s < sels.length; s++) {
      var sel = sels[s];
      if (!sel) continue;
      var keep = state.parallelPaths[s] || "";
      sel.innerHTML = "";
      var o0 = document.createElement("option");
      o0.value = "";
      o0.textContent = "— Ninguna —";
      sel.appendChild(o0);
      for (var i = 0; i < state.bibles.length; i++) {
        var b = state.bibles[i];
        if (b.path === primary) continue; // primary already shown in header
        var o = document.createElement("option");
        o.value = b.path;
        o.textContent = bibleLabel(b) + (b.title ? " — " + b.title : "");
        sel.appendChild(o);
      }
      // If saved path is primary or missing, clear
      if (keep && keep !== primary && bibleByPath(keep)) sel.value = keep;
      else {
        sel.value = "";
        state.parallelPaths[s] = "";
      }
    }
  }

  function setBibleView(view) {
    state.bibleView = view === "parallel" ? "parallel" : "single";
    var tabs = el("bible-subtabs");
    if (tabs) {
      var btns = tabs.querySelectorAll("[data-bible-view]");
      for (var i = 0; i < btns.length; i++) {
        var on = btns[i].getAttribute("data-bible-view") === state.bibleView;
        if (on) btns[i].classList.add("active");
        else btns[i].classList.remove("active");
        btns[i].setAttribute("aria-selected", on ? "true" : "false");
      }
    }
    var bar = el("parallel-bar");
    if (bar) {
      if (state.bibleView === "parallel") bar.classList.add("show");
      else bar.classList.remove("show");
    }
    saveParallelPrefs();
    if (state.bibleView === "parallel") {
      fillParallelSelects();
      loadParallelChapters().then(function () {
        renderVerses(el("search-verse").value);
      });
    } else {
      state.parallelByPath = {};
      renderVerses(el("search-verse") ? el("search-verse").value : "");
    }
  }

  function verseMapFromList(verses) {
    var map = {};
    for (var i = 0; i < (verses || []).length; i++) {
      map[verses[i].verse] = verses[i];
    }
    return map;
  }

  function loadParallelChapters() {
    var paths = activeParallelPaths();
    if (!paths.length) {
      state.parallelByPath = {};
      return Promise.resolve();
    }
    var jobs = paths.map(function (path) {
      var cacheKey = path + "|" + state.bookNumber + "|" + state.chapter;
      var cached = getChapterCache(cacheKey);
      if (cached) {
        var mod = bibleByPath(path);
        state.parallelByPath[path] = {
          verses: cached.verses,
          map: verseMapFromList(cached.verses),
          title: cached.title || "",
          abbr: bibleLabel(mod)
        };
        return Promise.resolve();
      }
      return invoke("get_bible_chapter", {
        modulePath: path,
        bookNumber: state.bookNumber,
        chapter: state.chapter
      }).then(function (res) {
        var verses = res.verses || [];
        putChapterCache(cacheKey, {
          verses: verses,
          title: res.moduleTitle || res.translation || "",
          translation: res.translation || ""
        });
        var mod = bibleByPath(path);
        state.parallelByPath[path] = {
          verses: verses,
          map: verseMapFromList(verses),
          title: res.moduleTitle || res.translation || "",
          abbr: bibleLabel(mod)
        };
      }).catch(function (e) {
        state.parallelByPath[path] = {
          verses: [],
          map: {},
          title: String(e.message || e),
          abbr: bibleLabel(bibleByPath(path)) || "?",
          error: true
        };
      });
    });
    // Drop paths no longer selected
    var keep = {};
    for (var i = 0; i < paths.length; i++) keep[paths[i]] = true;
    var next = {};
    for (var k in state.parallelByPath) {
      if (keep[k]) next[k] = state.parallelByPath[k];
    }
    state.parallelByPath = next;
    return Promise.all(jobs);
  }

  /* ---------- History / Recents ---------- */
  function loadHistory() {
    try {
      var raw = JSON.parse(localStorage.getItem(HISTORY_KEY) || "[]");
      return Array.isArray(raw) ? raw : [];
    } catch (e) {
      return [];
    }
  }

  function saveHistory(list) {
    try { localStorage.setItem(HISTORY_KEY, JSON.stringify(list)); } catch (e) {}
  }

  var _histTimer = null;
  function pushHistoryEntry() {
    if (!state.bookNumber || !state.chapter) return;
    // Debounce: chapter flips shouldn't spam identical rows mid-load
    if (_histTimer) clearTimeout(_histTimer);
    _histTimer = setTimeout(function () {
      var bm = bookMeta();
      var verse = state.selectedVerse != null ? state.selectedVerse : 1;
      var entry = {
        bookNumber: state.bookNumber,
        chapter: state.chapter,
        verse: verse,
        biblePath: state.bible ? state.bible.path : null,
        label: bm.name + " " + state.chapter + ":" + verse,
        bibleLabel: state.bible ? bibleLabel(state.bible) : "",
        ts: Date.now()
      };
      var list = loadHistory();
      // Remove same book|chapter|verse (keep freshest on top)
      var key = entry.bookNumber + "|" + entry.chapter + "|" + entry.verse;
      list = list.filter(function (h) {
        return (h.bookNumber + "|" + h.chapter + "|" + h.verse) !== key;
      });
      list.unshift(entry);
      if (list.length > HISTORY_MAX) list = list.slice(0, HISTORY_MAX);
      saveHistory(list);
    }, 400);
  }

  function formatHistWhen(ts) {
    if (!ts) return "";
    try {
      var d = new Date(ts);
      var now = new Date();
      var sameDay = d.toDateString() === now.toDateString();
      var time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
      if (sameDay) return "Hoy · " + time;
      return d.toLocaleDateString([], { day: "numeric", month: "short" }) + " · " + time;
    } catch (e) {
      return "";
    }
  }

  function openHistoryModal() {
    var modal = el("history-modal");
    if (!modal) return;
    renderHistoryList();
    modal.classList.add("open");
  }

  function closeHistoryModal() {
    var modal = el("history-modal");
    if (modal) modal.classList.remove("open");
  }

  function renderHistoryList() {
    var box = el("history-list");
    if (!box) return;
    var list = loadHistory();
    box.innerHTML = "";
    if (!list.length) {
      box.innerHTML = '<div class="hist-empty">Aún no hay historial. Navega por libros y capítulos y aparecerán aquí.</div>';
      return;
    }
    for (var i = 0; i < list.length; i++) {
      var h = list[i];
      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "hist-item";
      btn.innerHTML =
        '<span class="hist-ref">' + escapeHtml(h.label || (h.bookNumber + " " + h.chapter)) + "</span>" +
        '<span class="hist-meta">' +
          escapeHtml(h.bibleLabel || "") +
          (h.bibleLabel ? " · " : "") +
          escapeHtml(formatHistWhen(h.ts)) +
        "</span>";
      (function (entry) {
        btn.onclick = function () {
          closeHistoryModal();
          goToHistoryEntry(entry);
        };
      })(h);
      box.appendChild(btn);
    }
  }

  function goToHistoryEntry(entry) {
    if (!entry) return;
    var bookOk = false;
    for (var i = 0; i < state.books.length; i++) {
      if (state.books[i].number === entry.bookNumber) { bookOk = true; break; }
    }
    if (!bookOk) return;
    state.bookNumber = entry.bookNumber;
    state.chapter = entry.chapter || 1;
    state.selectedVerse = entry.verse != null ? entry.verse : 1;
    state.selectedWord = null;
    state.selectedWordIndex = -1;
    state.highlightMode = "verse";
    if (entry.biblePath) {
      var b = bibleByPath(entry.biblePath);
      if (b) state.bible = b;
    }
    fillChapterSelect();
    var chSel = el("sel-chapter");
    if (chSel) chSel.value = String(state.chapter);
    syncHeaderPills();
    // Switch to Biblia tab
    goToStudyTab("biblia");
    loadChapter();
  }

  function normKey(s) {
    return String(s || "")
      .toLowerCase()
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .replace(/\s+/g, " ")
      .trim();
  }

  function resolveBookAlias(name) {
    var key = normKey(name);
    // Prefer longer alias matches
    var best = null;
    var bestLen = 0;
    for (var i = 0; i < BOOK_ALIAS_LIST.length; i++) {
      var b = BOOK_ALIAS_LIST[i];
      for (var j = 0; j < b.a.length; j++) {
        var a = normKey(b.a[j]);
        if (key === a && a.length >= bestLen) {
          best = b.n;
          bestLen = a.length;
        }
      }
    }
    // Also match full Spanish names from loaded books
    if (state.books && state.books.length) {
      for (var k = 0; k < state.books.length; k++) {
        var bn = normKey(state.books[k].name);
        var ba = normKey(state.books[k].abbr);
        if (key === bn || key === ba) return state.books[k].number;
      }
    }
    return best;
  }

  function bookDisplayName(num) {
    if (state.books) {
      for (var i = 0; i < state.books.length; i++) {
        if (state.books[i].number === num) return state.books[i].name;
      }
    }
    for (var j = 0; j < BOOK_ALIAS_LIST.length; j++) {
      if (BOOK_ALIAS_LIST[j].n === num) return BOOK_ALIAS_LIST[j].a[0];
    }
    return "Libro " + num;
  }

  /**
   * Parse a matched ref string into structured data.
   * Does NOT change main Bible navigation (no auto-save of chapter/verse).
   */
  function parseRefMatch(bookRaw, ch, v1, v2) {
    var bn = resolveBookAlias(bookRaw);
    if (!bn) return null;
    var chapter = parseInt(ch, 10);
    var verseStart = parseInt(v1, 10);
    var verseEnd = v2 ? parseInt(v2, 10) : verseStart;
    if (!chapter || !verseStart) return null;
    if (verseEnd < verseStart) verseEnd = verseStart;
    return {
      bookNumber: bn,
      chapter: chapter,
      verseStart: verseStart,
      verseEnd: verseEnd,
      label: bookDisplayName(bn) + " " + chapter + ":" + verseStart +
        (verseEnd !== verseStart ? "–" + verseEnd : "")
    };
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  /**
   * Linkify Bible refs, Strong’s numbers (H/G), and Hebrew/Greek runs.
   * Used in commentary / dictionary / lexicon bodies (and tooltip verse text).
   * Non-overlapping: earliest match wins, then longer.
   */
  function linkifyRawSegment(raw) {
    var matches = [];
    var m;

    // Bible refs: "Mateo 5:12", "Mt. 5:12-14", "1 Cor 13:4", "Jn.3:16", "Gén_25:16" (e-Sword)
    var reRef =
      /(\b(?:[123]\s*)?(?:[A-Za-zÁÉÍÓÚÜÑáéíóúüñ.]{1,}(?:\s+[A-Za-zÁÉÍÓÚÜÑáéíóúüñ.]{2,}){0,2}))\.?\s*[_ ]?\s*(\d{1,3})\s*[:.,]\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?/g;
    while ((m = reRef.exec(raw)) !== null) {
      var bookRaw = m[1].replace(/\./g, "").replace(/\s+/g, " ").trim();
      var parsed = parseRefMatch(bookRaw, m[2], m[3], m[4] || "");
      if (parsed) {
        matches.push({
          start: m.index,
          end: m.index + m[0].length,
          html:
            '<span class="bibleref study-ref" tabindex="0" role="link"' +
            ' data-kind="bible"' +
            ' data-book="' + parsed.bookNumber + '"' +
            ' data-chapter="' + parsed.chapter + '"' +
            ' data-v1="' + parsed.verseStart + '"' +
            ' data-v2="' + parsed.verseEnd + '"' +
            ' data-label="' + escapeHtml(parsed.label) + '"' +
            ' title="Ver ' + escapeHtml(parsed.label) + ' (hover / clic)">' +
            escapeHtml(m[0]) +
            "</span>"
        });
      }
    }

    // Strong’s: H1234, G26, H 430, (G5463), #G26
    var reStrong = /(?:^|[^A-Za-z0-9])([#(\[]?\s*([HG])\s*0*(\d{1,5})\s*[)\]]?)/gi;
    while ((m = reStrong.exec(raw)) !== null) {
      var full = m[1];
      var lead = m[0].length - full.length; // char before token
      var start = m.index + lead;
      var code = m[2].toUpperCase() + String(parseInt(m[3], 10));
      if (!m[3] || isNaN(parseInt(m[3], 10))) continue;
      matches.push({
        start: start,
        end: start + full.length,
        html:
          '<span class="strongref study-ref" tabindex="0" role="link"' +
          ' data-kind="strong"' +
          ' data-term="' + escapeHtml(code) + '"' +
          ' data-label="' + escapeHtml(code) + '"' +
          ' title="Strong’s ' + escapeHtml(code) + ' — léxico">' +
          escapeHtml(full.trim()) +
          "</span>"
      });
    }

    // Hebrew (incl. presentation forms)
    var reHe = /[\u0590-\u05FF\uFB1D-\uFB4F]{1,}/g;
    while ((m = reHe.exec(raw)) !== null) {
      matches.push({
        start: m.index,
        end: m.index + m[0].length,
        html:
          '<span class="lemmaref study-ref" tabindex="0" role="link"' +
          ' data-kind="lemma"' +
          ' data-lang="he"' +
          ' data-term="' + escapeHtml(m[0]) + '"' +
          ' data-label="Hebreo: ' + escapeHtml(m[0]) + '"' +
          ' title="Buscar lema hebreo en léxico">' +
          escapeHtml(m[0]) +
          "</span>"
      });
    }

    // Greek (basic + extended)
    var reEl = /[\u0370-\u03FF\u1F00-\u1FFF]{2,}/g;
    while ((m = reEl.exec(raw)) !== null) {
      matches.push({
        start: m.index,
        end: m.index + m[0].length,
        html:
          '<span class="lemmaref study-ref" tabindex="0" role="link"' +
          ' data-kind="lemma"' +
          ' data-lang="el"' +
          ' data-term="' + escapeHtml(m[0]) + '"' +
          ' data-label="Griego: ' + escapeHtml(m[0]) + '"' +
          ' title="Buscar lema griego en léxico">' +
          escapeHtml(m[0]) +
          "</span>"
      });
    }

    if (!matches.length) return escapeHtml(raw);

    // Resolve overlaps: sort by start, then longer first
    matches.sort(function (a, b) {
      if (a.start !== b.start) return a.start - b.start;
      return (b.end - b.start) - (a.end - a.start);
    });
    var chosen = [];
    var cursor = 0;
    for (var i = 0; i < matches.length; i++) {
      if (matches[i].start < cursor) continue;
      chosen.push(matches[i]);
      cursor = matches[i].end;
    }

    var out = "";
    var last = 0;
    for (var j = 0; j < chosen.length; j++) {
      var ch = chosen[j];
      out += escapeHtml(raw.slice(last, ch.start));
      out += ch.html;
      last = ch.end;
    }
    out += escapeHtml(raw.slice(last));
    return out;
  }

  /** Escape HTML, WOC red spans, and clickable Bible references. */
  function formatRichText(text) {
    if (!text) return "";
    var raw = String(text);
    // Split by WOC markers so refs inside WOC still work and stay red
    var re = new RegExp(WOC_OPEN + "([\\s\\S]*?)" + WOC_CLOSE, "g");
    var out = "";
    var last = 0;
    var m;
    while ((m = re.exec(raw)) !== null) {
      if (m.index > last) {
        out += linkifyRawSegment(raw.slice(last, m.index));
      }
      out += '<span class="woc">' + linkifyRawSegment(m[1]) + "</span>";
      last = m.index + m[0].length;
    }
    if (last < raw.length) out += linkifyRawSegment(raw.slice(last));
    if (!out && raw) out = linkifyRawSegment(raw);
    return out;
  }

  function cacheKey(book, chapter) {
    var bp = state.bible ? state.bible.path : "";
    return bp + "|" + book + "|" + chapter;
  }

  function fetchChapterVerses(bookNumber, chapter) {
    if (!state.bible) {
      return Promise.reject(new Error("No hay Biblia principal seleccionada en la pestaña Biblia."));
    }
    var key = cacheKey(bookNumber, chapter);
    var hit = getChapterCache(key);
    if (hit) return Promise.resolve(hit);
    return invoke("get_bible_chapter", {
      modulePath: state.bible.path,
      bookNumber: bookNumber,
      chapter: chapter
    }).then(function (res) {
      var pack = {
        verses: res.verses || [],
        title: res.moduleTitle || res.translation || (state.bible.abbreviation || ""),
        translation: res.translation || state.bible.abbreviation || ""
      };
      putChapterCache(key, pack);
      return pack;
    });
  }

  function versesInRange(pack, v1, v2) {
    var lines = [];
    for (var i = 0; i < pack.verses.length; i++) {
      var v = pack.verses[i];
      if (v.verse >= v1 && v.verse <= v2) {
        lines.push({ verse: v.verse, text: v.text });
      }
    }
    return lines;
  }

  function formatVerseLines(lines) {
    if (!lines.length) {
      return "<span class='rt-loading'>No se encontró ese versículo en la Biblia actual. ¿Está seleccionada una Biblia en la pestaña Biblia?</span>";
    }
    var html = "";
    for (var i = 0; i < lines.length; i++) {
      // Plain body (no nested study-refs) for stable tooltips
      var body = plainVerseText(lines[i].text || "");
      html +=
        "<div style='margin-bottom:8px'><b style='color:var(--accent);text-decoration:underline;text-underline-offset:2px'>" +
        lines[i].verse +
        "</b> " +
        escapeHtml(body) +
        "</div>";
    }
    return html;
  }

  function positionTooltip(clientX, clientY) {
    var tip = el("ref-tooltip");
    var pad = 12;
    var tw = tip.offsetWidth || 320;
    var th = tip.offsetHeight || 120;
    var x = clientX + 14;
    var y = clientY + 16;
    if (x + tw > window.innerWidth - pad) x = clientX - tw - 12;
    if (y + th > window.innerHeight - pad) y = clientY - th - 12;
    if (x < pad) x = pad;
    if (y < pad) y = pad;
    tip.style.left = x + "px";
    tip.style.top = y + "px";
  }

  function hideRefTooltip() {
    var tip = el("ref-tooltip");
    tip.classList.remove("visible");
  }

  /** Prefer Strong’s-named lexicons, then current selection, then rest. */
  function orderedLexiconModules() {
    var list = (state.lexicons || []).slice();
    if (!list.length) return [];
    list.sort(function (a, b) {
      var ha = ((a.title || "") + " " + (a.filename || "") + " " + (a.abbreviation || "")).toLowerCase();
      var hb = ((b.title || "") + " " + (b.filename || "") + " " + (b.abbreviation || "")).toLowerCase();
      var sa = /strong/i.test(ha) ? 0 : /chavez|ch[aá]vez|vine|tuggy|multilex/i.test(ha) ? 1 : 2;
      var sb = /strong/i.test(hb) ? 0 : /chavez|ch[aá]vez|vine|tuggy|multilex/i.test(hb) ? 1 : 2;
      if (state.lex) {
        if (a.path === state.lex.path) sa = -1;
        if (b.path === state.lex.path) sb = -1;
      }
      if (sa !== sb) return sa - sb;
      return (a.abbreviation || a.filename || "").localeCompare(b.abbreviation || b.filename || "");
    });
    return list;
  }

  function pickLexiconModule() {
    var list = orderedLexiconModules();
    return list[0] || state.lex || null;
  }

  /**
   * Fetch Strong’s / lemma from lexicons until content is found.
   * Does not permanently cache empty results (so fixing module selection helps).
   */
  function fetchLexiconEntries(term) {
    term = String(term || "").trim();
    if (!term) {
      return Promise.reject(new Error("Sin término Strong’s / lema."));
    }
    var mods = orderedLexiconModules();
    if (!mods.length) {
      return Promise.reject(new Error(
        "No hay léxicos en e-Sword. Abrí Léxico y elegí Strong ° u otro módulo."
      ));
    }

    var cacheHitKey = "_any|" + term.toUpperCase();
    if (lexCache[cacheHitKey] && lexCache[cacheHitKey].rows && lexCache[cacheHitKey].rows.length) {
      return Promise.resolve(lexCache[cacheHitKey]);
    }

    var i = 0;
    function tryNext() {
      if (i >= mods.length) {
        return Promise.resolve({
          rows: [],
          moduleTitle: "",
          tried: mods.length
        });
      }
      var mod = mods[i++];
      var key = mod.path + "|" + term;
      if (lexCache[key] && lexCache[key].rows && lexCache[key].rows.length) {
        lexCache[cacheHitKey] = lexCache[key];
        return Promise.resolve(lexCache[key]);
      }
      return invoke("search_lexicon", {
        modulePath: mod.path,
        term: term,
        limit: 8
      }).then(function (rows) {
        var good = (rows || []).filter(function (r) {
          var d = r.definition || "";
          return d && d.indexOf("[Módulo cifrado") !== 0 && d.indexOf("[Contenido binario") !== 0;
        });
        if (good.length) {
          var pack = {
            rows: good,
            moduleTitle: (mod.abbreviation || mod.filename || "") + " — " + (mod.title || ""),
            path: mod.path
          };
          lexCache[key] = pack;
          lexCache[cacheHitKey] = pack;
          return pack;
        }
        return tryNext();
      }).catch(function () {
        return tryNext();
      });
    }
    return tryNext();
  }

  function formatLexLines(rows, emptyHint) {
    if (!rows || !rows.length) {
      return "<span class='rt-loading'>" +
        (emptyHint || "No se encontró en los léxicos disponibles. Probá H#### / G#### o Strong °.") +
        "</span>";
    }
    var html = "";
    for (var i = 0; i < rows.length; i++) {
      var r = rows[i];
      // Definitions may contain more refs/Strong’s — linkify them too
      html +=
        "<div style='margin-bottom:10px'>" +
        "<div style='font-weight:700;color:var(--accent);text-decoration:underline;text-underline-offset:3px;margin-bottom:4px'>" +
        escapeHtml(r.term || "") +
        "</div>" +
        "<div style='font-size:11px;color:var(--muted);margin-bottom:4px'>" +
        escapeHtml(r.title || r.source || "") +
        "</div>" +
        "<div>" + formatRichText(r.definition || "") + "</div>" +
        "</div>";
    }
    return html;
  }

  /** Keep tooltip open while pointer is over it */
  var refTooltipPinned = false;

  function showStudyTooltip(target, clientX, clientY) {
    var kind = target.getAttribute("data-kind") || "bible";
    var label = target.getAttribute("data-label") || "";
    var tip = el("ref-tooltip");
    el("rt-title").textContent = label;
    el("rt-body").innerHTML = '<span class="rt-loading">Cargando…</span>';
    el("rt-meta").textContent = "";
    tip.classList.add("visible");
    positionTooltip(clientX, clientY);

    if (kind === "bible") {
      if (!state.bible) {
        el("rt-meta").textContent = "Sin Biblia";
        el("rt-body").innerHTML =
          '<span class="rt-loading">Elegí una Biblia en la pestaña Biblia para ver el texto al pasar el cursor.</span>';
        return;
      }
      var book = parseInt(target.getAttribute("data-book"), 10);
      var chapter = parseInt(target.getAttribute("data-chapter"), 10);
      var v1 = parseInt(target.getAttribute("data-v1"), 10);
      var v2 = parseInt(target.getAttribute("data-v2"), 10);
      el("rt-meta").textContent =
        (state.bible.abbreviation || state.bible.filename) + " · vista rápida";
      fetchChapterVerses(book, chapter)
        .then(function (pack) {
          el("rt-meta").textContent =
            (pack.translation || pack.title || "") + " · no mueve tu capítulo abierto";
          el("rt-body").innerHTML = formatVerseLines(versesInRange(pack, v1, v2));
          positionTooltip(clientX, clientY);
        })
        .catch(function (e) {
          el("rt-body").innerHTML =
            '<span class="rt-loading">' + escapeHtml(String(e.message || e)) + "</span>";
        });
      return;
    }

    // strong | lemma → try lexicons until we get content
    var term = target.getAttribute("data-term") || "";
    el("rt-meta").textContent = "Buscando en léxicos…";
    fetchLexiconEntries(term)
      .then(function (pack) {
        if (!pack.rows || !pack.rows.length) {
          el("rt-meta").textContent = "Sin entrada";
          el("rt-body").innerHTML = formatLexLines([],
            "No hay entrada para «" + escapeHtml(term) + "» en tus léxicos e-Sword. " +
            "Instalá Strong ° / Chávez / Vine en e-Sword.");
          return;
        }
        el("rt-meta").textContent = (pack.moduleTitle || "Léxico") + " · vista rápida";
        el("rt-body").innerHTML = formatLexLines(pack.rows);
        positionTooltip(clientX, clientY);
      })
      .catch(function (e) {
        el("rt-meta").textContent = "Error";
        el("rt-body").innerHTML =
          '<span class="rt-loading">' + escapeHtml(String(e.message || e)) + "</span>";
      });
  }

  /** Sync Léxico tab to this Strong’s / lemma (interlink). */
  function syncLexiconTab(term, openTab) {
    term = String(term || "").trim();
    if (!term) return;
    preferStrongLexicon();
    if (state.lex && el("sel-mod") && state.tab === "lexico") {
      el("sel-mod").value = state.lex.path;
    }
    if (openTab) goToStudyTab("lexico");
    var input = el("term-search");
    if (input) {
      input.value = term;
    }
    // Mark selection chip-friendly
    if (/^[HG]\d+/i.test(term)) {
      state.selectedWord = term.toUpperCase();
      state.selectedStrongs = [{
        strong: term.toUpperCase(),
        spanish: "",
        greek: "",
        translit: "",
        sourceModule: "link"
      }];
    } else {
      state.selectedWord = term;
    }
    updateSelectionUI();
    if (typeof runLexiconSearch === "function") {
      runLexiconSearch(term, false);
    }
  }

  function openStudyPanel(target) {
    var kind = target.getAttribute("data-kind") || "bible";
    var label = target.getAttribute("data-label") || "";
    el("rp-title").textContent = label;
    el("rp-body").innerHTML = '<span class="rt-loading">Cargando…</span>';
    el("ref-panel-bg").classList.add("open");
    hideRefTooltip();

    if (kind === "bible") {
      var book = parseInt(target.getAttribute("data-book"), 10);
      var chapter = parseInt(target.getAttribute("data-chapter"), 10);
      var v1 = parseInt(target.getAttribute("data-v1"), 10);
      var v2 = parseInt(target.getAttribute("data-v2"), 10);
      el("rp-meta").textContent = state.bible
        ? "Biblia actual: " + (state.bible.abbreviation || state.bible.filename) +
          " · solo lectura (no cambia tu ubicación)"
        : "Elige una Biblia en la pestaña Biblia";
      fetchChapterVerses(book, chapter)
        .then(function (pack) {
          el("rp-meta").textContent =
            "Biblia: " + (pack.translation || pack.title || "") +
            " · vista rápida · no cambia libro/capítulo abierto";
          var html = formatVerseLines(versesInRange(pack, v1, v2));
          html +=
            '<div style="margin-top:12px">' +
            '<button type="button" id="rp-goto-verse" class="rp-action">Ir a este pasaje en Biblia</button>' +
            "</div>";
          el("rp-body").innerHTML = html;
          var btn = el("rp-goto-verse");
          if (btn) {
            btn.onclick = function () {
              closeRefPanel();
              state.bookNumber = book;
              state.chapter = chapter;
              state.selectedVerse = v1;
              state.selectedWord = null;
              state.selectedWordIndex = -1;
              state.highlightMode = "verse";
              fillChapterSelect();
              var chSel = el("sel-chapter");
              if (chSel) chSel.value = String(chapter);
              syncHeaderPills();
              goToStudyTab("biblia");
              loadChapter();
            };
          }
        })
        .catch(function (e) {
          el("rp-body").innerHTML =
            '<span class="rt-loading">' + escapeHtml(String(e.message || e)) + "</span>";
        });
      return;
    }

    var term = target.getAttribute("data-term") || "";
    el("rp-meta").textContent = "Léxico · sincronizando…";
    // Sync Léxico tab in background so modules stay linked
    syncLexiconTab(term, false);
    fetchLexiconEntries(term)
      .then(function (pack) {
        if (!pack.rows || !pack.rows.length) {
          el("rp-meta").textContent = "Sin entrada en léxicos";
          el("rp-body").innerHTML = formatLexLines([],
            "No se encontró «" + escapeHtml(term) + "». Revisá que Strong ° u otro léxico esté en e-Sword.");
          return;
        }
        el("rp-meta").textContent = "Léxico: " + (pack.moduleTitle || "") + " · solo lectura";
        var html = formatLexLines(pack.rows);
        html +=
          '<div style="margin-top:12px">' +
          '<button type="button" id="rp-open-lex" class="rp-action">Abrir en pestaña Léxico</button>' +
          "</div>";
        el("rp-body").innerHTML = html;
        var b = el("rp-open-lex");
        if (b) {
          b.onclick = function () {
            closeRefPanel();
            syncLexiconTab(term, true);
          };
        }
      })
      .catch(function (e) {
        el("rp-body").innerHTML =
          '<span class="rt-loading">' + escapeHtml(String(e.message || e)) + "</span>";
      });
  }

  function closeRefPanel() {
    el("ref-panel-bg").classList.remove("open");
  }

  function wireRefPopups() {
    document.addEventListener("mouseover", function (ev) {
      var tip = el("ref-tooltip");
      if (ev.target.closest && ev.target.closest("#ref-tooltip")) {
        if (refHideTimer) { clearTimeout(refHideTimer); refHideTimer = null; }
        refTooltipPinned = true;
        return;
      }
      var t = ev.target.closest && ev.target.closest(".study-ref");
      if (!t) return;
      refTooltipPinned = false;
      if (refHideTimer) { clearTimeout(refHideTimer); refHideTimer = null; }
      if (refHoverTimer) clearTimeout(refHoverTimer);
      var x = ev.clientX, y = ev.clientY;
      refHoverTimer = setTimeout(function () {
        showStudyTooltip(t, x, y);
      }, 160);
    });
    document.addEventListener("mousemove", function (ev) {
      var tip = el("ref-tooltip");
      if (!tip.classList.contains("visible")) return;
      if (ev.target.closest && (ev.target.closest(".study-ref") || ev.target.closest("#ref-tooltip"))) {
        if (!ev.target.closest("#ref-tooltip")) {
          positionTooltip(ev.clientX, ev.clientY);
        }
      }
    });
    document.addEventListener("mouseout", function (ev) {
      var related = ev.relatedTarget;
      if (related && related.closest) {
        if (related.closest("#ref-tooltip") || related.closest(".study-ref")) return;
      }
      var t = ev.target.closest && (ev.target.closest(".study-ref") || ev.target.closest("#ref-tooltip"));
      if (!t && !refTooltipPinned) return;
      if (refHoverTimer) { clearTimeout(refHoverTimer); refHoverTimer = null; }
      refHideTimer = setTimeout(function () {
        refTooltipPinned = false;
        hideRefTooltip();
      }, 220);
    });
    document.addEventListener("click", function (ev) {
      var t = ev.target.closest && ev.target.closest(".study-ref");
      if (t) {
        ev.preventDefault();
        ev.stopPropagation();
        openStudyPanel(t);
        return;
      }
    });
    el("rp-close").onclick = closeRefPanel;
    el("ref-panel-bg").onclick = function (ev) {
      if (ev.target === el("ref-panel-bg")) closeRefPanel();
    };
  }

  /** Build clickable words, preserving WOC red segments. */
  function appendTokenizedText(container, rawText, verseNum) {
    // Split by WOC markers first
    var chunks = [];
    var re = new RegExp(WOC_OPEN + "([\\s\\S]*?)" + WOC_CLOSE, "g");
    var last = 0;
    var m;
    while ((m = re.exec(rawText)) !== null) {
      if (m.index > last) {
        chunks.push({ woc: false, text: rawText.slice(last, m.index) });
      }
      chunks.push({ woc: true, text: m[1] });
      last = m.index + m[0].length;
    }
    if (last < rawText.length) {
      chunks.push({ woc: false, text: rawText.slice(last) });
    }
    if (!chunks.length) chunks.push({ woc: false, text: rawText });

    var wordIdx = 0;
    for (var c = 0; c < chunks.length; c++) {
      var chunk = chunks[c];
      var wrap = null;
      if (chunk.woc) {
        wrap = document.createElement("span");
        wrap.className = "woc";
        container.appendChild(wrap);
      }
      var parent = wrap || container;
      var parts = tokenizeWords(chunk.text);
      for (var p = 0; p < parts.length; p++) {
        var part = parts[p];
        if (part.type === "word") {
          var thisIdx = wordIdx;
          wordIdx++;
          var w = document.createElement("span");
          w.className = "word";
          w.textContent = part.text;
          var markStyle = getWordMarkStyle(verseNum, thisIdx);
          if (markStyle) w.classList.add("mark-" + markStyle);
          if (getWordUnderline(verseNum, thisIdx)) w.classList.add("mark-ul");
          if (
            state.highlightMode === "word" &&
            state.selectedVerse === verseNum &&
            state.selectedWordIndex === thisIdx
          ) {
            w.classList.add("word-hl");
          }
          (function (vNum, wordText, idx) {
            w.onclick = function (ev) {
              ev.stopPropagation();
              if (
                state.selectedVerse === vNum &&
                state.selectedWordIndex === idx &&
                state.highlightMode === "word"
              ) {
                clearSelection();
                return;
              }
              selectWord(vNum, wordText, idx);
            };
            w.oncontextmenu = function (ev) {
              handleVerseContextMenu(ev, vNum, wordText, idx);
            };
          })(verseNum, part.text, thisIdx);
          parent.appendChild(w);
        } else {
          parent.appendChild(document.createTextNode(part.text));
        }
      }
    }
    return wordIdx;
  }

  // Solo UNA selección activa a la vez (no multi-highlight persistente)
  // selectedWordIndex: índice de la palabra dentro del versículo (-1 = ninguna)

  function bookMeta() {
    for (var i = 0; i < state.books.length; i++) {
      if (state.books[i].number === state.bookNumber) return state.books[i];
    }
    return { name: "Libro", chapters: 1, number: state.bookNumber };
  }

  function noteKey() {
    if (state.selectedVerse == null) return null;
    return state.bookNumber + "|" + state.chapter + "|" + state.selectedVerse;
  }

  function pickDefaultBible(list) {
    var prefer = [/reina valera \(?1960\)?/i, /rv\s*1960/i, /rv\s*60/i];
    for (var p = 0; p < prefer.length; p++) {
      for (var i = 0; i < list.length; i++) {
        var b = list[i];
        var hay = (b.title || "") + " " + (b.abbreviation || "") + " " + (b.filename || "");
        if (prefer[p].test(hay) && !/interlineal/i.test(hay)) return b;
      }
    }
    for (var j = 0; j < list.length; j++) {
      if (!/interlineal|\+/i.test(list[j].title + list[j].abbreviation)) return list[j];
    }
    return list[0] || null;
  }

  function el(id) { return document.getElementById(id); }

  function showApp() {
    window.__ADC_APP_SHOWN__ = true;
    window.__ADC_BOOT_STARTED__ = true;
    var bootEl = el("boot");
    var appEl = el("app");
    if (bootEl) bootEl.style.display = "none";
    if (appEl) appEl.style.display = "flex";
  }

  // If mini-loader already painted, keep that flag when full app takes over.
  if (window.__ADC_APP_SHOWN__) {
    /* no-op: mini shell visible */
  }

  /**
   * @param {HTMLSelectElement} select
   * @param {Array} items
   * @param {function} getValue
   * @param {function} getLabel
   * @param {*} current
   * @param {function} [getTitle] optional tooltip / full name for each option
   * @param {boolean} [showContentMarks] if true, prefix ● for modules with content (study modules only)
   */
  function fillSelect(select, items, getValue, getLabel, current, getTitle, showContentMarks) {
    select.innerHTML = "";
    for (var i = 0; i < items.length; i++) {
      var item = items[i];
      var o = document.createElement("option");
      var val = getValue(item);
      o.value = val;
      var label = String(getLabel(item) || "");
      // Strip any leftover content markers from labels
      label = label.replace(/^[●○]\s+/, "");
      // 🔒 = e-Sword sqliteplus ciphertext (not readable outside e-Sword)
      var lock = item && item.encrypted ? "🔒 " : "";
      // ● only for study modules that have content — never for books/bibles
      var has = !!(showContentMarks && val && state.contentFlags[val]);
      var mark = "";
      if (showContentMarks && has) {
        mark = "● ";
        o.setAttribute("data-has-content", "1");
      }
      o.textContent = mark + lock + label;
      // Hover tooltip (full name) when the OS/browser shows option titles
      var tipParts = [];
      if (typeof getTitle === "function") {
        var tip = getTitle(item);
        if (tip) tipParts.push(tip);
      }
      if (showContentMarks) {
        if (has) tipParts.push("Tiene contenido para la selección actual");
        else tipParts.push("Sin entrada para la selección actual");
      }
      if (item && item.encrypted) {
        tipParts.push(
          "Cifrado por e-Sword (solo se abre en e-Sword). Preferí .cmti/.bbli/.dcti/.lexi del mismo recurso si existe."
        );
      }
      if (tipParts.length) o.title = tipParts.join(" · ");
      select.appendChild(o);
    }
    if (current != null) select.value = current;
    // Closed dropdown: hovering the control shows the selected item's full title
    if (typeof getTitle === "function") {
      select._getTitleForValue = function (value) {
        for (var j = 0; j < items.length; j++) {
          if (getValue(items[j]) === value) {
            var t = getTitle(items[j]) || "";
            if (items[j].encrypted) t += " · 🔒 cifrado e-Sword";
            return t;
          }
        }
        return "";
      };
      updateSelectTitle(select);
    }
  }

  function updateSelectTitle(select) {
    if (!select) return;
    if (typeof select._getTitleForValue === "function") {
      var tip = select._getTitleForValue(select.value);
      select.title = tip || "Biblia";
    }
  }

  function bibleFullName(b) {
    if (!b) return "";
    var abbr = b.abbreviation || b.filename || "";
    var title = (b.title || "").trim();
    if (title && abbr && title.toLowerCase() !== String(abbr).toLowerCase()) {
      return abbr + " — " + title;
    }
    return title || abbr || b.filename || "";
  }

  /**
   * Probe modules in background (fast EXISTS). Never blocks loading content.
   * after() is called immediately with current flags, then again when probe finishes.
   */
  function loadCmtLevelPrefs() {
    try {
      var raw = JSON.parse(localStorage.getItem(CMT_LEVELS_KEY) || "{}");
      if (typeof raw.verse === "boolean") state.cmtIncludeVerse = raw.verse;
      if (typeof raw.chapter === "boolean") state.cmtIncludeChapter = raw.chapter;
      if (typeof raw.book === "boolean") state.cmtIncludeBook = raw.book;
      // Never allow all-off
      if (!state.cmtIncludeVerse && !state.cmtIncludeChapter && !state.cmtIncludeBook) {
        state.cmtIncludeVerse = true;
      }
    } catch (e) {}
  }

  function saveCmtLevelPrefs() {
    try {
      localStorage.setItem(CMT_LEVELS_KEY, JSON.stringify({
        verse: !!state.cmtIncludeVerse,
        chapter: !!state.cmtIncludeChapter,
        book: !!state.cmtIncludeBook
      }));
    } catch (e) {}
  }

  function syncCmtLevelCheckboxes() {
    var v = el("cmt-lv-verse");
    var c = el("cmt-lv-chapter");
    var b = el("cmt-lv-book");
    if (v) v.checked = !!state.cmtIncludeVerse;
    if (c) c.checked = !!state.cmtIncludeChapter;
    if (b) b.checked = !!state.cmtIncludeBook;
  }

  function readCmtLevelsFromUI() {
    var v = el("cmt-lv-verse");
    var c = el("cmt-lv-chapter");
    var b = el("cmt-lv-book");
    state.cmtIncludeVerse = !!(v && v.checked);
    state.cmtIncludeChapter = !!(c && c.checked);
    state.cmtIncludeBook = !!(b && b.checked);
    if (!state.cmtIncludeVerse && !state.cmtIncludeChapter && !state.cmtIncludeBook) {
      state.cmtIncludeVerse = true;
      if (v) v.checked = true;
    }
    saveCmtLevelPrefs();
  }

  function cmtLevelArgs() {
    return {
      includeVerse: !!state.cmtIncludeVerse,
      includeChapter: !!state.cmtIncludeChapter,
      includeBook: !!state.cmtIncludeBook
    };
  }

  function cmtLevelLabel() {
    var parts = [];
    if (state.cmtIncludeVerse) parts.push("versículo");
    if (state.cmtIncludeChapter) parts.push("capítulo");
    if (state.cmtIncludeBook) parts.push("libro");
    return parts.length ? parts.join(" + ") : "versículo";
  }

  function studyModulesForKind(kind) {
    if (kind === "commentary") return state.commentaries || [];
    if (kind === "dictionary") return state.dictionaries || [];
    if (kind === "lexicon") return state.lexicons || [];
    return [];
  }

  function currentStudyModule(kind) {
    if (kind === "commentary") return state.cmt;
    if (kind === "dictionary") return state.dict;
    if (kind === "lexicon") return state.lex;
    return null;
  }

  function studySideIds(kind) {
    if (kind === "commentary") return { list: "cmt-side-list", hint: "cmt-side-hint" };
    if (kind === "dictionary") return { list: "dict-side-list", hint: "dict-side-hint" };
    if (kind === "lexicon") return { list: "lex-side-list", hint: "lex-side-hint" };
    return null;
  }

  function tabKind() {
    if (state.tab === "comentario") return "commentary";
    if (state.tab === "diccionario") return "dictionary";
    if (state.tab === "lexico") return "lexicon";
    return null;
  }

  /** Switch primary study module (updates dropdown + reloads content). */
  function selectStudyModule(kind, path) {
    var list = studyModulesForKind(kind);
    var mod = null;
    for (var i = 0; i < list.length; i++) {
      if (list[i].path === path) { mod = list[i]; break; }
    }
    if (!mod) return;
    if (kind === "commentary") state.cmt = mod;
    else if (kind === "dictionary") state.dict = mod;
    else if (kind === "lexicon") state.lex = mod;
    var sel = el("sel-mod");
    if (sel) {
      sel.value = path;
      // Trigger existing onchange loaders
      if (typeof sel.onchange === "function") sel.onchange();
      else {
        try { sel.dispatchEvent(new Event("change")); } catch (e) {}
      }
    } else {
      // Fallback reload
      if (kind === "commentary") loadCommentariesForVerse();
      else if (kind === "dictionary") {
        var td = (el("term-search") && el("term-search").value.trim()) || state.selectedWord || "";
        if (td) runDictionarySearch(td, false);
      } else if (kind === "lexicon") {
        var tl = (el("term-search") && el("term-search").value.trim()) || state.selectedWord || "";
        if (tl) runLexiconSearch(tl, false);
      }
    }
    renderStudySideRail(kind);
  }

  /** Term used for module probes (Strong’s for léxico, lema for diccionario). */
  function probeTermForKind(kind) {
    var typed = (el("term-search") && el("term-search").value.trim()) || "";
    if (kind === "lexicon") {
      if (state.selectedStrongs && state.selectedStrongs.length && state.selectedStrongs[0].strong) {
        return String(state.selectedStrongs[0].strong).trim();
      }
      if (typed && /^[HG]\d+/i.test(typed)) return typed.toUpperCase();
      if (state.selectedWord && /^[HG]\d+/i.test(state.selectedWord.trim())) {
        return state.selectedWord.trim().toUpperCase();
      }
      return typed || state.selectedWord || "";
    }
    if (kind === "dictionary") {
      return typed || state.selectedWord || "";
    }
    // commentary: optional text search only; verse probe ignores term
    return typed || "";
  }

  function contentFlagCount() {
    var n = 0;
    for (var k in state.contentFlags) {
      if (state.contentFlags[k]) n++;
    }
    return n;
  }

  /**
   * Side column: modules that have content for the current verse / term.
   * ● = has entry · primary highlighted · click to switch.
   */
  function renderStudySideRail(kind) {
    var ids = studySideIds(kind);
    if (!ids) return;
    var box = el(ids.list);
    var hint = el(ids.hint);
    if (!box) return;

    var list = studyModulesForKind(kind);
    var primary = currentStudyModule(kind);
    var primaryPath = primary && primary.path;
    var probing = !!state._probeInFlight;
    var withContent = [];
    var without = [];
    for (var i = 0; i < list.length; i++) {
      if (state.contentFlags[list[i].path]) withContent.push(list[i]);
      else without.push(list[i]);
    }

    // Primary first among those with content
    withContent.sort(function (a, b) {
      if (primaryPath && a.path === primaryPath) return -1;
      if (primaryPath && b.path === primaryPath) return 1;
      return (a.abbreviation || a.filename || "").localeCompare(b.abbreviation || b.filename || "");
    });

    var probeTerm = probeTermForKind(kind);
    if (hint) {
      if (probing) {
        hint.textContent = "Sondeando módulos con contenido…";
      } else if (kind === "commentary") {
        if (state.selectedVerse == null) {
          hint.textContent = "Selecciona un versículo en Biblia para ver qué comentarios tienen nota.";
        } else {
          hint.textContent = withContent.length
            ? "● " + withContent.length + " con nota para este versículo · clic para abrir"
            : "Ningún módulo tiene nota (filtro actual) para este versículo.";
        }
      } else if (kind === "dictionary") {
        if (!probeTerm) {
          hint.textContent = "Selecciona una palabra o escribe un lema para ver diccionarios con entrada.";
        } else {
          hint.textContent = withContent.length
            ? "● " + withContent.length + " con entrada para «" + probeTerm + "» · clic para abrir"
            : "Ningún diccionario tiene entrada para «" + probeTerm + "».";
        }
      } else {
        if (!probeTerm) {
          hint.textContent = "Selecciona una palabra (o Strong’s H/G) para ver léxicos con definición.";
        } else {
          hint.textContent = withContent.length
            ? "● " + withContent.length + " con definición para «" + probeTerm + "» · clic para abrir"
            : "Ningún léxico tiene «" + probeTerm + "». Prueba Strong’s (G26, H1254).";
        }
      }
    }

    box.innerHTML = "";
    if (probing && !withContent.length) {
      var loading = document.createElement("div");
      loading.className = "study-side-empty";
      loading.textContent = "Buscando módulos…";
      box.appendChild(loading);
      return;
    }

    if (!withContent.length) {
      var empty = document.createElement("div");
      empty.className = "study-side-empty";
      if (kind === "commentary" && state.selectedVerse == null) {
        empty.textContent = "Elige un versículo en la pestaña Biblia.";
      } else if ((kind === "dictionary" || kind === "lexicon") && !probeTerm) {
        empty.textContent = "Elige una palabra en Biblia o escribe el término arriba.";
      } else {
        empty.textContent = "Sin módulos con contenido para esta selección.";
      }
      box.appendChild(empty);
      return;
    }

    for (var k = 0; k < withContent.length; k++) {
      (function (mod) {
        var btn = document.createElement("button");
        btn.type = "button";
        var isPrimary = primaryPath && mod.path === primaryPath;
        btn.className = "study-side-item has-content" + (isPrimary ? " active" : "");
        btn.title = "● Tiene contenido · " + (mod.title || "") + " — " + (mod.filename || "");
        var abbr = mod.abbreviation || mod.filename || "Módulo";
        var lock = mod.encrypted ? "🔒 " : "";
        btn.innerHTML =
          '<span class="ssi-abbr"><span class="ssi-dot" aria-hidden="true">●</span> ' +
          lock + escapeHtml(abbr) +
          (isPrimary ? " · principal" : "") + "</span>" +
          '<span class="ssi-title">' + escapeHtml(mod.title || mod.filename || "") + "</span>";
        btn.onclick = function () {
          if (mod.path === primaryPath) return;
          selectStudyModule(kind, mod.path);
        };
        box.appendChild(btn);
      })(withContent[k]);
    }

    // Compact footer: how many modules lack content (collapsed, not interactive list)
    if (without.length && !probing) {
      var foot = document.createElement("div");
      foot.className = "study-side-empty";
      foot.style.marginTop = "8px";
      foot.textContent = "○ " + without.length + " sin entrada para esta selección";
      box.appendChild(foot);
    }
  }

  /** Extract Hebrew/Greek script runs from text. */
  function extractOriginalScript(text) {
    var s = String(text || "");
    var m = s.match(/[\u0590-\u05FF\uFB1D-\uFB4F]{2,}/);
    if (m) return m[0];
    m = s.match(/[\u0370-\u03FF\u1F00-\u1FFF]{2,}/);
    return m ? m[0] : "";
  }

  /**
   * Build lemma context for dict/lex: original (H/G script), Spanish, Strong’s, translit.
   */
  function buildLemmaContext(rows) {
    var spanish = state.selectedWord || "";
    var original = "";
    var strong = "";
    var translit = "";
    var source = "";

    if (state.selectedStrongs && state.selectedStrongs.length) {
      var h = state.selectedStrongs[0];
      strong = h.strong || "";
      original = h.greek || "";
      translit = h.translit || "";
      spanish = h.spanish || state.selectedWord || spanish;
      source = h.sourceModule || "";
    }

    var termInput = (el("term-search") && el("term-search").value.trim()) || "";
    if (!strong && /^[HG]\d+/i.test(termInput)) {
      strong = termInput.toUpperCase().replace(/\s+/g, "");
    }
    if (rows && rows.length) {
      var r0 = rows[0];
      if (!strong && r0.term && /^[HG]\d+/i.test(r0.term)) {
        strong = String(r0.term).toUpperCase();
      }
      if (!original) {
        original = extractOriginalScript(r0.term || "") ||
          extractOriginalScript(r0.definition || "");
      }
      // Headword that is not Strong’s code becomes Spanish-facing lemma
      if (!spanish && r0.term && !/^[HG]\d+/i.test(r0.term) && !extractOriginalScript(r0.term)) {
        spanish = r0.term;
      }
    }

    // If Spanish still empty but we have selectedWord that isn't Strong’s
    if (!spanish && state.selectedWord && !/^[HG]\d+/i.test(state.selectedWord)) {
      spanish = state.selectedWord;
    }
    // If only Strong’s typed, use term as display code (spanish may stay empty)
    if (!spanish && !original && strong) {
      spanish = ""; // keep explicit empty so UI can say "sin glosa ES"
    }

    return {
      spanish: spanish || "",
      original: original || "",
      strong: strong || "",
      translit: translit || "",
      source: source || ""
    };
  }

  function renderLemmaHeaderEl(ctx, moduleLabel) {
    var wrap = document.createElement("div");
    wrap.className = "lemma-header";
    var origLine = ctx.original || (ctx.strong ? ctx.strong : "—");
    var esLine = ctx.spanish
      ? ctx.spanish
      : (ctx.strong ? "(sin glosa en español — selecciona la palabra en la Biblia)" : "—");
    wrap.innerHTML =
      '<div class="lemma-original"></div>' +
      '<div class="lemma-spanish"><span class="lbl">ES</span><span class="es-val"></span></div>' +
      '<div class="lemma-meta"></div>';
    wrap.querySelector(".lemma-original").textContent = origLine;
    wrap.querySelector(".es-val").textContent = esLine;
    var metaBits = [];
    if (ctx.strong) metaBits.push("<code>" + escapeHtml(ctx.strong) + "</code>");
    if (ctx.translit) metaBits.push(escapeHtml(ctx.translit));
    if (moduleLabel) metaBits.push(escapeHtml(moduleLabel));
    if (ctx.source) metaBits.push(escapeHtml(ctx.source));
    wrap.querySelector(".lemma-meta").innerHTML = metaBits.join(" · ");
    return wrap;
  }

  function moduleLabelPlain(m) {
    return (m.abbreviation || m.filename || "") + " — " + (m.title || "");
  }

  function applyContentFlagsToDropdown(kind) {
    var sel = el("sel-mod");
    if (!sel || state.tab === "biblia" || state.tab === "temas") return;
    var cur = sel.value;
    var list =
      kind === "commentary" ? state.commentaries :
      kind === "dictionary" ? state.dictionaries : state.lexicons;
    if (!list || !list.length) return;
    // Sort: modules with content first, then alpha
    var sorted = list.slice().sort(function (a, b) {
      var ha = state.contentFlags[a.path] ? 0 : 1;
      var hb = state.contentFlags[b.path] ? 0 : 1;
      if (ha !== hb) return ha - hb;
      return (a.abbreviation || a.filename || "").localeCompare(b.abbreviation || b.filename || "");
    });
    if (kind === "commentary") state.commentaries = sorted;
    else if (kind === "dictionary") state.dictionaries = sorted;
    else state.lexicons = sorted;
    fillSelect(sel, sorted, function (m) { return m.path; }, moduleLabelPlain, cur,
      function (m) {
        return (m.title || "") + " · " + (m.filename || "") +
          (state.contentFlags[m.path] ? " · ● con contenido" : " · sin entrada");
      },
      true);
  }

  function refreshContentFlags(kind, items, after) {
    if (after) after(); // paint UI immediately
    renderStudySideRail(kind);
    if (!items || !items.length) {
      state.contentFlags = {};
      renderStudySideRail(kind);
      return;
    }

    // Commentary needs a verse; dict/lex need a term (Strong’s for lex)
    var term = probeTermForKind(kind);
    if (kind === "commentary" && state.selectedVerse == null) {
      state.contentFlags = {};
      state._probeInFlight = false;
      applyContentFlagsToDropdown(kind);
      renderStudySideRail(kind);
      return;
    }
    if ((kind === "dictionary" || kind === "lexicon") && !String(term || "").trim()) {
      state.contentFlags = {};
      state._probeInFlight = false;
      applyContentFlagsToDropdown(kind);
      renderStudySideRail(kind);
      return;
    }

    var paths = items.map(function (m) { return m.path; });
    var probeId = (state._probeSeq = (state._probeSeq || 0) + 1);
    // Debounce probes
    if (state._probeTimer) clearTimeout(state._probeTimer);
    state._probeInFlight = true;
    renderStudySideRail(kind);

    state._probeTimer = setTimeout(function () {
      var payload = {
        kind: kind,
        paths: paths,
        bookNumber: state.bookNumber,
        chapter: state.chapter,
        verse: state.selectedVerse != null ? state.selectedVerse : 1,
        term: term || null
      };
      if (kind === "commentary") {
        var lv = cmtLevelArgs();
        payload.includeVerse = lv.includeVerse;
        payload.includeChapter = lv.includeChapter;
        payload.includeBook = lv.includeBook;
      }
      invoke("probe_modules_content", payload).then(function (probes) {
        // Ignore stale probe results
        if (probeId !== state._probeSeq) return;
        state._probeInFlight = false;
        state.contentFlags = {};
        for (var i = 0; i < (probes || []).length; i++) {
          var p = probes[i];
          var pathKey = p.path || p.Path || "";
          var has = !!(p.hasContent || p.has_content);
          if (pathKey && has) state.contentFlags[pathKey] = true;
        }
        // Normalize flags onto known module paths (Windows path slash safety)
        var list =
          kind === "commentary" ? state.commentaries :
          kind === "dictionary" ? state.dictionaries : state.lexicons;
        var normalized = {};
        for (var j = 0; j < (list || []).length; j++) {
          var mp = list[j].path;
          if (state.contentFlags[mp]) {
            normalized[mp] = true;
            continue;
          }
          // match ignoring slash direction / case
          var mpN = String(mp || "").replace(/\//g, "\\").toLowerCase();
          for (var fk in state.contentFlags) {
            if (!state.contentFlags[fk]) continue;
            var fkN = String(fk || "").replace(/\//g, "\\").toLowerCase();
            if (fkN === mpN) {
              normalized[mp] = true;
              break;
            }
          }
        }
        state.contentFlags = normalized;
        applyContentFlagsToDropdown(kind);
        renderStudySideRail(kind);
      }).catch(function (err) {
        if (probeId !== state._probeSeq) return;
        state._probeInFlight = false;
        console.warn("probe_modules_content failed", err);
        renderStudySideRail(kind);
        var hint = studySideIds(kind) && el(studySideIds(kind).hint);
        if (hint) {
          hint.textContent = "No se pudo sondear módulos: " + String(err.message || err);
        }
      });
    }, 80);
  }

  function fillChapterSelect() {
    var max = bookMeta().chapters || 1;
    var sel = el("sel-chapter");
    if (!sel) return;
    sel.innerHTML = "";
    for (var c = 1; c <= max; c++) {
      var o = document.createElement("option");
      o.value = String(c);
      o.textContent = String(c);
      sel.appendChild(o);
    }
    sel.value = String(state.chapter);
  }

  function closeAllPillDropdowns() {
    var dds = document.querySelectorAll(".pill-dd.open");
    for (var i = 0; i < dds.length; i++) {
      dds[i].classList.remove("open");
      var btn = dds[i].querySelector(".pill-dd-btn");
      var panel = dds[i].querySelector(".pill-dd-panel");
      if (btn) btn.setAttribute("aria-expanded", "false");
      if (panel) panel.hidden = true;
    }
  }

  function togglePillDropdown(id) {
    var root = el(id);
    if (!root) return;
    var wasOpen = root.classList.contains("open");
    closeAllPillDropdowns();
    if (wasOpen) return;
    // Rebuild list when opening so active mark is correct after a silent switch
    if (id === "pill-bible") {
      try { fillBiblePill(); } catch (e) { console.warn(e); }
    } else if (id === "pill-book") {
      try { fillBookPill(); } catch (e) { console.warn(e); }
    }
    root.classList.add("open");
    var btn = root.querySelector(".pill-dd-btn");
    var panel = root.querySelector(".pill-dd-panel");
    if (btn) btn.setAttribute("aria-expanded", "true");
    if (panel) {
      panel.hidden = false;
      // Scroll active item into view
      var active = panel.querySelector(".pill-dd-item.active");
      if (active && active.scrollIntoView) {
        try { active.scrollIntoView({ block: "nearest" }); } catch (e) {}
      }
    }
  }

  /** Full book name on the pill (custom control — not native select). */
  function syncBookPill() {
    var text = el("pill-book-text");
    var btn = el("pill-book-btn");
    if (!text) return;
    var name = bookMeta().name || "Libro";
    text.textContent = name;
    if (btn) btn.title = name;
  }

  function syncBiblePill() {
    var text = el("pill-bible-text");
    var btn = el("pill-bible-btn");
    if (!text) return;
    var b = state.bible;
    var label = b ? (b.abbreviation || b.filename || "Biblia") : "Biblia";
    text.textContent = label;
    if (btn) btn.title = bibleFullName(b) || label;
  }

  function fillBookPill() {
    var panel = el("pill-book-panel");
    if (!panel) return;
    panel.innerHTML = "";
    var books = state.books || [];
    for (var i = 0; i < books.length; i++) {
      (function (b) {
        var item = document.createElement("button");
        item.type = "button";
        item.className = "pill-dd-item" +
          (b.number === state.bookNumber ? " active" : "");
        item.setAttribute("role", "option");
        item.textContent = b.name;
        item.onclick = function () {
          closeAllPillDropdowns();
          if (state.bookNumber === b.number) return;
          state.bookNumber = b.number;
          state.chapter = 1;
          state.selectedVerse = null;
          state.selectedWord = null;
          syncBookPill();
          fillBookPill(); // refresh active mark
          fillChapterSelect();
          loadChapter();
        };
        panel.appendChild(item);
      })(books[i]);
    }
    syncBookPill();
  }

  function fillBiblePill() {
    var panel = el("pill-bible-panel");
    if (!panel) return;
    panel.innerHTML = "";
    var list = state.bibles || [];
    var curPath = state.bible && state.bible.path;
    // DocumentFragment: one DOM insert instead of 50+ reflows (smoother under glass)
    var frag = document.createDocumentFragment();
    for (var i = 0; i < list.length; i++) {
      (function (b) {
        var item = document.createElement("button");
        item.type = "button";
        item.className = "pill-dd-item" + (curPath && b.path === curPath ? " active" : "");
        item.setAttribute("role", "option");
        var abbr = b.abbreviation || b.filename || "Biblia";
        var full = (b.title || "").trim();
        var main = document.createElement("span");
        main.textContent = abbr;
        item.appendChild(main);
        if (full && full.toLowerCase() !== String(abbr).toLowerCase()) {
          var sub = document.createElement("span");
          sub.className = "pdd-sub";
          sub.textContent = full;
          item.appendChild(sub);
        }
        item.title = bibleFullName(b) || abbr;
        item.onclick = function () {
          try {
            closeAllPillDropdowns();
            if (state.bible && state.bible.path === b.path) return;
            state.bible = b;
            syncBiblePill();
            // Don't rebuild the whole 50+ item list mid-click (can freeze WebView2);
            // refresh active mark the next time the menu opens via togglePillDropdown.
            if (state.parallelPaths[0] === b.path) state.parallelPaths[0] = "";
            if (state.parallelPaths[1] === b.path) state.parallelPaths[1] = "";
            saveParallelPrefs();
            try { fillParallelSelects(); } catch (pe) { console.warn(pe); }
            loadChapter();
          } catch (err) {
            console.error("bible switch failed", err);
            var errBox = el("chapter-error");
            if (errBox) {
              errBox.style.display = "block";
              errBox.textContent = "Error al cambiar Biblia: " + (err.message || err);
            }
          }
        };
        frag.appendChild(item);
      })(list[i]);
    }
    panel.appendChild(frag);
    syncBiblePill();
  }

  /** Keep header book/bible labels in sync after nav jumps */
  function syncHeaderPills() {
    syncBookPill();
    syncBiblePill();
    fillBookPill();
    fillBiblePill();
  }

  function tokenizeWords(text) {
    // Keep punctuation attached loosely; clickable tokens = sequences of letters/digits incl. accents
    var parts = [];
    var re = /([A-Za-zÁÉÍÓÚÜÑáéíóúüñ0-9]+)|([^A-Za-zÁÉÍÓÚÜÑáéíóúüñ0-9]+)/g;
    var m;
    while ((m = re.exec(text)) !== null) {
      if (m[1]) parts.push({ type: "word", text: m[1] });
      else if (m[2]) parts.push({ type: "other", text: m[2] });
    }
    return parts;
  }

  function updateSelectionUI() {
    var chip = el("sel-chip");
    var detail = el("sel-detail");
    var bm = bookMeta();
    var has = state.selectedVerse != null || state.selectedWord;
    el("btn-to-dict").disabled = !state.selectedWord && state.selectedVerse == null;
    el("btn-to-lex").disabled = !state.selectedWord;
    el("btn-to-cmt").disabled = !state.selectedWord && state.selectedVerse == null;

    if (state.selectedWord) {
      chip.className = "chip";
      var strongLabel = "";
      if (state.selectedStrongs && state.selectedStrongs.length) {
        strongLabel = " · " + state.selectedStrongs.map(function (h) { return h.strong; }).join(", ");
      }
      chip.textContent = "«" + state.selectedWord + "»" + strongLabel;
      var line =
        (state.selectedVerse != null
          ? bm.name + " " + state.chapter + ":" + state.selectedVerse + " · "
          : "") +
        "Palabra: «" + state.selectedWord + "»";
      if (state.selectedStrongs && state.selectedStrongs.length) {
        var h0 = state.selectedStrongs[0];
        line += " → " + h0.strong;
        if (h0.greek) line += " " + h0.greek;
        if (h0.translit) line += " (" + h0.translit + ")";
      } else if (state.strongsNote) {
        line += " · " + state.strongsNote;
      }
      detail.textContent = line;
    } else if (state.selectedVerse != null) {
      chip.className = "chip";
      chip.textContent = bm.name + " " + state.chapter + ":" + state.selectedVerse;
      detail.textContent = "Versículo completo seleccionado: " +
        bm.name + " " + state.chapter + ":" + state.selectedVerse;
    } else {
      chip.className = "chip empty";
      chip.textContent = "Sin selección";
      detail.textContent = "Clic en un versículo o palabra.";
    }
    updateHlBarUI();
  }

  function selectVerse(verseNum, scroll) {
    // Una sola selección: limpia palabra anterior
    state.selectedVerse = verseNum;
    state.selectedWord = null;
    state.selectedWordIndex = -1;
    state.selectedStrongs = [];
    state.strongsNote = "";
    state.highlightMode = "verse";
    renderVerses(el("search-verse").value);
    updateNoteUI();
    updateSelectionUI();
    updateHlBarUI();
    updateFooter();
    saveNavPosition();
    // Always re-probe ● indicators for the new verse when on a study tab
    if (state.tab !== "biblia" && state.tab !== "temas") refreshModuleDropdown();
    if (scroll) {
      var node = document.querySelector('.verse[data-verse="' + verseNum + '"]');
      if (node) node.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  }

  function selectWord(verseNum, word, wordIndex) {
    // Solo UNA palabra a la vez (cualquier versículo/capítulo)
    state.selectedVerse = verseNum;
    state.selectedWord = word;
    state.selectedWordIndex = typeof wordIndex === "number" ? wordIndex : -1;
    state.selectedStrongs = [];
    state.strongsNote = "";
    state.highlightMode = "word";
    renderVerses(el("search-verse").value);
    updateNoteUI();
    updateSelectionUI();
    updateHlBarUI();
    updateFooter();
    saveNavPosition();
    // Sync search box on dict/lex so probes use the right term
    if (state.tab === "diccionario" && el("term-search")) {
      el("term-search").value = word || "";
    }
    // Probe now with Spanish word (dict); after Strong’s resolve, probe again (lex)
    if (state.tab !== "biblia" && state.tab !== "temas") refreshModuleDropdown();
    resolveSelectedStrongs(false).then(function () {
      if (state.tab === "lexico" && state.selectedStrongs && state.selectedStrongs.length) {
        if (el("term-search")) el("term-search").value = state.selectedStrongs[0].strong;
      }
      // Re-probe with Strong’s / updated selection
      if (state.tab !== "biblia" && state.tab !== "temas") refreshModuleDropdown();
    });
  }

  function clearSelection() {
    state.selectedWord = null;
    state.selectedWordIndex = -1;
    state.selectedStrongs = [];
    state.strongsNote = "";
    state.highlightMode = null;
    // keep selectedVerse for notes, but no temporary selection outline
    renderVerses(el("search-verse").value);
    updateSelectionUI();
    updateHlBarUI();
  }

  /** Candidate interlinear Bibles from the scanned module list */
  function interlinearCandidates() {
    var list = state.bibles || [];
    var scored = [];
    for (var i = 0; i < list.length; i++) {
      var b = list[i];
      var hay = ((b.title || "") + " " + (b.filename || "") + " " + (b.abbreviation || "")).toLowerCase();
      if (!/interlineal|interlinear/.test(hay) && !/\+/.test(b.filename || "")) {
        // keep modules that clearly carry Strong markup in name
        if (!/strong|irv\s*1960\+|griego/.test(hay)) continue;
      }
      if (!/interlineal|interlinear|strong|\+/.test(hay)) continue;
      var score = 0;
      if (/interlineal|interlinear/.test(hay)) score += 50;
      if (/1960|irv/.test(hay)) score += 30;
      if (/griego|greek/.test(hay)) score += 10;
      if (/hebreo|hebrew|tanach/.test(hay)) score += 5;
      scored.push({ path: b.path, score: score });
    }
    // Always try explicit iRV-style filenames first if present
    scored.sort(function (a, b) { return b.score - a.score; });
    var paths = scored.map(function (s) { return s.path; });
    // Dedupe
    var seen = {};
    var out = [];
    for (var j = 0; j < paths.length; j++) {
      if (!seen[paths[j]]) {
        seen[paths[j]] = true;
        out.push(paths[j]);
      }
    }
    return out;
  }

  function preferStrongLexicon() {
    if (!state.lexicons || !state.lexicons.length) return;
    var prefer = state.lexicons.filter(function (l) {
      var h = (l.title || "") + " " + (l.filename || "") + " " + (l.abbreviation || "");
      return /strong/i.test(h) && !/swanson|barclay|tuggy|ch[aá]vez/i.test(h);
    });
    if (prefer.length) {
      state.lex = prefer[0];
      return;
    }
    prefer = state.lexicons.filter(function (l) {
      return /strong|multilex|tuggy|vine/i.test(
        (l.title || "") + " " + (l.filename || "") + " " + (l.abbreviation || "")
      );
    });
    if (prefer.length) state.lex = prefer[0];
  }

  /**
   * Map selected Spanish word → Strong’s via interlinear (e.g. Gozaos → G5463).
   * @param {boolean} openLex after resolve, jump to lexicon search
   */
  function resolveSelectedStrongs(openLex) {
    if (!state.selectedWord || state.selectedVerse == null) {
      if (openLex) goToStudyTab("lexico");
      return Promise.resolve(null);
    }
    // Already a Strong code
    if (/^[HG]\d+/i.test(state.selectedWord.trim())) {
      state.selectedStrongs = [{
        strong: state.selectedWord.trim().toUpperCase(),
        spanish: state.selectedWord,
        greek: "",
        translit: "",
        sourceModule: "direct"
      }];
      state.strongsNote = "código Strong’s";
      updateSelectionUI();
      if (openLex) openLexiconWithStrongs();
      return Promise.resolve(state.selectedStrongs);
    }

    var paths = interlinearCandidates();
    if (!paths.length) {
      state.selectedStrongs = [];
      state.strongsNote = "Sin Biblia interlineal (instala p. ej. iRV 1960+)";
      updateSelectionUI();
      if (openLex) openLexiconWithStrongs();
      return Promise.resolve(null);
    }

    state.strongsNote = "Resolviendo griego/hebreo…";
    updateSelectionUI();

    return invoke("resolve_strongs", {
      bookNumber: state.bookNumber,
      chapter: state.chapter,
      verse: state.selectedVerse,
      word: state.selectedWord,
      wordIndex: state.selectedWordIndex >= 0 ? state.selectedWordIndex : null,
      interlinearPaths: paths
    }).then(function (res) {
      state.selectedStrongs = (res && res.hits) || [];
      state.strongsNote = (res && res.note) || "";
      if (state.selectedStrongs.length) {
        var h = state.selectedStrongs[0];
        state.strongsNote =
          "«" + (h.spanish || state.selectedWord) + "» → " + h.strong +
          (h.greek ? " · " + h.greek : "") +
          (h.translit ? " (" + h.translit + ")" : "") +
          (h.sourceModule ? " · " + h.sourceModule : "");
      }
      updateSelectionUI();
      if (openLex) openLexiconWithStrongs();
      return state.selectedStrongs;
    }).catch(function (e) {
      state.selectedStrongs = [];
      state.strongsNote = String(e.message || e);
      updateSelectionUI();
      if (openLex) openLexiconWithStrongs();
      return null;
    });
  }

  function openLexiconWithStrongs() {
    preferStrongLexicon();
    // Ensure setTab uses the resolved code
    goToStudyTab("lexico");
  }

  function refreshModuleDropdown() {
    var sel = el("sel-mod");
    if (!sel) return;
    var current = sel.value ||
      (state.tab === "comentario" && state.cmt && state.cmt.path) ||
      (state.tab === "diccionario" && state.dict && state.dict.path) ||
      (state.tab === "lexico" && state.lex && state.lex.path) ||
      "";
    if (state.tab === "comentario") {
      refreshContentFlags("commentary", state.commentaries, function () {
        fillSelect(sel, state.commentaries, function (m) { return m.path; },
          moduleLabelPlain, current, function (m) { return m.title || m.filename || ""; }, true);
      });
    } else if (state.tab === "diccionario") {
      refreshContentFlags("dictionary", state.dictionaries, function () {
        fillSelect(sel, state.dictionaries, function (m) { return m.path; },
          moduleLabelPlain, current, function (m) { return m.title || m.filename || ""; }, true);
      });
    } else if (state.tab === "lexico") {
      refreshContentFlags("lexicon", state.lexicons, function () {
        fillSelect(sel, state.lexicons, function (m) { return m.path; },
          moduleLabelPlain, current, function (m) { return m.title || m.filename || ""; }, true);
      });
    }
  }

  function plainVerseText(text) {
    return String(text || "")
      .replace(/\uE000|\uE001/g, "")
      .replace(/<[^>]+>/g, "")
      .replace(/\s+/g, " ")
      .trim();
  }

  function renderVerses(filter) {
    var list = el("verse-list");
    list.innerHTML = "";
    var q = (filter || "").toLowerCase().trim();
    var count = 0;
    var parallelPaths = activeParallelPaths();
    var parallelOn = state.bibleView === "parallel" && parallelPaths.length > 0;
    var primaryAbbr = state.bible ? bibleLabel(state.bible) : "Principal";

    for (var i = 0; i < state.verses.length; i++) {
      var v = state.verses[i];
      var primaryPlain = plainVerseText(v.text);
      var match = !q || primaryPlain.toLowerCase().indexOf(q) >= 0 || String(v.verse).indexOf(q) >= 0;
      if (!match && parallelOn) {
        for (var pi = 0; pi < parallelPaths.length; pi++) {
          var pdata = state.parallelByPath[parallelPaths[pi]];
          var pv = pdata && pdata.map ? pdata.map[v.verse] : null;
          if (pv && plainVerseText(pv.text).toLowerCase().indexOf(q) >= 0) {
            match = true;
            break;
          }
        }
      }
      if (!match) continue;
      count++;

      var row = document.createElement("div");
      row.className = "verse";
      row.setAttribute("data-verse", String(v.verse));
      // Primer versículo del capítulo (más grande, como Biblia impresa)
      if (v.verse === 1 || i === 0) {
        row.classList.add("verse-first");
      }

      // Permanent color mark (semantic style id → CSS class mark-gold etc.)
      var markStyle = getVerseMarkStyle(v.verse);
      if (markStyle) row.classList.add("mark-" + markStyle);
      if (getVerseUnderline(v.verse)) row.classList.add("mark-ul");

      // Temporary selection outline (current focus) — separate from permanent marks
      var isVerseHl =
        state.selectedVerse === v.verse && state.highlightMode === "verse";
      if (isVerseHl) row.classList.add("verse-hl");

      var vn = document.createElement("span");
      vn.className = "vn";
      vn.textContent = String(v.verse);
      vn.title = "Clic: seleccionar · Clic derecho: menú";
      (function (verseNum) {
        vn.onclick = function (ev) {
          ev.stopPropagation();
          selectVerse(verseNum, false);
        };
        vn.oncontextmenu = function (ev) {
          handleVerseContextMenu(ev, verseNum, null, -1);
        };
      })(v.verse);

      row.appendChild(vn);

      if (parallelOn) {
        row.classList.add("parallel-row");
        row.classList.add("cols-" + (1 + parallelPaths.length));

        var col0 = document.createElement("div");
        col0.className = "pcol primary vt";
        var tag0 = document.createElement("span");
        tag0.className = "pcol-tag";
        tag0.textContent = primaryAbbr;
        col0.appendChild(tag0);
        appendTokenizedText(col0, v.text, v.verse);
        row.appendChild(col0);

        for (var p = 0; p < parallelPaths.length; p++) {
          var path = parallelPaths[p];
          var info = state.parallelByPath[path];
          var col = document.createElement("div");
          col.className = "pcol";
          var tag = document.createElement("span");
          tag.className = "pcol-tag";
          tag.textContent = (info && info.abbr) || bibleLabel(bibleByPath(path)) || ("B" + (p + 2));
          col.appendChild(tag);
          if (info && info.error) {
            var err = document.createElement("span");
            err.className = "status";
            err.textContent = "No se pudo cargar";
            col.appendChild(err);
          } else {
            var ov = info && info.map ? info.map[v.verse] : null;
            var ot = document.createElement("span");
            ot.className = "vt";
            if (ov && ov.text) {
              // Parallel text is read-only (no word select / marks) to keep primary interactive
              ot.textContent = plainVerseText(ov.text);
            } else {
              ot.className = "status";
              ot.textContent = "—";
            }
            col.appendChild(ot);
          }
          row.appendChild(col);
        }
      } else {
        var vt = document.createElement("span");
        vt.className = "vt";
        appendTokenizedText(vt, v.text, v.verse);
        row.appendChild(vt);
      }

      // Right-click empty padding / whole row → verse menu (words have their own)
      (function (verseNum) {
        row.oncontextmenu = function (ev) {
          if (ev.target && ev.target.classList && ev.target.classList.contains("word")) return;
          handleVerseContextMenu(ev, verseNum, null, -1);
        };
      })(v.verse);
      list.appendChild(row);
    }

    if (!count) {
      list.innerHTML = '<div class="status" style="padding:20px;text-align:center">Sin versículos para mostrar.</div>';
    }
  }

  function updateNoteUI() {
    var box = el("note-box");
    var label = el("note-label");
    var k = noteKey();
    if (!k) {
      box.disabled = true;
      box.value = "";
      label.textContent = "Selecciona un versículo";
      return;
    }
    box.disabled = false;
    box.value = notes[k] || "";
    label.textContent = bookMeta().name + " " + state.chapter + ":" + state.selectedVerse;
  }

  function updateFooter() {
    var bm = bookMeta();
    var ref = bm.name + " " + state.chapter +
      (state.selectedVerse != null ? ":" + state.selectedVerse : "");
    if (state.selectedWord) ref += " · «" + state.selectedWord + "»";
    el("foot-left").textContent =
      ref + " · " + state.verses.length + " vv." +
      (state.bible ? " · " + (state.bible.abbreviation || state.bible.filename) : "");
    el("foot-right").textContent =
      "B:" + state.bibles.length +
      " C:" + state.commentaries.length +
      " D:" + state.dictionaries.length +
      " L:" + state.lexicons.length +
      " · C:\\Program Files (x86)\\e-Sword";
  }

  function openVerseModal() {
    var bm = bookMeta();
    el("verse-modal-title").textContent = "Versículos · " + bm.name + " " + state.chapter;
    el("verse-modal-sub").textContent =
      state.verses.length
        ? "Hay " + state.verses.length + " versículos. Elige uno:"
        : "Carga el capítulo primero.";
    var grid = el("verse-pick-grid");
    grid.innerHTML = "";
    for (var i = 0; i < state.verses.length; i++) {
      var v = state.verses[i].verse;
      var b = document.createElement("button");
      b.type = "button";
      b.textContent = String(v);
      if (state.selectedVerse === v) b.className = "current";
      (function (num) {
        b.onclick = function () {
          closeVerseModal();
          selectVerse(num, true);
        };
      })(v);
      grid.appendChild(b);
    }
    el("verse-modal").classList.add("open");
  }

  function closeVerseModal() {
    el("verse-modal").classList.remove("open");
  }

  // ---- Suggest dropdown helpers ----
  function closeSuggest() {
    el("term-suggest").classList.remove("open");
    el("term-suggest").innerHTML = "";
  }

  function openSuggest(items, onPick) {
    var box = el("term-suggest");
    box.innerHTML = "";
    if (!items || !items.length) {
      box.classList.remove("open");
      return;
    }
    for (var i = 0; i < items.length; i++) {
      var it = items[i];
      var btn = document.createElement("button");
      btn.type = "button";
      btn.innerHTML =
        '<div class="s-title"></div><div class="s-sub"></div>';
      btn.querySelector(".s-title").textContent = it.title;
      btn.querySelector(".s-sub").textContent = it.sub || "";
      (function (item) {
        btn.onclick = function (ev) {
          ev.preventDefault();
          onPick(item);
          closeSuggest();
        };
      })(it);
      box.appendChild(btn);
    }
    box.classList.add("open");
  }

  function setTab(tab) {
    state.tab = tab;
    var buttons = document.querySelectorAll("#tabs button");
    for (var i = 0; i < buttons.length; i++) {
      buttons[i].classList.toggle("active", buttons[i].getAttribute("data-tab") === tab);
    }
    var panels = document.querySelectorAll(".panel");
    for (var j = 0; j < panels.length; j++) {
      panels[j].classList.toggle("active", panels[j].id === "panel-" + tab);
    }

    var modbar = el("modbar");
    var sel = el("sel-mod");
    var wrap = el("term-search-wrap");
    var termInput = el("term-search");
    closeSuggest();

    if (tab === "biblia") {
      modbar.classList.remove("show");
      wrap.style.display = "none";
      return;
    }

    if (tab === "temas") {
      modbar.classList.remove("show");
      wrap.style.display = "none";
      renderThemeGrid();
      return;
    }

    modbar.classList.add("show");
    wrap.style.display = "block";

    if (tab === "comentario") {
      termInput.placeholder = "Opcional: buscar texto en el módulo…";
      termInput.value = "";
      // Fill dropdown instantly, probe ● in background
      fillSelect(sel, state.commentaries, function (m) { return m.path; },
        moduleLabelPlain, state.cmt && state.cmt.path, null, true);
      refreshContentFlags("commentary", state.commentaries, null);
      syncCmtLevelCheckboxes();
      if (state.selectedVerse != null) {
        loadCommentariesForVerse();
      } else {
        el("cmt-list").innerHTML =
          '<div class="status"><b>Cómo ver un comentario:</b><br/>' +
          "1. En Biblia, haz clic en el <b>número</b> del versículo.<br/>" +
          "2. Vuelve a Comentario — el menú y la columna derecha marcan <b>●</b> los que tienen nota.<br/>" +
          "3. Filtros: por defecto solo <b>Versículo</b>. Activa <b>Capítulo</b> o <b>Libro</b> si quieres intros generales.</div>";
        el("cmt-ref").textContent = "Sin versículo seleccionado";
        renderStudySideRail("commentary");
      }
    } else if (tab === "diccionario") {
      termInput.placeholder = "Palabra (Dios, crear, luz…)";
      termInput.value = state.selectedWord || "";
      fillSelect(sel, state.dictionaries, function (m) { return m.path; },
        moduleLabelPlain, state.dict && state.dict.path, null, true);
      refreshContentFlags("dictionary", state.dictionaries, null);
      if (state.selectedWord || termInput.value.trim()) {
        runDictionarySearch(termInput.value.trim() || state.selectedWord, true);
      } else {
        el("dict-list").innerHTML =
          '<div class="status"><b>Cómo usar el diccionario:</b><br/>' +
          "1. Clic en una palabra en Biblia, o escribe el <b>lema</b> (ej. <b>Dios</b>, <b>AMOR</b>).<br/>" +
          "2. El menú y la columna derecha marcan <b>●</b> los diccionarios con entrada.<br/>" +
          "3. Solo se muestran <b>entradas cuyo título coincide</b> — no menciones sueltas en el texto.</div>";
        el("dict-status").textContent = "Listo para buscar.";
        renderStudySideRail("dictionary");
      }
    } else if (tab === "lexico") {
      termInput.placeholder = "Strong’s: H1254 o G26 — o clic una palabra en Biblia";
      preferStrongLexicon();
      fillSelect(sel, state.lexicons, function (m) { return m.path; },
        moduleLabelPlain, state.lex && state.lex.path, null, true);

      // Prefer resolved Strong’s from interlinear (Gozaos → G5463)
      var lexTerm = "";
      if (state.selectedStrongs && state.selectedStrongs.length) {
        lexTerm = state.selectedStrongs[0].strong;
      } else if (state.selectedWord && /^[HG]\d+/i.test(state.selectedWord.trim())) {
        lexTerm = state.selectedWord.trim().toUpperCase();
      }

      if (lexTerm) {
        termInput.value = lexTerm;
        refreshContentFlags("lexicon", state.lexicons, null);
        runLexiconSearch(lexTerm, true);
        if (state.selectedStrongs && state.selectedStrongs.length) {
          var meta = state.selectedStrongs.map(function (h) {
            return h.strong +
              (h.greek ? " " + h.greek : "") +
              (h.translit ? " (" + h.translit + ")" : "");
          }).join(" · ");
          el("lex-status").textContent =
            "«" + state.selectedWord + "» → " + meta +
            (state.strongsNote && state.strongsNote.indexOf("→") < 0 ? " · " + state.strongsNote : "");
        }
      } else if (state.selectedWord) {
        // Still resolving or failed — try resolve now, then search + probe ●
        termInput.value = state.selectedWord;
        el("lex-list").innerHTML =
          '<div class="status">Buscando el número Strong’s de «' +
          state.selectedWord + "» en el interlineal…</div>";
        renderStudySideRail("lexicon");
        resolveSelectedStrongs(false).then(function () {
          if (state.selectedStrongs && state.selectedStrongs.length) {
            var code = state.selectedStrongs[0].strong;
            termInput.value = code;
            refreshContentFlags("lexicon", state.lexicons, null);
            runLexiconSearch(code, true);
            var h = state.selectedStrongs[0];
            el("lex-status").textContent =
              "«" + state.selectedWord + "» → " + code +
              (h.greek ? " · " + h.greek : "") +
              (h.translit ? " (" + h.translit + ")" : "");
          } else {
            termInput.value = state.selectedWord;
            refreshContentFlags("lexicon", state.lexicons, null);
            el("lex-list").innerHTML =
              '<div class="status">' +
              (state.strongsNote
                ? escapeHtml(state.strongsNote)
                : "No se pudo mapear «" + escapeHtml(state.selectedWord) + "» a un Strong’s.") +
              "<br/><br/>Prueba: elige otra palabra, o escribe un código (G5463 / H1254)." +
              "<br/>Necesitas una Biblia interlineal (p. ej. <b>iRV 1960+</b>) en e-Sword." +
              "</div>";
            el("lex-status").textContent = "Sin Strong’s para «" + state.selectedWord + "»";
          }
        });
      } else {
        termInput.value = "";
        refreshContentFlags("lexicon", state.lexicons, null);
        el("lex-list").innerHTML =
          '<div class="status"><b>Léxico Strong’s:</b><br/>' +
          "1. En <b>Biblia</b>, clic en una palabra (ej. <b>Gozaos</b> en Mt 5:12).<br/>" +
          "2. Pulsa <b>→ Léxico</b> o esta pestaña — se resuelve al griego/hebreo (G5463…).<br/>" +
          "3. O escribe un código: <b>H1254</b>, <b>G26</b>.<br/>" +
          "4. <b>●</b> en el menú / columna = léxico con definición para ese código.</div>";
        el("lex-status").textContent = "Selecciona una palabra en Biblia o escribe un Strong’s.";
      }
    }
  }

  function loadChapter() {
    if (!state.bible) return;
    var gen = ++_chapterLoadGen;
    var biblePath = state.bible.path;
    var bookNumber = state.bookNumber;
    var chapter = state.chapter;

    try { fillChapterSelect(); } catch (e) { console.warn(e); }

    var statusEl = el("chapter-status");
    var errEl = el("chapter-error");
    if (statusEl) {
      statusEl.textContent = "Cargando " + bookMeta().name + " " + chapter + "…";
    }
    if (errEl) errEl.style.display = "none";

    var cacheKey = biblePath + "|" + bookNumber + "|" + chapter;
    var cached = getChapterCache(cacheKey);

    function stillCurrent() {
      return gen === _chapterLoadGen &&
        state.bible &&
        state.bible.path === biblePath &&
        state.bookNumber === bookNumber &&
        state.chapter === chapter;
    }

    function applyChapterResult(res) {
      if (!stillCurrent()) return;
      state.verses = res.verses || [];
      putChapterCache(cacheKey, {
        verses: state.verses,
        title: res.moduleTitle || res.translation || "",
        translation: res.translation || ""
      });
      // Keep selection if still in chapter, else first verse
      var still = false;
      if (state.selectedVerse != null) {
        for (var i = 0; i < state.verses.length; i++) {
          if (state.verses[i].verse === state.selectedVerse) { still = true; break; }
        }
      }
      if (!still) {
        state.selectedVerse = state.verses.length ? state.verses[0].verse : null;
        state.selectedWord = null;
        state.highlightMode = state.selectedVerse != null ? "verse" : null;
      }

      var finish = function () {
        if (!stillCurrent()) return;
        var status =
          (res.moduleTitle || res.translation || "") + " · " + state.verses.length + " versículos";
        if (state.bibleView === "parallel") {
          var extra = activeParallelPaths().length;
          if (extra) status += " · paralela +" + extra;
        }
        if (statusEl) statusEl.textContent = status;
        try {
          renderVerses(el("search-verse") ? el("search-verse").value : "");
          updateNoteUI();
          updateSelectionUI();
          updateHlBarUI();
          updateFooter();
          saveNavPosition();
        } catch (renderErr) {
          console.error("render after chapter load failed", renderErr);
          if (errEl) {
            errEl.style.display = "block";
            errEl.textContent = "Error al mostrar capítulo: " + (renderErr.message || renderErr);
          }
        }
      };

      if (state.bibleView === "parallel") {
        return loadParallelChapters().then(finish).catch(function (pe) {
          console.warn(pe);
          finish();
        });
      }
      state.parallelByPath = {};
      finish();
    }

    // Instant paint when chapter is already in memory (back/forward, parallel swap)
    if (cached) {
      applyChapterResult({
        verses: cached.verses,
        moduleTitle: cached.title,
        translation: cached.translation
      });
      return;
    }

    invoke("get_bible_chapter", {
      modulePath: biblePath,
      bookNumber: bookNumber,
      chapter: chapter
    }).then(function (res) {
      applyChapterResult(res);
    }).catch(function (e) {
      if (!stillCurrent()) return;
      state.verses = [];
      try { renderVerses(""); updateHlBarUI(); } catch (re) { console.warn(re); }
      if (errEl) {
        errEl.style.display = "block";
        errEl.textContent = String(e.message || e);
      }
      if (statusEl) statusEl.textContent = "Error al cargar capítulo";
    });
  }

  function loadCommentariesForVerse() {
    var box = el("cmt-list");
    var bm = bookMeta();
    var ref = bm.name + " " + state.chapter +
      (state.selectedVerse != null ? ":" + state.selectedVerse : "");
    var modName = state.cmt
      ? (state.cmt.abbreviation || state.cmt.filename)
      : "(ningún módulo)";
    var levels = cmtLevelLabel();
    el("cmt-ref").textContent =
      ref + " · " + modName + " · filtro: " + levels;
    if (!state.cmt || state.selectedVerse == null) {
      box.innerHTML = '<div class="status">Selecciona un versículo en Biblia y un módulo de comentario.</div>';
      renderStudySideRail("commentary");
      return;
    }
    var t0 = Date.now();
    box.innerHTML = '<div class="status">Cargando comentario de ' + ref +
      " (" + levels + ")…</div>";
    renderStudySideRail("commentary");
    var lv = cmtLevelArgs();
    invoke("get_verse_commentaries", {
      modulePath: state.cmt.path,
      bookNumber: state.bookNumber,
      chapter: state.chapter,
      verse: state.selectedVerse,
      includeVerse: lv.includeVerse,
      includeChapter: lv.includeChapter,
      includeBook: lv.includeBook
    }).then(function (rows) {
      var ms = Date.now() - t0;
      el("cmt-ref").textContent =
        ref + " · " + modName + " · " + (rows ? rows.length : 0) +
        " bloque(s) · " + levels + " · " + ms + " ms";
      var extra = "";
      if (!state.cmtIncludeChapter || !state.cmtIncludeBook) {
        extra =
          "<br/><span class='hint'>Si esperabas más texto, marca <b>Capítulo</b> y/o <b>Libro</b> arriba. " +
          "Por defecto solo se muestra la nota de <b>este versículo</b>.</span>";
      }
      renderCommentaryList(rows, null, {
        emptyHint:
          "No hay comentario de <b>" + levels + "</b> en este módulo para " + ref + ".<br/>" +
          "Eso es normal: muchos módulos (ej. Vida Plena) solo anotan algunos versículos.<br/>" +
          "Prueba un módulo con <b>●</b> en la <b>columna derecha</b>, o activa <b>Capítulo</b>/<b>Libro</b>." +
          extra +
          "<br/><span class='hint'>F.B. Meyer / TSK cifrados (sqliteplus) aún no se pueden abrir aquí.</span>"
      });
      renderStudySideRail("commentary");
    }).catch(function (e) {
      box.innerHTML = '<div class="error">No se pudo leer el módulo:<br/>' +
        String(e.message || e) + "</div>";
      renderStudySideRail("commentary");
    });
  }

  function renderCommentaryList(rows, selectedRef, opts) {
    var box = el("cmt-list");
    opts = opts || {};
    if (!rows || !rows.length) {
      box.innerHTML = '<div class="status">' +
        (opts.emptyHint || "Sin resultados en este módulo.") +
        "</div>";
      return;
    }
    var good = [];
    var cipher = [];
    for (var i = 0; i < rows.length; i++) {
      var t = rows[i].text || "";
      if (t.indexOf("[Módulo cifrado") === 0 || t.indexOf("[Contenido binario") === 0) {
        cipher.push(rows[i]);
      } else {
        good.push(rows[i]);
      }
    }
    var show = good.length ? good : cipher;
    box.innerHTML = "";
    if (!good.length && cipher.length) {
      var warn = document.createElement("div");
      warn.className = "error";
      warn.innerHTML =
        "Este módulo está <b>cifrado por e-Sword</b> (formato antiguo sqliteplus: .cmtx / .dctx / .lexx / .bblx).<br/>" +
        "No es un fallo de la app: el texto está protegido y <b>solo e-Sword</b> puede descifrarlo.<br/>" +
        "Solución práctica: usá la versión moderna del mismo recurso si la tenés " +
        "(<b>.cmti</b> / <b>.dcti</b> / <b>.lexi</b> / <b>.bbli</b>). " +
        "Ej.: <b>01 Diccionario strong.lexi</b> en lugar de <b>strong.lexx</b>.";
      box.appendChild(warn);
    } else if (good.length) {
      var info = document.createElement("div");
      info.className = "status";
      info.style.marginBottom = "10px";
      var levels = { verse: 0, chapter: 0, book: 0 };
      for (var k = 0; k < good.length; k++) {
        levels[good[k].level] = (levels[good[k].level] || 0) + 1;
      }
      info.textContent =
        "Mostrando " + good.length + " bloque(s): " +
        (levels.verse || 0) + " de versículo · " +
        (levels.chapter || 0) + " de capítulo · " +
        (levels.book || 0) + " de libro";
      box.appendChild(info);
    }
    for (var j = 0; j < show.length; j++) {
      var r = show[j];
      var div = document.createElement("div");
      div.className = "entry" + (selectedRef && r.reference === selectedRef ? " hit-selected" : "");
      var levelLabel =
        r.level === "verse" ? "VERSÍCULO" :
        r.level === "chapter" ? "CAPÍTULO" :
        r.level === "book" ? "LIBRO" : (r.level || "");
      div.innerHTML = '<div class="meta"></div><div class="body"></div>';
      // Linkify reference line too (e.g. "Mateo 5:12" / Strong’s in titles)
      div.querySelector(".meta").innerHTML =
        "[" + escapeHtml(levelLabel) + "] " +
        formatRichText(r.reference || "") +
        " · " + escapeHtml(r.title || r.source || "");
      div.querySelector(".body").innerHTML = formatRichText(r.text || "");
      box.appendChild(div);
    }
  }

  function runCommentarySearch(term, fillSuggest) {
    // Búsqueda opcional por texto DENTRO del módulo (además del versículo)
    if (!state.cmt || !term) {
      loadCommentariesForVerse();
      return;
    }
    el("cmt-ref").textContent =
      "Buscando «" + term + "» en " + (state.cmt.abbreviation || state.cmt.filename) +
      " (solo módulos de texto plano)…";
    el("cmt-list").innerHTML = '<div class="status">Buscando en el texto del módulo…</div>';
    invoke("search_commentary_term", {
      modulePath: state.cmt.path,
      term: term,
      limit: 30
    }).then(function (rows) {
      state.cmtHits = rows || [];
      if (fillSuggest && state.cmtHits.length) {
        openSuggest(state.cmtHits.map(function (r) {
          return {
            title: r.reference || "(sin ref.)",
            sub: (r.level || "") + " · " + ((r.text || "").slice(0, 80).replace(/\s+/g, " ")) + "…",
            data: r
          };
        }), function (item) {
          var rest = state.cmtHits.filter(function (x) { return x.reference !== item.data.reference; });
          renderCommentaryList([item.data].concat(rest), item.data.reference, {});
        });
      }
      if (!state.cmtHits.length) {
        // Fallback: always show verse commentary so user isn't stuck empty
        el("cmt-list").innerHTML =
          '<div class="status">No se encontró «' + term +
          '» como texto en este módulo. Mostrando el comentario del <b>versículo</b>…</div>';
        loadCommentariesForVerse();
        return;
      }
      renderCommentaryList(state.cmtHits, null, {});
    }).catch(function (e) {
      el("cmt-list").innerHTML = '<div class="error">' + String(e.message || e) +
        '</div><div class="status" style="margin-top:8px">Intentando comentario del versículo…</div>';
      loadCommentariesForVerse();
    });
  }

  function renderDictEntries(rows, selectedTerm) {
    var box = el("dict-list");
    if (!rows || !rows.length) {
      box.innerHTML =
        '<div class="status">No hay <b>entrada</b> con ese lema en este diccionario.<br/><br/>' +
        "Solo se buscan <b>títulos de entrada</b> (como un diccionario real / e-Sword), " +
        "no menciones sueltas dentro de otras definiciones.<br/><br/>" +
        "<b>Prueba así:</b><br/>" +
        "• Lema o raíz: <b>DIOS</b>, <b>AMOR</b>, <b>CREAR</b><br/>" +
        "• Módulos con ● para este lema (columna derecha)<br/>" +
        "• Otra palabra en la pestaña Biblia</div>";
      renderStudySideRail("dictionary");
      return;
    }
    box.innerHTML = "";
    var ctx = buildLemmaContext(rows);
    // Prefer first entry term for Spanish if looking up Spanish lemma
    if (!ctx.spanish && rows[0] && rows[0].term && !/^[HG]\d+/i.test(rows[0].term)) {
      ctx.spanish = rows[0].term;
    }
    if (!ctx.original && rows[0]) {
      ctx.original = extractOriginalScript(rows[0].term || "") ||
        extractOriginalScript(rows[0].definition || "") ||
        (rows[0].term || "");
    }
    var modLabel = state.dict
      ? (state.dict.abbreviation || state.dict.filename || "")
      : "";
    box.appendChild(renderLemmaHeaderEl(ctx, modLabel));

    for (var i = 0; i < rows.length; i++) {
      var r = rows[i];
      var div = document.createElement("div");
      div.className = "entry" + (selectedTerm && r.term === selectedTerm ? " hit-selected" : "");
      div.innerHTML = '<h2 class="theme-heading"></h2><div class="meta"></div><div class="body"></div>';
      // Title shows original script / headword + Spanish when available
      var head = r.term || "";
      var origInTerm = extractOriginalScript(head);
      var titleParts = [];
      if (origInTerm) titleParts.push(origInTerm);
      else if (head) titleParts.push(head);
      if (ctx.spanish && head && head.toLowerCase() !== ctx.spanish.toLowerCase()) {
        titleParts.push("· " + ctx.spanish);
      }
      div.querySelector("h2").textContent = titleParts.join(" ") || head;
      div.querySelector(".meta").textContent = r.title || r.source || "";
      div.querySelector(".body").innerHTML = formatRichText(r.definition || "");
      box.appendChild(div);
    }
    renderStudySideRail("dictionary");
  }

  function runDictionarySearch(term, fillSuggest) {
    if (!state.dict) {
      el("dict-list").innerHTML = '<div class="status">Elige un diccionario en el menú de arriba.</div>';
      renderStudySideRail("dictionary");
      return;
    }
    if (!term) {
      el("dict-list").innerHTML = '<div class="status">Escribe una palabra (ej. Dios, crear, luz) o selecciónala en la Biblia.</div>';
      renderStudySideRail("dictionary");
      return;
    }
    el("dict-status").textContent =
      "Buscando «" + term + "» en " + (state.dict.abbreviation || state.dict.filename) + "…";
    el("dict-list").innerHTML = '<div class="status">Buscando entradas (solo títulos / lemas)…</div>';
    refreshContentFlags("dictionary", state.dictionaries, null);
    invoke("suggest_dictionary_topics", {
      modulePath: state.dict.path,
      term: term,
      limit: 40
    }).then(function (rows) {
      state.dictHits = rows || [];
      el("dict-status").textContent =
        state.dictHits.length
          ? "✓ " + state.dictHits.length + " entrada(s) para «" + term +
            "» · ES + original arriba · otros módulos a la derecha"
          : "No hay entradas para «" + term + "» en este diccionario. Prueba otro en la columna derecha o ●.";
      if (fillSuggest && state.dictHits.length) {
        openSuggest(state.dictHits.map(function (r) {
          return {
            title: r.term,
            sub: (r.title || r.source || "") + " · " + ((r.definition || "").slice(0, 70).replace(/\s+/g, " ")) + "…",
            data: r
          };
        }), function (item) {
          el("term-search").value = item.data.term;
          renderDictEntries(state.dictHits, item.data.term);
          var nodes = el("dict-list").querySelectorAll(".entry");
          for (var i = 0; i < nodes.length; i++) {
            if (nodes[i].classList.contains("hit-selected")) {
              nodes[i].scrollIntoView({ block: "start", behavior: "smooth" });
              break;
            }
          }
        });
      }
      renderDictEntries(state.dictHits, null);
    }).catch(function (e) {
      el("dict-list").innerHTML = '<div class="error">Error al leer el diccionario:<br/>' +
        String(e.message || e) + "</div>";
      renderStudySideRail("dictionary");
    });
  }

  function runLexiconSearch(term, fillSuggest) {
    if (!state.lex) {
      el("lex-list").innerHTML = '<div class="status">Elige un léxico (recomendado: <b>Strong °</b>).</div>';
      renderStudySideRail("lexicon");
      return;
    }
    if (!term) {
      el("lex-list").innerHTML =
        '<div class="status"><b>Léxico Strong’s — uso fácil:</b><br/>' +
        "• Hebreo: <b>H1254</b> (crear), <b>H430</b> (Elohim)<br/>" +
        "• Griego: <b>G26</b> (amor), <b>G2316</b> (Dios)<br/>" +
        "• Clic en una palabra en Biblia → se muestra <b>original + ES</b><br/>" +
        "Una palabra del texto español casi nunca es la clave del léxico; usa el número Strong’s.</div>";
      renderStudySideRail("lexicon");
      return;
    }
    var t0 = Date.now();
    el("lex-status").textContent =
      "Buscando «" + term + "» en " + (state.lex.abbreviation || state.lex.filename) + "…";
    el("lex-list").innerHTML = '<div class="status">Consulta rápida por Strong’s…</div>';
    refreshContentFlags("lexicon", state.lexicons, null);
    invoke("search_lexicon", {
      modulePath: state.lex.path,
      term: term,
      limit: 20
    }).then(function (rows) {
      state.lexHits = rows || [];
      var ms = Date.now() - t0;
      el("lex-status").textContent =
        state.lexHits.length
          ? "✓ " + state.lexHits.length + " entrada(s) · " + ms + " ms · original + ES arriba · otros a la derecha"
          : "Sin «" + term + "». Prueba H1254 o G26, o otro léxico en la columna derecha.";
      if (fillSuggest && state.lexHits.length) {
        openSuggest(state.lexHits.map(function (r) {
          return {
            title: r.term,
            sub: (r.title || r.source || "") + " · " + ((r.definition || "").slice(0, 70).replace(/\s+/g, " ")) + "…",
            data: r
          };
        }), function (item) {
          el("term-search").value = item.data.term;
          renderLexEntries(state.lexHits, item.data.term);
        });
      }
      renderLexEntries(state.lexHits, null);
    }).catch(function (e) {
      el("lex-list").innerHTML = '<div class="error">Error al leer el léxico:<br/>' +
        String(e.message || e) + "</div>";
      renderStudySideRail("lexicon");
    });
  }

  function renderLexEntries(rows, selectedTerm) {
    var box = el("lex-list");
    if (!rows || !rows.length) {
      box.innerHTML =
        '<div class="status">Sin resultados.<br/><br/>' +
        '<b>Cómo usar el Léxico:</b><br/>' +
        "1. Elige un módulo con ● (recomendado: <b>Strong °</b>, <b>Chávez</b>, <b>Vine</b>).<br/>" +
        "2. Escribe un código Strong’s: <b>H1254</b> (hebreo «crear») o <b>G26</b> (griego «amor»).<br/>" +
        "3. Clic en una palabra en Biblia para ver <b>original + traducción ES</b>.<br/>" +
        "4. Columna derecha = otros léxicos con la misma entrada.</div>";
      renderStudySideRail("lexicon");
      return;
    }
    box.innerHTML = "";
    var ctx = buildLemmaContext(rows);
    if (!ctx.original && rows[0]) {
      ctx.original = extractOriginalScript(rows[0].definition || "") ||
        extractOriginalScript(rows[0].term || "") ||
        (rows[0].term || "");
    }
    // Spanish is required for clarity when studying H/G words
    if (!ctx.spanish && state.selectedWord && !/^[HG]\d+/i.test(state.selectedWord)) {
      ctx.spanish = state.selectedWord;
    }
    var modLabel = state.lex
      ? (state.lex.abbreviation || state.lex.filename || "")
      : "";
    box.appendChild(renderLemmaHeaderEl(ctx, modLabel));

    for (var i = 0; i < rows.length; i++) {
      var r = rows[i];
      var div = document.createElement("div");
      div.className = "entry" + (selectedTerm && r.term === selectedTerm ? " hit-selected" : "");
      div.innerHTML = '<h2 class="theme-heading"></h2><div class="meta"></div><div class="body"></div>';
      var head = r.term || "";
      var orig = extractOriginalScript(r.definition || "") || extractOriginalScript(head) || "";
      var titleBits = [];
      if (orig) titleBits.push(orig);
      if (head && head !== orig) titleBits.push(head);
      if (ctx.spanish) titleBits.push("· ES: " + ctx.spanish);
      div.querySelector("h2").textContent = titleBits.join(" ") || head;
      var metaLine = r.title || r.source || "";
      if (ctx.strong && metaLine.indexOf(ctx.strong) < 0) {
        metaLine = (metaLine ? metaLine + " · " : "") + ctx.strong;
      }
      div.querySelector(".meta").textContent = metaLine;
      div.querySelector(".body").innerHTML = formatRichText(r.definition || "");
      box.appendChild(div);
    }
    renderStudySideRail("lexicon");
  }

  function goToStudyTab(tab) {
    setTab(tab);
    // buttons in tabs
    var buttons = document.querySelectorAll("#tabs button");
    for (var i = 0; i < buttons.length; i++) {
      if (buttons[i].getAttribute("data-tab") === tab) buttons[i].classList.add("active");
    }
  }

  function wireEvents() {
    if (window.__ADC_EVENTS_WIRED__) return;
    window.__ADC_EVENTS_WIRED__ = true;

    // Custom book / bible pills (full label always visible)
    if (el("pill-book-btn")) {
      el("pill-book-btn").onclick = function (ev) {
        ev.stopPropagation();
        togglePillDropdown("pill-book");
      };
    }
    if (el("pill-bible-btn")) {
      el("pill-bible-btn").onclick = function (ev) {
        ev.stopPropagation();
        togglePillDropdown("pill-bible");
      };
    }
    document.addEventListener("click", function (ev) {
      if (ev.target && ev.target.closest && ev.target.closest(".pill-dd")) return;
      closeAllPillDropdowns();
    });
    document.addEventListener("keydown", function (ev) {
      if (ev.key === "Escape") closeAllPillDropdowns();
    });

    if (el("sel-chapter")) {
      el("sel-chapter").onchange = function () {
        state.chapter = parseInt(this.value, 10) || 1;
        state.selectedVerse = null;
        state.selectedWord = null;
        loadChapter();
      };
    }
    if (el("btn-prev")) {
      el("btn-prev").onclick = function () {
        if (state.chapter > 1) {
          state.chapter--;
          state.selectedVerse = null;
          state.selectedWord = null;
          loadChapter();
        }
      };
    }
    if (el("btn-next")) {
      el("btn-next").onclick = function () {
        var max = bookMeta().chapters || 1;
        if (state.chapter < max) {
          state.chapter++;
          state.selectedVerse = null;
          state.selectedWord = null;
          loadChapter();
        }
      };
    }
    if (el("btn-verse-picker")) el("btn-verse-picker").onclick = openVerseModal;
    if (el("verse-modal-close")) el("verse-modal-close").onclick = closeVerseModal;
    if (el("verse-modal")) {
      el("verse-modal").onclick = function (ev) {
        if (ev.target === el("verse-modal")) closeVerseModal();
      };
    }

    // Lectura / Paralela subtabs + history
    var subtabs = el("bible-subtabs");
    if (subtabs) {
      subtabs.addEventListener("click", function (ev) {
        var btn = ev.target && ev.target.closest
          ? ev.target.closest("[data-bible-view]")
          : null;
        if (!btn) return;
        setBibleView(btn.getAttribute("data-bible-view"));
      });
    }
    function onParallelSelectChange(idx) {
      return function () {
        state.parallelPaths[idx] = this.value || "";
        // Avoid same bible twice
        if (idx === 0 && state.parallelPaths[0] && state.parallelPaths[0] === state.parallelPaths[1]) {
          state.parallelPaths[1] = "";
          var s2 = el("sel-parallel-2");
          if (s2) s2.value = "";
        }
        if (idx === 1 && state.parallelPaths[1] && state.parallelPaths[1] === state.parallelPaths[0]) {
          state.parallelPaths[0] = "";
          var s1 = el("sel-parallel-1");
          if (s1) s1.value = "";
        }
        saveParallelPrefs();
        loadParallelChapters().then(function () {
          renderVerses(el("search-verse").value);
          var extra = activeParallelPaths().length;
          if (state.bible && el("chapter-status")) {
            var base = el("chapter-status").textContent.replace(/\s·\sparalela.*$/, "");
            el("chapter-status").textContent = extra
              ? base + " · paralela +" + extra
              : base;
          }
        });
      };
    }
    if (el("sel-parallel-1")) el("sel-parallel-1").onchange = onParallelSelectChange(0);
    if (el("sel-parallel-2")) el("sel-parallel-2").onchange = onParallelSelectChange(1);

    if (el("btn-history")) {
      el("btn-history").onclick = openHistoryModal;
    }
    if (el("history-modal-close")) {
      el("history-modal-close").onclick = closeHistoryModal;
    }
    if (el("history-modal")) {
      el("history-modal").onclick = function (ev) {
        if (ev.target === el("history-modal")) closeHistoryModal();
      };
    }
    if (el("btn-hist-clear")) {
      el("btn-hist-clear").onclick = function () {
        saveHistory([]);
        renderHistoryList();
      };
    }

    wireRefPopups();
    el("search-verse").oninput = function () {
      renderVerses(this.value);
    };
    el("note-box").oninput = function () {
      var k = noteKey();
      if (!k) return;
      if (!this.value.trim()) delete notes[k];
      else notes[k] = this.value;
      saveNotes(notes);
    };
    el("tabs").onclick = function (ev) {
      var t = ev.target && ev.target.getAttribute("data-tab");
      if (t) setTab(t);
    };
    el("sel-mod").onchange = function () {
      var path = this.value;
      if (state.tab === "comentario") {
        for (var i = 0; i < state.commentaries.length; i++) {
          if (state.commentaries[i].path === path) state.cmt = state.commentaries[i];
        }
        // Por defecto: comentario del versículo (lo que indica el ●)
        var t = el("term-search").value.trim();
        if (t) runCommentarySearch(t, true);
        else loadCommentariesForVerse();
      } else if (state.tab === "diccionario") {
        for (var j = 0; j < state.dictionaries.length; j++) {
          if (state.dictionaries[j].path === path) state.dict = state.dictionaries[j];
        }
        var td = el("term-search").value.trim() || state.selectedWord || "";
        if (td) runDictionarySearch(td, true);
        else {
          el("dict-list").innerHTML = '<div class="status">Escribe una palabra para buscar en este diccionario.</div>';
        }
      } else if (state.tab === "lexico") {
        for (var k = 0; k < state.lexicons.length; k++) {
          if (state.lexicons[k].path === path) state.lex = state.lexicons[k];
        }
        var tl = el("term-search").value.trim() || state.selectedWord || "H1254";
        el("term-search").value = tl;
        runLexiconSearch(tl, true);
      }
    };

    var termTimer;
    el("term-search").oninput = function () {
      var term = this.value.trim();
      clearTimeout(termTimer);
      termTimer = setTimeout(function () {
        if (!term) {
          closeSuggest();
          // Clear ● when search box emptied
          if (state.tab === "diccionario") {
            refreshContentFlags("dictionary", state.dictionaries, null);
          } else if (state.tab === "lexico") {
            refreshContentFlags("lexicon", state.lexicons, null);
          }
          return;
        }
        if (state.tab === "diccionario") {
          refreshContentFlags("dictionary", state.dictionaries, null);
          runDictionarySearch(term, true);
        } else if (state.tab === "comentario") {
          runCommentarySearch(term, true);
        } else if (state.tab === "lexico") {
          refreshContentFlags("lexicon", state.lexicons, null);
          runLexiconSearch(term, true);
        }
      }, 280);
    };
    el("term-search").onkeydown = function (ev) {
      if (ev.key === "Escape") closeSuggest();
    };
    document.addEventListener("click", function (ev) {
      if (!el("term-search-wrap").contains(ev.target)) closeSuggest();
    });

    el("btn-to-dict").onclick = function () {
      goToStudyTab("diccionario");
    };
    el("btn-to-lex").onclick = function () {
      if (!state.selectedWord) return;
      // Resolve Spanish → Strong’s first, then open Léxico with G/H code
      if (state.selectedStrongs && state.selectedStrongs.length) {
        openLexiconWithStrongs();
      } else {
        resolveSelectedStrongs(true);
      }
    };
    el("btn-to-cmt").onclick = function () {
      goToStudyTab("comentario");
    };
    el("btn-clear-sel").onclick = clearSelection;

    // Commentary level filters (verse / chapter / book isolation)
    function onCmtLevelChange() {
      readCmtLevelsFromUI();
      if (state.tab === "comentario") {
        if (state.selectedVerse != null) loadCommentariesForVerse();
        refreshContentFlags("commentary", state.commentaries, null);
      }
    }
    ["cmt-lv-verse", "cmt-lv-chapter", "cmt-lv-book"].forEach(function (id) {
      var node = el(id);
      if (node) node.onchange = onCmtLevelChange;
    });
    syncCmtLevelCheckboxes();

    // Permanent multi-color highlights (semantic style ids) — side panel
    var hlBar = el("hl-bar");
    if (hlBar) {
      hlBar.addEventListener("click", function (ev) {
        var t = ev.target;
        if (!t || !t.classList) return;
        if (t.id === "btn-hl-clear" || (t.classList.contains("hl-clear"))) {
          clearHighlight();
          return;
        }
        if (t.classList.contains("hl-chip") && t.getAttribute("data-style")) {
          var styleId = t.getAttribute("data-style");
          // Click same active chip → remove
          if (currentSelectionMarkStyle() === styleId) clearHighlight();
          else applyHighlight(styleId);
        }
      });
    }

    // Soft right-click context menu
    var ctxMenu = el("ctx-menu");
    if (ctxMenu) {
      ctxMenu.addEventListener("click", function (ev) {
        var t = ev.target;
        if (!t) return;
        // Color chips inside submenu
        if (t.classList && t.classList.contains("hl-chip")) {
          ev.stopPropagation();
          if (t.getAttribute("data-action") === "hl-clear" || t.classList.contains("hl-clear")) {
            clearHighlight();
            refreshCtxMenuUI();
            return;
          }
          var styleId = t.getAttribute("data-style");
          if (styleId) {
            if (currentSelectionMarkStyle() === styleId) clearHighlight();
            else applyHighlight(styleId);
            refreshCtxMenuUI();
          }
          return;
        }
        var item = t.closest ? t.closest("[data-action]") : null;
        if (!item || !ctxMenu.contains(item)) return;
        var action = item.getAttribute("data-action");
        if (action === "highlight") {
          var sub = el("ctx-hl-sub");
          if (sub) sub.classList.toggle("open");
          return; // keep menu open
        }
        if (action === "copy") {
          var p = selectionCopyPayload();
          copyTextToClipboard(p.text).then(function () {
            closeCtxMenu();
          }).catch(function () { closeCtxMenu(); });
          return;
        }
        if (action === "share") {
          shareSelection().then(function () {
            closeCtxMenu();
          }).catch(function () { closeCtxMenu(); });
          return;
        }
        if (action === "underline") {
          toggleUnderline();
          closeCtxMenu();
          return;
        }
        if (action === "clear-marks") {
          clearAllMarksOnSelection();
          closeCtxMenu();
          return;
        }
      });
    }
    document.addEventListener("mousedown", function (ev) {
      if (!ctxState.open) return;
      var menu = el("ctx-menu");
      if (menu && !menu.contains(ev.target)) closeCtxMenu();
    });
    document.addEventListener("scroll", function () {
      if (ctxState.open) closeCtxMenu();
    }, true);
    window.addEventListener("resize", function () {
      if (ctxState.open) closeCtxMenu();
    });
    // Block native context menu on the Bible verse list
    var verseList = el("verse-list");
    if (verseList) {
      verseList.addEventListener("contextmenu", function (ev) {
        // Handlers on verse/word call preventDefault; this is a safety net
        if (ev.target && (ev.target.closest && (ev.target.closest(".verse") || ev.target.closest(".word")))) {
          ev.preventDefault();
        }
      });
    }

    document.addEventListener("keydown", function (ev) {
      if (ev.key === "Escape") {
        closeCtxMenu();
        closeVerseModal();
        closeHistoryModal();
        closeRefPanel();
        hideRefTooltip();
      }
    });
  }

  function boot() {
    // Allow boot after mini-loader painted the shell; only skip if fully wired.
    if (window.__ADC_FULL_WIRED__ || window.__ADC_START_RUNNING__) return;
    window.__ADC_BOOT_STARTED__ = true;
    var host = window.__ADC_HOST__ || "";
    var hostHint =
      host === "winui3" ? "Conectando con WinUI 3 + adc-api…" :
      host === "edge-app" ? "Conectando con shell estable…" :
      getApiBase() ? "Conectando con adc-api…" :
      "Buscando API…";
    setBoot(hostHint);

    var tries = 0;
    var maxTries = 40; // ~10s

    function attempt() {
      if (window.__ADC_FULL_WIRED__ || window.__ADC_START_RUNNING__) return;
      tries++;
      if (!getInvoke()) {
        if (tries >= maxTries) {
          setBoot(
            "Sin API. Ejecutá Start-ADC-Stable.bat",
            true
          );
          return;
        }
        setBoot("Esperando adc-api… (" + tries + "/" + maxTries + ")");
        setTimeout(attempt, 250);
        return;
      }
      pingApi().then(function (ok) {
        if (window.__ADC_FULL_WIRED__ || window.__ADC_START_RUNNING__) return;
        if (!ok) {
          if (tries >= maxTries) {
            setBoot(
              "adc-api no responde en " + (getApiBase() || "127.0.0.1:17865") +
              ". Ejecutá Start-ADC-Stable.bat",
              true
            );
            return;
          }
          setBoot("API no lista… reintentando (" + tries + "/" + maxTries + ")");
          setTimeout(attempt, 250);
          return;
        }
        start();
      });
    }
    attempt();
  }

  // Hand-off for early boot script
  window.__ADC_RUN_BOOT__ = boot;

  function start() {
    if (window.__ADC_START_RUNNING__) return;
    window.__ADC_START_RUNNING__ = true;
    setBoot("Escaneando e-Sword…");
    var scanTimeout = new Promise(function (_, reject) {
      setTimeout(function () {
        reject(new Error(
          "Timeout escaneando e-Sword (45s). ¿La carpeta de e-Sword existe?"
        ));
      }, 45000);
    });
    Promise.race([
      Promise.all([
        invoke("list_esword_modules", { path: null }),
        invoke("list_bible_books")
      ]),
      scanTimeout
    ]).then(function (pair) {
      try {
        setBoot("Preparando interfaz… (" + ((pair[0] || []).length) + " módulos)");
        state.modules = pair[0] || [];
        state.books = pair[1] || [];
        state.bibles = state.modules.filter(function (m) { return m.moduleType === "bible"; });
        state.commentaries = state.modules.filter(function (m) { return m.moduleType === "commentary"; });
        state.dictionaries = state.modules.filter(function (m) { return m.moduleType === "dictionary"; });
        state.lexicons = state.modules.filter(function (m) { return m.moduleType === "lexicon"; });

        state.bible = pickDefaultBible(state.bibles);
        state.cmt = state.commentaries[0] || null;
        state.dict = state.dictionaries.filter(function (d) {
          return /vine|strong|expositivo|pik|mundo hispano|lockward|ort[ií]z|macarthur/i.test(
            d.title + d.filename + d.abbreviation
          );
        })[0] || state.dictionaries[0] || null;
        state.lex = state.lexicons.filter(function (l) {
          return /strong/i.test(l.title + l.filename + l.abbreviation) &&
            !/swanson|barclay|tuggy/i.test(l.title + l.filename);
        })[0] || state.lexicons.filter(function (l) {
          return /strong|ch[aá]vez|vine|multil/i.test(l.title + l.filename + l.abbreviation);
        })[0] || state.lexicons[0] || null;
        state.cmt = state.commentaries.filter(function (c) {
          return /btx|recobro|rbr|scofield|expositor|vp|kadosh/i.test(
            c.title + c.filename + c.abbreviation
          );
        })[0] || state.commentaries[0] || null;

        var nav = loadNavPosition();
        if (nav) {
          if (nav.biblePath) {
            for (var bi = 0; bi < state.bibles.length; bi++) {
              if (state.bibles[bi].path === nav.biblePath) {
                state.bible = state.bibles[bi];
                break;
              }
            }
          }
          if (nav.cmtPath) {
            for (var ci = 0; ci < state.commentaries.length; ci++) {
              if (state.commentaries[ci].path === nav.cmtPath) {
                state.cmt = state.commentaries[ci];
                break;
              }
            }
          }
          if (nav.dictPath) {
            for (var di = 0; di < state.dictionaries.length; di++) {
              if (state.dictionaries[di].path === nav.dictPath) {
                state.dict = state.dictionaries[di];
                break;
              }
            }
          }
          if (nav.lexPath) {
            for (var li = 0; li < state.lexicons.length; li++) {
              if (state.lexicons[li].path === nav.lexPath) {
                state.lex = state.lexicons[li];
                break;
              }
            }
          }
          if (nav.bookNumber >= 1 && nav.bookNumber <= 66) {
            state.bookNumber = nav.bookNumber;
          }
          var maxCh = 150;
          for (var bi2 = 0; bi2 < state.books.length; bi2++) {
            if (state.books[bi2].number === state.bookNumber) {
              maxCh = state.books[bi2].chapters || 150;
              break;
            }
          }
          if (nav.chapter >= 1 && nav.chapter <= maxCh) {
            state.chapter = nav.chapter;
          }
          if (nav.selectedVerse != null && nav.selectedVerse >= 1) {
            state.selectedVerse = nav.selectedVerse;
            state.highlightMode = "verse";
          }
        }

        var modCountEl = el("mod-count");
        if (modCountEl) {
          modCountEl.textContent = state.modules.length + " módulos e-Sword";
        }

        fillBookPill();
        fillBiblePill();
        fillChapterSelect();

        loadParallelPrefs();
        loadCmtLevelPrefs();
        wireEvents();
        loadSavedTheme();
        var view = state.bibleView;
        state.bibleView = "single";
        setBibleView(view === "parallel" ? "parallel" : "single");
        fillParallelSelects();
        showApp();
        updateFooter();
        updateSelectionUI();
        loadChapter();
        document.title = "Asignación del Cielo Bible";
        window.__ADC_FULL_WIRED__ = true;
        window.__ADC_START_RUNNING__ = false;
      } catch (err) {
        console.error(err);
        window.__ADC_START_RUNNING__ = false;
        // If mini-loader already showed the shell, don't trap the user on boot.
        if (!window.__ADC_APP_SHOWN__) {
          setBoot("Error preparando UI: " + (err.message || err), true);
        }
      }
    }).catch(function (e) {
      window.__ADC_START_RUNNING__ = false;
      if (!window.__ADC_APP_SHOWN__) {
        setBoot("Error: " + (e.message || e), true);
      } else {
        console.warn("full start failed after mini paint", e);
      }
    });
  }

  // Apply theme ASAP (even on boot screen)
  try {
    var earlyDefault = (window.__ADC_HOST__ === "winui3" || window.__ADC_HOST__ === "edge-app")
      ? "fluent-glass"
      : "true-dark";
    var early = localStorage.getItem(THEME_KEY) || earlyDefault;
    // Removed themes fall back to default for this host
    if (early === "liquid-glass") early = earlyDefault;
    document.documentElement.setAttribute("data-theme", early);
    document.documentElement.removeAttribute("data-glass");
  } catch (e) {}

  // Show host chip on desktop shells
  try {
    if (window.__ADC_HOST__ === "winui3" || window.__ADC_HOST__ === "edge-app") {
      var chip = document.getElementById("host-chip");
      if (chip) {
        chip.classList.add("show");
        chip.textContent = window.__ADC_HOST__ === "edge-app" ? "Stable" : "Fluent";
        chip.title = window.__ADC_HOST__ === "edge-app"
          ? "Shell estable (Edge app + adc-api)"
          : "Corriendo en WinUI 3";
      }
    }
  } catch (e) {}

  // Expose again after full init (boot closes over latest start)
  window.__ADC_RUN_BOOT__ = boot;
  window.__ADC_WIRE_EVENTS__ = wireEvents;

  // If mini-loader already painted the shell, still take over interactions ASAP.
  try {
    if (window.__ADC_APP_SHOWN__ && !window.__ADC_FULL_WIRED__) {
      boot();
    } else if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", boot);
    } else {
      boot();
    }
  } catch (e) {
    console.error("boot entry failed", e);
    try {
      wireEvents();
    } catch (e2) {
      console.error(e2);
    }
  }
})();

