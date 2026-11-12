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
  readFont: "espada37.readFont",
};

// Prefer the list from themes.js (loaded before this file; beats stale WebView caches of it)
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

/**
 * Latin reading font for Bible / commentary / dictionary / lexicon text and notes (🎨 panel).
 * "system" = no data-read-font on <html> (the app's normal font). The other ids each have an
 * html[data-read-font="…"] rule and @font-face faces (ui/fonts/) in styles.v7.css.
 * Google Sans Flex → Roboto Flex are EspadaSwift's ReadingFontFamily (same files, same labels).
 * Greek and Hebrew always use Libertinus Serif, whatever is picked here (CSS unicode-range).
 */
const READ_FONTS = [
  { id: "system", label: "Sistema" },
  { id: "googlesans", label: "Google Sans Flex" },
  { id: "roboto", label: "Roboto" },
  { id: "inter", label: "Inter" },
  { id: "robotoflex", label: "Roboto Flex" },
  { id: "opendyslexic", label: "OpenDyslexic" },
  { id: "atkinson", label: "Atkinson" },
  { id: "lexend", label: "Lexend" },
];

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
  /** The user picked that source (an automatic default is not remembered as a choice) */
  crossRefChosen: false,
  /** Sources offered in the panel (backend list + UI-offered TCB) */
  crossRefSources: [],
  /** Content-probe results (Map path→has) keyed by kind + query/verse (LRU, see probeCacheGet) */
  probeCache: new Map(),
  /** Token of the newest probe; late replies of older probes never touch the UI */
  probeSeq: 0,
  /** In-flight probes by key → Promise<Map|null> (shared by the dropdown and the side pane) */
  probeInflight: new Map(),
  /** Token of the newest loadModules (a folder chosen later wins over a slower earlier one) */
  modulesLoadSeq: 0,
  /** Token of the newest 'Otros comentarios' render (ISS-15) */
  paneSeq: 0,
  /** Debounce timer of the side pane while stepping verses (ISS-18) */
  paneTimer: 0,
  /** Word tapped inside the commentary, answered in the side pane (ISS-19) */
  cmtWord: null,
  /** ISS-31: the loaded folder is a session-only fallback (the saved one is unmounted). */
  modulesDirSessionOnly: false,
  cmtWordLabel: "", // the tapped run's own text (λόγος); cmtWord is the folded lookup key (λογοσ)
  cmtWordError: "",
  /** The chosen lexicon's entry for that word's Strong ({topic, html, moduleAbbr}) */
  cmtWordEntry: null,
  /** Collapsed plain text of the main commentary's entries at module|b:c:v (side pane skips copies) */
  primaryCmtPlain: { key: "", texts: new Set() },
  /** Verse the selected word was resolved at ({book,chapter,verse}); persisted for relaunch */
  wordOrigin: null,
  /** The selected word's resolve_word still running ({word, seq, done}); views wait for it */
  wordResolving: null,
  /** One-shot lexicon topic from a side card ({path, topic}) shown first in Léxico */
  lexTopic: null,
  /** Word under the cursor at the last right-click ({text, at: tapContext}) */
  ctxWord: null,
  /** The backend answered with a lemma index field (ISS-43): Greek/Hebrew input works */
  lemmaIndexAvailable: false,
  /** Token of the newest study render; older renders never write #study-content */
  renderSeq: 0,
  /** e-Sword style topic index of the Diccionario view (window of the sorted topic list) */
  dictIndex: null,
  /** Token of the newest index request; older replies never paint */
  dictIndexSeq: 0,
  /** A topic opened from the index ({module, rowid, topic}); null = the word's results */
  dictBrowse: null,
  /** Debounce of the index jump while typing in «Buscar…» */
  dictIndexTimer: 0,
  /** Debounce of opening the topic reached with ↑/↓ (its own timer: typing never cancels it) */
  dictKeyTimer: 0,
  /** Token of the newest chapter load; older loads never write #bible-content */
  chapterSeq: 0,
  /** What #study-content currently shows (same key → inline refresh, no blanking) */
  renderedKey: "",
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

// User decision 3: default source TSKe (one exact list per verse); offer the Spanish
// «Tesoro del Conocimiento Bíblico» (TCB). The default is not saved as if the user chose it.
const isTcbModule = (m) => /tesoro del conocimiento/i.test(m.title || "") || /^tcb$/i.test(String(m.abbreviation || "").trim());
function defaultCrossRefSource(sources) {
  const ok = sources.filter((s) => !s.encrypted);
  const hay = (s) => `${s.title || ""} ${String(s.path || "").split("/").pop()}`;
  return (
    ok.find((s) => /\btske\b/i.test(hay(s))) ||
    ok.find((s) => /\btsk/i.test(hay(s))) ||
    ok.find((s) => /tesoro del conocimiento|\btcb\b/i.test(hay(s))) ||
    ok[0] ||
    sources[0] ||
    null
  );
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
  sources = sources.slice();
  // An older backend does not list TCB: offer it here (read through get_commentary).
  if (!sources.some((s) => modulesOf("commentary").some((m) => m.path === s.path && isTcbModule(m)))) {
    const tcb = modulesOf("commentary").find((m) => isTcbModule(m) && !m.encrypted);
    if (tcb) {
      sources.push({ path: tcb.path, title: `${tcb.abbreviation} — ${tcb.title}`, kind: "commentary", encrypted: false, uiOnly: true });
    }
  }
  state.crossRefSources = sources;
  const chosen =
    state.crossRefChosen && sources.some((s) => s.path === state.crossRefSourcePath) ? state.crossRefSourcePath : "";
  fillSelect(
    sel,
    sources,
    "path",
    (s) => {
      const enc = s.encrypted ? "🔒 " : "";
      return `${enc}${s.title}`;
    },
    chosen || defaultCrossRefSource(sources)?.path || ""
  );
  state.crossRefSourcePath = sel.value || "";
}

/** TCB on an older backend: the verse's own rows from get_commentary (not the chapter summary). */
async function loadUiCrossReferences(source) {
  const entries = await invoke("get_commentary", {
    modulePath: source.path,
    book: state.book,
    chapter: state.chapter,
    verse: state.verse,
    filters: { verse: true, chapter: false, book: false },
  });
  const exact = (entries || []).filter((e) => new RegExp(`^v\\.?\\s*${state.verse}$`, "i").test(String(e.label || "").trim()));
  const rows = exact.length ? exact : entries || [];
  return {
    source: source.title,
    sourcePath: source.path,
    entries: rows.map((e) => ({ label: "", html: linkifyHtml(e.html, { book: state.book, chapter: state.chapter }) })),
    note: rows.length ? null : `Sin referencias para ${bookAbbr(state.book)} ${state.chapter}:${state.verse} en «${source.title}».`,
  };
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
    const uiSource = state.crossRefSources.find((s) => s.uiOnly && s.path === state.crossRefSourcePath);
    if (uiSource) {
      const payload = await loadUiCrossReferences(uiSource);
      if (hint) hint.textContent = `Fuente: ${payload.source}. Clic en una referencia para ir al pasaje.`;
      renderCrossReferences(payload);
      return;
    }
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

  // Raw HTML blocks (TSK commentary blob, TCB rows): their references navigate on click.
  list.onclick = async (ev) => {
    const ref = ev.target.closest(".bible-ref");
    if (!ref) return;
    ev.preventDefault();
    const { bookNum, ch, v1 } = await resolveRefElement(ref);
    if (!bookNum) return;
    closePanels();
    await goTo(bookNum, ch, v1);
  };
  list.innerHTML = entries
    .map(
      (e) =>
        `<div class="crossref-block">${e.label ? `<div class="tip-muted" style="margin-bottom:6px">${escapeHtml(
          e.label
        )}</div>` : ""}${e.html || escapeHtml(e.label || "")}</div>`
    )
    .join("");
  normalizeBibleRefElements(list);
}

