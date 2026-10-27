/**
 * Espada3.7 — Asignación del Cielo Bible (macOS)
 * Tabbed workspace + interconnected Bible ↔ Comentario ↔ Diccionario ↔ Léxico
 */

const invoke = (cmd, args = {}) => {
  if (window.__TAURI_INTERNALS__) {
    return window.__TAURI__.core.invoke(cmd, args);
  }
  console.warn("Tauri no disponible:", cmd, args);
  return Promise.reject(new Error("Tauri no disponible"));
};

const LS = {
  theme: "espada37.theme",
  modulesDir: "espada37.modulesDir",
  nav: "espada37.nav",
  notes: "espada37.notes",
  highlights: "espada37.highlights",
  parallel: "espada37.parallel",
  history: "espada37.history",
  study: "espada37.study",
};

// Prefer inline list from index.html (beats stale WebView caches of this file)
const THEMES =
  Array.isArray(window.ESPADA_THEMES) && window.ESPADA_THEMES.length
    ? window.ESPADA_THEMES
    : [
        { id: "true-dark", label: "True Dark" },
        { id: "plain-white", label: "Plain White" },
        { id: "warm-paper", label: "Warm Paper" },
        { id: "high-contrast", label: "High Contrast" },
        { id: "pastel-green", label: "Pastel Green" },
        { id: "pastel-yellow", label: "Pastel Yellow" },
        { id: "light-blue", label: "Light Blue" },
        { id: "frosted-glass", label: "Frosted Glass" },
        { id: "muted-lila", label: "Muted Lila" },
      ];

const HL_COLORS = ["gold", "sky", "rose", "lime", "violet", "peach"];

const state = {
  books: [],
  modules: [],
  modulesDir: "",
  book: 43,
  chapter: 3,
  verse: 16,
  biblePath: "",
  mode: "lectura",
  parallel1: "",
  parallel2: "",
  /** @type {'biblia'|'comentario'|'diccionario'|'lexico'} */
  workspace: "biblia",
  studyModule: "",
  filters: { verse: true, chapter: false, book: false },
  selectedVerses: new Set(),
  selectedWord: null,
  lastStrong: "",
  chapterData: null,
  history: [],
  highlights: {},
  notes: {},
  linkTarget: null,
  tooltipTimer: null,
  /** @type {null | object} last resolve_word result */
  wordLinks: null,
  /** Guards against stale async word lookups overwriting a newer click */
  interconnectSeq: 0,
  interlinearPath: "",
  modulesByKind: {
    comentario: "",
    diccionario: "",
    lexico: "",
  },
  /** Preferred cross-reference source path (OpenBible SQLite or TSK .cmti) */
  crossRefSourcePath: "",
};

// —— Persistence ——
function loadLS(key, fallback) {
  try {
    const v = localStorage.getItem(key);
    return v == null ? fallback : JSON.parse(v);
  } catch {
    return fallback;
  }
}
function saveLS(key, val) {
  localStorage.setItem(key, JSON.stringify(val));
}
function persistNav() {
  saveLS(LS.nav, {
    book: state.book,
    chapter: state.chapter,
    verse: state.verse,
    biblePath: state.biblePath,
  });
}
function persistParallel() {
  saveLS(LS.parallel, {
    mode: state.mode,
    parallel1: state.parallel1,
    parallel2: state.parallel2,
  });
}
function persistHighlights() {
  saveLS(LS.highlights, state.highlights);
}
function persistNotes() {
  saveLS(LS.notes, state.notes);
}
function persistHistory() {
  saveLS(LS.history, state.history.slice(0, 40));
}
function isCrossRefModule(m) {
  if (!m || m.kind !== "commentary") return false;
  const hay = `${m.title} ${m.abbreviation} ${m.filename}`.toLowerCase();
  return /tsk|tske|torrey|cross.?ref|xref|x-ref|referencias cruzadas|pasajes paralelos|treasury of scripture/.test(
    hay
  );
}

async function openCrossReferences() {
  openPanel("#panel-crossrefs");
  await loadCrossReferenceSources();
  await reloadCrossReferences();
}

async function loadCrossReferenceSources() {
  const sel = $("#sel-crossref-source");
  if (!sel) return;
  let sources = [];
  try {
    if (state.modulesDir) {
      sources = await invoke("list_cross_reference_sources", {
        modulesDir: state.modulesDir,
      });
    }
  } catch (e) {
    console.warn("list_cross_reference_sources", e);
  }
  if (!Array.isArray(sources)) sources = [];

  fillSelect(
    sel,
    sources,
    "path",
    (s) => {
      const enc = s.encrypted ? "🔒 " : "";
      return `${enc}${s.title}`;
    },
    state.crossRefSourcePath
  );
  state.crossRefSourcePath = sel.value || "";
  const study = loadLS(LS.study, {});
  if (study && typeof study === "object") {
    study.crossRefSourcePath = state.crossRefSourcePath;
    saveLS(LS.study, study);
  }
}

async function reloadCrossReferences() {
  const list = $("#crossrefs-list");
  const fromEl = $("#crossrefs-from");
  const hint = $("#crossrefs-hint");
  if (!list) return;

  const fromLabel = `${bookName(state.book)} ${state.chapter}:${state.verse}`;
  if (fromEl) fromEl.textContent = fromLabel;

  if (!state.modulesDir) {
    list.innerHTML = `<div class="empty-state">Seleccione una carpeta de módulos en Ajustes.</div>`;
    return;
  }

  list.innerHTML = `<div class="empty-state soft">Cargando referencias…</div>`;
  try {
    const payload = await invoke("get_cross_references", {
      modulesDir: state.modulesDir,
      book: state.book,
      chapter: state.chapter,
      verse: state.verse,
      sourcePath: state.crossRefSourcePath || null,
      limit: 80,
    });
    if (payload?.sourcePath) {
      state.crossRefSourcePath = payload.sourcePath;
      const sel = $("#sel-crossref-source");
      if (sel && payload.sourcePath) sel.value = payload.sourcePath;
    }
    if (hint) {
      const src = payload?.source ? `Fuente: ${payload.source}` : "";
      hint.textContent = src
        ? `${src}. Clic en una referencia para ir al pasaje.`
        : "Pasajes relacionados. Clic en una referencia para ir al pasaje.";
    }
    renderCrossReferences(payload);
  } catch (e) {
    list.innerHTML = `<div class="empty-state">${escapeHtml(String(e))}</div>`;
  }
}

function renderCrossReferences(payload) {
  const list = $("#crossrefs-list");
  if (!list) return;
  if (!payload) {
    list.innerHTML = `<div class="empty-state soft">Sin datos.</div>`;
    return;
  }
  if (payload.note && (!payload.entries || payload.entries.length === 0)) {
    list.innerHTML = `<div class="empty-state">${escapeHtml(payload.note)}</div>`;
    return;
  }
  const entries = payload.entries || [];
  if (!entries.length) {
    list.innerHTML = `<div class="empty-state soft">No hay referencias para este versículo.</div>`;
    return;
  }

  // Discrete navigable hits (OpenBible / parsed TSK)
  const navigable = entries.filter((e) => e.toBook > 0 && e.toChapter > 0);
  if (navigable.length) {
    list.innerHTML = navigable
      .map((e) => {
        const votes =
          e.votes > 0
            ? `<span class="cr-votes" title="Votos OpenBible">${e.votes}</span>`
            : "";
        return `<button type="button" class="crossref-row" data-book="${e.toBook}" data-chapter="${e.toChapter}" data-verse="${e.toVerseStart}" data-verse-end="${e.toVerseEnd}">
          <span class="cr-label">${escapeHtml(e.label)}</span>
          ${votes}
        </button>`;
      })
      .join("");
    list.querySelectorAll(".crossref-row").forEach((btn) => {
      btn.onclick = async () => {
        const b = Number(btn.dataset.book);
        const c = Number(btn.dataset.chapter);
        const v = Number(btn.dataset.verse);
        closePanels();
        await goTo(b, c, v);
      };
    });
    return;
  }

  // Raw HTML blocks (TSK commentary blob)
  list.innerHTML = entries
    .map(
      (e) =>
        `<div class="crossref-block">${e.label ? `<div class="tip-muted" style="margin-bottom:6px">${escapeHtml(
          e.label
        )}</div>` : ""}${e.html || escapeHtml(e.label || "")}</div>`
    )
    .join("");
}

function persistStudy() {
  saveLS(LS.study, {
    workspace: state.workspace,
    modulesByKind: state.modulesByKind,
    filters: state.filters,
    selectedWord: state.selectedWord,
    crossRefSourcePath: state.crossRefSourcePath,
    lastStrong: state.lastStrong,
  });
}

const $ = (sel, el = document) => el.querySelector(sel);
const $$ = (sel, el = document) => [...el.querySelectorAll(sel)];

function bookName(n) {
  return state.books.find((b) => b.number === n)?.name || `Libro ${n}`;
}
function bookAbbr(n) {
  return state.books.find((b) => b.number === n)?.abbr || String(n);
}
function bookChapters(n) {
  return state.books.find((b) => b.number === n)?.chapters || 1;
}
function noteKey(b = state.book, c = state.chapter) {
  return `${b}:${c}`;
}
function hlKey(b, c, v) {
  return `${b}:${c}:${v}`;
}
function modulesOf(kind) {
  return state.modules.filter((m) => m.kind === kind);
}
function escapeHtml(s) {
  return String(s ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
function escapeAttr(s) {
  return escapeHtml(s).replace(/'/g, "&#39;");
}

// —— Soft UI ——
function openPanel(id) {
  $("#overlay").classList.remove("hidden");
  $(id).classList.remove("hidden");
}
function closePanels() {
  $("#overlay").classList.add("hidden");
  $$(".soft-panel").forEach((p) => p.classList.add("hidden"));
  hideCtx();
  hideTooltip();
}
function showTooltip(html, x, y) {
  const tip = $("#tooltip");
  tip.innerHTML = html;
  tip.classList.remove("hidden");
  const pad = 12;
  const rect = tip.getBoundingClientRect();
  let left = x + 14;
  let top = y + 16;
  if (left + rect.width > window.innerWidth - pad) left = x - rect.width - 10;
  if (top + rect.height > window.innerHeight - pad) top = y - rect.height - 10;
  tip.style.left = `${Math.max(pad, left)}px`;
  tip.style.top = `${Math.max(pad, top)}px`;
}
function hideTooltip() {
  clearTimeout(state.tooltipTimer);
  $("#tooltip").classList.add("hidden");
}
function hideCtx() {
  $("#ctx-menu").classList.add("hidden");
}
function alertSoft(title, msg) {
  $("#link-panel-title").textContent = title;
  $("#link-panel-body").innerHTML = `<p>${escapeHtml(msg)}</p>`;
  $("#btn-goto-passage").classList.add("hidden");
  $("#btn-open-lexicon").classList.add("hidden");
  openPanel("#panel-link");
}

// —— Theme ——
/**
 * Frosted Glass = native macOS NSVisualEffectView under a transparent webview
 * so wallpaper / apps behind the window actually bleed through.
 * Other themes clear that effect and use solid CSS fills.
 */
let _frostSeq = 0;
/** Re-apply native frost after sleep / focus so it stays navy, not milky white. */
function wireFrostResilience() {
  if (window.__espadaFrostResilience) return;
  window.__espadaFrostResilience = true;
  const reassert = () => {
    const id = document.body?.dataset?.theme;
    if (id === "frosted-glass") {
      applyWindowVibrancy("frosted-glass");
    }
  };
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") reassert();
  });
  window.addEventListener("focus", reassert);
  // macOS wake / display reconfig often fires pageshow or a delayed focus
  window.addEventListener("pageshow", reassert);
}

async function applyWindowVibrancy(themeId) {
  const seq = ++_frostSeq;
  const wantFrost = themeId === "frosted-glass";
  const stamp = $("#build-stamp");
  wireFrostResilience();
  try {
    // Preferred path: custom Rust command (clear + apply on main thread, waits for result).
    const status = await invoke("set_frosted_window", { enabled: wantFrost });
    if (seq !== _frostSeq) return;
    console.info("[Espada frost]", status);
    if (stamp) {
      stamp.textContent = wantFrost
        ? `· Frosted Glass · navy lock`
        : `· Asignación del Cielo`;
    }
  } catch (err) {
    console.warn("set_frosted_window falló, intentando setEffects:", err);
    if (stamp) stamp.textContent = `· frost-err: ${String(err).slice(0, 48)}`;
    try {
      const tw = window.__TAURI__?.webviewWindow?.getCurrentWebviewWindow?.();
      if (!tw) return;
      if (wantFrost) {
        await tw.setBackgroundColor?.("#00000000");
        await tw.setEffects({
          effects: ["underWindowBackground"],
          state: "active",
          radius: 20,
        });
      } else if (typeof tw.clearEffects === "function") {
        await tw.clearEffects();
      } else {
        await tw.setEffects(null);
      }
    } catch (err2) {
      console.warn("Vibrancy no disponible:", err2);
    }
  }
}

function themeList() {
  if (Array.isArray(window.ESPADA_THEMES) && window.ESPADA_THEMES.length) {
    return window.ESPADA_THEMES;
  }
  return THEMES;
}

function applyTheme(id) {
  const list = themeList();
  if (!list.some((t) => t.id === id)) id = "true-dark";
  document.body.dataset.theme = id;
  document.documentElement.dataset.theme = id;
  // Class hook for CSS (gaps / chrome that need full freeze transparency)
  document.body.classList.toggle("theme-frosted", id === "frosted-glass");
  document.documentElement.classList.toggle("theme-frosted", id === "frosted-glass");
  localStorage.setItem(LS.theme, id);
  // Active state only in the theme picker panel (no chrome strip / quickbar)
  $$("#theme-grid .theme-card").forEach((c) =>
    c.classList.toggle("active", c.dataset.theme === id)
  );
  applyWindowVibrancy(id);
}

/** Themes only appear in #panel-theme (🎨), not as a separate chrome strip. */
function renderThemeGrid() {
  const grid = $("#theme-grid");
  if (!grid) {
    console.error("theme-grid no encontrado");
    return;
  }
  const list = themeList();
  const active = document.body.dataset.theme || "true-dark";
  grid.innerHTML = list
    .map(
      (t) =>
        `<button type="button" class="theme-card${t.id === active ? " active" : ""}" data-theme="${t.id}" title="${t.label}">
        <span class="theme-swatch" aria-hidden="true"></span>
        <span class="theme-card-label">${escapeHtml(t.label)}</span>
      </button>`
    )
    .join("");
  // Single handler path (no document-level duplicate)
  grid.onclick = (e) => {
    const btn = e.target.closest(".theme-card");
    if (btn?.dataset.theme) applyTheme(btn.dataset.theme);
  };
  const title = $("#theme-panel-title");
  if (title) title.textContent = `Temas (${list.length})`;
  const hint = $("#theme-panel-hint");
  if (hint) {
    hint.innerHTML = `Elija un tema para la lectura. <strong>${list.length}</strong> disponibles.`;
  }
  console.info("[Espada] themes in picker:", list.map((t) => t.id).join(", "));
}

// —— Modules ——
function fillSelect(sel, items, valueKey, labelFn, selected) {
  const prev = selected ?? sel.value;
  sel.innerHTML = items
    .map((it) => {
      const val = it[valueKey];
      return `<option value="${escapeAttr(val)}" title="${escapeAttr(it.title || "")}">${escapeHtml(labelFn(it))}</option>`;
    })
    .join("");
  if (prev && [...sel.options].some((o) => o.value === prev)) sel.value = prev;
  else if (items.length) {
    const firstOk = items.find((i) => !i.encrypted) || items[0];
    sel.value = firstOk[valueKey];
  }
}

async function loadModules(dir) {
  if (!dir) return;
  const meta = $("#modules-meta");
  if (meta) meta.textContent = "Escaneando módulos…";
  state.modulesDir = dir;
  localStorage.setItem(LS.modulesDir, dir);
  $("#modules-path").value = dir;
  try {
    state.modules = await invoke("list_modules", { modulesDir: dir });
  } catch (e) {
    state.modules = [];
    alertSoft("Error al escanear módulos", String(e));
    return;
  }

  const bibles = modulesOf("bible");
  fillSelect(
    $("#sel-bible"),
    bibles,
    "path",
    (m) => `${m.encrypted ? "🔒 " : ""}${m.abbreviation}`,
    state.biblePath
  );
  state.biblePath = $("#sel-bible").value;
  updateBibleTitleTooltip();

  const emptyOpt = [{ path: "", abbreviation: "— Ninguna —", title: "", encrypted: false }];
  fillSelect(
    $("#sel-parallel1"),
    emptyOpt.concat(bibles),
    "path",
    (m) => (m.path ? `${m.encrypted ? "🔒 " : ""}${m.abbreviation}` : m.abbreviation),
    state.parallel1
  );
  fillSelect(
    $("#sel-parallel2"),
    emptyOpt.concat(bibles),
    "path",
    (m) => (m.path ? `${m.encrypted ? "🔒 " : ""}${m.abbreviation}` : m.abbreviation),
    state.parallel2
  );
  state.parallel1 = $("#sel-parallel1").value;
  state.parallel2 = $("#sel-parallel2").value;

  try {
    state.interlinearPath =
      (await invoke("find_interlinear", { modulesDir: dir })) ||
      bibles.find((b) => /interlineal|interlinear|irv/i.test(b.filename + b.title))?.path ||
      "";
  } catch {
    state.interlinearPath = "";
  }

  const counts = {
    bible: modulesOf("bible").length,
    commentary: modulesOf("commentary").length,
    dictionary: modulesOf("dictionary").length,
    lexicon: modulesOf("lexicon").length,
  };
  if (meta) {
    meta.textContent = `Biblias: ${counts.bible} · Comentarios: ${counts.commentary} · Diccionarios: ${counts.dictionary} · Léxicos: ${counts.lexicon}${
      state.interlinearPath ? " · Interlineal detectado" : ""
    }`;
  }

  fillStudyModuleSelect();
  await reloadChapter();
  await refreshWorkspace();
}

function updateBibleTitleTooltip() {
  const m = state.modules.find((x) => x.path === state.biblePath);
  $("#sel-bible").title = m ? m.title : "";
}

async function fillStudyModuleSelect() {
  const sel = $("#sel-study-module");
  const kindMap = {
    comentario: "commentary",
    diccionario: "dictionary",
    lexico: "lexicon",
  };
  const kind = kindMap[state.workspace];
  if (!kind) {
    sel.innerHTML = "";
    return;
  }
  let list = modulesOf(kind).filter((m) => !m.encrypted || kind === "commentary");
  if (kind === "lexicon") {
    const extra = modulesOf("dictionary").filter((m) =>
      /strong|lexic|vine|thayer|bdb|hebrew|hebreo|griego|greek/i.test(
        `${m.abbreviation} ${m.title} ${m.filename}`
      )
    );
    const seen = new Set(list.map((m) => m.path));
    for (const m of extra) if (!seen.has(m.path)) list.push(m);
  }

  const probeQuery =
    state.workspace === "lexico"
      ? (state.lastStrong || state.selectedWord || $("#dict-query")?.value || "")
      : (state.selectedWord || $("#dict-query")?.value || "");

  let availabilityMap = new Map();
  try {
    const probeRes = await invoke("probe_modules_content", {
      kind,
      paths: list.map((m) => m.path),
      book: state.book,
      chapter: state.chapter,
      verse: state.verse,
      query: probeQuery,
      filters: state.filters,
    });
    if (Array.isArray(probeRes)) {
      for (const r of probeRes) {
        availabilityMap.set(r.path, r.has);
      }
    }
  } catch (_) {}

  // Sort list: modules WITH content first, then alphabetical
  list.sort((a, b) => {
    const hasA = availabilityMap.get(a.path) ? 1 : 0;
    const hasB = availabilityMap.get(b.path) ? 1 : 0;
    if (hasA !== hasB) return hasB - hasA;
    return a.abbreviation.localeCompare(b.abbreviation, "es");
  });

  const saved = state.modulesByKind[state.workspace] || "";
  fillSelect(
    sel,
    list,
    "path",
    (m) => {
      const has = availabilityMap.get(m.path);
      const tag = has ? "✦ " : "";
      const enc = m.encrypted ? "🔒 " : "";
      const hasNote = has ? " — (Con datos)" : "";
      return `${tag}${enc}${m.abbreviation} — ${m.title}${hasNote}`;
    },
    saved
  );
  state.studyModule = sel.value;
  state.modulesByKind[state.workspace] = sel.value;
  persistStudy();
}

// —— Workspace tabs ——
function setWorkspace(ws, { skipReload = false } = {}) {
  state.workspace = ws;
  $$(".ws-tab").forEach((t) => {
    const on = t.dataset.ws === ws;
    t.classList.toggle("active", on);
    t.setAttribute("aria-selected", on ? "true" : "false");
  });
  const isBible = ws === "biblia";
  $("#view-biblia").classList.toggle("hidden", !isBible);
  $("#view-study").classList.toggle("hidden", isBible);
  $("#study-chrome").classList.toggle("hidden", isBible);
  $("#commentary-filters").classList.toggle("hidden", ws !== "comentario");
  $("#dict-search-row").classList.toggle(
    "hidden",
    ws !== "diccionario" && ws !== "lexico"
  );
  const dq = $("#dict-query");
  if (dq) {
    if (ws === "lexico") dq.placeholder = "Strong G26 / H430, hebreo, griego o lema…";
    else if (ws === "diccionario") dq.placeholder = "Palabra o tema en español…";
    // Always sync search box to the active selection (never keep a stale word)
    if (ws === "diccionario" && state.selectedWord) {
      dq.value = state.selectedWord;
    } else if (ws === "lexico") {
      dq.value = state.lastStrong || state.selectedWord || dq.value;
    }
  }
  updateAnalysisChrome();
  fillStudyModuleSelect();
  persistStudy();
  if (!skipReload) refreshWorkspace();
}

async function refreshWorkspace() {
  updateContextBar();
  await refreshAnalysisPane();
  if (state.workspace === "biblia") return;
  await loadStudyView();
}

function updateAnalysisChrome() {
  const title = $("#analysis-title");
  const hint = $("#analysis-hint");
  const qbox = $("#analysis-query");
  if (!title || !hint) return;
  if (state.workspace === "biblia") {
    title.textContent = "Análisis";
    hint.innerHTML =
      "Clic en una palabra del texto: el centro puede abrir Diccionario/Léxico; aquí verá <strong>otros</strong> módulos con el mismo lema.";
  } else if (state.workspace === "comentario") {
    title.textContent = "Otros comentarios";
    hint.innerHTML =
      "El centro muestra el comentario principal. Aquí, <strong>otros</strong> módulos sobre el mismo versículo (resumen).";
  } else if (state.workspace === "diccionario") {
    title.textContent = "Otros diccionarios";
    hint.innerHTML =
      "El centro es el diccionario elegido arriba. Aquí, <strong>otras</strong> definiciones del mismo lema.";
  } else if (state.workspace === "lexico") {
    title.textContent = "Otros léxicos";
    hint.innerHTML =
      "El centro es el léxico principal. Aquí, <strong>otros</strong> Strong/lemas del mismo término.";
  }
  const label =
    state.workspace === "comentario"
      ? `${bookAbbr(state.book)} ${state.chapter}:${state.verse}`
      : state.selectedWord || state.lastStrong || "";
  if (qbox) {
    if (label) {
      qbox.classList.remove("hidden");
      qbox.innerHTML = `<span class="aq-label">Consulta</span> <strong>${escapeHtml(label)}</strong>`;
    } else {
      qbox.classList.add("hidden");
      qbox.innerHTML = "";
    }
  }
}

function updateContextBar() {
  const w = state.selectedWord || "—";
  $("#ws-ctx-word").textContent = w;
  const strongs = state.wordLinks?.strongs || [];
  $("#ws-ctx-strongs").innerHTML = strongs
    .map((s) => `<button type="button" class="chip-strong" data-strong="${escapeAttr(s)}">${escapeHtml(s)}</button>`)
    .join(" ");
  updateAnalysisChrome();
}

function snippet(plain, n = 160) {
  const s = (plain || "").replace(/\s+/g, " ").trim();
  if (s.length <= n) return s;
  return s.slice(0, n) + "…";
}

/**
 * Side panel: OTHER modules only (not the primary study module).
 * Main tab = one module in full; side = brief alternatives.
 */
async function refreshAnalysisPane() {
  const box = $("#link-panel-inline");
  if (!box) return;

  const primary = state.studyModule || "";
  const ws = state.workspace;

  if (ws === "comentario") {
    box.innerHTML = `<div class="empty-state soft">Cargando otros comentarios…</div>`;
    const others = modulesOf("commentary").filter((m) => m.path !== primary && !m.encrypted);
    const cards = [];
    // Limit parallel work
    for (const m of others.slice(0, 10)) {
      try {
        const entries = await invoke("get_commentary", {
          modulePath: m.path,
          book: state.book,
          chapter: state.chapter,
          verse: state.verse,
          filters: state.filters,
        });
        if (!entries?.length) continue;
        const first = entries[0];
        cards.push({
          kind: "cmt",
          path: m.path,
          abbr: m.abbreviation,
          topic: first.label || "v.",
          plain: first.plain || "",
        });
      } catch {
        /* skip */
      }
    }
    if (!cards.length) {
      box.innerHTML = `<div class="empty-state soft">Ningún otro comentario tiene notas para ${escapeHtml(
        bookAbbr(state.book)
      )} ${state.chapter}:${state.verse} con los filtros actuales.</div>`;
      return;
    }
    box.innerHTML =
      `<div class="ctx-section-title">Otros sobre este versículo</div>` +
      cards
        .map(
          (c) => `
      <article class="mini-entry" data-kind="cmt" data-path="${escapeAttr(c.path)}">
        <div class="mini-meta">${escapeHtml(c.abbr)} · ${escapeHtml(c.topic)}</div>
        <div class="mini-body">${escapeHtml(snippet(c.plain))}</div>
      </article>`
        )
        .join("");
    return;
  }

  if (ws === "diccionario" || ws === "biblia") {
    const word = state.selectedWord;
    if (!word) {
      box.innerHTML = `<div class="empty-state soft">Haga clic en una palabra del texto bíblico para ver <strong>otros</strong> diccionarios y léxicos aquí.</div>`;
      return;
    }
    if (!state.wordLinks) {
      box.innerHTML = `<div class="empty-state soft">Buscando «${escapeHtml(word)}»…</div>`;
      return;
    }
    const wl = state.wordLinks;
    const dictOthers = (wl.dictionary || []).filter((e) => e.modulePath !== primary);
    const lexOthers = (wl.lexicon || []).filter((e) => e.modulePath !== primary);

    // On Biblia tab, show both kinds; on Diccionario focus other dicts (+ brief lex)
    let html = `<div class="ctx-word-title">${escapeHtml(wl.word)}</div>`;
    if (wl.strongs?.length) {
      html += `<div class="ctx-row">${wl.strongs
        .map((s) => `<button type="button" class="chip-strong" data-strong="${escapeAttr(s)}">${escapeHtml(s)}</button>`)
        .join("")}</div>`;
    }

    if (ws === "diccionario" || ws === "biblia") {
      html += `<div class="ctx-section-title">${ws === "diccionario" ? "Otros diccionarios" : "Diccionarios"}</div>`;
      if (dictOthers.length) {
        html += dictOthers
          .slice(0, 8)
          .map(
            (e) => `
        <article class="mini-entry" data-kind="dict" data-path="${escapeAttr(e.modulePath)}" data-topic="${escapeAttr(e.topic)}">
          <div class="mini-meta">${escapeHtml(e.moduleAbbr)} · ${escapeHtml(e.topic)}</div>
          <div class="mini-body">${escapeHtml(snippet(e.plain))}</div>
        </article>`
          )
          .join("");
      } else {
        html += `<div class="empty-state soft">No hay otras definiciones de diccionario para este lema.</div>`;
      }
    }

    if (ws === "biblia" || ws === "diccionario") {
      html += `<div class="ctx-section-title">${ws === "diccionario" ? "Léxico relacionado" : "Léxicos"}</div>`;
      if (lexOthers.length || (wl.lexicon || []).length) {
        const list = (ws === "diccionario" ? lexOthers : wl.lexicon || []).slice(0, 6);
        if (list.length) {
          html += list
            .map(
              (e) => `
          <article class="mini-entry" data-kind="lex" data-path="${escapeAttr(e.modulePath)}" data-topic="${escapeAttr(e.topic)}">
            <div class="mini-meta">${escapeHtml(e.moduleAbbr)} · ${escapeHtml(e.topic)}</div>
            <div class="mini-body">${escapeHtml(snippet(e.plain))}</div>
          </article>`
            )
            .join("");
        } else {
          html += `<div class="empty-state soft">Sin otros léxicos para este término.</div>`;
        }
      } else {
        html += `<div class="empty-state soft">Sin Strong/léxico aún (ayuda un interlineal).</div>`;
      }
    }

    // Wire interlinear units if present on Biblia analysis
    box.querySelectorAll?.(".il-unit[data-lookup]")?.forEach?.(() => {});
    box.innerHTML = html;
    // re-query after assignment
    box.querySelectorAll(".il-unit").forEach((el) => {
      el.addEventListener("click", () => {
        const s = el.getAttribute("data-strong");
        const w = el.getAttribute("data-lookup");
        if (s) interconnectWord(s, { stay: true });
        else if (w) interconnectWord(w, { stay: true });
      });
    });
    return;
  }

  if (ws === "lexico") {
    const q = state.lastStrong || state.selectedWord;
    if (!q) {
      box.innerHTML = `<div class="empty-state soft">Busque un Strong o palabra; aquí verá <strong>otros</strong> léxicos.</div>`;
      return;
    }
    if (!state.wordLinks) {
      box.innerHTML = `<div class="empty-state soft">Buscando…</div>`;
      return;
    }
    const wl = state.wordLinks;
    const lexOthers = (wl.lexicon || []).filter((e) => e.modulePath !== primary);
    const dictOthers = (wl.dictionary || []).filter((e) => e.modulePath !== primary);
    const hit = wl.matchedHit;
    let html = `<div class="ctx-word-title">${escapeHtml(wl.word)}</div>`;
    // Mini original ‖ Spanish strip
    if (hit && (hit.original || hit.spanish)) {
      html += `<div class="ctx-il-pair">
        <div class="ctx-il-orig ${hit.script === "hebrew" ? "dir-rtl" : ""}">${
          hit.original ? escapeHtml(hit.original) : "—"
        }</div>
        <div class="ctx-il-sep">‖</div>
        <div class="ctx-il-es">${hit.spanish ? escapeHtml(hit.spanish) : "—"}</div>
      </div>`;
      if (hit.translit) {
        html += `<div class="ctx-il-tr">${escapeHtml(hit.translit)}</div>`;
      }
    } else if ((wl.hebrew && wl.hebrew[0]) || (wl.greek && wl.greek[0])) {
      const o = (wl.hebrew && wl.hebrew[0]) || (wl.greek && wl.greek[0]);
      html += `<div class="ctx-il-pair"><div class="ctx-il-orig">${escapeHtml(o)}</div></div>`;
    }
    if (wl.strongs?.length) {
      html += `<div class="ctx-row">${wl.strongs
        .map((s) => `<button type="button" class="chip-strong" data-strong="${escapeAttr(s)}">${escapeHtml(s)}</button>`)
        .join("")}</div>`;
    }
    // Verse interlinear strip (first 10 units)
    if (wl.interlinearHits?.length) {
      html += `<div class="ctx-section-title">Interlineal del versículo</div>`;
      html += `<div class="ctx-il-verse">${wl.interlinearHits
        .slice(0, 14)
        .map((h) => {
          const o = h.original || "·";
          const es = h.spanish || "";
          return `<button type="button" class="il-unit" data-strong="${escapeAttr(
            h.strong || ""
          )}" data-lookup="${escapeAttr(h.spanish || h.original || "")}" title="${escapeAttr(
            [h.strong, h.translit].filter(Boolean).join(" · ")
          )}">
            <span class="il-o ${h.script === "hebrew" ? "dir-rtl" : ""}">${escapeHtml(o)}</span>
            <span class="il-s">${escapeHtml(es)}</span>
          </button>`;
        })
        .join("")}</div>`;
    }
    html += `<div class="ctx-section-title">Otros léxicos</div>`;
    if (lexOthers.length) {
      html += lexOthers
        .slice(0, 8)
        .map(
          (e) => `
      <article class="mini-entry" data-kind="lex" data-path="${escapeAttr(e.modulePath)}" data-topic="${escapeAttr(e.topic)}">
        <div class="mini-meta">${escapeHtml(e.moduleAbbr)} · ${escapeHtml(e.topic)}</div>
        <div class="mini-body">${escapeHtml(snippet(e.plain))}</div>
      </article>`
        )
        .join("");
    } else {
      html += `<div class="empty-state soft">Ningún otro léxico tiene esta entrada (o solo está en el módulo principal).</div>`;
    }
    if (dictOthers.length) {
      html += `<div class="ctx-section-title">Diccionarios relacionados</div>`;
      html += dictOthers
        .slice(0, 4)
        .map(
          (e) => `
      <article class="mini-entry" data-kind="dict" data-path="${escapeAttr(e.modulePath)}" data-topic="${escapeAttr(e.topic)}">
        <div class="mini-meta">${escapeHtml(e.moduleAbbr)} · ${escapeHtml(e.topic)}</div>
        <div class="mini-body">${escapeHtml(snippet(e.plain))}</div>
      </article>`
        )
        .join("");
    }
    box.innerHTML = html;
    box.querySelectorAll(".il-unit").forEach((el) => {
      el.addEventListener("click", () => {
        const s = el.getAttribute("data-strong");
        const w = el.getAttribute("data-lookup");
        if (s) interconnectWord(s, { stay: true });
        else if (w) interconnectWord(w, { stay: true });
      });
    });
  }
}