function persistStudy() {
  saveLS(LS.study, {
    workspace: state.workspace,
    modulesByKind: state.modulesByKind,
    filters: state.filters,
    selectedWord: state.selectedWord,
    crossRefSourcePath: state.crossRefSourcePath,
    crossRefChosen: state.crossRefChosen,
    lastStrong: state.lastStrong,
    wordOrigin: state.wordOrigin,
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
/**
 * Canonical Strong code ('H430', 'G26') or "" — only a whole H/G code with a value > 0.
 * Rejects words and junk the backend may return as codes: 'H0' (from «hijo»), 'G0', 'H107119'.
 */
function strongCode(s) {
  const m = String(s ?? "").trim().match(/^([HG])\s*0*(\d{1,5})$/i);
  return m && Number(m[2]) > 0 ? m[1].toUpperCase() + String(Number(m[2])) : "";
}

/**
 * A lookup_lexicon reply for a Strong query is kept only if its topic IS that code. The HEAD
 * backend falls back to a LIKE match and can answer G26 with a neighbour (G260) (ISS-04).
 * Non-Strong queries (a word, a DBADV topic) are not filtered. Accents on a topic are ignored
 * (VGNT has «G2624́»).
 */
function lexEntryMatches(entry, query) {
  if (!entry) return false;
  const code = strongCode(query);
  if (!code) return true;
  const topic = String(entry.topic ?? "").normalize("NFD").replace(/[\u0300-\u036f]/g, "");
  return strongCode(topic) === code;
}

/** A response field the backend may send camelCase (serde) or snake_case. */
function fieldOf(obj, camel, snake) {
  if (!obj) return undefined;
  return obj[camel] !== undefined ? obj[camel] : obj[snake];
}
function hasField(obj, camel, snake) {
  return !!obj && (camel in obj || snake in obj);
}
/** Valid Strong codes of one interlinear hit ('strong' may hold several; 'strongs' if sent). */
function hitCodes(hit) {
  if (!hit) return [];
  const raw = [hit.strong || "", ...(Array.isArray(hit.strongs) ? hit.strongs : [])].join(" ");
  return [...new Set(raw.split(/[\s,;]+/).map(strongCode).filter(Boolean))];
}
/**
 * Codes whose definitions the Léxico shows in full (ISS-04): the current Strong plus the
 * matched interlinear unit's codes. One unit ('τὸν υἱὸν') arrives as the hit plus continuation
 * hits with no original word; their codes belong to it too (older backend: G3588 then G5207).
 */
function primaryStrongs(wl, lastStrong = state.lastStrong) {
  const out = [];
  const add = (c) => {
    const code = strongCode(c);
    if (code && !out.includes(code)) out.push(code);
  };
  add(lastStrong);
  const hit = wl && wl.matchedHit;
  if (hit) {
    hitCodes(hit).forEach(add);
    const hits = wl.interlinearHits || [];
    const i = hits.findIndex(
      (h) => h.spanish === hit.spanish && h.strong === hit.strong && h.original === hit.original
    );
    for (let j = i + 1; i >= 0 && j < hits.length && !hits[j].original && hits[j].spanish === hit.spanish; j++) {
      // The Greek article riding on a noun (newer backend: G5207 + G3588) stays a related chip.
      hitCodes(hits[j]).forEach((c) => (c === "G3588" && out.length ? null : add(c)));
    }
  }
  return out;
}
/** Every other valid code the backend returned (neighbours, lemma hits): shown as chips only. */
function relatedStrongs(wl, primary = primaryStrongs(wl)) {
  const out = [];
  const add = (c) => {
    const code = strongCode(c);
    if (code && !primary.includes(code) && !out.includes(code)) out.push(code);
  };
  ((wl && wl.strongs) || []).forEach(add);
  const rel = wl && wl.related;
  if (Array.isArray(rel)) rel.forEach((r) => add(typeof r === "string" ? r : r && (r.strong || r.topic)));
  return out;
}
function strongChip(code, secondary = false) {
  return `<button type="button" class="chip-strong${secondary ? " chip-secondary" : ""}" data-strong="${escapeAttr(
    code
  )}">${escapeHtml(code)}</button>`;
}
/** Chips only (context bar, side pane): main codes, then related ones in a quieter style. */
function strongChipsHtml(wl) {
  const primary = primaryStrongs(wl);
  return [...primary.map((c) => strongChip(c)), ...relatedStrongs(wl, primary).map((c) => strongChip(c, true))].join(
    " "
  );
}
/** Labelled bar for the Diccionario / Léxico views: «Strong:» main codes, «Relacionados:» the rest. */
function strongBarHtml(wl, extra = "") {
  const primary = primaryStrongs(wl);
  const related = relatedStrongs(wl, primary);
  if (!primary.length && !related.length) return "";
  let h = "";
  if (primary.length) h += `<span class="sync-label">Strong:</span> ${primary.map((c) => strongChip(c)).join(" ")}`;
  if (related.length) {
    h += `<span class="sync-label${primary.length ? " sync-label-secondary" : ""}">${
      primary.length ? "Relacionados:" : "Posibles números Strong:"
    }</span> ${related.map((c) => strongChip(c, true)).join(" ")}`;
  }
  return `<div class="sync-bar">${h}${extra}</div>`;
}
/**
 * ISS-36: a word the Bible tags with its own Strong wins over the interlinear guess. A newer
 * backend already does this (no-op then); an older one ignores taggedStrongs.
 */
function preferOwnTags(result, tagged) {
  if (!result || !tagged || !tagged.length) return result;
  const codes = (result.strongs || []).map(strongCode).filter(Boolean);
  if (codes[0] === tagged[0]) return result;
  const strongs = [...tagged, ...(result.strongs || []).filter((c) => !tagged.includes(strongCode(c)))];
  const own = (h) => hitCodes(h).includes(tagged[0]);
  const ownHit = own(result.matchedHit) ? result.matchedHit : (result.interlinearHits || []).find(own) || null;
  const out = Object.assign({}, result, { strongs, matchedHit: ownHit });
  if (!ownHit) {
    // The interlinear's originals belong to another word: don't show them on this card.
    out.hebrew = [];
    out.greek = [];
  }
  return out;
}
/**
 * A word's own tag group with the content code first: the Greek article G3588 and the Hebrew
 * object marker H853 go last (kept when alone). Same rule as the backend's `primary_first`;
 * tags are stored in Greek/Hebrew order, so «Dios» (G3588 G2316) would otherwise open «el».
 */
function primaryFirst(codes) {
  const secondary = (c) => c === "G3588" || c === "H853";
  if (codes.every(secondary)) return codes;
  return [...codes.filter((c) => !secondary(c)), ...codes.filter(secondary)];
}
/**
 * ISS-02/35/36: what the backend needs to resolve THIS tapped word: its verse, the Bible it
 * was read in (a Paralela column's own module), which of the same words in the verse it is
 * (only real .word tokens count; morphology codes, footnotes and Strong links never are
 * .word; words of a section title the backend marks as .vtitle are skipped, since the
 * interlinear it is aligned with has no titles), and the Strong numbers the Bible stores on it.
 */
function tapContext(wordEl) {
  const scope = wordEl.closest(".verse");
  const verse = Number(scope?.dataset.verse || wordEl.closest("tr")?.dataset.verse || state.verse);
  const key = normalizeLoose(cleanQueryWord(wordEl.textContent));
  let occurrence = 0;
  if (scope && key) {
    for (const w of scope.querySelectorAll(".word")) {
      if (w === wordEl) break;
      if (w.closest(".vtitle")) continue;
      if (normalizeLoose(cleanQueryWord(w.textContent)) === key) occurrence++;
    }
  }
  const taggedStrongs = primaryFirst(
    String(wordEl.dataset.strong || "")
      .split(/\s+/)
      .map(strongCode)
      .filter(Boolean)
  );
  const biblePath = wordEl.closest("[data-module]")?.dataset.module || state.biblePath || "";
  return { book: state.book, chapter: state.chapter, verse, biblePath, occurrence, taggedStrongs };
}
/** ISS-43: promise Greek/Hebrew input only once the backend reports a lemma index. */
function updateQueryPlaceholder() {
  const dq = $("#dict-query");
  if (!dq) return;
  if (state.workspace === "lexico") {
    dq.placeholder = state.lemmaIndexAvailable
      ? "Strong G26 / H430, palabra griega o hebrea…"
      : "Número Strong (G26 / H430) o palabra…";
  } else if (state.workspace === "diccionario") dq.placeholder = "Palabra o tema en español…";
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

/** Unknown ids (old or hand-edited storage) fall back to the system font. */
function applyReadFont(id) {
  if (!READ_FONTS.some((f) => f.id === id)) id = "system";
  const root = document.documentElement;
  if (id === "system") root.removeAttribute("data-read-font");
  else root.setAttribute("data-read-font", id);
  try {
    localStorage.setItem(LS.readFont, id);
  } catch (err) {
    console.warn("No se pudo guardar la fuente de lectura:", err);
  }
  $$("#font-grid .font-card").forEach((c) => {
    const on = c.dataset.font === id;
    c.classList.toggle("active", on);
    c.setAttribute("aria-pressed", on ? "true" : "false");
  });
}

/** Font cards in #panel-theme: "Aa" drawn in each font (CSS .font-card[data-font]), label below. */
function renderFontGrid() {
  const grid = $("#font-grid");
  if (!grid) return;
  const active = document.documentElement.getAttribute("data-read-font") || "system";
  grid.innerHTML = READ_FONTS.map((f) => {
    const on = f.id === active;
    return `<button type="button" class="font-card${on ? " active" : ""}" data-font="${f.id}" aria-pressed="${on}" title="${escapeHtml(f.label)}">
        <span class="font-sample" aria-hidden="true">Aa</span>
        <span class="font-card-label">${escapeHtml(f.label)}</span>
      </button>`;
  }).join("");
  grid.onclick = (e) => {
    const btn = e.target.closest(".font-card");
    if (btn?.dataset.font) applyReadFont(btn.dataset.font);
  };
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

/**
 * Scan `dir` and rebuild every list. Returns false when the folder could not be read.
 * persist:false — load for this session only (init fallback; the saved folder is kept, ISS-31).
 */
async function loadModules(dir, { refreshStudy = true, persist = true } = {}) {
  if (!dir) return false;
  const meta = $("#modules-meta");
  if (meta) meta.textContent = "Escaneando módulos…";
  const prevDir = state.modulesDir;
  const loadSeq = ++state.modulesLoadSeq;
  let modules;
  try {
    modules = await invoke("list_modules", { modulesDir: dir });
    if (loadSeq !== state.modulesLoadSeq) return false; // another folder was chosen meanwhile
  } catch (e) {
    if (loadSeq !== state.modulesLoadSeq) return false;
    // ISS-31: nothing is saved (or replaced) until the folder was actually read.
    if (meta) meta.textContent = "No se pudo leer la carpeta.";
    alertSoft("Error al escanear módulos", String(e));
    return false;
  }
  state.modulesDir = dir;
  state.modulesDirSessionOnly = !persist;
  if (persist) localStorage.setItem(LS.modulesDir, dir);
  $("#modules-path").value = dir;
  state.modules = Array.isArray(modules) ? modules : [];
  // ISS-46: a folder switch (or «Actualizar lista») drops results and choices of the old list.
  const reloading = !!prevDir;
  if (reloading) {
    state.interconnectSeq++; // a lookup still running belongs to the old list
    state.wordLinks = null;
    state.lastStrong = "";
    state.lexTopic = null;
    state.cmtWord = null;
    const inList = (p) => state.modules.some((m) => m.path === p);
    for (const ws of Object.keys(state.modulesByKind)) {
      if (state.modulesByKind[ws] && !inList(state.modulesByKind[ws])) state.modulesByKind[ws] = "";
    }
    if (state.crossRefSourcePath && !inList(state.crossRefSourcePath)) state.crossRefSourcePath = "";
  }

  const bibles = modulesOf("bible");
  // ISS-23: a saved Bible that still exists wins; otherwise RVR1960 (not the first in the list).
  const bibleWanted = bibles.some((b) => b.path === state.biblePath)
    ? state.biblePath
    : preferredModule("bible", bibles)?.path || "";
  fillSelect(
    $("#sel-bible"),
    bibles,
    "path",
    (m) => `${m.encrypted ? "🔒 " : ""}${m.abbreviation}`,
    bibleWanted
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

  // ISS-44: only the backend's validated pick (no filename guess: those files parse 0 hits).
  let interlinear = "";
  try {
    interlinear = (await invoke("find_interlinear", { modulesDir: dir })) || "";
  } catch {
    interlinear = "";
  }
  if (loadSeq !== state.modulesLoadSeq) return false;
  state.interlinearPath = interlinear;

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

  state.probeCache.clear(); // probe results belong to the previous folder's module list
  state.probeInflight.clear(); // …and so do probes still running: never share or cache them
  state.probeSeq++;
  selectStudyModuleSync(state.workspace);
  await reloadChapter();
  if (reloading) {
    persistStudy();
    if (state.selectedWord) {
      // Re-resolve the selected word once against the new list (its old Bible may be gone).
      const o = state.wordOrigin;
      const at = o ? Object.assign({}, o, { biblePath: state.modules.some((m) => m.path === o.biblePath) ? o.biblePath : "" }) : null;
      await interconnectWord(state.selectedWord, { stay: true, render: false, at });
    }
  }
  if (refreshStudy) await refreshWorkspace();
  return true;
}

function updateBibleTitleTooltip() {
  const m = state.modules.find((x) => x.path === state.biblePath);
  $("#sel-bible").title = m ? m.title : "";
}

// —— Study module per tab ——
// The module a study tab renders with is chosen synchronously (saved choice, else a
// fallback that is never persisted). The content probe only annotates and re-sorts the
// dropdown; it can switch an automatic pick to one with content, never a user's choice.
const STUDY_KIND = { comentario: "commentary", diccionario: "dictionary", lexico: "lexicon" };
const LEXICON_EXTRA_RE = /strong|lexic|vine|thayer|bdb|hebrew|hebreo|griego|greek/i;
const PROBE_CACHE_MAX = 50;
/** How long a first Diccionario render waits for the content probe before using the sync pick. */
const FIRST_PROBE_WAIT_MS = 1200;

/** Modules offered for a study tab — the same list the dropdown shows. */
function studyCandidates(ws) {
  const kind = STUDY_KIND[ws];
  if (!kind) return [];
  const list = modulesOf(kind).filter((m) => !m.encrypted || kind === "commentary");
  if (kind === "lexicon") {
    const seen = new Set(list.map((m) => m.path));
    for (const m of modulesOf("dictionary")) {
      if (!seen.has(m.path) && LEXICON_EXTRA_RE.test(`${m.abbreviation} ${m.title} ${m.filename}`)) {
        list.push(m);
      }
    }
  }
  return list;
}

// —— Preferred modules (ISS-23) ——
// Used only when nothing valid is saved (and to order the side pane, the hover preview and the
// passage-preview fallback Bible). Automatic picks are never written to modulesByKind.
const modHay = (m) => `${m.filename || ""} ${m.abbreviation || ""} ${m.title || ""}`;
const PREFERRED = {
  bible: [
    (m) => /^00RVR1960\.bbli$/i.test(m.filename || ""),
    (m) => /^RV1960 Reina Valera 1960\.bbli$/i.test(m.filename || ""),
    (m) => /^RVR?1960$/i.test(String(m.abbreviation || "").trim()),
  ],
  commentary: [/matthew henry/i, /macarthur/i, /mundo hispano/i, /plenitud/i],
  dictionary: [/nelson/i, /vila.?escuain/i, /certeza/i, /mundo hispano/i],
  lexicon: [/\bSTRONGes\b/i, /strong ?spa\b/i, /strongs? ?plus/i, /multil[eé]xico/i, /bdb-thayeres/i],
};
// Sorted after everything else (English modules a Spanish reader rarely wants first).
const DEPRIORITIZED = {
  commentary: [
    /^(adam clarke|biblical illustrator|e\. w\. bullinger|expositor's bible|jamieson-fausset-brown|keil & delitzsch|nasb|phc|pnt|popular new testament|preacher's homiletical|summarized bible|tske|voice|web)$/i,
  ],
  lexicon: [/\bAHLB\b|ancient hebrew lexicon/i],
};
/** 0… = position in the preferred list, 100 = neutral, 200 = deprioritized. */
function preferenceRank(m, kind) {
  const prefs = PREFERRED[kind] || [];
  for (let i = 0; i < prefs.length; i++) {
    const p = prefs[i];
    if (typeof p === "function" ? p(m) : p.test(modHay(m))) return i;
  }
  for (const re of DEPRIORITIZED[kind] || []) {
    if (re.test(String(m.abbreviation || "").trim()) || re.test(modHay(m))) return 200;
  }
  return 100;
}
/** The first readable preferred module of `kind` in `list` (scan order breaks ties), or null. */
function preferredModule(kind, list = modulesOf(kind)) {
  let best = null;
  let bestRank = 100;
  for (const m of list) {
    if (m.encrypted) continue;
    const r = preferenceRank(m, kind);
    if (r < bestRank) {
      best = m;
      bestRank = r;
    }
  }
  return best;
}

/**
 * Modules with content first (when a probe result is known), then preferred, then alphabetical.
 * Commentaries: the user's preferred list (Matthew Henry → MacArthur → …) comes before content —
 * a preferred commentary without a verse note still answers with its chapter note (ISS-16), so
 * an English commentary with a verse note must not displace it. Content still orders within
 * each group.
 */
function rankStudyModules(list, availability, kind = "") {
  const has = (m) => (availability && availability.get(m.path) ? 1 : 0);
  const group = (m) => (kind === "commentary" && preferenceRank(m, kind) < 100 ? 0 : 1);
  return [...list].sort(
    (a, b) =>
      group(a) - group(b) ||
      has(b) - has(a) ||
      preferenceRank(a, kind) - preferenceRank(b, kind) ||
      a.abbreviation.localeCompare(b.abbreviation, "es")
  );
}

/** The user's saved module for `ws`, only if it is still a candidate for that tab. */
function savedStudyModule(ws) {
  const saved = state.modulesByKind[ws] || "";
  return saved && studyCandidates(ws).some((m) => m.path === saved) ? saved : "";
}

function studyProbeQuery(ws) {
  const dq = $("#dict-query")?.value || "";
  if (ws === "lexico") return strongCode(state.lastStrong) || state.selectedWord || dq;
  return state.selectedWord || dq;
}
function studyProbeKey(ws, query = studyProbeQuery(ws)) {
  const kind = STUDY_KIND[ws];
  if (kind === "commentary") {
    const f = state.filters || {};
    return `${kind}|${state.book}:${state.chapter}:${state.verse}|${f.verse ? 1 : 0}${f.chapter ? 1 : 0}${f.book ? 1 : 0}`;
  }
  return `${kind}|${query}`;
}
function probeCacheGet(key) {
  const v = state.probeCache.get(key);
  if (!v) return null;
  state.probeCache.delete(key); // LRU: most recent last
  state.probeCache.set(key, v);
  return v;
}
function probeCachePut(key, availability) {
  state.probeCache.delete(key);
  state.probeCache.set(key, availability);
  while (state.probeCache.size > PROBE_CACHE_MAX) {
    state.probeCache.delete(state.probeCache.keys().next().value);
  }
}
function cachedStudyProbe(ws) {
  return STUDY_KIND[ws] ? probeCacheGet(studyProbeKey(ws)) : null;
}

/**
 * Decide (and show) the module for `ws` without waiting on any IPC.
 * Saved choice if still valid; otherwise the first readable module (content-first when a
 * probe result for the current query is cached). The fallback is NOT written to modulesByKind.
 */
function selectStudyModuleSync(ws = state.workspace) {
  const sel = $("#sel-study-module");
  if (!STUDY_KIND[ws]) {
    state.studyModule = "";
    if (sel) sel.innerHTML = "";
    return "";
  }
  const availability = cachedStudyProbe(ws);
  const ranked = rankStudyModules(studyCandidates(ws), availability, STUDY_KIND[ws]);
  const pick = savedStudyModule(ws) || (ranked.find((m) => !m.encrypted) || ranked[0])?.path || "";
  state.studyModule = pick;
  if (sel) {
    fillSelect(
      sel,
      ranked,
      "path",
      (m) => {
        const has = availability?.get(m.path);
        const tag = has ? "✦ " : "";
        const enc = m.encrypted ? "🔒 " : "";
        const hasNote = has ? " — (Con datos)" : "";
        return `${tag}${enc}${m.abbreviation} — ${m.title}${hasNote}`;
      },
      pick
    );
  }
  return pick;
}

/**
 * Content probe for `ws` at the current query/verse: cached result, the in-flight promise for
 * the same key, or one new probe_modules_content call. Resolves to Map(path→has) or null.
 */
function studyProbe(ws = state.workspace) {
  const kind = STUDY_KIND[ws];
  if (!kind) return Promise.resolve(null);
  const list = studyCandidates(ws);
  if (!list.length) return Promise.resolve(null);
  const query = studyProbeQuery(ws);
  const key = studyProbeKey(ws, query);
  const cached = probeCacheGet(key);
  if (cached) return Promise.resolve(cached);
  const running = state.probeInflight.get(key);
  if (running) return running; // same probe already running: share it
  const p = invoke("probe_modules_content", {
    kind,
    paths: list.map((m) => m.path),
    book: state.book,
    chapter: state.chapter,
    verse: state.verse,
    query,
    filters: state.filters,
  })
    .then(
      (res) => {
        if (!Array.isArray(res)) return null;
        const map = new Map(res.map((r) => [r.path, !!r.has]));
        // Only a probe still current for its key is cached (a folder switch drops older ones).
        if (state.probeInflight.get(key) === p) probeCachePut(key, map);
        return map;
      },
      () => null
    )
    .then((map) => {
      if (state.probeInflight.get(key) === p) state.probeInflight.delete(key);
      return map;
    });
  state.probeInflight.set(key, p);
  return p;
}

/**
 * Annotate the module dropdown with ✦ (has content for the current query/verse).
 * Never overwrites a user's choice; drops late replies (tab, query or newer probe changed).
 * Re-renders only when nothing is saved and the content-first pick differs from the sync pick.
 */
async function fillStudyModuleSelect(ws = state.workspace) {
  if (!STUDY_KIND[ws] || !studyCandidates(ws).length) return;
  const key = studyProbeKey(ws);
  const token = ++state.probeSeq;
  const availability = await studyProbe(ws);
  if (!availability) return;
  if (token !== state.probeSeq || state.workspace !== ws || studyProbeKey(ws) !== key) return;
  const before = state.studyModule;
  const pick = selectStudyModuleSync(ws);
  if (pick && pick !== before && !savedStudyModule(ws)) {
    await loadStudyView();
    // 'Otros comentarios' was built for the previous primary; rebuild it without the new one.
    if (ws === "comentario" && state.workspace === ws && state.studyModule === pick) await refreshAnalysisPane();
  }
}

// —— Workspace tabs ——
function setWorkspace(ws, { skipReload = false } = {}) {
  if (ws !== "comentario") state.cmtWord = null; // the word card belongs to the commentary view
  state.workspace = ws;
  $$(".ws-tab").forEach((t) => {
    const on = t.dataset.ws === ws;
    t.classList.toggle("active", on);
    t.setAttribute("aria-selected", on ? "true" : "false");
  });
  const isBible = ws === "biblia";
  const wasHidden = $("#view-biblia").classList.contains("hidden");
  $("#view-biblia").classList.toggle("hidden", !isBible);
  if (isBible && wasHidden) {
    // Back from Comentario (verse stepping): show the verse the user was reading (ISS-18).
    requestAnimationFrame(() => $("#bible-content .verse.selected")?.scrollIntoView({ block: "center" }));
  }
  $("#view-study").classList.toggle("hidden", isBible);
  $("#study-chrome").classList.toggle("hidden", isBible);
  $("#commentary-filters").classList.toggle("hidden", ws !== "comentario");
  $("#verse-stepper")?.classList.toggle("hidden", ws !== "comentario");
  $("#dict-search-row").classList.toggle(
    "hidden",
    ws !== "diccionario" && ws !== "lexico"
  );
  const dq = $("#dict-query");
  if (dq) {
    updateQueryPlaceholder();
    // Always sync search box to the active selection (never keep a stale word)
    if (ws === "diccionario" && state.selectedWord) {
      dq.value = state.selectedWord;
    } else if (ws === "lexico") {
      dq.value = state.lastStrong || state.selectedWord || dq.value;
    }
  }
  // Decide this tab's module before anything renders (never the previous tab's module)
  selectStudyModuleSync(ws);
  updateAnalysisChrome();
  persistStudy();
  if (!skipReload) refreshWorkspace();
}

async function refreshWorkspace() {
  updateContextBar();
  if (state.workspace === "comentario") {
    // ISS-15: the chosen commentary renders first; 'Otros comentarios' never delays it.
    await loadStudyView();
    await refreshAnalysisPane();
    return;
  }
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
  $("#ws-ctx-strongs").innerHTML = state.wordLinks ? strongChipsHtml(state.wordLinks) : "";
  updateAnalysisChrome();
}

function snippet(plain, n = 160) {
  const s = (plain || "").replace(/\s+/g, " ").trim();
  if (s.length <= n) return s;
  return s.slice(0, n) + "…";
}

const CMT_PANE_FIRST = 10;
const collapsePlain = (t) => String(t || "").replace(/\s+/g, " ").trim();
/** The entry that represents a commentary in the side pane: its exact-verse note first (ISS-17). */
function representativeEntry(entries, verse = state.verse) {
  const exact = entries.find((e) => e.scope === "verse" && new RegExp(`^v\\.?\\s*${verse}$`, "i").test(String(e.label || "").trim()));
  return exact || entries.find((e) => e.scope === "verse") || entries[0];
}

/**
 * 'Otros comentarios' (ISS-15): modules the probe says have a note here, minus the primary,
 * Spanish/preferred first; the first 10 are fetched (behind a token), copies are dropped,
 * and «Mostrar todos (N)» fetches the rest.
 */
async function refreshCommentaryPane(box, primary = state.studyModule || "", { all = false } = {}) {
  const seq = ++state.paneSeq;
  const at = { book: state.book, chapter: state.chapter, verse: state.verse };
  const filters = Object.assign({}, state.filters);
  const isCurrent = () =>
    seq === state.paneSeq &&
    state.workspace === "comentario" &&
    (state.studyModule || "") === primary &&
    state.book === at.book &&
    state.chapter === at.chapter &&
    state.verse === at.verse;
  // The word card is re-read at every write (a word may be tapped while the pane loads).
  box.innerHTML = `${commentaryWordCardHtml()}<div class="empty-state soft">Cargando otros comentarios…</div>`;
  wireCommentaryWordCard(box);
  const availability = await studyProbe("comentario");
  if (!isCurrent()) return;
  let candidates = modulesOf("commentary").filter((m) => m.path !== primary && !m.encrypted);
  if (availability) candidates = candidates.filter((m) => availability.get(m.path));
  candidates = rankStudyModules(candidates, null, "commentary");
  const batch = all ? candidates : candidates.slice(0, CMT_PANE_FIRST);
  const results = await Promise.all(
    batch.map((m) =>
      invoke("get_commentary", {
        modulePath: m.path,
        book: at.book,
        chapter: at.chapter,
        verse: at.verse,
        filters,
      }).then(
        (entries) => ({ m, entries }),
        () => ({ m, entries: null }) // side panes skip unreadable modules quietly
      )
    )
  );
  if (!isCurrent()) return;
  const primaryKey = `${primary}|${at.book}:${at.chapter}:${at.verse}`;
  const seen = new Set(state.primaryCmtPlain.key === primaryKey ? state.primaryCmtPlain.texts : []);
  const cards = [];
  for (const { m, entries } of results) {
    if (!entries || !entries.length) continue;
    const e = representativeEntry(entries, at.verse);
    const plain = collapsePlain(e.plain);
    if (plain && seen.has(plain)) continue; // a copy of a module already shown
    if (plain) seen.add(plain);
    cards.push({ path: m.path, abbr: m.abbreviation, title: m.title, topic: e.label || "v.", plain });
  }
  const where = `${escapeHtml(bookAbbr(at.book))} ${at.chapter}:${at.verse}`;
  if (!cards.length) {
    box.innerHTML = `${commentaryWordCardHtml()}<div class="empty-state soft">Ningún otro comentario tiene notas para ${where} con los filtros actuales.</div>`;
    wireCommentaryWordCard(box);
    return;
  }
  const more =
    !all && candidates.length > batch.length
      ? `<button type="button" class="glass-btn full subtle cmt-show-all">Mostrar todos (${candidates.length})</button>`
      : "";
  box.innerHTML =
    commentaryWordCardHtml() +
    `<div class="ctx-section-title">Otros sobre este versículo</div>` +
    cards
      .map(
        (c) => `
      <article class="mini-entry" data-kind="cmt" data-path="${escapeAttr(c.path)}" title="${escapeAttr(c.title || "")}">
        <div class="mini-meta">${escapeHtml(c.abbr)} · ${escapeHtml(c.topic)}</div>
        <div class="mini-body">${escapeHtml(snippet(c.plain))}</div>
      </article>`
      )
      .join("") +
    more;
  wireCommentaryWordCard(box);
  const btn = box.querySelector(".cmt-show-all");
  if (btn) {
    btn.onclick = (ev) => {
      ev.stopPropagation();
      btn.disabled = true;
      btn.textContent = "Cargando…";
      refreshCommentaryPane(box, primary, { all: true });
    };
  }
}
// —— Word tapped inside a commentary (ISS-19): a card at the top of the side pane ——
/** Lexicon entries in the user's order: the chosen Léxico module, then the preferred ones (ISS-23). */
function rankLexEntries(entries, kind = "lexicon") {
  const chosen = kind === "lexicon" ? savedStudyModule("lexico") : savedStudyModule("diccionario");
  const rank = (e) => {
    if (chosen && e.modulePath === chosen) return -1;
    const m = state.modules.find((x) => x.path === e.modulePath);
    return m ? preferenceRank(m, kind) : 150;
  };
  return (entries || [])
    .map((e, i) => ({ e, i }))
    .sort((a, b) => rank(a.e) - rank(b.e) || a.i - b.i)
    .map((x) => x.e);
}
function commentaryWordCardHtml() {
  const word = state.cmtWord;
  if (!word) return `<div class="cmt-word-slot"></div>`;
  const label = state.cmtWordLabel || word; // shown; `word` stays the lookup key
  const wl = state.wordLinks;
  let inner;
  if (state.cmtWordError) {
    inner = `<div class="empty-state error" role="alert">No se pudo buscar «${escapeHtml(label)}». <span class="error-detail">${escapeHtml(
      state.cmtWordError.slice(0, 200)
    )}</span></div><div class="cmt-word-actions"><button type="button" class="glass-btn compact cmt-close-word">Cerrar</button></div>`;
  } else if (!wl || normalizeLoose(wl.word) !== normalizeLoose(word)) {
    inner = `<div class="empty-state soft">Buscando «${escapeHtml(label)}»…</div>`;
  } else {
    // The chosen/preferred lexicon's own entry (looked up after the resolve), else the best
    // backend entry for the word's own code (never a prefix neighbour such as G260 for G26).
    const codes = primaryStrongs(wl);
    const own = (e) => (codes.length && codes.includes(strongCode(e.topic)) ? 0 : 1);
    const lexRanked = rankLexEntries(wl.lexicon, "lexicon");
    const lex =
      state.cmtWordEntry ||
      lexRanked.map((e, i) => ({ e, i })).sort((a, b) => own(a.e) - own(b.e) || a.i - b.i).map((x) => x.e)[0];
    const dict = lex ? null : rankLexEntries(wl.dictionary, "dictionary")[0];
    const entry = lex || dict;
    const chips = strongChipsHtml(wl);
    inner = `<div class="ctx-word-title">${escapeHtml(state.cmtWordLabel || wl.word || word)}</div>`;
    if (chips) inner += `<div class="ctx-row">${chips}</div>`;
    if (entry) {
      inner += `<article class="cmt-word-entry">
        <div class="mini-meta">${escapeHtml(entry.moduleAbbr || "")} · ${escapeHtml(entry.topic || "")}</div>
        <div class="study-body">${linkifyHtml(entry.html || escapeHtml(entry.plain || ""), { book: state.book, chapter: state.chapter })}</div>
      </article>`;
    } else {
      inner += `<div class="empty-state soft">Sin definición para «${escapeHtml(label)}» en los léxicos ni diccionarios.</div>`;
    }
    inner += `<div class="cmt-word-actions">
      <button type="button" class="glass-btn compact cmt-open-lex">Abrir en Léxico</button>
      <button type="button" class="glass-btn compact subtle cmt-close-word" aria-label="Cerrar la palabra">Cerrar</button>
    </div>`;
  }
  return `<div class="cmt-word-slot"><section class="cmt-word-card" aria-label="Palabra del comentario">${inner}</section></div>`;
}
function wireCommentaryWordCard(box) {
  const card = box && box.querySelector(".cmt-word-card");
  if (!card) return;
  normalizeBibleRefElements(card);
  card.onclick = async (e) => {
    if (e.target.closest(".cmt-open-lex")) {
      e.stopPropagation();
      state.cmtWord = null;
      setWorkspace("lexico");
      return;
    }
    if (e.target.closest(".cmt-close-word")) {
      e.stopPropagation();
      state.cmtWord = null;
      updateCommentaryWordCard();
      return;
    }
    const link = e.target.closest(".bible-ref, .strong-ref, .word-like");
    if (!link) return; // chips: the document handler (→ Léxico)
    e.stopPropagation();
    e.preventDefault();
    if (link.classList.contains("word-like")) await lookupWordInCommentary(link.dataset.lookup || link.textContent, link.textContent);
    else await openLinkPanel(link);
  };
}
/** Re-render only the word card (the other commentaries below are left alone). */
function updateCommentaryWordCard() {
  const box = $("#link-panel-inline");
  const slot = box && box.querySelector(".cmt-word-slot");
  if (!slot || state.workspace !== "comentario") return;
  const tmp = document.createElement("div");
  tmp.innerHTML = commentaryWordCardHtml();
  slot.innerHTML = tmp.firstElementChild ? tmp.firstElementChild.innerHTML : "";
  wireCommentaryWordCard(box);
}
/** `label`: the tapped text as written (accents, final ς); `q` is the folded lookup key. */
async function lookupWordInCommentary(q, label = "") {
  const w = cleanQueryWord(q);
  if (!w) return;
  state.cmtWord = w;
  state.cmtWordLabel = String(label || "").trim();
  state.cmtWordError = "";
  state.cmtWordEntry = null;
  updateCommentaryWordCard(); // «Buscando…»
  await interconnectWord(w, { stay: true, render: false });
  if (state.cmtWord !== w) return;
  const code = state.wordLinks ? primaryStrongs(state.wordLinks)[0] : "";
  const lexModule =
    savedStudyModule("lexico") || preferredModule("lexicon", studyCandidates("lexico"))?.path || "";
  if (code && lexModule) {
    try {
      const entry = await invoke("lookup_lexicon", { modulePath: lexModule, strong: code });
      if (state.cmtWord !== w) return;
      if (lexEntryMatches(entry, code)) {
        const m = state.modules.find((x) => x.path === lexModule);
        state.cmtWordEntry = Object.assign({}, entry, { modulePath: lexModule, moduleAbbr: m?.abbreviation || "" });
      }
    } catch {
      /* the backend's own entries are used */
    }
  }
  if (state.cmtWord === w) updateCommentaryWordCard();
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
    await refreshCommentaryPane(box, primary);
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
    const chips = strongChipsHtml(wl);
    if (chips) html += `<div class="ctx-row">${chips}</div>`;

    if (ws === "diccionario" || ws === "biblia") {
      html += `<div class="ctx-section-title">${ws === "diccionario" ? "Otros diccionarios" : "Diccionarios"}</div>`;
      if (dictOthers.length) {
        html += dictOthers
          .slice(0, 8)
          .map(
            (e) => `
        <article class="mini-entry" data-kind="dict" data-path="${escapeAttr(e.modulePath)}" data-topic="${escapeAttr(e.topic)}">
          <div class="mini-meta">${escapeHtml(e.moduleAbbr)} · ${escapeHtml(e.topic)}${strongBridgeNote(e, wl)}</div>
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
    const chips = strongChipsHtml(wl);
    if (chips) html += `<div class="ctx-row">${chips}</div>`;
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
  const stepLabel = $("#verse-stepper-label");
  if (stepLabel) stepLabel.textContent = `v.${state.verse}`;
  const stepPrev = $("#btn-verse-prev");
  if (stepPrev) stepPrev.disabled = state.book === 1 && state.chapter === 1 && state.verse === 1;
  setVerseNextUnknown(false); // a new position may have a known verse count again
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
  // The selected word keeps the result from the verse it was tapped in (wordLinks.origin);
  // re-resolving it in the new verse produced wrong Strongs.
  await refreshWorkspace();
}

// —— Verse stepping in Comentario (ISS-18) ——
// ‹ v.16 › and ←/→ / Alt+↑/↓: next or previous verse without goTo, history or a word re-resolve.
// Past the last verse → next chapter v.1; before v.1 → previous chapter's last verse.
const verseCountCache = new Map();
function knownVerseCount(book, chapter) {
  const d = state.chapterData;
  if (d && d.book === book && d.chapter === chapter && (!d.module || d.module === state.biblePath) && d.verses?.length) {
    return Math.max(...d.verses.map((v) => Number(v.verse) || 0));
  }
  return verseCountCache.get(`${state.biblePath}|${book}:${chapter}`) || 0;
}
async function verseCountOf(book, chapter) {
  const known = knownVerseCount(book, chapter);
  if (known || !state.biblePath) return known;
  try {
    const n = Number(await invoke("get_verse_count", { modulePath: state.biblePath, book, chapter })) || 0;
    if (n > 0) verseCountCache.set(`${state.biblePath}|${book}:${chapter}`, n);
    return n;
  } catch {
    return 0;
  }
}
/** ISS-18 review: with no verse count (no Bible, or get_verse_count failed) › is disabled. */
function setVerseNextUnknown(unknown) {
  const next = $("#btn-verse-next");
  if (!next) return;
  next.disabled = unknown;
  next.title = unknown ? "No se conoce el número de versículos de este capítulo" : "Versículo siguiente (→ o Alt+↓)";
}
let verseStepChain = Promise.resolve();
/** Steps are applied one after another (fast clicks / key repeat never read a stale verse). */
function stepVerse(delta) {
  verseStepChain = verseStepChain.then(() => applyVerseStep(delta)).catch((e) => console.error(e));
  return verseStepChain;
}
async function applyVerseStep(delta) {
  if (state.workspace !== "comentario") return;
  let { book, chapter } = state;
  let verse = state.verse + delta;
  if (delta > 0) {
    const count = await verseCountOf(book, chapter);
    if (!count) {
      // Unknown chapter end: never walk past it (3:37, 3:38…); › stays off until we move.
      if (state.workspace === "comentario" && book === state.book && chapter === state.chapter) setVerseNextUnknown(true);
      return;
    }
    if (verse > count) {
      if (chapter < bookChapters(book)) chapter += 1;
      else if (book < 66) {
        book += 1;
        chapter = 1;
      } else return; // Ap 22:21
      verse = 1;
    }
  } else if (verse < 1) {
    if (chapter > 1) chapter -= 1;
    else if (book > 1) {
      book -= 1;
      chapter = bookChapters(book);
    } else return; // Gn 1:1
    verse = (await verseCountOf(book, chapter)) || 1;
  }
  if (state.workspace !== "comentario") return;
  const chapterChanged = book !== state.book || chapter !== state.chapter;
  state.book = book;
  state.chapter = chapter;
  state.verse = verse;
  state.selectedVerses = new Set([verse]);
  persistNav();
  if (chapterChanged) {
    fillChapters();
    $("#sel-book").value = String(book);
    $("#sel-chapter").value = String(chapter);
    await reloadChapter(); // keeps the hidden Bible (and chapterData, verse counts) in sync
    if (state.book !== book || state.chapter !== chapter || state.verse !== verse) return;
  } else {
    updateLocationLabel();
    $$(".verse", $("#bible-content")).forEach((el) =>
      el.classList.toggle("selected", Number(el.dataset.verse) === verse)
    );
  }
  loadStudyView(); // renderSeq drops older renders
  clearTimeout(state.paneTimer);
  state.paneTimer = setTimeout(() => {
    if (state.workspace === "comentario") refreshAnalysisPane();
  }, 150);
}

// —— Bible render ——
// A character entity is one atomic unit: never split, never tapped on its own. Letter
// entities (&eacute;, &#233;) stay inside their word; everything else (&amp; &lt; &gt;
// &nbsp; dashes…) is emitted as plain text between words.
const RE_ENTITY_SPLIT = /(&(?:#\d+|#[xX][0-9a-fA-F]+|[A-Za-z][A-Za-z0-9]*);)/;
const NON_LETTER_ENTITIES = new Set(
  ("amp lt gt quot apos nbsp ensp emsp thinsp shy zwnj zwj lrm rlm ndash mdash hellip laquo raquo " +
    "lsquo rsquo sbquo ldquo rdquo bdquo iexcl iquest middot bull sect para deg copy reg trade " +
    "dagger times divide plusmn prime").split(" ")
);
function entityIsLetter(ent) {
  const body = ent.slice(1, -1);
  if (body[0] !== "#") return !NON_LETTER_ENTITIES.has(body.toLowerCase());
  const cp = /^#x/i.test(body) ? parseInt(body.slice(2), 16) : parseInt(body.slice(1), 10);
  if (!Number.isFinite(cp) || cp <= 0 || cp > 0x10ffff) return false;
  return /[\p{L}\p{M}\p{N}]/u.test(String.fromCodePoint(cp));
}
// A word is letters/marks/digits with internal apostrophes (yeshu’ah). Everything else —
// punctuation, curly quotes, *, †, °, • — is plain text between words. A bracketed footnote
// marker is plain text too: [a], [12], [6b], [j11] (any digit), or an all-capitals code ([ADV] in
// the Greek TR). Other bracketed words are supplied by the translator ([es], [yo], [su] in
// NRV2000, TNM, VIN…) and stay tappable;
// only a one-letter supplied word ([y]) cannot be told from a marker and stays plain.
// 'Di-s' (CJ's Elohim) is the only hyphenated word (ISS-37).
const RE_WORD_RUNS =
  /(\[(?:(?=[\p{L}\p{N}]{0,2}\p{N})[\p{L}\p{N}]{1,3}|\p{L}\p{M}*|\p{Lu}{2,3})\])|((?:Di-s|DI-S)(?![\p{L}\p{M}\p{N}])|[\p{L}\p{M}\p{N}]+(?:['’][\p{L}\p{M}\p{N}]+)*)|([^\p{L}\p{M}\p{N}[]+|\[)/gu;
const RE_HAS_LETTER = /\p{L}/u;
const RE_BARE_STRONG = /^[HG]\d{1,5}$/;
// Text inside these elements is never wrapped: morphology codes, footnote markers, Strong
// links and words the backend already tagged with their own Strong (ISS-20 / ISS-36).
const RE_NOWRAP_TAG =
  /^<(?:span\b[^>]*\sclass\s*=\s*"(?:[^"]*\s)?(?:morph|strong-ref|word)(?:\s[^"]*)?"|sup\b[^>]*\sclass\s*=\s*"(?:[^"]*\s)?fn(?:\s[^"]*)?")/i;
const VOID_TAGS = new Set(["br", "img", "hr", "wbr", "input", "meta", "link", "col", "area", "source"]);

function tokenizeWords(html) {
  const parts = html.split(/(<[^>]+>)/);
  let out = "";
  let w = 0;
  let word = "";
  const stack = []; // open elements: { name, nowrap }
  let nowrap = 0;
  const flush = () => {
    if (!word) return;
    if (RE_BARE_STRONG.test(word)) {
      // A bare Strong code in the verse text (untagged '+' Bibles) opens the link panel.
      out += `<span class="strong-ref" data-strong="${word}">${word}</span>`;
    } else if (!RE_HAS_LETTER.test(word) && !word.includes("&")) {
      out += word; // digits only: not a tappable word
    } else {
      out += `<span class="word" data-word="${w++}">${word}</span>`;
    }
    word = "";
  };
  for (const part of parts) {
    if (!part) continue;
    if (part.startsWith("<")) {
      flush();
      out += part;
      const m = part.match(/^<(\/?)([A-Za-z][\w-]*)/);
      if (m && !part.endsWith("/>")) {
        const name = m[2].toLowerCase();
        if (m[1]) {
          const i = stack.map((e) => e.name).lastIndexOf(name);
          if (i >= 0) {
            stack.splice(i).forEach((e) => e.nowrap && nowrap--);
          }
        } else if (!VOID_TAGS.has(name)) {
          const nw = RE_NOWRAP_TAG.test(part);
          stack.push({ name, nowrap: nw });
          if (nw) nowrap++;
        }
      }
      continue;
    }
    if (nowrap > 0) {
      out += part;
      continue;
    }
    part.split(RE_ENTITY_SPLIT).forEach((seg, i) => {
      if (!seg) return;
      if (i % 2 === 1) {
        // entity
        if (entityIsLetter(seg)) word += seg;
        else {
          flush();
          out += seg;
        }
        return;
      }
      for (const m of seg.matchAll(RE_WORD_RUNS)) {
        if (m[2]) word += m[2];
        else {
          flush();
          out += m[1] || m[3];
        }
      }
    });
    flush();
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
  state.selectedVerses = new Set([state.verse]); // only the current verse is drawn as selected
  updateLocationLabel();
  const root = $("#bible-content");
  if (!state.biblePath) {
    root.innerHTML = `<div class="empty-state">Seleccione una carpeta de módulos y una Biblia.</div>`;
    return;
  }
  root.innerHTML = `<div class="empty-state soft">Cargando…</div>`;
  // A later chapter/book/Bible change wins even if its reply comes back first (async backend).
  const seq = ++state.chapterSeq;
  const isCurrent = () => seq === state.chapterSeq;
  try {
    if (state.mode === "paralela") await renderParallel(root, isCurrent);
    else {
      const data = await invoke("get_bible_chapter", {
        modulePath: state.biblePath,
        book: state.book,
        chapter: state.chapter,
      });
      if (!isCurrent()) return;
      state.chapterData = data;
      renderReading(root, data);
    }
  } catch (e) {
    if (!isCurrent()) return;
    // ISS-26: Spanish headline + small raw detail, «Reintentar» / «Elegir otro módulo» (bible.onclick).
    renderError(root, e, state.mode === "paralela" ? "" : state.biblePath);
  }
  if (!isCurrent()) return;
  requestAnimationFrame(() => {
    const el = root.querySelector(`.verse[data-verse="${state.verse}"]`);
    if (el) {
      el.classList.add("selected");
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

async function renderParallel(root, isCurrent = () => true) {
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
  if (!isCurrent()) return;
  // Each column remembers its own module: a tap there resolves against that Bible (ISS-35/36).
  const colPaths = unique;
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
      .map((m, i) => {
        const v = m.get(vn);
        return `<td class="verse" data-verse="${vn}" data-module="${escapeAttr(colPaths[i])}">${
          v ? `<span class="verse-body">${tokenizeWords(v.html)}</span>` : "—"
        }</td>`;
      })
      .join("");
    rows += `<tr data-verse="${vn}"><td class="vnum" data-verse="${vn}">${vn}</td>${cells}</tr>`;
  }
  root.innerHTML = `<table class="parallel-table"><thead><tr><th>#</th>${headers}</tr></thead><tbody>${rows}</tbody></table>`;
}

// —— Interlinking in HTML ——
const RE_STRONG = /\b([HG]\d{1,5})\b/gi;
// —— Book references in study text (ISS-25) ——
// Spanish names and abbreviations plus e-Sword tokens (Jua, Apo, Ose, Hec, Luk…). Books of the
// numbered series list their tokens WITHOUT the 1/2/3 prefix. Also a resolver fallback.
const BOOK_TOKENS = [
  [1, "Génesis Genesis Gén Gen Gn Ge"], [2, "Éxodo Exodo Éxo Exo Éx Ex"], [3, "Levítico Levitico Lev Lv"],
  [4, "Números Numeros Núm Num Nm Nu"], [5, "Deuteronomio Deut Dt"], [6, "Josué Josue Jos"],
  [7, "Jueces Jue Jdc Jdg"], [8, "Rut Rth Rt Ru"], [15, "Esdras Esd Ezr"], [16, "Nehemías Nehemias Neh"],
  [17, "Ester Est"], [18, "Job"], [19, "Salmos Salmo Sal Sl"], [20, "Proverbios Prov Pro Pr"],
  [21, "Eclesiastés Eclesiastes Ecles Ecl Ecc Ec"], [22, "Cantares Cantar Cant Cnt Can Ct"],
  [23, "Isaías Isaias Isa Is"], [24, "Jeremías Jeremias Jer Jr"], [25, "Lamentaciones Lam Lm"],
  [26, "Ezequiel Ezeq Eze Ez"], [27, "Daniel Dan Dn"], [28, "Oseas Ose Hos Os"], [29, "Joel Jl"],
  [30, "Amós Amos Am"], [31, "Abdías Abdias Abd Oba Ob"], [32, "Jonás Jonas Jon"], [33, "Miqueas Miq Mi"],
  [34, "Nahúm Nahum Nah"], [35, "Habacuc Hab"], [36, "Sofonías Sofonias Sof Zep"], [37, "Hageo Hag"],
  [38, "Zacarías Zacarias Zac Zc"], [39, "Malaquías Malaquias Mal"], [40, "Mateo Mat Mt"],
  [41, "Marcos Mar Mr Mc"], [42, "Lucas Luc Luk Lc"], [43, "Juan Jua Joh Jn"], [44, "Hechos Hech Hec Hch Act"],
  [45, "Romanos Rom Rm Ro"], [48, "Gálatas Galatas Gál Gal Gá Ga"], [49, "Efesios Efe Eph Ef"],
  [50, "Filipenses Fil Flp Php"], [51, "Colosenses Col"], [56, "Tito Tit Tt"],
  [57, "Filemón Filemon Flmn Flm Phm"], [58, "Hebreos Heb He Hb"], [59, "Santiago Sant Stg St"],
  [65, "Judas Jud Jd"], [66, "Apocalipsis Apoc Apo Ap Rev"],
];
// [first book number, tokens] — '1 Samuel' = 9, '2 Samuel' = 10…
const NUMBERED_BOOK_TOKENS = [
  [9, "Samuel Sam Sa S"], [11, "Reyes Rey Re R"], [13, "Crónicas Cronicas Crón Cron Cró Cro Cr"],
  [46, "Corintios Cor Co"], [52, "Tesalonicenses Tes Ts Th"], [54, "Timoteo Tim Ti"],
  [60, "Pedro Ped Pe P"], [62, "Juan Jua Jn Jo"],
];
const bookKey = (t) => String(t).normalize("NFD").replace(/\p{M}/gu, "").toLowerCase().replace(/[.\s]/g, "");
const BOOK_BY_TOKEN = new Map();
for (const [n, toks] of BOOK_TOKENS) for (const t of toks.split(" ")) BOOK_BY_TOKEN.set(bookKey(t), n);
const NUMBERED_BY_TOKEN = new Map();
for (const [n, toks] of NUMBERED_BOOK_TOKENS) for (const t of toks.split(" ")) NUMBERED_BY_TOKEN.set(bookKey(t), n);
/** Book number for '1 Co', 'Jua', 'Éx.', '2Ti'… (static table; null when unknown). */
function bookNumberOf(raw) {
  const s = String(raw || "").trim().replace(/\.$/, "");
  const m = s.match(/^([123])\s*(.+)$/);
  if (m) {
    const base = NUMBERED_BY_TOKEN.get(bookKey(m[2]));
    const n = Number(m[1]);
    if (base && n <= (base === 62 ? 3 : 2)) return base + n - 1;
    return null;
  }
  return BOOK_BY_TOKEN.get(bookKey(s)) || null;
}
const reAlt = (lists) =>
  [...new Set(lists.flatMap(([, t]) => t.split(" ")))]
    .sort((a, b) => b.length - a.length)
    .map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join("|");
// No lookbehind (macOS 11 WebKit): the char before the reference is captured and re-emitted.
// Groups: 1 before · 2 book · 3 dot · 4 chapter · 5 separator · 6 verse · 7 end verse (or end
// chapter when 8 is present) · 8 end verse in the end chapter ('Ef 5:21-6:9').
const RE_REF = new RegExp(
  `(^|[^\\p{L}\\p{N}])((?:[123]\\s?(?:${reAlt(NUMBERED_BOOK_TOKENS)}))|(?:${reAlt(BOOK_TOKENS)}))(\\.?)\\s*(\\d{1,3})` +
    `(?:(\\s*[:：]\\s*|\\.(?=\\d)|,(?=\\d))(\\d{1,3})(?:\\s*[-–—]\\s*(\\d{1,3})(?:\\s*:\\s*(\\d{1,3}))?)?)?(?!\\p{N})`,
  "giu"
);
// Chapter-only refs ('Sal 23') need a capitalised book that is not also a common Spanish word.
const CHAPTER_ONLY_EXCLUDED = new Set(["am", "os", "mi", "he", "dan", "ex", "ge", "st", "can", "mar", "pro", "act", "rev", "jon", "ose", "sa", "re", "s", "r", "p"]);
// Relative refs to the current chapter: 'v. 5', 'vv. 3-5', '(v.16)', 'versículos 3-5'.
const RE_V_ONLY = /(^|[^\p{L}\p{N}])(vv?\.?|vers[íi]culos?\.?)\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?(?!\p{N})/giu;
/** 'Jn 3:16', 'Jua 6:32', 'Ef 5:21-6:9' (backend data-raw / text fallback; verse required). */
function parseRefText(raw) {
  const m = String(raw || "").match(
    /^\s*((?:[123]\s*)?[\p{L}.]+?)\.?\s*(\d{1,3})\s*[:：.,]\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3})(?:\s*:\s*(\d{1,3}))?)?/u
  );
  if (!m) return null;
  const out = { book: m[1].replace(/\.$/, ""), ch: Number(m[2]), v1: Number(m[3]), v2: Number(m[4] || m[3]), chEnd: 0, vEnd2: 0 };
  if (m[5] && Number(m[4]) > out.ch) {
    out.chEnd = Number(m[4]);
    out.vEnd2 = Number(m[5]);
    out.v2 = out.v1;
  } else if (m[5]) out.v2 = out.v1;
  return out;
}

// ISS-19: a run starts with a letter and keeps its combining marks (decoded breathings and
// accents never cut a word in two); fewer than 2 base letters is not a word.
const RE_HEBREW = /(?=\p{L})\p{sc=Hebrew}(?:\p{sc=Hebrew}|\p{M})*/gu;
const RE_GREEK = /(?=\p{L})\p{sc=Greek}(?:(?=\p{L})\p{sc=Greek}|\p{M})*/gu;
const baseLetterCount = (w) => (String(w).match(/\p{L}/gu) || []).length;
/** Greek lookup key: NFD, no marks, lowercase, final sigma → σ (ISS-19; the backend folds too). */
function foldGreek(w) {
  return String(w || "")
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .replace(/ς/g, "σ");
}

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

  // Only real tags/comments pass through; any other '<' or '>' is text (and may sit next to a ref).
  const parts = html.split(/(<\/?[A-Za-z][^<>]*>|<!--[\s\S]*?-->)/);
  // Text inside a link the backend already made (bible-ref / strong-ref) is never wrapped again.
  const stack = [];
  let inLink = 0;
  let out = parts
    .map((part, i) => {
      if (!part) return part;
      if (i % 2 === 1) {
        const t = part.match(/^<(\/?)([A-Za-z][\w-]*)/);
        if (t && !part.endsWith("/>")) {
          const name = t[2].toLowerCase();
          if (t[1]) {
            const at = stack.map((e) => e.name).lastIndexOf(name);
            if (at >= 0) stack.splice(at).forEach((e) => e.link && inLink--);
          } else if (!VOID_TAGS.has(name)) {
            const link = /\sclass\s*=\s*"(?:[^"]*\s)?(?:bible-ref|bible-ref-raw|strong-ref)(?:\s[^"]*)?"/i.test(part);
            stack.push({ name, link });
            if (link) inLink++;
          }
        }
        return part;
      }
      if (inLink > 0) return part;
      let s = part;
      s = s.replace(RE_STRONG, (m) =>
        park(`<span class="strong-ref" data-strong="${m.toUpperCase()}">${m}</span>`)
      );
      s = s.replace(RE_REF, (m, pre, book, dot, ch, sep, v1, v2, vEnd2, offset, str) => {
        const text = m.slice(pre.length);
        const c = Number(ch);
        const bookRaw = book.replace(/\s+/g, " ");
        const num = bookNumberOf(bookRaw);
        if (!v1) {
          // Chapter-only ('Sal 23' → v.1): capitalised book, not a Spanish word, real chapter.
          const after = str.slice(offset + m.length);
          if (!/^(?:[123]|\p{Lu})/u.test(bookRaw) || CHAPTER_ONLY_EXCLUDED.has(bookKey(bookRaw))) return m;
          if (/^\p{L}/u.test(after) || /^\s*[:：]\s*\d/.test(after)) return m;
          const known = num && state.books.find((b) => b.number === num);
          if (c < 1 || (known && c > known.chapters)) return m;
          return (
            pre +
            park(`<span class="bible-ref" data-book="${escapeAttr(bookRaw)}" data-chapter="${c}" data-verse="1" data-verse-end="1" title="Ver pasaje">${text}</span>`)
          );
        }
        const vs = Number(v1);
        if (c < 1 || c > 150 || vs < 1 || vs > 176) return m;
        let attrs = `data-book="${escapeAttr(bookRaw)}" data-chapter="${c}" data-verse="${vs}"`;
        if (vEnd2 && Number(v2) > c) {
          // Cross-chapter range: the preview shows chapter c from v1 to its end (ISS-25).
          attrs += ` data-verse-end="${vs}" data-chapter-end="${Number(v2)}" data-verse-end2="${Number(vEnd2)}"`;
        } else {
          const ve = v2 && !vEnd2 && Number(v2) >= 1 ? Number(v2) : vs;
          attrs += ` data-verse-end="${ve}"`;
        }
        return pre + park(`<span class="bible-ref" ${attrs} title="Ver pasaje">${text}</span>`);
      });
      s = s.replace(RE_V_ONLY, (m, pre, _word, v1, v2) => {
        const vs = Number(v1);
        const ve = v2 ? Number(v2) : vs;
        if (vs < 1 || vs > 176) return m;
        return (
          pre +
          park(
            `<span class="bible-ref" data-book="${escapeAttr(bookNameForCtx)}" data-chapter="${context.chapter}" data-verse="${vs}" data-verse-end="${ve}" title="Ver pasaje">${m.slice(pre.length)}</span>`
          )
        );
      });
      s = s.replace(RE_HEBREW, (m) =>
        baseLetterCount(m) < 2
          ? m
          : park(`<span class="hebrew-run word-like" data-lookup="${escapeAttr(m)}">${m}</span>`)
      );
      s = s.replace(RE_GREEK, (m) =>
        baseLetterCount(m) < 2
          ? m
          : park(`<span class="greek-run word-like" data-lookup="${escapeAttr(foldGreek(m))}">${m}</span>`)
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
    const r = parseRefText(el.dataset.raw || el.textContent || "");
    if (r) {
      el.dataset.book = r.book;
      el.dataset.chapter = String(r.ch);
      el.dataset.verse = String(r.v1);
      el.dataset.verseEnd = String(r.v2);
      if (r.chEnd) {
        el.dataset.chapterEnd = String(r.chEnd);
        el.dataset.verseEnd2 = String(r.vEnd2);
      }
      el.classList.remove("bible-ref-raw");
    }
  });
}

function selectedBibleMeta() {
  return bibleMeta(state.biblePath);
}
function bibleMeta(path) {
  const m = state.modules.find((x) => x.path === path);
  return {
    path,
    abbr: m?.abbreviation || "Biblia",
    title: m?.title || "",
  };
}
/** ISS-40: the Bible a preview falls back to (preferred RVR1960, ISS-23). */
function referenceBiblePath() {
  return preferredModule("bible", modulesOf("bible"))?.path || "";
}
const passageHasText = (p) => !!p && (p.verses || []).some((v) => String(v.plain || "").trim());

/**
 * Passage text for a preview (tooltip / link panel). Tries the sidebar Bible; when it rejects or
 * has no text (NT-only Bible, other numbering), retries once with the reference Bible.
 * Resolves { passage, path, fromSidebar, missing?, sidebarAbbr? }; rejects with a Spanish message.
 */
async function fetchPassage(bookNum, chapter, verseStart, verseEnd) {
  if (!state.biblePath) throw new Error("No hay Biblia seleccionada en el panel izquierdo.");
  const sidebar = state.biblePath;
  const args = (modulePath) => ({ modulePath, book: bookNum, chapter, verseStart, verseEnd: verseEnd || verseStart });
  try {
    const passage = await invoke("get_passage", args(sidebar));
    if (passageHasText(passage)) return { passage, path: sidebar, fromSidebar: true };
  } catch {
    /* explained below */
  }
  // Why the sidebar Bible failed: the book (or chapter) is absent, or it numbers verses differently.
  const count = async (c) => Number(await invoke("get_verse_count", { modulePath: sidebar, book: bookNum, chapter: c }).catch(() => 0)) || 0;
  const missing = (await count(chapter)) > 0 ? "verse" : chapter !== 1 && (await count(1)) > 0 ? "verse" : "book";
  const sidebarAbbr = bibleMeta(sidebar).abbr;
  const ref = referenceBiblePath();
  if (ref && ref !== sidebar) {
    try {
      const passage = await invoke("get_passage", args(ref));
      if (passageHasText(passage)) return { passage, path: ref, fromSidebar: false, missing, sidebarAbbr };
    } catch {
      /* both failed */
    }
  }
  throw new Error(
    missing === "book"
      ? `Esta Biblia (${sidebarAbbr}) no contiene este libro.`
      : `Versículo no encontrado en ${sidebarAbbr} (numeración distinta).`
  );
}

/** `res` = fetchPassage result; `ref` = resolveRefElement result (for cross-chapter labels). */
function formatPassageHtml(res, bookNum, ref = null) {
  const passage = res.passage || res;
  const bible = bibleMeta(res.path || state.biblePath);
  const v0 = passage.verseStart;
  const v1 = passage.verseEnd;
  const range = v0 === v1 ? String(v0) : `${v0}–${v1}`;
  let refLabel = `${bookName(bookNum)} ${passage.chapter}:${range}`;
  let partial = "";
  if (ref && ref.chEnd > passage.chapter) {
    // ISS-25: 'Jn 3:16-4:2' — the preview shows chapter 3 from v.16 to its end.
    refLabel = `${bookName(bookNum)} ${passage.chapter}:${ref.v1}–${ref.chEnd}:${ref.vEnd2}`;
    partial = `<div class="passage-note">Se muestra hasta ${passage.chapter}:${v1}.</div>`;
  }
  const abbr = passage.abbreviation || bible.abbr;
  const title = passage.title || bible.title;
  let fallback = "";
  if (res.fromSidebar === false) {
    fallback = `<div class="passage-note passage-fallback">${escapeHtml(abbr)} — ${
      res.missing === "book" ? "no está en" : "versículo no encontrado en"
    } ${escapeHtml(res.sidebarAbbr || "")}</div>`;
  }
  const verses = (passage.verses || [])
    .map(
      (v) =>
        `<p class="passage-verse"><sup class="passage-vnum">${v.verse}</sup> ${escapeHtml(v.plain)}</p>`
    )
    .join("");
  return {
    ref: refLabel,
    bibleLabel: abbr,
    bibleTitle: title,
    fromSidebar: res.fromSidebar !== false,
    html: `
      <div class="passage-head">
        <div class="passage-ref">${escapeHtml(refLabel)}</div>
        <div class="passage-bible" title="${escapeAttr(title)}">${escapeHtml(abbr)}</div>
      </div>
      ${partial}${fallback}
      <div class="passage-body">${verses || "<p class='empty-state soft'>Sin texto en este módulo.</p>"}</div>
    `,
  };
}

// —— Interconnect word ——
/**
 * The word to look up: edge punctuation and symbols (curly quotes, *, †, °, •, —) and a
 * trailing footnote group ('nuevo[a]') are removed; inner apostrophes and 'Di-s' are kept.
 */
function cleanQueryWord(word) {
  let w = String(word ?? "").trim();
  for (let prev = null; prev !== w; ) {
    prev = w;
    w = w
      .replace(/\[[^\]\s]{1,3}\][\p{P}\p{S}\s]*$/u, "")
      .replace(/^[\p{P}\p{S}\s]+|[\p{P}\p{S}\s]+$/gu, "");
  }
  return w;
}

/**
 * Resolve a tapped/typed word (or Strong code) and sync every pane to it.
 * render:false — caller renders the study view itself (avoids nested double renders).
 */
async function interconnectWord(
  word,
  { openDict = false, stay = false, render = true, at = null, typed = false } = {}
) {
  const w = cleanQueryWord(word);
  if (!w) return;

  const seq = ++state.interconnectSeq;
  const wsAtStart = state.workspace;
  // Views that need this word while it resolves wait for it (keeping the tap context) instead
  // of resolving it again without occurrence / Bible / own tags (resolveSelectedWord).
  let doneResolving;
  const resolving = { word: w, seq, done: new Promise((r) => (doneResolving = r)) };
  const finishResolving = () => {
    if (state.wordResolving === resolving) state.wordResolving = null;
    doneResolving();
  };
  state.wordResolving = resolving;
  // Resolve in the verse the word belongs to (right-click / relaunch pass it explicitly).
  const origin =
    at && at.book && at.chapter && at.verse
      ? { book: Number(at.book), chapter: Number(at.chapter), verse: Number(at.verse) }
      : { book: state.book, chapter: state.chapter, verse: state.verse };
  // Tap context, kept with the origin so a relaunch re-sends the same lookup:
  // the Bible the word was read in (ISS-35; a Paralela column's own module), which of the
  // same words in the verse it was (ISS-02), its own Strong tags (ISS-36), typed vs tapped.
  const biblePath = (at && at.biblePath) || state.biblePath || "";
  if (biblePath) origin.biblePath = biblePath;
  if (at && Number.isInteger(at.occurrence) && at.occurrence >= 0) origin.occurrence = at.occurrence;
  const tagged = ((at && at.taggedStrongs) || []).map(strongCode).filter(Boolean);
  if (tagged.length) origin.taggedStrongs = tagged;
  if (typed || (at && at.typed)) origin.typed = true;
  state.selectedWord = w;
  state.wordLinks = null;
  state.wordOrigin = origin;
  state.lexTopic = null;
  // A typed/clicked code becomes the Strong at once; any other word clears it until resolved.
  // Only real codes count: 'H0' (from «hijo») never becomes the current Strong.
  state.lastStrong = strongCode(w);

  // Critical: search field must follow the NEW word (was sticky on previous query)
  const dq = $("#dict-query");
  if (dq) dq.value = w;

  updateContextBar();
  $("#sel-info").textContent = `Palabra: ${w}`;
  // The Comentario side pane depends on the verse, not the word: don't refetch it per word.
  const wordPane = () => (state.workspace === "comentario" ? null : refreshAnalysisPane());
  try {
    await wordPane();
  } catch (e) {
    finishResolving(); // never leave a waiting view on «Cargando…»
    throw e;
  }

  const dictPaths = modulesOf("dictionary")
    .filter((m) => !m.encrypted)
    .map((m) => m.path);
  const lexPaths = modulesOf("lexicon")
    .filter((m) => !m.encrypted)
    .map((m) => m.path);

  try {
    const args = {
      word: w,
      dictPaths,
      lexPaths,
      interlinearPath: state.interlinearPath || null,
      book: origin.book,
      chapter: origin.chapter,
      verse: origin.verse,
    };
    // Optional extras (an older backend ignores unknown keys).
    if (origin.biblePath) args.biblePath = origin.biblePath;
    if (origin.occurrence != null) args.occurrence = origin.occurrence;
    if (origin.taggedStrongs) args.taggedStrongs = origin.taggedStrongs;
    if (origin.typed) args.typed = true;
    const raw = await invoke("resolve_word", args);
    // Drop stale responses (user already clicked another word)
    if (seq !== state.interconnectSeq) return finishResolving();

    const result = preferOwnTags(raw, origin.taggedStrongs);
    if (hasField(result, "lemmaIndexUsed", "lemma_index_used")) {
      state.lemmaIndexAvailable = true;
      updateQueryPlaceholder();
    }
    state.wordLinks = Object.assign({}, result, { origin });
    const codes = (result.strongs || []).map(strongCode).filter(Boolean);
    // Several lemma-index candidates for a typed Greek/Hebrew word: let the user pick (chips).
    const ambiguous =
      fieldOf(result, "lemmaIndexUsed", "lemma_index_used") === true && !result.matchedHit && codes.length > 1;
    if (codes[0] && !ambiguous) state.lastStrong = codes[0];
    // Keep search box on the word the user actually clicked (not a different Strong topic)
    if (dq && state.workspace === "diccionario") dq.value = w;
    if (dq && state.workspace === "lexico" && state.lastStrong) {
      // Prefer Strong in lexicon tab when available
      dq.value = state.lastStrong;
    }
    finishResolving();

    updateContextBar();
    await wordPane();

    if (!render) {
      /* caller renders */
    } else if (openDict && state.workspace !== wsAtStart) {
      // The user chose another tab while the word was resolving: that choice wins (its view
      // waited for this result, see resolveSelectedWord). Never pull them back to Diccionario.
    } else if (openDict && state.workspace !== "diccionario") {
      setWorkspace("diccionario");
    } else if (openDict) {
      await loadStudyView(); // already on Diccionario: one render, no tab re-entry
    } else if (!stay && state.workspace !== "biblia") {
      await loadStudyView();
    } else if (state.workspace === "diccionario" || state.workspace === "lexico") {
      await loadStudyView();
    }
    persistStudy();
  } catch (e) {
    finishResolving();
    if (seq !== state.interconnectSeq) return;
    console.error(e);
    if (state.workspace === "comentario" && state.cmtWord) {
      state.cmtWordError = String(e); // shown in the word card; the other commentaries stay
      return;
    }
    $("#link-panel-inline").innerHTML = errorNoteHtml(e);
  }
}

/**
 * A study view needs `q` resolved. If the tap that selected it is still resolving, wait for
 * that result (same seq, same word) — re-resolving would drop its tap context and its reply.
 */
async function resolveSelectedWord(q) {
  const r = state.wordResolving;
  if (r && r.seq === state.interconnectSeq && normalizeLoose(r.word) === normalizeLoose(q)) {
    await r.done;
    return; // resolved, failed (error already shown) or superseded by a newer word: never redo it
  }
  await interconnectWord(q, { stay: true, render: false });
}

function clearWordSelection() {
  state.interconnectSeq++;
  state.cmtWord = null;
  state.selectedWord = null;
  state.wordLinks = null;
  state.wordOrigin = null;
  state.lexTopic = null;
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

// —— Error states (ISS-26) ——
/** A Spanish headline for a backend error; the raw text is kept as a small detail line. */
function friendlyError(err) {
  const raw = String((err && err.message) || err || "").trim();
  let title = "No se pudo leer este módulo.";
  if (/no such (table|column)|formato de m[oó]dulo/i.test(raw)) title = "Formato de módulo no compatible.";
  else if (/unable to open|no such file|ya no est[aá] en la carpeta/i.test(raw))
    title = "El módulo ya no está en la carpeta. Pulse «Actualizar lista de módulos» en Ajustes.";
  else if (/not a database|file is encrypted|no es un m[oó]dulo v[aá]lido/i.test(raw)) title = "El archivo no es un módulo válido.";
  return { title, detail: raw && raw !== title ? raw : "" };
}
/** Main-pane error block (side panes keep skipping failures quietly). */
function errorHtml(err, module = "") {
  const { title, detail } = friendlyError(err);
  const abbr = state.modules.find((m) => m.path === module)?.abbreviation || "";
  return `<div class="empty-state error" role="alert">
    <div class="error-title"><span class="error-icon" aria-hidden="true">⚠</span> ${escapeHtml(title)}</div>
    ${abbr ? `<div class="error-module">Módulo: <strong>${escapeHtml(abbr)}</strong></div>` : ""}
    ${detail ? `<div class="error-detail">${escapeHtml(detail.slice(0, 240))}</div>` : ""}
    <div class="error-actions">
      <button type="button" class="glass-btn compact" data-error-act="retry">Reintentar</button>
      <button type="button" class="glass-btn compact subtle" data-error-act="choose">Elegir otro módulo</button>
    </div>
  </div>`;
}
/** Side-pane variant: Spanish headline + detail, no action buttons (ISS-26). */
function errorNoteHtml(err) {
  const { title, detail } = friendlyError(err);
  return `<div class="empty-state error" role="alert">
    <div class="error-title"><span class="error-icon" aria-hidden="true">⚠</span> ${escapeHtml(title)}</div>
    ${detail ? `<div class="error-detail">${escapeHtml(detail.slice(0, 240))}</div>` : ""}
  </div>`;
}
function renderError(container, err, module = "") {
  container.innerHTML = errorHtml(err, module);
}

// —— Study views ——
function studyRenderKey(ws, module) {
  const f = state.filters || {};
  const q =
    ws === "lexico"
      ? strongCode(state.lastStrong) || state.selectedWord || ""
      : ws === "diccionario"
        ? state.selectedWord || ""
        : "";
  return `${ws}|${module}|${q}|${state.book}:${state.chapter}:${state.verse}|${f.verse ? 1 : 0}${f.chapter ? 1 : 0}${f.book ? 1 : 0}`;
}
function setStudyRefreshing(content, on) {
  content.classList.toggle("is-refreshing", on);
  if (on) content.setAttribute("aria-busy", "true");
  else content.removeAttribute("aria-busy");
}

async function loadStudyView() {
  const content = $("#study-content");
  const ws = state.workspace;
  const seq = ++state.renderSeq;
  const isCurrent = () => seq === state.renderSeq && state.workspace === ws;
  // Re-validate synchronously: a renderer never runs with another tab's module.
  let module = selectStudyModuleSync(ws);
  let loadingShown = false;

  // Diccionario with nothing saved and no probe result yet: the content-first pick usually
  // replaces the sync pick (Nelson → Vila-Escuain for «amó»). Wait for it (bounded) so the
  // first paint is not «Sin resultados» from a module that is swapped out a moment later.
  if (module && ws === "diccionario" && !savedStudyModule(ws) && !cachedStudyProbe(ws) && studyCandidates(ws).length > 1 && studyProbeQuery(ws)) {
    if (!content.children.length || state.renderedKey !== studyRenderKey(ws, module)) {
      setStudyRefreshing(content, false);
      state.renderedKey = "";
      content.innerHTML = `<div class="empty-state soft">Cargando…</div>`;
      loadingShown = true;
    }
    await Promise.race([studyProbe(ws), new Promise((r) => setTimeout(r, FIRST_PROBE_WAIT_MS))]);
    if (!isCurrent()) return;
    module = selectStudyModuleSync(ws);
  }

  if (ws !== "diccionario" || !module) showDictIndex(false);
  if (!module) {
    setStudyRefreshing(content, false);
    state.renderedKey = "";
    content.innerHTML = `<div class="empty-state soft">No hay módulos de este tipo en la carpeta.</div>`;
    return;
  }

  const renderKey = studyRenderKey(ws, module);
  if (renderKey === state.renderedKey && content.children.length) {
    setStudyRefreshing(content, true); // same query: keep the text, show a small spinner
  } else {
    setStudyRefreshing(content, false);
    state.renderedKey = "";
    if (!loadingShown) content.innerHTML = `<div class="empty-state soft">Cargando…</div>`;
  }

  try {
    if (ws === "comentario") {
      await renderCommentaryView(content, module, isCurrent);
    } else if (ws === "diccionario") {
      await renderDictionaryView(content, module, isCurrent);
    } else if (ws === "lexico") {
      await renderLexiconView(content, module, isCurrent);
    }
  } catch (e) {
    if (isCurrent()) renderError(content, e, module);
  }
  if (!isCurrent()) return;
  setStudyRefreshing(content, false);
  state.renderedKey = studyRenderKey(ws, module);
  // Rendered first; the probe only annotates the dropdown afterwards.
  fillStudyModuleSelect(ws);
}

// ISS-27: the label already says the scope in Spanish («v.16», «Cap. 3», «Libro»).
const SCOPE_ES = { verse: "Versículo", chapter: "Capítulo", book: "Libro" };
const CHAPTER_ONLY = { verse: false, chapter: true, book: false };

/** Plain text of the current Bible's verse (ISS-18: shown under the commentary banner). */
async function versePlainFor(at) {
  const d = state.chapterData;
  if (d && d.book === at.book && d.chapter === at.chapter && (!d.module || d.module === state.biblePath)) {
    const v = (d.verses || []).find((x) => x.verse === at.verse);
    return v ? v.plain || "" : "";
  }
  if (!state.biblePath) return "";
  try {
    return (await invoke("get_verse_plain", { modulePath: state.biblePath, book: at.book, chapter: at.chapter, verse: at.verse })) || "";
  } catch {
    return "";
  }
}

/**
 * Comentario open and the Bible changed: swap the verse text under the banner to the new
 * Bible's wording in place (the commentary itself does not depend on the Bible, so it is
 * not refetched). Without a verse paragraph to update, the view is rendered again.
 */
async function refreshCommentaryVerseText() {
  if (state.workspace !== "comentario") return;
  const content = $("#study-content");
  if (!content || !content.querySelector(".study-banner")) return;
  const at = { book: state.book, chapter: state.chapter, verse: state.verse };
  const bible = state.biblePath;
  const seq = state.renderSeq;
  const plain = await versePlainFor(at);
  const same =
    state.workspace === "comentario" && state.biblePath === bible && state.renderSeq === seq &&
    state.book === at.book && state.chapter === at.chapter && state.verse === at.verse;
  if (!same) return; // a newer Bible, verse or render owns the pane
  const p = content.querySelector(".study-verse-text");
  if (p && plain) p.textContent = plain;
  else if (p || plain) await loadStudyView();
}

async function renderCommentaryView(content, module, isCurrent = () => true) {
  const at = { book: state.book, chapter: state.chapter, verse: state.verse };
  const f = Object.assign({}, state.filters);
  const versePromise = versePlainFor(at);
  let entries = await invoke("get_commentary", {
    modulePath: module,
    book: at.book,
    chapter: at.chapter,
    verse: at.verse,
    filters: state.filters,
  });
  if (!isCurrent()) return;
  // ISS-16 (user decision): no note for this verse → this module's chapter commentary, labelled.
  // Book introductions only appear when «Libro» is ticked.
  let chapterFallback = false;
  if (!entries.length && f.verse && !f.chapter) {
    // A failing fallback only means «no chapter note»: the verse result already answered.
    const chapterEntries = await invoke("get_commentary", {
      modulePath: module,
      book: at.book,
      chapter: at.chapter,
      verse: at.verse,
      filters: CHAPTER_ONLY,
    }).catch(() => []);
    if (!isCurrent()) return;
    if (chapterEntries && chapterEntries.length) {
      entries = chapterEntries;
      chapterFallback = true;
    }
  }
  const versePlain = await versePromise;
  if (!isCurrent()) return;
  state.primaryCmtPlain = {
    key: `${module}|${at.book}:${at.chapter}:${at.verse}`,
    texts: new Set(entries.map((e) => collapsePlain(e.plain)).filter(Boolean)),
  };
  const abbr = state.modules.find((m) => m.path === module)?.abbreviation || "";
  const where = `${bookName(at.book)} ${at.chapter}:${at.verse}`;
  let html =
    `<div class="study-banner">Comentario · ${escapeHtml(where)}</div>` +
    (versePlain ? `<p class="study-verse-text">${escapeHtml(versePlain)}</p>` : "");
  if (!entries.length) {
    html += `<div class="empty-state soft">Sin nota para ${escapeHtml(bookAbbr(at.book))} ${at.chapter}:${at.verse} en <strong>${escapeHtml(
      abbr
    )}</strong>. Active «Capítulo» o «Libro» arriba para ver sus notas generales, o elija otro comentario.</div>`;
    content.innerHTML = html;
    return;
  }
  if (chapterFallback) {
    html += `<div class="study-fallback-note" role="note"><strong>Comentario del capítulo</strong> · Este comentario no tiene nota para el versículo ${at.verse}; se muestra su comentario del capítulo ${at.chapter}.</div>`;
  }
  html += entries
    .map(
      (e) => `
        <article class="study-entry${chapterFallback ? " is-chapter-fallback" : ""}">
          <div class="scope-label">${escapeHtml(e.label || SCOPE_ES[e.scope] || "")}</div>
          <div class="study-body">${linkifyHtml(e.html, { book: at.book, chapter: at.chapter })}</div>
        </article>`
    )
    .join("");
  content.innerHTML = html;
  normalizeBibleRefElements(content);
}

// ISS-08 + user decision: Diccionario shows exact / headword / lemma (singular) matches only.
// Verb-form expansions (repudia → REPUDIAR), multi-word 'contains' hits and multi-word
// headwords starting with the word ('phrase': «aguas» → AGUAS DE CELO; WP3 contract) are
// dropped, never listed. A missing match_kind (older backend) counts as exact.
const EXCLUDED_DICT_MATCH_KINDS = new Set(["verbform", "contains", "phrase"]);
/**
 * Accuracy round 2026-11-11: a verb or adjective has no article of its own, so the backend
 * reaches the concept through the word's Strong family (amó → AMOR, creó → CREACIÓN) and tags
 * those entries `strong`. They are labelled so they never read as the word itself.
 */
const isStrongBridgeEntry = (e) => String(fieldOf(e, "matchKind", "match_kind") || "").toLowerCase() === "strong";
function strongBridgeNote(e, wl) {
  if (!isStrongBridgeEntry(e)) return "";
  const code = ((wl && wl.strongs) || []).map(strongCode).find(Boolean) || "";
  const tip = code ? `Artículo del concepto, hallado por el Strong ${code} de «${(wl && wl.word) || ""}»` : "Artículo del concepto, hallado por el número Strong";
  return ` <span class="match-note" title="${escapeAttr(tip)}">relacionado${code ? ` · ${escapeHtml(code)}` : ""}</span>`;
}
const isShownDictEntry = (e) =>
  !EXCLUDED_DICT_MATCH_KINDS.has(String(fieldOf(e, "matchKind", "match_kind") || "").toLowerCase());

// —— Diccionario topic index, e-Sword style (user choice 2026-11-11) ——
// Beside the article, the module's topics in Spanish alphabetical order: a tap jumps to the
// exact topic or marks where the word would go («▸ pierda»); any topic opens with a click or
// the arrow keys. Big modules (24 000 topics) come in windows; «Anteriores…»/«Siguientes…»
// and scrolling near an edge load more.
const DICT_INDEX_BEFORE = 40;
const DICT_INDEX_AFTER = 80;
const DICT_INDEX_PAGE = 120;
const DICT_INDEX_MAX_ROWS = 600;

function showDictIndex(on) {
  const nav = $("#dict-index");
  const view = $("#view-study");
  if (!nav || !view) return;
  nav.classList.toggle("hidden", !on);
  view.classList.toggle("has-index", !!on);
  if (!on) state.dictIndex = null;
}

/** Load the window of `module`'s topic index around `query` and paint it. */
async function renderDictIndex(module, query, isCurrent = () => true) {
  const nav = $("#dict-index");
  if (!nav || !module) return;
  showDictIndex(true);
  const seq = ++state.dictIndexSeq;
  let slice = null;
  try {
    slice = await invoke("dictionary_index", {
      modulePath: module,
      query: query || null,
      before: DICT_INDEX_BEFORE,
      after: DICT_INDEX_AFTER,
    });
  } catch (e) {
    if (seq !== state.dictIndexSeq || !isCurrent()) return;
    state.dictIndex = null;
    nav.innerHTML = `<div class="dict-index-head">Índice</div><div class="empty-state soft">${escapeHtml(friendlyError(e).title)}</div>`;
    return;
  }
  if (seq !== state.dictIndexSeq || !isCurrent()) return;
  if (!slice) {
    nav.innerHTML = "";
    state.dictIndex = null;
    return;
  }
  state.dictIndex = {
    module,
    query: query || "",
    total: slice.total || 0,
    start: slice.start || 0,
    focus: slice.focus ?? null,
    exact: !!slice.exact,
    rows: slice.rows || [],
    selected: state.dictBrowse && state.dictBrowse.module === module ? state.dictBrowse.rowid : null,
    loading: false,
  };
  paintDictIndex({ scrollTo: "focus" });
}

const dictIndexMarker = (q) =>
  `<div class="dict-index-marker" title="${escapeAttr(`Aquí iría «${q}»: no es un tema de este diccionario`)}">▸ ${escapeHtml(q)}</div>`;

function paintDictIndex({ scrollTo = "" } = {}) {
  const nav = $("#dict-index");
  const ix = state.dictIndex;
  if (!nav || !ix) return;
  const end = ix.start + ix.rows.length;
  const marker = !ix.exact && ix.query && ix.focus != null;
  // Header outside the scrolling part: nothing slides under it, whatever the theme's background.
  let html = `<div class="dict-index-head">Índice · ${Number(ix.total).toLocaleString("es")} temas</div><div class="dict-index-scroll">`;
  if (ix.start > 0) html += `<button type="button" class="dict-index-more" data-dir="prev">▲ Anteriores…</button>`;
  html += `<div class="dict-index-list" role="listbox" aria-label="Temas del diccionario">`;
  for (const r of ix.rows) {
    if (marker && r.pos === ix.focus) html += dictIndexMarker(ix.query);
    const isFocus = ix.exact && r.pos === ix.focus;
    const selected = ix.selected != null ? r.rowid === ix.selected : isFocus;
    html += `<button type="button" class="dict-index-row${isFocus ? " focus" : ""}" role="option" aria-selected="${selected}" data-rowid="${r.rowid}" data-pos="${r.pos}" title="${escapeAttr(r.topic)}">${escapeHtml(r.topic)}</button>`;
  }
  if (marker && ix.focus >= end && end === ix.total) html += dictIndexMarker(ix.query);
  html += `</div>`;
  if (end < ix.total) html += `<button type="button" class="dict-index-more" data-dir="next">▼ Siguientes…</button>`;
  html += `</div>`;
  nav.innerHTML = html;
  nav.onclick = onDictIndexClick;
  nav.onkeydown = onDictIndexKey;
  const scroller = nav.querySelector(".dict-index-scroll");
  if (scroller) scroller.onscroll = onDictIndexScroll;
  if (scrollTo === "focus") {
    const t = nav.querySelector(".dict-index-marker, .dict-index-row.focus");
    if (t && t.scrollIntoView) t.scrollIntoView({ block: "center" });
  } else if (scrollTo) {
    const t = nav.querySelector(`.dict-index-row[data-pos="${scrollTo}"]`);
    if (t && t.scrollIntoView) t.scrollIntoView({ block: "start" });
  }
}

/** «Anteriores…» / «Siguientes…»: the next window of the same index, kept in place. */
async function loadMoreDictIndex(dir) {
  const ix = state.dictIndex;
  if (!ix || ix.loading) return;
  const end = ix.start + ix.rows.length;
  if ((dir === "prev" && ix.start <= 0) || (dir === "next" && end >= ix.total)) return;
  const anchor = dir === "prev" ? Math.max(0, ix.start - DICT_INDEX_PAGE) : end;
  ix.loading = true;
  const seq = ++state.dictIndexSeq;
  let slice = null;
  try {
    slice = await invoke("dictionary_index", {
      modulePath: ix.module,
      query: ix.query || null,
      anchor,
      before: 0,
      after: DICT_INDEX_PAGE,
    });
  } catch (_) {
    slice = null;
  }
  ix.loading = false;
  if (seq !== state.dictIndexSeq || state.dictIndex !== ix || !slice) return;
  const rows = slice.rows || [];
  const keepFrom = ix.start;
  if (dir === "prev") {
    const add = rows.filter((r) => r.pos < ix.start);
    if (!add.length) return;
    ix.rows = add.concat(ix.rows);
    ix.start = add[0].pos;
    if (ix.rows.length > DICT_INDEX_MAX_ROWS) ix.rows = ix.rows.slice(0, DICT_INDEX_MAX_ROWS);
    paintDictIndex({ scrollTo: String(keepFrom) });
  } else {
    const add = rows.filter((r) => r.pos >= end);
    if (!add.length) return;
    ix.rows = ix.rows.concat(add);
    if (ix.rows.length > DICT_INDEX_MAX_ROWS) {
      const drop = ix.rows.length - DICT_INDEX_MAX_ROWS;
      ix.rows = ix.rows.slice(drop);
      ix.start = ix.rows[0].pos;
    }
    const first = add[0].pos;
    paintDictIndex({ scrollTo: String(Math.max(ix.start, first - 4)) });
  }
}

function onDictIndexScroll() {
  const box = $("#dict-index .dict-index-scroll");
  const ix = state.dictIndex;
  if (!box || !ix || ix.loading) return;
  if (box.scrollTop < 24 && ix.start > 0) loadMoreDictIndex("prev");
  else if (box.scrollHeight - box.scrollTop - box.clientHeight < 24 && ix.start + ix.rows.length < ix.total) loadMoreDictIndex("next");
}

async function onDictIndexClick(e) {
  const more = e.target.closest(".dict-index-more");
  if (more) {
    await loadMoreDictIndex(more.dataset.dir);
    return;
  }
  const row = e.target.closest(".dict-index-row");
  if (row) await openDictTopic(Number(row.dataset.rowid), row.textContent);
}

/** ↑/↓ move through the topics and open them (like e-Sword); Home/End jump in the window. */
function onDictIndexKey(e) {
  const row = e.target.closest(".dict-index-row");
  if (!row || !["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
  const rows = $$("#dict-index .dict-index-row");
  const i = rows.indexOf(row);
  const j = e.key === "ArrowDown" ? i + 1 : e.key === "ArrowUp" ? i - 1 : e.key === "Home" ? 0 : rows.length - 1;
  e.preventDefault();
  if (j < 0) return void loadMoreDictIndex("prev");
  if (j >= rows.length) return void loadMoreDictIndex("next");
  const next = rows[j];
  next.focus();
  clearTimeout(state.dictKeyTimer);
  state.dictKeyTimer = setTimeout(() => openDictTopic(Number(next.dataset.rowid), next.textContent), 120);
}

/** Show exactly one index topic in the article column. */
async function openDictTopic(rowid, topic) {
  const ix = state.dictIndex;
  const module = (ix && ix.module) || state.studyModule;
  if (!module || !Number.isFinite(rowid)) return;
  state.dictBrowse = { module, rowid, topic };
  if (ix) {
    ix.selected = rowid;
    $$("#dict-index .dict-index-row").forEach((r) => r.setAttribute("aria-selected", String(Number(r.dataset.rowid) === rowid)));
  }
  const content = $("#study-content");
  const seq = ++state.renderSeq; // a click supersedes any study render still running
  let entry = null;
  let err = null;
  try {
    entry = await invoke("get_dictionary_entry", { modulePath: module, rowid });
  } catch (e) {
    err = e;
  }
  if (seq !== state.renderSeq || state.workspace !== "diccionario" || state.dictBrowse?.rowid !== rowid) return;
  const abbr = state.modules.find((m) => m.path === module)?.abbreviation || "";
  const q = (state.selectedWord || "").trim();
  let html = `<div class="study-banner">Diccionario · ${escapeHtml(abbr)} · ${escapeHtml(topic)}</div>`;
  if (q) html += `<div class="dict-browse-bar"><button type="button" class="glass-btn compact" id="btn-dict-back">← Volver a «${escapeHtml(q)}»</button></div>`;
  if (err) html += errorHtml(err, module);
  else if (!entry) html += `<div class="empty-state soft">Este tema no tiene texto en <strong>${escapeHtml(abbr)}</strong>.</div>`;
  else
    html += `
      <article class="study-entry">
        <div class="topic">${escapeHtml(entry.topic)}</div>
        <div class="study-body">${linkifyHtml(entry.html, { book: state.book, chapter: state.chapter })}</div>
      </article>`;
  setStudyRefreshing(content, false);
  content.innerHTML = html;
  normalizeBibleRefElements(content);
  content.scrollTop = 0;
  state.renderedKey = ""; // the word's results are not on screen any more
  const back = $("#btn-dict-back");
  if (back)
    back.onclick = () => {
      state.dictBrowse = null;
      loadStudyView();
    };
}

async function renderDictionaryView(content, module = state.studyModule, isCurrent = () => true) {
  // Prefer live selection over stale input
  const q = (state.selectedWord || $("#dict-query").value || "").trim();
  state.dictBrowse = null;
  if (!q) {
    content.innerHTML = `<div class="empty-state soft">Escriba una palabra en español (p. ej. Jehová, Jerusalén), haga clic en una palabra de la Biblia o elija un tema del índice.<br/>El panel derecho mostrará <strong>otros</strong> diccionarios.</div>`;
    await renderDictIndex(module, "", isCurrent);
    return;
  }
  $("#dict-query").value = q;

  if (!state.wordLinks || normalizeLoose(state.wordLinks.word) !== normalizeLoose(q)) {
    await resolveSelectedWord(q);
    if (!isCurrent()) return;
  }

  const wl = state.wordLinks;
  const abbr = state.modules.find((m) => m.path === module)?.abbreviation || "";
  let html = `<div class="study-banner">Diccionario · ${escapeHtml(abbr)} · «${escapeHtml(q)}»</div>`;

  html += strongBarHtml(
    wl,
    ` <button type="button" class="glass-btn compact" id="btn-jump-lex-from-dict">Ir a Léxico</button>`
  );

  // PRIMARY module only (others go to the side panel)
  let entries = [];
  let lookupError = null;
  const bridgeArgs = {};
  const tapStrongs = ((wl && wl.strongs) || []).map(strongCode).filter(Boolean).slice(0, 2);
  if (tapStrongs.length) {
    bridgeArgs.strongs = tapStrongs;
    bridgeArgs.lexPaths = modulesOf("lexicon")
      .filter((m) => !m.encrypted)
      .map((m) => m.path);
  }
  try {
    entries = await invoke("lookup_dictionary", {
      modulePath: module,
      query: q,
      limit: 40,
      ...bridgeArgs,
    });
  } catch (e) {
    entries = [];
    lookupError = e; // ISS-26: a failure is not «Sin resultados»
  }
  if (!isCurrent()) return;

  entries = (entries || []).filter(isShownDictEntry);
  if (lookupError) {
    html += errorHtml(lookupError, module);
  } else if (!entries.length) {
    html += `<div class="empty-state soft">Sin resultados en <strong>${escapeHtml(
      abbr
    )}</strong> para «${escapeHtml(
      q
    )}». El índice de la izquierda marca dónde iría: elija un tema con un clic, cambie el módulo arriba o revise el panel derecho (otros diccionarios).</div>`;
  } else {
    html += entries
      .map(
        (e) => `
      <article class="study-entry">
        <div class="topic">${escapeHtml(e.topic)}${strongBridgeNote(e, wl)}</div>
        <div class="study-body">${linkifyHtml(e.html, { book: state.book, chapter: state.chapter })}</div>
      </article>`
      )
      .join("");
  }

  content.innerHTML = html;
  normalizeBibleRefElements(content);
  const jump = $("#btn-jump-lex-from-dict");
  if (jump) jump.onclick = () => setWorkspace("lexico");
  await renderDictIndex(module, q, isCurrent);
  if (!isCurrent()) return;
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
  // Only real codes: the backend's 'H0'/'G0' never reach the card (ISS-03/04).
  const strong = hitCodes(hit)[0] || primaryStrongs(wl)[0] || strongCode(wl.word);
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
  const o = wl.origin || state; // the verse the word was tapped in, not the current one
  const loc = `${bookName(o.book)} ${o.chapter}:${o.verse}`;

  // Related Spanish glosses for same Strong in this verse
  const relatedEs = (wl.interlinearHits || [])
    .filter((h) => strong && hitCodes(h).includes(strong))
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
      ${wl.interlinearUsed && (wl.interlinearHits || []).length ? `<span class="lex-badge">Interlineal</span>` : ""}
    </div>
    <div class="lex-card-pair">
      <div class="lex-col">
        <div class="lex-col-label">${escapeHtml(originalLabel)}</div>
        <div class="lex-original ${script === "hebrew" ? "dir-rtl" : ""}" ${
          strong ? `data-strong="${escapeAttr(strong)}" role="button" tabindex="0" title="Ver ${escapeAttr(strong)} en el léxico"` : ""
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

async function renderLexiconView(content, module = state.studyModule, isCurrent = () => true) {
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
    await resolveSelectedWord(q);
    if (!isCurrent()) return;
  }
  const wl = state.wordLinks;
  const abbr = state.modules.find((m) => m.path === module)?.abbreviation || "";

  let html = `<div class="study-banner">Léxico · ${escapeHtml(abbr)}</div>`;

  // ★ Hebrew/Greek LEFT · Spanish RIGHT card
  html += buildLexiconInterlinearCard(wl);

  // ISS-04: full definitions only for the current Strong and the matched unit's codes; every
  // other code the backend returned (G25 → G250, G251…) is a «Relacionados» chip.
  const primary = primaryStrongs(wl);
  const related = relatedStrongs(wl, primary);
  html += strongBarHtml(wl);

  // PRIMARY module definition(s). Without any code, the word itself is looked up (Vine-style
  // Spanish lemmas); a code-shaped but invalid query ('H0') is not.
  const queries = primary.slice();
  if (!queries.length && !/^[HG]\s*\d+$/i.test(q)) queries.push(q);
  const seen = new Set();
  const blocks = [];

  // A side card with a non-Strong topic (DBADV, Chávez…) shows that exact entry first.
  const topic = state.lexTopic && state.lexTopic.path === module ? state.lexTopic.topic : "";
  if (topic) {
    try {
      const entry = await invoke("lookup_lexicon", { modulePath: module, strong: topic });
      if (!isCurrent()) return;
      if (entry) {
        seen.add(entry.topic);
        blocks.push(entry);
      }
    } catch {
      /* fall through to the Strong lookups */
    }
  }

  let lookupError = null;
  let lookupOk = 0;
  for (const code of queries) {
    try {
      const entry = await invoke("lookup_lexicon", {
        modulePath: module,
        strong: code,
      });
      lookupOk++;
      if (lexEntryMatches(entry, code)) {
        const key = entry.topic;
        if (!seen.has(key)) {
          seen.add(key);
          blocks.push(entry);
        }
      }
    } catch (e) {
      lookupError = lookupError || e; // shown only if no lookup in this module worked (ISS-26)
    }
    if (!isCurrent()) return;
  }

  const noStrong = !primary.length && !related.length;
  if (!blocks.length && lookupError && !lookupOk) {
    html += errorHtml(lookupError, module);
  } else if (!blocks.length && noStrong && !state.interlinearPath && !strongCode(q)) {
    // ISS-44: without an interlinear a Spanish word cannot be linked to a Strong number.
    html += `<div class="empty-state soft">No se encontró un interlineal en la carpeta; el toque de palabras no puede enlazar números Strong. Escriba un número Strong (p. ej. G26 o H430) o añada un interlineal (iRV 1960+) a la carpeta de módulos.</div>`;
  } else if (!blocks.length && noStrong && wl && wl.interlinearUsed && !strongCode(q)) {
    // ISS-02: the interlinear was read, but this word has no Strong there.
    html += wl.origin && wl.origin.typed
      ? `<div class="empty-state soft">Sin número Strong para «${escapeHtml(
          wl.word || q
        )}» en el interlineal de este versículo. Escriba un número Strong (p. ej. G26 o H430) para buscarlo directamente.</div>`
      : `<div class="empty-state soft">Sin número Strong en el interlineal para esta palabra (traducción distinta).</div>`;
  } else if (!blocks.length && !primary.length && related.length) {
    html += `<div class="empty-state soft">Esta palabra corresponde a varios números Strong. Elija uno arriba.</div>`;
  } else if (!blocks.length) {
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
  // .original-chip clicks are handled once by #study-content.onclick (class word-like).

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
  let chEnd = Number(el.dataset.chapterEnd || 0);
  let vEnd2 = Number(el.dataset.verseEnd2 || 0);

  // Fallback: parse data-raw or text content
  if ((!bookRaw || !ch || !v1) && (el.dataset.raw || el.textContent)) {
    const raw = el.dataset.raw || el.textContent;
    const r = parseRefText(raw);
    if (r) {
      ({ book: bookRaw, ch, v1, v2, chEnd, vEnd2 } = r);
    } else {
      // "v. 5" already has book/chapter from linkify
      const mv = String(raw).match(/v+\.?\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?/i);
      if (mv && bookRaw) {
        v1 = Number(mv[1]);
        v2 = Number(mv[2] || mv[1]);
      }
    }
  }
  if (!(chEnd > ch)) {
    chEnd = 0;
    vEnd2 = 0;
  } else {
    v2 = 999; // chapter ch from v1 to its end (get_passage clips; no end-chapter API)
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
  // e-Sword / Spanish abbreviations an older backend does not know ('Jua', 'Apo', '1 Co'…)
  if (!bookNum && bookRaw) bookNum = bookNumberOf(bookRaw);
  return { bookNum, ch, v1, v2, chEnd, vEnd2 };
}

async function tooltipForTarget(el) {
  if (el.classList.contains("bible-ref")) {
    if (!state.biblePath) {
      return `<div class="tip-title">${escapeHtml(el.textContent)}</div><div class="tip-muted">Seleccione una Biblia a la izquierda.</div>`;
    }
    const ref = await resolveRefElement(el);
    const { bookNum, ch, v1, v2 } = ref;
    if (!bookNum) {
      return `<div class="tip-title">${escapeHtml(el.textContent)}</div><div class="tip-muted">Referencia no reconocida</div>`;
    }
    try {
      const res = await fetchPassage(bookNum, ch, v1, v2);
      const fmt = formatPassageHtml(res, bookNum, ref);
      return `<div class="tip-passage">${fmt.html}<div class="tip-hint">${
        fmt.fromSidebar ? "Clic para leer el pasaje completo" : "Clic para ver el detalle"
      } · ${escapeHtml(fmt.bibleLabel)}</div></div>`;
    } catch (e) {
      return `<div class="tip-title">${escapeHtml(el.textContent)}</div><div class="tip-muted">${escapeHtml(
        e && e.message ? e.message : String(e)
      )}</div>`;
    }
  }
  if (el.classList.contains("strong-ref") || el.classList.contains("chip-strong")) {
    const code = (el.dataset.strong || el.textContent || "").toUpperCase();
    try {
      const entry = await invoke("lookup_lexicon_multi", { modulePaths: tooltipLexPaths(), strong: code });
      if (!entry) return `<div class="tip-title">${escapeHtml(code)}</div><div>No encontrado</div>`;
      return lexTooltipHtml(entry);
    } catch (e) {
      return `<div class="tip-title">${escapeHtml(code)}</div>`;
    }
  }
  return null;
}

/**
 * ISS-45: lexicons for the hover preview — the one chosen in Léxico, then the preferred Spanish
 * Strong lexicons (ISS-23), then the rest; English AHLB last. state.modules is not re-sorted.
 */
function tooltipLexPaths() {
  const chosen = savedStudyModule("lexico");
  const rest = modulesOf("lexicon")
    .filter((m) => !m.encrypted && m.path !== chosen)
    .map((m, i) => ({ m, i, r: preferenceRank(m, "lexicon") }))
    .sort((a, b) => a.r - b.r || a.i - b.i)
    .map((x) => x.m.path);
  return chosen ? [chosen, ...rest] : rest;
}
/** First line 'lemma · transliteration — glosses' when the entry starts with the original word. */
function lexTooltipHtml(entry) {
  const plain = String(entry.plain || "").replace(/\s+/g, " ").trim();
  const m = plain.match(/^(?:→\s*)?((?:[\p{sc=Greek}\p{sc=Hebrew}]|\p{M})+)\s*([\p{sc=Latin}\p{M}'’-]+)?/u);
  let head = "";
  let body = plain;
  if (m && baseLetterCount(m[1]) >= 1) {
    const rest = plain.slice(m[0].length).replace(/^[\s;,]+/, "");
    const gloss = (rest.match(/—\s*([^—]+?)\s*\.?\s*$/) || [])[1] || "";
    head = `<div class="tip-lemma"><span class="tip-orig">${escapeHtml(m[1].trim())}</span>${
      m[2] ? ` · ${escapeHtml(m[2])}` : ""
    }${gloss ? ` — ${escapeHtml(gloss)}` : ""}</div>`;
    body = rest;
  }
  return `<div class="tip-title">${escapeHtml(entry.topic)}</div>${head}<div>${escapeHtml(body.slice(0, 280))}${
    body.length > 280 ? "…" : ""
  }</div>`;
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
    const ref = await resolveRefElement(el);
    const { bookNum, ch, v1, v2 } = ref;
    title.textContent = el.textContent;
    if (!state.biblePath) {
      body.innerHTML = `<p>Seleccione una Biblia en el panel izquierdo para previsualizar el pasaje.</p>`;
      openPanel("#panel-link");
      return;
    }
    if (bookNum) {
      let fromSidebar = false;
      try {
        const res = await fetchPassage(bookNum, ch, v1, v2);
        const fmt = formatPassageHtml(res, bookNum, ref);
        fromSidebar = fmt.fromSidebar;
        body.innerHTML = `${fmt.html}<p class="tip-muted" style="margin-top:10px">Texto de: <strong>${escapeHtml(
          fmt.bibleLabel
        )}</strong>${fmt.bibleTitle ? ` — ${escapeHtml(fmt.bibleTitle)}` : ""}</p>`;
      } catch (e) {
        body.innerHTML = `<p>${escapeHtml(e && e.message ? e.message : String(e))}</p>`;
      }
      state.linkTarget = { type: "ref", book: bookNum, chapter: ch, verse: v1 };
      // ISS-40: 'Ir a pasaje' only when the sidebar Bible itself has the passage.
      if (fromSidebar) {
        btnGo.classList.remove("hidden");
        btnGo.textContent = "Ir a pasaje en Biblia";
      }
    } else {
      body.innerHTML = `<p>No se pudo resolver la referencia «${escapeHtml(el.textContent)}».</p>`;
    }
    openPanel("#panel-link");
    return;
  }

  if (el.classList.contains("strong-ref") || el.classList.contains("chip-strong")) {
    const code = (el.dataset.strong || el.textContent || "").toUpperCase();
    title.textContent = code;
    state.lastStrong = strongCode(code);
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
  if ($("#btn-verse-prev")) $("#btn-verse-prev").onclick = () => stepVerse(-1);
  if ($("#btn-verse-next")) $("#btn-verse-next").onclick = () => stepVerse(1);
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
      state.crossRefChosen = true; // only an explicit pick is remembered
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
    await refreshCommentaryVerseText();
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
    // ISS-15: the new primary must leave 'Otros comentarios' (and the old one may join it).
    if (state.workspace === "comentario") await refreshAnalysisPane();
  };
  const onFilter = (name) => async (e) => {
    state.filters[name] = e.target.checked;
    persistStudy();
    await loadStudyView();
    if (state.workspace === "comentario") await refreshAnalysisPane(); // ISS-15: never stale
  };
  $("#flt-verse").onchange = onFilter("verse");
  $("#flt-chapter").onchange = onFilter("chapter");
  $("#flt-book").onchange = onFilter("book");
  $("#btn-dict-search").onclick = async () => {
    const q = $("#dict-query").value.trim();
    // Typed words are not tied to a word of the open verse (ISS-02 typed mode).
    if (q) await interconnectWord(q, { stay: true, render: false, typed: true });
    await loadStudyView();
  };
  $("#dict-query").onkeydown = (e) => {
    if (e.key === "Enter") $("#btn-dict-search").click();
  };
  // e-Sword: the topic index follows what is typed (Enter still runs the full search).
  $("#dict-query").oninput = () => {
    if (state.workspace !== "diccionario") return;
    clearTimeout(state.dictIndexTimer);
    state.dictIndexTimer = setTimeout(() => {
      const module = (state.dictIndex && state.dictIndex.module) || state.studyModule;
      if (state.workspace === "diccionario" && module) renderDictIndex(module, $("#dict-query").value.trim());
    }, 150);
  };

  $("#notes-side").oninput = (e) => {
    state.notes[noteKey()] = e.target.value;
    persistNotes();
  };

  $("#btn-clear-word").onclick = () => clearWordSelection();

  // Strong chips / mini entries (side analysis → promote module to primary)
  document.addEventListener("click", async (e) => {
    // #study-content has its own handler; handling its chips here too doubled every lookup.
    if (e.target.closest("#study-content")) return;
    const chip = e.target.closest(".chip-strong");
    if (chip) {
      e.preventDefault();
      const code = chip.dataset.strong;
      await interconnectWord(code, { stay: true, render: false });
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
        const topic = mini.dataset.topic || "";
        const code = strongCode(topic);
        if (code) state.lastStrong = code;
        // Non-Strong topics are shown as-is (never re-resolved as if they were a tapped word).
        state.lexTopic = topic && !code ? { path: mini.dataset.path, topic } : null;
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
      if (await loadModules(dir)) closePanels();
    } catch (e) {
      alertSoft("Carpeta", String(e));
    }
  }
  $("#btn-pick-modules").onclick = async () => {
    try {
      const dir = await invoke("pick_modules_dir");
      if (dir && (await loadModules(dir))) closePanels();
    } catch (e) {
      alertSoft("Diálogo de carpeta", `${String(e)}\n\nPegue la ruta y use «Usar esta ruta».`);
    }
  };
  $("#btn-apply-path").onclick = () => applyModulesPath($("#modules-path").value);
  // ISS-31: the backend finds «for mac espada» for this user (no hard-coded home folder).
  $("#btn-use-default-espada").onclick = async () => {
    let dir = null;
    try {
      dir = await invoke("get_default_modules_dir");
    } catch {
      dir = null;
    }
    if (dir) return applyModulesPath(dir);
    const meta = $("#modules-meta");
    if (meta) meta.textContent = "No se encontró «for mac espada» en Descargas. Elija la carpeta con «Elegir…».";
  };
  $("#btn-refresh-modules").onclick = async () => {
    if (!state.modulesDir) return;
    try {
      await invoke("refresh_modules", { modulesDir: state.modulesDir });
      // A session-only fallback folder stays session-only: the saved (unmounted) path is kept.
      await loadModules(state.modulesDir, { persist: !state.modulesDirSessionOnly });
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
  /** Make the verse containing `el` the current (and only selected) verse. */
  const selectVerseOf = (el) => {
    const verseEl = el.closest(".verse, tr");
    if (!verseEl?.dataset.verse) return;
    state.verse = Number(verseEl.dataset.verse);
    state.selectedVerses = new Set([state.verse]);
    $$(".verse", bible).forEach((v) => {
      v.classList.toggle("selected", Number(v.dataset.verse) === state.verse);
    });
    persistNav();
    updateLocationLabel();
  };
  bible.onclick = async (e) => {
    const errAct = e.target.closest("[data-error-act]");
    if (errAct) {
      e.preventDefault();
      if (errAct.dataset.errorAct === "retry") await reloadChapter();
      else $("#sel-bible")?.focus();
      return;
    }
    const strong = e.target.closest(".strong-ref");
    if (strong) {
      e.preventDefault();
      selectVerseOf(strong); // the code is resolved in its own verse
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
      return;
    }
    const word = e.target.closest(".word, .word-like");
    if (word) {
      $$(".word.word-selected, .word-like.word-selected").forEach((w) =>
        w.classList.remove("word-selected")
      );
      word.classList.add("word-selected");
      const text = (word.dataset.lookup || word.textContent || "").trim();
      selectVerseOf(word);
      // Always treat as a NEW query (clears stale word / search box). The tap context says
      // which Bible, which occurrence and which own Strong tags this word has.
      const at = word.classList.contains("word") ? tapContext(word) : null;
      await interconnectWord(text, { openDict: true, at });
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
    }
  };

  bible.oncontextmenu = (e) => {
    state.ctxWord = null;
    const verse = e.target.closest(".verse");
    if (verse?.dataset.verse) {
      const v = Number(verse.dataset.verse);
      if (!state.selectedVerses.has(v)) {
        // Keep a multi-selection (Copiar/Resaltar act on it); otherwise select this verse.
        state.selectedVerses = new Set([v]);
        state.verse = v;
        $$(".verse", bible).forEach((el) => {
          el.classList.toggle("selected", Number(el.dataset.verse) === v);
        });
        persistNav();
        updateLocationLabel();
      }
      // 'Buscar palabra' looks up THIS word in ITS verse (never a stale selection).
      const word = e.target.closest(".word");
      if (word) state.ctxWord = { text: word.textContent.trim(), verse: v, at: tapContext(word) };
    }
    e.preventDefault();
    showCtx(e.clientX, e.clientY);
  };

  // Study content clicks
  $("#study-content").onclick = async (e) => {
    const act = e.target.closest("[data-error-act]");
    if (act) {
      e.preventDefault();
      if (act.dataset.errorAct === "retry") {
        state.renderedKey = "";
        await loadStudyView();
      } else $("#sel-study-module")?.focus();
      return;
    }
    const el = e.target.closest(".bible-ref, .strong-ref, .chip-strong, .word-like, .lex-original[data-strong]");
    if (!el) return;
    e.preventDefault();
    // ISS-19: in Comentario a word or Strong tap is answered in the side pane; the commentary
    // keeps its text and scroll position (no re-render, no tab switch).
    if (state.workspace === "comentario" && (el.classList.contains("word-like") || el.classList.contains("strong-ref"))) {
      const q = el.classList.contains("strong-ref")
        ? (el.dataset.strong || el.textContent || "").toUpperCase()
        : el.dataset.lookup || el.textContent;
      const label = el.classList.contains("word-like") ? el.textContent : "";
      $$(".word-like.word-selected, .strong-ref.word-selected", $("#study-content")).forEach((x) => x.classList.remove("word-selected"));
      el.classList.add("word-selected");
      await lookupWordInCommentary(q, label);
      return;
    }
    if (el.classList.contains("word-like")) {
      await interconnectWord(el.dataset.lookup || el.textContent, { stay: true, render: false });
      await loadStudyView();
      return;
    }
    // ISS-33: the big Hebrew/Greek word on the lexicon card takes the same path as a Strong chip.
    if (el.classList.contains("chip-strong") || el.classList.contains("strong-ref") || el.classList.contains("lex-original")) {
      const code = (el.dataset.strong || el.textContent || "").toUpperCase();
      await interconnectWord(code, { stay: true, render: false });
      if (state.workspace !== "lexico") setWorkspace("lexico");
      else await loadStudyView();
      return;
    }
    await openLinkPanel(el);
  };

  // ISS-33: role="button" elements of the study pane answer Enter and Space like a click.
  $("#study-content").addEventListener("keydown", (e) => {
    if (e.key !== "Enter" && e.key !== " ") return;
    const el = e.target.closest && e.target.closest(".lex-original[data-strong]");
    if (!el) return;
    e.preventDefault();
    el.click();
  });

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
      const cw = state.ctxWord;
      state.ctxWord = null;
      const w = cw?.text || window.getSelection()?.toString()?.trim();
      const at = cw ? cw.at || { book: state.book, chapter: state.chapter, verse: cw.verse } : null;
      if (w) await interconnectWord(w, { openDict: true, at });
    }
  };

  document.addEventListener("click", (e) => {
    if (!e.target.closest("#ctx-menu")) hideCtx();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      closePanels();
      hideCtx();
      return;
    }
    // ISS-18: verse stepping in Comentario. Never while typing, choosing in a select, or with a
    // panel open; ↑/↓ alone keep scrolling long notes.
    if (state.workspace !== "comentario" || e.metaKey || e.ctrlKey || e.shiftKey) return;
    const t = e.target;
    if (t && typeof t.closest === "function" && t.closest("input, textarea, select, [contenteditable]")) return;
    if (!$("#overlay").classList.contains("hidden")) return;
    let delta = 0;
    if (e.altKey && e.key === "ArrowDown") delta = 1;
    else if (e.altKey && e.key === "ArrowUp") delta = -1;
    else if (!e.altKey && e.key === "ArrowRight") delta = 1;
    else if (!e.altKey && e.key === "ArrowLeft") delta = -1;
    if (!delta) return;
    e.preventDefault();
    stepVerse(delta);
  });
}

// —— Init ——
async function init() {
  applyTheme(localStorage.getItem(LS.theme) || "true-dark");
  // Theme grid is rebuilt when 🎨 opens the picker panel
  renderThemeGrid();
  renderFontGrid();
  applyReadFont(localStorage.getItem(LS.readFont) || "system");

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
    state.lastStrong = strongCode(study.lastStrong); // a saved 'H0' is ignored
    state.wordOrigin = study.wordOrigin || null;
    state.crossRefSourcePath = study.crossRefSourcePath || "";
    // Older versions saved their automatic pick (NASB); only an explicit choice is kept now.
    state.crossRefChosen = study.crossRefChosen === true;
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

  // ISS-31: the saved folder, else the backend's default; a fallback used because the saved
  // folder is missing (unmounted drive) is loaded for this session only — the saved path stays.
  const saved = localStorage.getItem(LS.modulesDir) || "";
  let defaultDir = "";
  try {
    defaultDir = (await invoke("get_default_modules_dir")) || "";
  } catch {
    defaultDir = "";
  }
  let dir = saved || defaultDir;
  let persist = true;
  if (saved) {
    let usable = false;
    try {
      const test = await invoke("list_modules", { modulesDir: saved });
      usable = !!(test && test.length); // a valid but empty folder still falls back
    } catch {
      usable = false;
    }
    if (!usable) {
      dir = defaultDir && defaultDir !== saved ? defaultDir : "";
      persist = false;
    }
  }

  // The study view renders once, from setWorkspace below (not also from loadModules).
  const loaded = dir ? await loadModules(dir, { refreshStudy: false, persist }) : false;
  if (!loaded) {
    const meta = $("#modules-meta");
    if (meta) {
      meta.textContent = saved
        ? `No se encontró la carpeta de módulos guardada (${saved}). Conecte el disco o elija otra carpeta.`
        : "Elija la carpeta de módulos e-Sword (p. ej. «for mac espada» en Descargas).";
    }
    if (saved) $("#modules-path").value = saved;
    openPanel("#panel-settings");
  }

  // Relaunch with a remembered word: resolve it once, in the verse it came from
  // (otherwise the side pane waits on «Buscando…» forever).
  if (state.selectedWord && !state.wordLinks && state.modules.length) {
    await interconnectWord(state.selectedWord, { stay: true, render: false, at: state.wordOrigin });
  }

  setWorkspace(state.workspace === "biblia" ? "biblia" : state.workspace, { skipReload: false });
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => setTimeout(init, 50));
} else {
  setTimeout(init, 50);
}