// —— Navigation ——
function fillBooks() {
  $("#sel-book").innerHTML = state.books
    .map((b) => `<option value="${b.number}">${b.name}</option>`)
    .join("");
  $("#sel-book").value = String(state.book);
}
function fillChapters() {
  const n = bookChapters(state.book);
  $("#sel-chapter").innerHTML = Array.from({ length: n }, (_, i) => {
    const c = i + 1;
    return `<option value="${c}">${c}</option>`;
  }).join("");
  if (state.chapter > n) state.chapter = n;
  if (state.chapter < 1) state.chapter = 1;
  $("#sel-chapter").value = String(state.chapter);
}
function updateLocationLabel() {
  const m = state.modules.find((x) => x.path === state.biblePath);
  $("#location-label").textContent = `${bookName(state.book)} ${state.chapter}:${state.verse}${
    m ? ` · ${m.abbreviation}` : ""
  }`;
  $("#btn-verse-pick").textContent = String(state.verse);
  $("#chapter-heading").textContent = `${bookName(state.book)} ${state.chapter}:${state.verse}`;
  updateHistoryButtonTooltip();
}
function pushHistory() {
  const entry = {
    book: state.book,
    chapter: state.chapter,
    verse: state.verse,
    biblePath: state.biblePath,
    label: `${bookName(state.book)} ${state.chapter}:${state.verse}`,
    ts: Date.now(),
  };
  state.history = [
    entry,
    ...state.history.filter(
      (h) => !(h.book === entry.book && h.chapter === entry.chapter && h.verse === entry.verse)
    ),
  ].slice(0, 40);
  persistHistory();
  updateHistoryButtonTooltip();
}
async function goTo(book, chapter, verse, { history = true } = {}) {
  state.book = book;
  state.chapter = chapter;
  state.verse = verse || 1;
  state.selectedVerses = new Set([state.verse]);
  fillChapters();
  $("#sel-book").value = String(book);
  $("#sel-chapter").value = String(chapter);
  if (history) pushHistory();
  persistNav();
  await reloadChapter();
  if (state.selectedWord) await interconnectWord(state.selectedWord, { stay: true });
  await refreshWorkspace();
}

// —— Bible render ——
function tokenizeWords(html) {
  const parts = html.split(/(<[^>]+>)/);
  let out = "";
  let w = 0;
  for (const part of parts) {
    if (!part) continue;
    if (part.startsWith("<")) {
      out += part;
      continue;
    }
    out += part.replace(/([^\s.,;:!?¡¿()«»""''—–\-]+)/g, (word) => {
      const id = w++;
      return `<span class="word" data-word="${id}">${word}</span>`;
    });
  }
  return out;
}
function applyMarksToVerseEl(el, book, chapter, verse) {
  const mark = state.highlights[hlKey(book, chapter, verse)];
  el.classList.remove(...HL_COLORS.map((c) => `hl-${c}`), "mark-underline");
  if (!mark) return;
  if (mark.color && HL_COLORS.includes(mark.color)) el.classList.add(`hl-${mark.color}`);
  if (mark.underline) el.classList.add("mark-underline");
}

async function reloadChapter() {
  updateLocationLabel();
  const root = $("#bible-content");
  if (!state.biblePath) {
    root.innerHTML = `<div class="empty-state">Seleccione una carpeta de módulos y una Biblia.</div>`;
    return;
  }
  root.innerHTML = `<div class="empty-state soft">Cargando…</div>`;
  try {
    if (state.mode === "paralela") await renderParallel(root);
    else {
      const data = await invoke("get_bible_chapter", {
        modulePath: state.biblePath,
        book: state.book,
        chapter: state.chapter,
      });
      state.chapterData = data;
      renderReading(root, data);
    }
  } catch (e) {
    root.innerHTML = `<div class="empty-state">${escapeHtml(String(e))}</div>`;
  }
  requestAnimationFrame(() => {
    const el = root.querySelector(`.verse[data-verse="${state.verse}"]`);
    if (el) {
      el.classList.add("selected", "temp-select");
      el.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  });
  // Sync notes
  const notes = $("#notes-side");
  if (notes) notes.value = state.notes[noteKey()] || "";
}

function renderReading(root, data) {
  if (!data.verses?.length) {
    root.innerHTML = `<div class="empty-state">Capítulo vacío o no disponible en este módulo.</div>`;
    return;
  }
  root.innerHTML = data.verses
    .map((v, idx) => {
      const body = tokenizeWords(v.html);
      const first = idx === 0 ? " first-verse" : "";
      return `<p class="verse${first}" data-verse="${v.verse}" data-book="${data.book}" data-chapter="${data.chapter}">
        <sup class="verse-num" data-verse="${v.verse}">${v.verse}</sup><span class="verse-body">${body}</span>
      </p>`;
    })
    .join("");
  $$(".verse", root).forEach((el) => {
    applyMarksToVerseEl(el, data.book, data.chapter, Number(el.dataset.verse));
  });
}

async function renderParallel(root) {
  const paths = [state.biblePath, state.parallel1, state.parallel2].filter(Boolean);
  const unique = [...new Set(paths)];
  const chapters = await Promise.all(
    unique.map(async (p) => {
      try {
        return await invoke("get_bible_chapter", {
          modulePath: p,
          book: state.book,
          chapter: state.chapter,
        });
      } catch (e) {
        return { module: p, abbreviation: "?", title: String(e), verses: [] };
      }
    })
  );
  const maps = chapters.map((c) => {
    const m = new Map();
    (c.verses || []).forEach((v) => m.set(v.verse, v));
    return m;
  });
  const headers = chapters
    .map((c) => `<th title="${escapeAttr(c.title || "")}">${escapeHtml(c.abbreviation || "?")}</th>`)
    .join("");
  const verseNums = new Set();
  chapters.forEach((c) => (c.verses || []).forEach((v) => verseNums.add(v.verse)));
  const sorted = [...verseNums].sort((a, b) => a - b);
  let rows = "";
  for (const vn of sorted) {
    const cells = maps
      .map((m) => {
        const v = m.get(vn);
        return `<td class="verse" data-verse="${vn}">${v ? v.html : "—"}</td>`;
      })
      .join("");
    rows += `<tr data-verse="${vn}"><td class="vnum" data-verse="${vn}">${vn}</td>${cells}</tr>`;
  }
  root.innerHTML = `<table class="parallel-table"><thead><tr><th>#</th>${headers}</tr></thead><tbody>${rows}</tbody></table>`;
}

// —— Interlinking in HTML ——
const RE_STRONG = /\b([HG]\d{1,5})\b/gi;
// Longer abbrs first (Prov before Pro before Pr) so "Pro 23:24" is one link, not Pro + 23:24.
const RE_REF =
  /\b((?:[123]\s*)?(?:Génesis|Genesis|Éxodo|Exodo|Levítico|Levitico|Números|Numeros|Deuteronomio|Josué|Josue|Jueces|Rut|Samuel|Reyes|Crónicas|Cronicas|Esdras|Nehemías|Nehemias|Ester|Job|Salmos?|Proverbios|Eclesiastés|Eclesiastes|Cantares|Cantar(?:es)?|Isaías|Isaias|Jeremías|Jeremias|Lamentaciones|Ezequiel|Daniel|Oseas|Joel|Amós|Amos|Abdías|Abdias|Jonás|Jonas|Miqueas|Nahúm|Nahum|Habacuc|Sofonías|Sofonias|Hageo|Zacarías|Zacarias|Malaquías|Malaquias|Mateo|Marcos|Lucas|Juan|Hechos|Romanos|Corintios|Gálatas|Galatas|Efesios|Filipenses|Colosenses|Tesalonicenses|Timoteo|Tito|Filemón|Filemon|Hebreos|Santiago|Pedro|Judas|Apocalipsis|Gén|Gn|Ge|Gen|Éx|Exo|Ex|Lv|Lev|Nm|Núm|Num|Nu|Dt|Deut|Jos|Jue|Jdc|Rt|Ru|1S|1Sa|1Sam|2S|2Sa|2Sam|1R|1Re|1Rey|2R|2Re|2Rey|1Cr|1Cro|2Cr|2Cro|Esd|Ezr|Neh|Est|Sal|Sl|Prov|Pro|Pr|Ec|Ecl|Cnt|Can|Ct|Is|Isa|Jer|Jr|Lm|Lam|Ez|Eze|Dn|Dan|Os|Hos|Jl|Am|Abd|Ob|Jon|Mi|Miq|Nah|Hab|Sof|Hag|Zac|Zc|Mal|Mt|Mat|Mr|Mc|Mar|Lc|Luc|Jn|Joh|Hch|Hech|Act|Ro|Rom|Rm|1Co|1Cor|2Co|2Cor|Gá|Gal|Ga|Ef|Efe|Eph|Fil|Flp|Php|Col|1Ts|1Tes|1Th|2Ts|2Tes|2Th|1Ti|1Tim|2Ti|2Tim|Tit|Tt|Flm|Flmn|Phm|Heb|He|Stg|Sant|St|1P|1Pe|1Ped|2P|2Pe|2Ped|1Jn|1Jo|2Jn|2Jo|3Jn|3Jo|Jud|Jd|Apoc|Ap|Rev)(?:\.?))\s+(\d{1,3})\s*[:：\.]\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?/gi;
// Bare "23:24" without a book is too ambiguous (duplicated Pro 23:24 → broken popup).
// Only explicit verse markers relative to current chapter:
const RE_V_ONLY = /\b(?:v|vv|vers(?:ículo|ículos|iculo|iculos)?\.?)\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?\b/gi;
const RE_HEBREW = /[\u0590-\u05FF]{2,}/g;
const RE_GREEK = /[\u0370-\u03FF\u1F00-\u1FFF]{2,}/g;

/**
 * Linkify study HTML. ctx = { book, chapter } from current navigation for relative refs.
 * Uses placeholders so relative "1:5" does not re-match labels of full refs.
 */
function linkifyHtml(html, ctx = null) {
  if (!html) return "";
  const context = ctx || { book: state.book, chapter: state.chapter };
  const bookNameForCtx = bookName(context.book);
  const hold = [];
  const park = (htmlSnippet) => {
    const i = hold.length;
    hold.push(htmlSnippet);
    return `\uE000${i}\uE001`;
  };

  const parts = html.split(/(<[^>]+>)/);
  let out = parts
    .map((part) => {
      if (!part || part.startsWith("<")) return part;
      let s = part;
      s = s.replace(RE_STRONG, (m) =>
        park(`<span class="strong-ref" data-strong="${m.toUpperCase()}">${m}</span>`)
      );
      s = s.replace(RE_REF, (match, book, ch, v1, v2) => {
        const range = v2 ? `${v1}-${v2}` : v1;
        const b = book.trim().replace(/\.$/, "");
        // One span for the whole "Pro 23:24" — never leave 23:24 as a second link
        return park(
          `<span class="bible-ref" data-book="${escapeAttr(b)}" data-chapter="${ch}" data-verse="${v1}" data-verse-end="${v2 || v1}" title="Ver pasaje">${b} ${ch}:${range}</span>`
        );
      });
      s = s.replace(RE_V_ONLY, (match, v1, v2) => {
        const vs = Number(v1);
        const ve = v2 ? Number(v2) : vs;
        if (vs < 1 || vs > 200) return match;
        const range = ve !== vs ? `${vs}-${ve}` : String(vs);
        return park(
          `<span class="bible-ref" data-book="${escapeAttr(bookNameForCtx)}" data-chapter="${context.chapter}" data-verse="${vs}" data-verse-end="${ve}" title="Ver pasaje">v. ${range}</span>`
        );
      });
      s = s.replace(RE_HEBREW, (m) =>
        park(`<span class="hebrew-run word-like" data-lookup="${escapeAttr(m)}">${m}</span>`)
      );
      s = s.replace(RE_GREEK, (m) =>
        park(`<span class="greek-run word-like" data-lookup="${escapeAttr(m)}">${m}</span>`)
      );
      return s;
    })
    .join("");

  out = out.replace(/\uE000(\d+)\uE001/g, (_, i) => hold[Number(i)] || "");
  return out;
}

/** Ensure raw bible-ref spans (from backend) have resolved book attrs where missing. */
function normalizeBibleRefElements(root) {
  if (!root) return;
  root.querySelectorAll(".bible-ref-raw[data-raw]").forEach((el) => {
    const raw = el.dataset.raw || el.textContent || "";
    const m = raw.match(
      /^\s*((?:[123]\s*)?[A-Za-zÁÉÍÓÚáéíóúñÑ\.]+)\s+(\d{1,3})\s*[:：\.]\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?\s*$/i
    );
    if (m) {
      el.dataset.book = m[1].replace(/\.$/, "");
      el.dataset.chapter = m[2];
      el.dataset.verse = m[3];
      el.dataset.verseEnd = m[4] || m[3];
      el.classList.remove("bible-ref-raw");
    }
  });
}

function selectedBibleMeta() {
  const m = state.modules.find((x) => x.path === state.biblePath);
  return {
    path: state.biblePath,
    abbr: m?.abbreviation || "Biblia",
    title: m?.title || "",
  };
}

/** Load passage text from the Bible currently selected in the sidebar. */
async function fetchPassage(bookNum, chapter, verseStart, verseEnd) {
  if (!state.biblePath) throw new Error("No hay Biblia seleccionada en el panel izquierdo.");
  return invoke("get_passage", {
    modulePath: state.biblePath,
    book: bookNum,
    chapter,
    verseStart,
    verseEnd: verseEnd || verseStart,
  });
}

function formatPassageHtml(passage, bookNum) {
  const bible = selectedBibleMeta();
  const v0 = passage.verseStart;
  const v1 = passage.verseEnd;
  const range = v0 === v1 ? String(v0) : `${v0}–${v1}`;
  const ref = `${bookName(bookNum)} ${passage.chapter}:${range}`;
  const verses = (passage.verses || [])
    .map(
      (v) =>
        `<p class="passage-verse"><sup class="passage-vnum">${v.verse}</sup> ${escapeHtml(v.plain)}</p>`
    )
    .join("");
  return {
    ref,
    bibleLabel: passage.abbreviation || bible.abbr,
    bibleTitle: passage.title || bible.title,
    html: `
      <div class="passage-head">
        <div class="passage-ref">${escapeHtml(ref)}</div>
        <div class="passage-bible" title="${escapeAttr(passage.title || bible.title)}">${escapeHtml(
      passage.abbreviation || bible.abbr
    )}</div>
      </div>
      <div class="passage-body">${verses || "<p class='empty-state soft'>Sin texto en este módulo.</p>"}</div>
    `,
  };
}

// —— Interconnect word ——
async function interconnectWord(word, { openDict = false, stay = false } = {}) {
  const w = (word || "").trim().replace(/^[\s.,;:!?¡¿«»"'()]+|[\s.,;:!?¡¿«»"'()]+$/g, "");
  if (!w) return;

  const seq = ++state.interconnectSeq;
  state.selectedWord = w;
  state.wordLinks = null;
  // If user clicked a Spanish/Greek/Hebrew word (not H/G code), clear stale Strong until resolved
  if (!/^[HGhg]\d+$/.test(w)) {
    state.lastStrong = "";
  } else {
    state.lastStrong = w.toUpperCase();
  }

  // Critical: search field must follow the NEW word (was sticky on previous query)
  const dq = $("#dict-query");
  if (dq) dq.value = w;

  updateContextBar();
  $("#sel-info").textContent = `Palabra: ${w}`;
  await refreshAnalysisPane();

  const dictPaths = modulesOf("dictionary")
    .filter((m) => !m.encrypted)
    .map((m) => m.path);
  const lexPaths = modulesOf("lexicon")
    .filter((m) => !m.encrypted)
    .map((m) => m.path);

  try {
    const result = await invoke("resolve_word", {
      word: w,
      dictPaths,
      lexPaths,
      interlinearPath: state.interlinearPath || null,
      book: state.book,
      chapter: state.chapter,
      verse: state.verse,
    });
    // Drop stale responses (user already clicked another word)
    if (seq !== state.interconnectSeq) return;

    state.wordLinks = result;
    if (result.strongs?.length) {
      state.lastStrong = result.strongs[0];
    }
    // Keep search box on the word the user actually clicked (not a different Strong topic)
    if (dq && state.workspace === "diccionario") dq.value = w;
    if (dq && state.workspace === "lexico" && state.lastStrong) {
      // Prefer Strong in lexicon tab when available
      dq.value = state.lastStrong;
    }

    updateContextBar();
    await refreshAnalysisPane();

    if (openDict) {
      setWorkspace("diccionario");
    } else if (!stay && state.workspace !== "biblia") {
      await loadStudyView();
    } else if (state.workspace === "diccionario" || state.workspace === "lexico") {
      await loadStudyView();
    }
    persistStudy();
  } catch (e) {
    if (seq !== state.interconnectSeq) return;
    console.error(e);
    $("#link-panel-inline").innerHTML = `<div class="empty-state">${escapeHtml(String(e))}</div>`;
  }
}

function clearWordSelection() {
  state.interconnectSeq++;
  state.selectedWord = null;
  state.wordLinks = null;
  state.lastStrong = "";
  const dq = $("#dict-query");
  if (dq) dq.value = "";
  $("#sel-info").textContent = "";
  $$(".word.word-selected, .word-like.word-selected").forEach((el) =>
    el.classList.remove("word-selected")
  );
  updateContextBar();
  refreshAnalysisPane();
  persistStudy();
  if (state.workspace === "diccionario" || state.workspace === "lexico") {
    loadStudyView();
  }
}

// —— Study views ——
async function loadStudyView() {
  const content = $("#study-content");
  fillStudyModuleSelect();

  if (!state.studyModule) {
    content.innerHTML = `<div class="empty-state soft">No hay módulos de este tipo en la carpeta.</div>`;
    return;
  }

  content.innerHTML = `<div class="empty-state soft">Cargando…</div>`;

  try {
    if (state.workspace === "comentario") {
      const entries = await invoke("get_commentary", {
        modulePath: state.studyModule,
        book: state.book,
        chapter: state.chapter,
        verse: state.verse,
        filters: state.filters,
      });
      if (!entries.length) {
        content.innerHTML = `<div class="empty-state soft">Sin comentario para ${bookAbbr(state.book)} ${state.chapter}:${state.verse} con los filtros actuales.</div>`;
        return;
      }
      content.innerHTML =
        `<div class="study-banner">Comentario · ${escapeHtml(bookName(state.book))} ${state.chapter}:${state.verse}</div>` +
        entries
          .map(
            (e) => `
        <article class="study-entry">
          <div class="scope-label">${escapeHtml(e.label)} · ${escapeHtml(e.scope)}</div>
          <div class="study-body">${linkifyHtml(e.html, { book: state.book, chapter: state.chapter })}</div>
        </article>`
          )
          .join("");
      normalizeBibleRefElements(content);
    } else if (state.workspace === "diccionario") {
      await renderDictionaryView(content);
    } else if (state.workspace === "lexico") {
      await renderLexiconView(content);
    }
  } catch (e) {
    content.innerHTML = `<div class="empty-state">${escapeHtml(String(e))}</div>`;
  }
}

async function renderDictionaryView(content) {
  // Prefer live selection over stale input
  const q = (state.selectedWord || $("#dict-query").value || "").trim();
  if (!q) {
    content.innerHTML = `<div class="empty-state soft">Escriba una palabra en español (p. ej. Jehová, Jerusalén) o haga clic en una palabra de la Biblia.<br/>El panel derecho mostrará <strong>otros</strong> diccionarios.</div>`;
    return;
  }
  $("#dict-query").value = q;

  if (!state.wordLinks || normalizeLoose(state.wordLinks.word) !== normalizeLoose(q)) {
    await interconnectWord(q, { stay: true });
  }

  const wl = state.wordLinks;
  const abbr =
    state.modules.find((m) => m.path === state.studyModule)?.abbreviation || "";
  let html = `<div class="study-banner">Diccionario · ${escapeHtml(abbr)} · «${escapeHtml(q)}»</div>`;

  if (wl?.strongs?.length) {
    html += `<div class="sync-bar">Strong: ${wl.strongs
      .map((s) => `<button type="button" class="chip-strong" data-strong="${escapeAttr(s)}">${escapeHtml(s)}</button>`)
      .join(" ")}
      <button type="button" class="glass-btn compact" id="btn-jump-lex-from-dict">Ir a Léxico</button>
      </div>`;
  }

  // PRIMARY module only (others go to the side panel)
  let entries = [];
  try {
    entries = await invoke("lookup_dictionary", {
      modulePath: state.studyModule,
      query: q,
      limit: 40,
    });
  } catch {
    entries = [];
  }

  if (!entries.length) {
    html += `<div class="empty-state soft">Sin resultados en <strong>${escapeHtml(
      abbr
    )}</strong> para «${escapeHtml(q)}». Cambie el módulo arriba o revise el panel derecho (otros diccionarios).</div>`;
  } else {
    html += entries
      .map(
        (e) => `
      <article class="study-entry">
        <div class="topic">${escapeHtml(e.topic)}</div>
        <div class="study-body">${linkifyHtml(e.html, { book: state.book, chapter: state.chapter })}</div>
      </article>`
      )
      .join("");
  }

  content.innerHTML = html;
  normalizeBibleRefElements(content);
  const jump = $("#btn-jump-lex-from-dict");
  if (jump) jump.onclick = () => setWorkspace("lexico");
  await refreshAnalysisPane();
}

/**
 * Lexicon study card: original (H/G) LEFT · Spanish RIGHT · Strong below.
 * Mirrors iOS LexiconView spanishWordCard.
 */
function buildLexiconInterlinearCard(wl) {
  if (!wl) return "";
  const hit = wl.matchedHit || null;
  const spanish =
    (hit && hit.spanish) ||
    (wl.word && !/^[HG]\d+$/i.test(wl.word.trim()) ? wl.word : "") ||
    "";
  let original =
    (hit && hit.original) ||
    (wl.hebrew && wl.hebrew[0]) ||
    (wl.greek && wl.greek[0]) ||
    "";
  const translit = (hit && hit.translit) || "";
  const strong =
    (hit && hit.strong) ||
    (wl.strongs && wl.strongs[0]) ||
    (/^[HG]\d+$/i.test(String(wl.word || "").trim()) ? String(wl.word).toUpperCase() : "");
  const script =
    (hit && hit.script) ||
    (original && /[\u0590-\u05FF]/.test(original)
      ? "hebrew"
      : original && /[\u0370-\u03FF\u1F00-\u1FFF]/.test(original)
        ? "greek"
        : "");
  const originalLabel =
    script === "hebrew"
      ? "PALABRA EN HEBREO"
      : script === "greek"
        ? "PALABRA EN GRIEGO"
        : original
          ? "ORIGINAL"
          : "ORIGINAL";
  const loc = `${bookName(state.book)} ${state.chapter}:${state.verse}`;

  // Related Spanish glosses for same Strong in this verse
  const relatedEs = (wl.interlinearHits || [])
    .filter((h) => h.strong && strong && h.strong.toUpperCase() === strong.toUpperCase())
    .map((h) => h.spanish)
    .filter((s, i, a) => s && a.indexOf(s) === i);

  // Other originals in the verse (chips)
  const otherOrig = (wl.interlinearHits || [])
    .map((h) => h.original)
    .filter((o) => o && o !== original)
    .filter((s, i, a) => a.indexOf(s) === i)
    .slice(0, 12);

  return `
  <article class="lex-card">
    <div class="lex-card-loc"><span class="lex-pin">📍</span> ${escapeHtml(loc)}
      ${wl.interlinearUsed ? `<span class="lex-badge">Interlineal</span>` : ""}
    </div>
    <div class="lex-card-pair">
      <div class="lex-col">
        <div class="lex-col-label">${escapeHtml(originalLabel)}</div>
        <div class="lex-original ${script === "hebrew" ? "dir-rtl" : ""}" ${
          strong ? `data-strong="${escapeAttr(strong)}" role="button" tabindex="0"` : ""
        }>${original ? escapeHtml(original) : "—"}</div>
        ${
          translit
            ? `<div class="lex-translit" title="Pronunciación">${escapeHtml(translit)}</div>`
            : ""
        }
      </div>
      <div class="lex-col">
        <div class="lex-col-label">PALABRA EN ESPAÑOL</div>
        <div class="lex-spanish">${spanish ? escapeHtml(spanish) : "—"}</div>
      </div>
    </div>
    ${
      strong
        ? `<div class="lex-strong-row">
            <div class="lex-col-label">STRONG</div>
            <button type="button" class="chip-strong lex-strong-btn" data-strong="${escapeAttr(
              strong
            )}">${escapeHtml(strong)}</button>
          </div>`
        : ""
    }
    ${
      otherOrig.length
        ? `<div class="lex-chips-block">
            <div class="lex-col-label">HEBREO / GRIEGO EN EL VERSÍCULO</div>
            <div class="lex-chips">${otherOrig
              .map(
                (o) =>
                  `<button type="button" class="lex-chip original-chip word-like" data-lookup="${escapeAttr(
                    o
                  )}">${escapeHtml(o)}</button>`
              )
              .join("")}</div>
          </div>`
        : ""
    }
    ${
      relatedEs.length > 1
        ? `<div class="lex-chips-block">
            <div class="lex-col-label">MISMO STRONG EN EL VERSÍCULO</div>
            <div class="lex-chips">${relatedEs
              .map((s) => `<span class="lex-chip">${escapeHtml(s)}</span>`)
              .join("")}</div>
          </div>`
        : ""
    }
  </article>`;
}

async function renderLexiconView(content) {
  // Prefer Strong from latest interconnect, then selection, then box
  let q = (state.lastStrong || state.selectedWord || $("#dict-query").value || "").trim();
  if (!q) {
    content.innerHTML = `<div class="empty-state soft">Toque una palabra en la Biblia (RV1960). Espada la enlaza al Strong (H… / G…) vía el interlineal y muestra <strong>hebreo/griego + español</strong> aquí.</div>`;
    return;
  }
  $("#dict-query").value = q;

  if (
    !state.wordLinks ||
    (normalizeLoose(state.wordLinks.word) !== normalizeLoose(q) &&
      !(state.wordLinks.strongs || []).some((s) => normalizeLoose(s) === normalizeLoose(q)))
  ) {
    await interconnectWord(q, { stay: true });
  }
  const wl = state.wordLinks;
  const abbr =
    state.modules.find((m) => m.path === state.studyModule)?.abbreviation || "";

  let html = `<div class="study-banner">Léxico · ${escapeHtml(abbr)}</div>`;

  // ★ Hebrew/Greek LEFT · Spanish RIGHT card
  html += buildLexiconInterlinearCard(wl);

  if (wl?.strongs?.length) {
    html += `<div class="sync-bar">Strong: ${wl.strongs
      .map(
        (s) =>
          `<button type="button" class="chip-strong" data-strong="${escapeAttr(s)}">${escapeHtml(
            s
          )}</button>`
      )
      .join(" ")}</div>`;
  }

  // PRIMARY module definition(s)
  const queries = [];
  if (wl?.strongs?.length) queries.push(...wl.strongs);
  if (!queries.includes(q)) queries.push(q);
  const seen = new Set();
  const blocks = [];

  for (const code of queries) {
    try {
      const entry = await invoke("lookup_lexicon", {
        modulePath: state.studyModule,
        strong: code,
      });
      if (entry) {
        const key = entry.topic;
        if (!seen.has(key)) {
          seen.add(key);
          blocks.push(entry);
        }
      }
    } catch {
      /* skip */
    }
  }

  if (!blocks.length) {
    html += `<div class="empty-state soft">No se encontró definición en <strong>${escapeHtml(
      abbr
    )}</strong>. Pruebe otro módulo arriba o mire el panel derecho (otros léxicos).</div>`;
  } else {
    html += `<div class="study-banner sub">Definición · ${escapeHtml(abbr)}</div>`;
    html += blocks
      .map(
        (e) => `
      <article class="study-entry">
        <div class="topic">${escapeHtml(e.topic)}</div>
        <div class="study-body">${linkifyHtml(e.html, { book: state.book, chapter: state.chapter })}</div>
      </article>`
      )
      .join("");
  }

  content.innerHTML = html;
  normalizeBibleRefElements(content);

  // Wire chips that look up original-language words
  content.querySelectorAll(".original-chip[data-lookup]").forEach((el) => {
    el.addEventListener("click", () => {
      const w = el.getAttribute("data-lookup");
      if (w) interconnectWord(w, { stay: true });
    });
  });

  await refreshAnalysisPane();
}

function normalizeLoose(s) {
  return String(s || "")
    .toLowerCase()
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .replace(/[^a-z0-9\u0590-\u05ff\u0370-\u03ff]/gi, "");
}

// —— Tooltips / passage preview (uses sidebar Bible selection) ——
async function resolveRefElement(el) {
  let bookRaw = el.dataset.book;
  let ch = Number(el.dataset.chapter);
  let v1 = Number(el.dataset.verse);
  let v2 = Number(el.dataset.verseEnd || el.dataset.verse || v1);

  // Fallback: parse data-raw or text content
  if ((!bookRaw || !ch || !v1) && (el.dataset.raw || el.textContent)) {
    const raw = el.dataset.raw || el.textContent;
    const m = String(raw).match(
      /^\s*((?:[123]\s*)?[A-Za-zÁÉÍÓÚáéíóúñÑ\.]+)\s+(\d{1,3})\s*[:：\.]\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?/i
    );
    if (m) {
      bookRaw = m[1].replace(/\.$/, "");
      ch = Number(m[2]);
      v1 = Number(m[3]);
      v2 = Number(m[4] || m[3]);
    } else {
      // "v. 5" already has book/chapter from linkify
      const mv = String(raw).match(/v+\.?\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?/i);
      if (mv && bookRaw) {
        v1 = Number(mv[1]);
        v2 = Number(mv[2] || mv[1]);
      }
    }
  }

  let bookNum = null;
  if (bookRaw) {
    try {
      bookNum = await invoke("resolve_book_ref", { raw: bookRaw });
    } catch {
      bookNum = null;
    }
  }
  // If book name is already Spanish full name from context
  if (!bookNum && bookRaw) {
    const found = state.books.find(
      (b) => normalizeLoose(b.name) === normalizeLoose(bookRaw) || normalizeLoose(b.abbr) === normalizeLoose(bookRaw)
    );
    if (found) bookNum = found.number;
  }
  return { bookNum, ch, v1, v2 };
}

async function tooltipForTarget(el) {
  if (el.classList.contains("bible-ref")) {
    if (!state.biblePath) {
      return `<div class="tip-title">${escapeHtml(el.textContent)}</div><div class="tip-muted">Seleccione una Biblia a la izquierda.</div>`;
    }
    const { bookNum, ch, v1, v2 } = await resolveRefElement(el);
    if (!bookNum) {
      return `<div class="tip-title">${escapeHtml(el.textContent)}</div><div class="tip-muted">Referencia no reconocida</div>`;
    }
    try {
      const passage = await fetchPassage(bookNum, ch, v1, v2);
      const fmt = formatPassageHtml(passage, bookNum);
      return `<div class="tip-passage">${fmt.html}<div class="tip-hint">Clic para leer el pasaje completo · ${escapeHtml(
        fmt.bibleLabel
      )}</div></div>`;
    } catch (e) {
      return `<div class="tip-title">${escapeHtml(el.textContent)}</div><div class="tip-muted">${escapeHtml(
        String(e)
      )}</div>`;
    }
  }
  if (el.classList.contains("strong-ref") || el.classList.contains("chip-strong")) {
    const code = (el.dataset.strong || el.textContent || "").toUpperCase();
    const lexPaths = modulesOf("lexicon")
      .filter((m) => !m.encrypted)
      .map((m) => m.path);
    try {
      const entry = await invoke("lookup_lexicon_multi", { modulePaths: lexPaths, strong: code });
      if (!entry) return `<div class="tip-title">${escapeHtml(code)}</div><div>No encontrado</div>`;
      return `<div class="tip-title">${escapeHtml(entry.topic)}</div><div>${escapeHtml((entry.plain || "").slice(0, 280))}</div>`;
    } catch (e) {
      return `<div class="tip-title">${escapeHtml(code)}</div>`;
    }
  }
  return null;
}

async function openLinkPanel(el) {
  const title = $("#link-panel-title");
  const body = $("#link-panel-body");
  const btnGo = $("#btn-goto-passage");
  const btnLex = $("#btn-open-lexicon");
  btnGo.classList.add("hidden");
  btnLex.classList.add("hidden");
  state.linkTarget = null;

  if (el.classList.contains("bible-ref")) {
    const { bookNum, ch, v1, v2 } = await resolveRefElement(el);
    title.textContent = el.textContent;
    if (!state.biblePath) {
      body.innerHTML = `<p>Seleccione una Biblia en el panel izquierdo para previsualizar el pasaje.</p>`;
      openPanel("#panel-link");
      return;
    }
    if (bookNum) {
      try {
        const passage = await fetchPassage(bookNum, ch, v1, v2);
        const fmt = formatPassageHtml(passage, bookNum);
        body.innerHTML = `${fmt.html}<p class="tip-muted" style="margin-top:10px">Texto de: <strong>${escapeHtml(
          fmt.bibleLabel
        )}</strong>${fmt.bibleTitle ? ` — ${escapeHtml(fmt.bibleTitle)}` : ""}</p>`;
      } catch (e) {
        body.innerHTML = `<p>${escapeHtml(String(e))}</p>`;
      }
      state.linkTarget = { type: "ref", book: bookNum, chapter: ch, verse: v1 };
      btnGo.classList.remove("hidden");
      btnGo.textContent = "Ir a pasaje en Biblia";
    } else {
      body.innerHTML = `<p>No se pudo resolver la referencia «${escapeHtml(el.textContent)}».</p>`;
    }
    openPanel("#panel-link");
    return;
  }

  if (el.classList.contains("strong-ref") || el.classList.contains("chip-strong")) {
    const code = (el.dataset.strong || el.textContent || "").toUpperCase();
    title.textContent = code;
    state.lastStrong = code;
    await interconnectWord(code, { stay: true });
    state.linkTarget = { type: "strong", code };
    btnLex.classList.remove("hidden");
    const wl = state.wordLinks;
    if (wl?.lexicon?.[0]) {
      body.innerHTML = `<p><strong>${escapeHtml(wl.lexicon[0].topic)}</strong></p><div>${linkifyHtml(wl.lexicon[0].html)}</div>`;
    } else body.innerHTML = `<p>Abra la pestaña Léxico para el detalle completo.</p>`;
    openPanel("#panel-link");
  }
}

// —— Context menu / highlights ——
function showCtx(x, y) {
  const menu = $("#ctx-menu");
  menu.classList.remove("hidden");
  const r = menu.getBoundingClientRect();
  let left = x;
  let top = y;
  if (left + r.width > window.innerWidth - 8) left = window.innerWidth - r.width - 8;
  if (top + r.height > window.innerHeight - 8) top = window.innerHeight - r.height - 8;
  menu.style.left = `${left}px`;
  menu.style.top = `${top}px`;
}
function selectedVerseNums() {
  if (state.selectedVerses.size) return [...state.selectedVerses];
  return [state.verse];
}
function getCopyText() {
  const data = state.chapterData;
  if (!data?.verses) return window.getSelection()?.toString() || "";
  const nums = selectedVerseNums().sort((a, b) => a - b);
  const lines = nums.map((n) => {
    const v = data.verses.find((x) => x.verse === n);
    return v ? `${n} ${v.plain}` : "";
  });
  return `${lines.filter(Boolean).join("\n")}\n— ${bookName(state.book)} ${state.chapter}:${nums.join(",")}`;
}
async function actCopy() {
  try {
    await navigator.clipboard.writeText(getCopyText());
  } catch {
    /* ignore */
  }
  hideCtx();
}
async function actShare() {
  const text = getCopyText();
  if (navigator.share) {
    try {
      await navigator.share({ text, title: "Espada3.7" });
    } catch {
      /* cancel */
    }
  } else await actCopy();
  hideCtx();
}
function setHighlight(color) {
  for (const v of selectedVerseNums()) {
    const key = hlKey(state.book, state.chapter, v);
    const cur = state.highlights[key] || {};
    cur.color = color;
    state.highlights[key] = cur;
    const el = document.querySelector(`.verse[data-verse="${v}"]`);
    if (el) applyMarksToVerseEl(el, state.book, state.chapter, v);
  }
  persistHighlights();
  hideCtx();
}
function setUnderline() {
  for (const v of selectedVerseNums()) {
    const key = hlKey(state.book, state.chapter, v);
    const cur = state.highlights[key] || {};
    cur.underline = !cur.underline;
    state.highlights[key] = cur;
    const el = document.querySelector(`.verse[data-verse="${v}"]`);
    if (el) applyMarksToVerseEl(el, state.book, state.chapter, v);
  }
  persistHighlights();
  hideCtx();
}
function clearMarks() {
  for (const v of selectedVerseNums()) {
    delete state.highlights[hlKey(state.book, state.chapter, v)];
    const el = document.querySelector(`.verse[data-verse="${v}"]`);
    if (el) applyMarksToVerseEl(el, state.book, state.chapter, v);
  }
  persistHighlights();
  hideCtx();
}

async function openVersePicker() {
  let count = 30;
  if (state.biblePath) {
    try {
      count = await invoke("get_verse_count", {
        modulePath: state.biblePath,
        book: state.book,
        chapter: state.chapter,
      });
    } catch {
      count = state.chapterData?.verses?.length || 30;
    }
  }
  if (!count || count < 1) count = 30;
  $("#verse-grid").innerHTML = Array.from({ length: count }, (_, i) => {
    const v = i + 1;
    return `<button type="button" data-v="${v}" class="${v === state.verse ? "active" : ""}">${v}</button>`;
  }).join("");
  openPanel("#panel-verse-picker");
}
function openHistory() {
  const list = $("#history-list");
  if (!state.history.length) {
    list.innerHTML = `<div class="empty-state soft">Aún no hay lugares recientes.</div>`;
  } else {
    list.innerHTML = state.history
      .map(
        (h, i) => `
      <button type="button" class="history-item" data-idx="${i}">
        <span class="h-title">${escapeHtml(h.label)}</span>
        <span class="h-meta">${new Date(h.ts).toLocaleString()}</span>
      </button>`
      )
      .join("");
  }
  openPanel("#panel-history");
}

function updateHistoryButtonTooltip() {
  const btn = $("#btn-last-visited");
  if (!btn) return;
  if (state.history.length > 1) {
    const prev = state.history[1];
    btn.title = `Último visitado: ${prev.label} (Volver al pasaje anterior)`;
  } else {
    btn.title = "Último versículo visitado (Volver al pasaje anterior)";
  }
}

// —— Events ——
function bindEvents() {
  // Workspace tabs
  $$(".ws-tab").forEach((tab) => {
    tab.onclick = () => setWorkspace(tab.dataset.ws);
  });

  $("#sel-book").onchange = async (e) => {
    state.book = Number(e.target.value);
    state.chapter = 1;
    state.verse = 1;
    fillChapters();
    pushHistory();
    persistNav();
    await reloadChapter();
    await refreshWorkspace();
  };
  $("#sel-chapter").onchange = async (e) => {
    state.chapter = Number(e.target.value);
    state.verse = 1;
    pushHistory();
    persistNav();
    await reloadChapter();
    await refreshWorkspace();
  };
  
  const goPrev = async () => {
    if (state.chapter > 1) await goTo(state.book, state.chapter - 1, 1);
    else if (state.book > 1) await goTo(state.book - 1, bookChapters(state.book - 1), 1);
  };
  const goNext = async () => {
    const max = bookChapters(state.book);
    if (state.chapter < max) await goTo(state.book, state.chapter + 1, 1);
    else if (state.book < 66) await goTo(state.book + 1, 1, 1);
  };

  if ($("#btn-prev-ch")) $("#btn-prev-ch").onclick = goPrev;
  if ($("#btn-next-ch")) $("#btn-next-ch").onclick = goNext;
  if ($("#btn-prev-passage")) $("#btn-prev-passage").onclick = goPrev;
  if ($("#btn-next-passage")) $("#btn-next-passage").onclick = goNext;
  if ($("#btn-location-chip")) $("#btn-location-chip").onclick = () => openVersePicker();

  if ($("#btn-last-visited")) {
    $("#btn-last-visited").onclick = async () => {
      if (state.history.length > 1) {
        const prev = state.history[1];
        await goTo(prev.book, prev.chapter, prev.verse);
      } else if (state.history.length === 1) {
        const prev = state.history[0];
        await goTo(prev.book, prev.chapter, prev.verse);
      } else {
        openHistory();
      }
    };
  }

  $("#btn-verse-pick").onclick = () => openVersePicker();
  $("#btn-history").onclick = () => openHistory();
  $("#btn-settings").onclick = () => openPanel("#panel-settings");
  const btnCross = $("#btn-crossrefs");
  if (btnCross) {
    btnCross.onclick = () => openCrossReferences();
  }
  const selCr = $("#sel-crossref-source");
  if (selCr) {
    selCr.onchange = async () => {
      state.crossRefSourcePath = selCr.value || "";
      persistStudy();
      await reloadCrossReferences();
    };
  }
  const btnTheme = $("#btn-theme");
  if (btnTheme) {
    btnTheme.onclick = () => {
      renderThemeGrid();
      openPanel("#panel-theme");
    };
  }

  // Window drag fallback (titlebar). Needs core:window:allow-start-dragging.
  // data-tauri-drag-region is primary; this helps if the attribute is ignored.
  const titlebar = document.querySelector(".titlebar");
  if (titlebar) {
    titlebar.addEventListener("mousedown", async (e) => {
      if (e.button !== 0) return;
      if (e.target.closest("button, a, input, select, textarea, [data-no-drag]")) return;
      try {
        const tw = window.__TAURI__?.webviewWindow?.getCurrentWebviewWindow?.();
        if (tw?.startDragging) await tw.startDragging();
      } catch (err) {
        console.warn("startDragging:", err);
      }
    });
  }

  $("#sel-bible").onchange = async (e) => {
    state.biblePath = e.target.value;
    updateBibleTitleTooltip();
    persistNav();
    await reloadChapter();
  };
  $("#sel-parallel1").onchange = (e) => {
    state.parallel1 = e.target.value;
    persistParallel();
    if (state.mode === "paralela") reloadChapter();
  };
  $("#sel-parallel2").onchange = (e) => {
    state.parallel2 = e.target.value;
    persistParallel();
    if (state.mode === "paralela") reloadChapter();
  };
  $$(".mode-btn").forEach((btn) => {
    btn.onclick = async () => {
      $$(".mode-btn").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      state.mode = btn.dataset.mode;
      $("#parallel-controls").classList.toggle("hidden", state.mode !== "paralela");
      persistParallel();
      await reloadChapter();
    };
  });

  $("#sel-study-module").onchange = async (e) => {
    state.studyModule = e.target.value;
    state.modulesByKind[state.workspace] = e.target.value;
    persistStudy();
    await loadStudyView();
  };
  $("#flt-verse").onchange = async (e) => {
    state.filters.verse = e.target.checked;
    persistStudy();
    await loadStudyView();
  };
  $("#flt-chapter").onchange = async (e) => {
    state.filters.chapter = e.target.checked;
    persistStudy();
    await loadStudyView();
  };
  $("#flt-book").onchange = async (e) => {
    state.filters.book = e.target.checked;
    persistStudy();
    await loadStudyView();
  };
  $("#btn-dict-search").onclick = async () => {
    const q = $("#dict-query").value.trim();
    if (q) await interconnectWord(q, { stay: true });
    await loadStudyView();
  };
  $("#dict-query").onkeydown = (e) => {
    if (e.key === "Enter") $("#btn-dict-search").click();
  };

  $("#notes-side").oninput = (e) => {
    state.notes[noteKey()] = e.target.value;
    persistNotes();
  };

  $("#btn-clear-word").onclick = () => clearWordSelection();

  // Strong chips / mini entries (side analysis → promote module to primary)
  document.addEventListener("click", async (e) => {
    const chip = e.target.closest(".chip-strong");
    if (chip) {
      e.preventDefault();
      const code = chip.dataset.strong;
      await interconnectWord(code, { stay: true });
      setWorkspace("lexico");
      return;
    }
    const mini = e.target.closest(".mini-entry");
    if (mini) {
      if (mini.dataset.kind === "dict") {
        state.modulesByKind.diccionario = mini.dataset.path;
        state.studyModule = mini.dataset.path;
        if (mini.dataset.topic) {
          state.selectedWord = mini.dataset.topic;
          $("#dict-query").value = mini.dataset.topic;
        }
        setWorkspace("diccionario");
      } else if (mini.dataset.kind === "lex") {
        state.modulesByKind.lexico = mini.dataset.path;
        state.studyModule = mini.dataset.path;
        if (mini.dataset.topic) {
          $("#dict-query").value = mini.dataset.topic;
          if (/^[HG]\d+$/i.test(mini.dataset.topic)) state.lastStrong = mini.dataset.topic.toUpperCase();
        }
        setWorkspace("lexico");
      } else if (mini.dataset.kind === "cmt") {
        state.modulesByKind.comentario = mini.dataset.path;
        state.studyModule = mini.dataset.path;
        setWorkspace("comentario");
      }
    }
  });

  // Settings path
  async function applyModulesPath(raw) {
    const path = (raw || "").trim();
    if (!path) return alertSoft("Carpeta", "Indique una ruta de carpeta.");
    try {
      const dir = await invoke("set_modules_path", { path });
      await loadModules(dir);
      closePanels();
    } catch (e) {
      alertSoft("Carpeta", String(e));
    }
  }
  $("#btn-pick-modules").onclick = async () => {
    try {
      const dir = await invoke("pick_modules_dir");
      if (dir) {
        await loadModules(dir);
        closePanels();
      }
    } catch (e) {
      alertSoft("Diálogo de carpeta", `${String(e)}\n\nPegue la ruta y use «Usar esta ruta».`);
    }
  };
  $("#btn-apply-path").onclick = () => applyModulesPath($("#modules-path").value);
  $("#btn-use-default-espada").onclick = () =>
    applyModulesPath("/Users/amed301/Downloads/for mac espada");
  $("#btn-refresh-modules").onclick = async () => {
    if (!state.modulesDir) return;
    try {
      await invoke("refresh_modules", { modulesDir: state.modulesDir });
      await loadModules(state.modulesDir);
    } catch (e) {
      alertSoft("Módulos", String(e));
    }
  };

  $("#overlay").onclick = closePanels;
  $$("[data-close]").forEach((b) => (b.onclick = closePanels));

  $("#verse-grid").onclick = async (e) => {
    const btn = e.target.closest("button[data-v]");
    if (!btn) return;
    state.verse = Number(btn.dataset.v);
    state.selectedVerses = new Set([state.verse]);
    persistNav();
    closePanels();
    pushHistory();
    updateLocationLabel();
    await reloadChapter();
    await refreshWorkspace();
  };
  $("#history-list").onclick = async (e) => {
    const item = e.target.closest(".history-item");
    if (!item) return;
    const h = state.history[Number(item.dataset.idx)];
    if (!h) return;
    closePanels();
    if (h.biblePath) {
      state.biblePath = h.biblePath;
      $("#sel-bible").value = h.biblePath;
    }
    await goTo(h.book, h.chapter, h.verse, { history: false });
  };

  $("#btn-goto-passage").onclick = async () => {
    const t = state.linkTarget;
    if (t?.type === "ref") {
      closePanels();
      hideTooltip();
      setWorkspace("biblia");
      await goTo(t.book, t.chapter, t.verse);
    }
  };
  $("#btn-open-lexicon").onclick = async () => {
    const t = state.linkTarget;
    if (t?.type === "strong") {
      closePanels();
      $("#dict-query").value = t.code;
      setWorkspace("lexico");
    }
  };

  // Bible interactions
  const bible = $("#bible-content");
  bible.onclick = async (e) => {
    const strong = e.target.closest(".strong-ref");
    if (strong) {
      e.preventDefault();
      await openLinkPanel(strong);
      return;
    }
    const ref = e.target.closest(".bible-ref");
    if (ref) {
      e.preventDefault();
      await openLinkPanel(ref);
      return;
    }
    const num = e.target.closest(".verse-num, td.vnum");
    if (num) {
      const v = Number(num.dataset.verse);
      state.verse = v;
      state.selectedVerses = e.shiftKey
        ? new Set([...state.selectedVerses, v])
        : new Set([v]);
      $$(".verse", bible).forEach((el) => {
        el.classList.toggle("selected", state.selectedVerses.has(Number(el.dataset.verse)));
      });
      persistNav();
      updateLocationLabel();
      if (state.workspace === "comentario") {
        await loadStudyView();
        await refreshAnalysisPane();
      }
      return;
    }
    const word = e.target.closest(".word, .word-like");
    if (word) {
      $$(".word.word-selected, .word-like.word-selected").forEach((w) =>
        w.classList.remove("word-selected")
      );
      word.classList.add("word-selected");
      const text = (word.dataset.lookup || word.textContent || "").trim();
      const verseEl = word.closest(".verse, tr");
      if (verseEl?.dataset.verse) {
        state.verse = Number(verseEl.dataset.verse);
        state.selectedVerses = new Set([state.verse]);
        $$(".verse", bible).forEach((el) => {
          el.classList.toggle("selected", Number(el.dataset.verse) === state.verse);
        });
        persistNav();
        updateLocationLabel();
      }
      // Always treat as a NEW query (clears stale word / search box)
      await interconnectWord(text, { openDict: true });
      return;
    }
    const verse = e.target.closest(".verse");
    if (verse?.dataset.verse) {
      state.verse = Number(verse.dataset.verse);
      state.selectedVerses = new Set([state.verse]);
      $$(".verse", bible).forEach((el) => {
        el.classList.toggle("selected", Number(el.dataset.verse) === state.verse);
      });
      persistNav();
      updateLocationLabel();
      if (state.workspace === "comentario") await loadStudyView();
    }
  };

  bible.oncontextmenu = (e) => {
    const verse = e.target.closest(".verse");
    if (verse?.dataset.verse) {
      const v = Number(verse.dataset.verse);
      if (!state.selectedVerses.has(v)) {
        state.selectedVerses = new Set([v]);
        state.verse = v;
      }
      const word = e.target.closest(".word");
      if (word) state.selectedWord = word.textContent.trim();
    }
    e.preventDefault();
    showCtx(e.clientX, e.clientY);
  };

  // Study content clicks
  $("#study-content").onclick = async (e) => {
    const el = e.target.closest(".bible-ref, .strong-ref, .chip-strong, .word-like");
    if (!el) return;
    e.preventDefault();
    if (el.classList.contains("word-like")) {
      await interconnectWord(el.dataset.lookup || el.textContent, { stay: true });
      await loadStudyView();
      return;
    }
    if (el.classList.contains("chip-strong") || el.classList.contains("strong-ref")) {
      const code = (el.dataset.strong || el.textContent || "").toUpperCase();
      await interconnectWord(code, { stay: true });
      if (state.workspace !== "lexico") setWorkspace("lexico");
      else await loadStudyView();
      return;
    }
    await openLinkPanel(el);
  };

  // Hover tooltips — stable for bible refs (don't hide when moving into tooltip)
  let tooltipAnchor = null;
  document.addEventListener("mouseover", (e) => {
    const el = e.target.closest(".bible-ref, .strong-ref, .chip-strong");
    const onTip = e.target.closest("#tooltip");
    if (onTip) return;
    if (!el) return;
    tooltipAnchor = el;
    clearTimeout(state.tooltipTimer);
    state.tooltipTimer = setTimeout(async () => {
      if (tooltipAnchor !== el) return;
      const html = await tooltipForTarget(el);
      if (html && tooltipAnchor === el) {
        showTooltip(html, e.clientX, e.clientY);
        $("#tooltip").classList.toggle("tip-wide", el.classList.contains("bible-ref"));
      }
    }, 220);
  });
  document.addEventListener("mouseout", (e) => {
    const el = e.target.closest(".bible-ref, .strong-ref, .chip-strong");
    if (!el) return;
    const to = e.relatedTarget;
    if (to && (el.contains(to) || to.closest?.("#tooltip"))) return;
    // Small delay so user can move into the tooltip if needed
    clearTimeout(state.tooltipTimer);
    state.tooltipTimer = setTimeout(() => {
      if (tooltipAnchor === el) {
        tooltipAnchor = null;
        hideTooltip();
      }
    }, 120);
  });
  $("#tooltip")?.addEventListener("mouseleave", () => {
    tooltipAnchor = null;
    hideTooltip();
  });

  $("#ctx-menu").onclick = async (e) => {
    const chip = e.target.closest(".hl-chip");
    if (chip) return setHighlight(chip.dataset.hl);
    const btn = e.target.closest("button[data-act]");
    if (!btn) return;
    const act = btn.dataset.act;
    if (act === "copy") await actCopy();
    else if (act === "share") await actShare();
    else if (act === "underline") setUnderline();
    else if (act === "clear") clearMarks();
    else if (act === "lookup") {
      hideCtx();
      const w = state.selectedWord || window.getSelection()?.toString()?.trim();
      if (w) await interconnectWord(w, { openDict: true });
    }
  };

  document.addEventListener("click", (e) => {
    if (!e.target.closest("#ctx-menu")) hideCtx();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      closePanels();
      hideCtx();
    }
  });
}

// —— Init ——
async function init() {
  applyTheme(localStorage.getItem(LS.theme) || "true-dark");
  // Theme grid is rebuilt when 🎨 opens the picker panel
  renderThemeGrid();

  const nav = loadLS(LS.nav, null);
  if (nav) {
    state.book = nav.book || 43;
    state.chapter = nav.chapter || 3;
    state.verse = nav.verse || 16;
    state.biblePath = nav.biblePath || "";
  }
  const par = loadLS(LS.parallel, null);
  if (par) {
    state.mode = par.mode || "lectura";
    state.parallel1 = par.parallel1 || "";
    state.parallel2 = par.parallel2 || "";
  }
  state.highlights = loadLS(LS.highlights, {});
  state.notes = loadLS(LS.notes, {});
  state.history = loadLS(LS.history, []);
  const study = loadLS(LS.study, null);
  if (study) {
    state.workspace = study.workspace || study.tab || "biblia";
    if (state.workspace === "comentario" || state.workspace === "diccionario" || state.workspace === "lexico" || state.workspace === "biblia") {
      /* ok */
    } else state.workspace = "biblia";
    state.modulesByKind = study.modulesByKind || state.modulesByKind;
    state.filters = study.filters || state.filters;
    state.selectedWord = study.selectedWord || null;
    state.lastStrong = study.lastStrong || "";
    state.crossRefSourcePath = study.crossRefSourcePath || "";
  }

  $("#flt-verse").checked = !!state.filters.verse;
  $("#flt-chapter").checked = !!state.filters.chapter;
  $("#flt-book").checked = !!state.filters.book;
  $$(".mode-btn").forEach((b) => b.classList.toggle("active", b.dataset.mode === state.mode));
  $("#parallel-controls").classList.toggle("hidden", state.mode !== "paralela");

  bindEvents();

  try {
    state.books = await invoke("get_books");
  } catch {
    state.books = Array.from({ length: 66 }, (_, i) => ({
      number: i + 1,
      name: `Libro ${i + 1}`,
      abbr: String(i + 1),
      chapters: 1,
    }));
  }
  fillBooks();
  fillChapters();
  updateLocationLabel();

  let dir = localStorage.getItem(LS.modulesDir) || "";
  const preferred = "/Users/amed301/Downloads/for mac espada";
  try {
    const defaultDir = (await invoke("get_default_modules_dir")) || "";
    if (!dir) dir = defaultDir || preferred;
    else {
      try {
        const test = await invoke("list_modules", { modulesDir: dir });
        if (!test?.length) dir = defaultDir || preferred;
      } catch {
        dir = defaultDir || preferred;
      }
    }
  } catch {
    if (!dir) dir = preferred;
  }

  if (dir) await loadModules(dir);
  else openPanel("#panel-settings");

  setWorkspace(state.workspace === "biblia" ? "biblia" : state.workspace, { skipReload: false });
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => setTimeout(init, 50));
} else {
  setTimeout(init, 50);
}
