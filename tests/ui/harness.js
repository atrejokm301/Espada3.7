/*
 * Espada3.7 UI test harness for JavaScriptCore (jsc shell).
 *
 * Runs the real ui/app.v7.js (unmodified structure) against:
 *  - a small DOM built by parsing the real ui/index.html (selectors, bubbling events, innerHTML),
 *  - a virtual clock (setTimeout / requestAnimationFrame) + microtask draining,
 *  - an IPC mock (window.__TAURI__.core.invoke) answered from fixtures recorded from the real
 *    Rust backend (tests/ui/fixtures/<set>.json, produced by tests/ui/recorder/record.sh).
 *
 * Delivery modes:
 *  - "fifo" (default): replies in issue order, serialized like today's sync commands on the
 *    macOS main thread (end = max(now, busyUntil) + latency).
 *  - "hold": replies are queued; settle({order:"reverse"}) / release("reverse") delivers them
 *    newest-first, emulating async commands finishing out of order.
 *
 * One scenario per jsc process (app.v7.js declares top-level consts): run.sh runs
 *   jsc harness.js test_x.js _run.js -- <scenario>
 */
"use strict";
var window = globalThis;
var TESTS_DIR = typeof HARNESS_TESTS_DIR === "string" ? HARNESS_TESTS_DIR : ".";
// The app UI lives in <repo>/ui; these tests live in <repo>/tests/ui (outside frontendDist).
var UI_DIR = typeof HARNESS_UI_DIR === "string" && HARNESS_UI_DIR ? HARNESS_UI_DIR : TESTS_DIR + "/../../ui";
// FIXTURE_DIR=/path (run.sh / record.sh) keeps the recorded fixtures outside ui/ (ui/ is the
// app's frontendDist: everything in it is embedded in the release binary).
var FIXTURE_DIR = typeof HARNESS_FIXTURE_DIR === "string" && HARNESS_FIXTURE_DIR ? HARNESS_FIXTURE_DIR : TESTS_DIR + "/fixtures";
var MODE = typeof HARNESS_MODE === "string" ? HARNESS_MODE : "test"; // test | collect

var H = {
  now: 0,
  timers: [],
  tid: 0,
  busyUntil: 0,
  calls: [], // every invoke, in issue order
  pending: [], // held replies (hold mode)
  delivery: "fifo",
  writes: [], // innerHTML writes to tracked containers
  tracked: new Set(["study-content", "link-panel-inline", "bible-content", "sel-study-module"]),
  missing: [],
  requests: [], // collect mode: fixture requests
  overrides: {},
  consoleErrors: [],
  uncaught: [],
  selectionText: "",
  fixture: null,
  fixtureName: "",
  LAT: {
    get_books: 1, get_default_modules_dir: 1, list_modules: 20, find_interlinear: 20,
    get_bible_chapter: 5, resolve_word: 100, probe_modules_content: 150, lookup_dictionary: 10,
    lookup_lexicon: 5, get_commentary: 5, lookup_lexicon_multi: 10, get_passage: 3,
  },
};

// ───────────────────────── console / timers ─────────────────────────
var console = {
  log: (...a) => print(a.map(String).join(" ")),
  info: () => {},
  debug: () => {},
  warn: () => {},
  error: (...a) => H.consoleErrors.push(a.map((x) => (x && x.stack) || String(x)).join(" ")),
};
function setTimeout(fn, ms, ...args) {
  const id = ++H.tid;
  H.timers.push({ t: H.now + Math.max(0, Number(ms) || 0), id, fn, args });
  return id;
}
function clearTimeout(id) {
  H.timers = H.timers.filter((t) => t.id !== id);
}
var setInterval = () => 0;
var clearInterval = () => {};
function requestAnimationFrame(fn) {
  return setTimeout(() => fn(H.now), 16);
}
var cancelAnimationFrame = clearTimeout;
function runTimer(t) {
  H.now = Math.max(H.now, t.t);
  try {
    track(t.fn(...(t.args || [])));
  } catch (e) {
    H.uncaught.push(e);
  }
  drainMicrotasks();
}
function nextTimer() {
  if (!H.timers.length) return null;
  H.timers.sort((a, b) => a.t - b.t || a.id - b.id);
  return H.timers.shift();
}
/** Run everything (timers, IPC replies) until nothing is left. In hold mode, releases held replies in `order`. */
function settle({ order = "fifo", maxSteps = 200000 } = {}) {
  drainMicrotasks();
  for (let i = 0; i < maxSteps; i++) {
    const t = nextTimer();
    if (t) {
      runTimer(t);
      continue;
    }
    if (H.pending.length) {
      release(order);
      continue;
    }
    return;
  }
  throw new Error("settle: did not become idle");
}
/** Advance the virtual clock by ms, running due timers (held IPC replies stay held). */
function advance(ms) {
  const until = H.now + ms;
  drainMicrotasks();
  for (;;) {
    H.timers.sort((a, b) => a.t - b.t || a.id - b.id);
    if (!H.timers.length || H.timers[0].t > until) break;
    runTimer(H.timers.shift());
  }
  H.now = until;
}
function track(ret) {
  if (ret && typeof ret.then === "function") ret.then(null, (e) => H.uncaught.push(e));
  return ret;
}

// ───────────────────────── HTML entities ─────────────────────────
var NAMED = {
  amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " ", ndash: "–", mdash: "—", hellip: "…",
  laquo: "«", raquo: "»", iexcl: "¡", iquest: "¿", copy: "©", reg: "®", sect: "§", para: "¶", middot: "·",
  lsquo: "‘", rsquo: "’", ldquo: "“", rdquo: "”", aacute: "á", eacute: "é", iacute: "í", oacute: "ó",
  uacute: "ú", Aacute: "Á", Eacute: "É", Iacute: "Í", Oacute: "Ó", Uacute: "Ú", ntilde: "ñ", Ntilde: "Ñ",
  uuml: "ü", Uuml: "Ü", deg: "°", dagger: "†",
};
// Names a browser also decodes WITHOUT the trailing ';' in text (subset of the legacy table).
var LEGACY = new Set(["amp", "lt", "gt", "quot", "nbsp", "copy", "reg", "sect", "para", "middot", "laquo", "raquo",
  "iexcl", "iquest", "aacute", "eacute", "iacute", "oacute", "uacute", "Aacute", "Eacute", "Iacute", "Oacute",
  "Uacute", "ntilde", "Ntilde", "uuml", "Uuml", "deg"]);
function decodeEntities(s) {
  return String(s).replace(/&(#\d+;?|#[xX][0-9a-fA-F]+;?|[A-Za-z][A-Za-z0-9]*;?)/g, (m, body) => {
    if (body[0] === "#") {
      const semi = body.endsWith(";");
      const num = body.slice(1, semi ? -1 : undefined);
      const cp = num[0] === "x" || num[0] === "X" ? parseInt(num.slice(1), 16) : parseInt(num, 10);
      return Number.isFinite(cp) && cp > 0 && cp < 0x110000 ? String.fromCodePoint(cp) : m;
    }
    if (body.endsWith(";")) {
      const n = body.slice(0, -1);
      return n in NAMED ? NAMED[n] : m;
    }
    // no semicolon: longest legacy prefix
    for (let k = body.length; k > 1; k--) {
      const n = body.slice(0, k);
      if (LEGACY.has(n)) return NAMED[n] + body.slice(k);
    }
    return m;
  });
}
function escText(s) {
  return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/ /g, "&nbsp;");
}
function escAttr(s) {
  return String(s).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/ /g, "&nbsp;");
}

// ───────────────────────── DOM ─────────────────────────
var VOID = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"]);
var RAW = new Set(["script", "style", "textarea", "title"]);

class TextNode {
  constructor(data) {
    this.nodeType = 3;
    this.data = data;
    this.parentNode = null;
  }
  get textContent() {
    return this.data;
  }
  set textContent(v) {
    this.data = String(v);
  }
  get nodeValue() {
    return this.data;
  }
  closest() {
    return null;
  }
}

function camelToData(k) {
  return "data-" + k.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
}
function dataToCamel(n) {
  return n.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
}

class Element {
  constructor(tag) {
    this.nodeType = 1;
    this.localName = String(tag).toLowerCase();
    this.tagName = this.localName.toUpperCase();
    this.attrs = new Map();
    this.childNodes = [];
    this.parentNode = null;
    this.listeners = {};
    this.style = {};
    this._value = undefined;
    this._checked = undefined;
    this._selIdx = undefined;
    const self = this;
    this.dataset = new Proxy(
      {},
      {
        get(_, k) {
          if (typeof k !== "string") return undefined;
          const v = self.attrs.get(camelToData(k));
          return v === undefined ? undefined : v;
        },
        set(_, k, v) {
          self.setAttribute(camelToData(k), String(v));
          return true;
        },
        deleteProperty(_, k) {
          self.attrs.delete(camelToData(k));
          return true;
        },
        has(_, k) {
          return self.attrs.has(camelToData(k));
        },
        ownKeys() {
          return [...self.attrs.keys()].filter((n) => n.startsWith("data-")).map(dataToCamel);
        },
        getOwnPropertyDescriptor(_, k) {
          const v = self.attrs.get(camelToData(k));
          return v === undefined ? undefined : { value: v, enumerable: true, configurable: true, writable: true };
        },
      }
    );
    this.classList = {
      _list: () => (self.attrs.get("class") || "").split(/\s+/).filter(Boolean),
      _set(arr) {
        self.attrs.set("class", [...new Set(arr)].join(" "));
      },
      add(...c) {
        this._set(this._list().concat(c));
      },
      remove(...c) {
        this._set(this._list().filter((x) => !c.includes(x)));
      },
      contains(c) {
        return this._list().includes(c);
      },
      toggle(c, force) {
        const has = this.contains(c);
        const on = force === undefined ? !has : !!force;
        if (on && !has) this.add(c);
        if (!on && has) this.remove(c);
        return on;
      },
      get length() {
        return this._list().length;
      },
      toString() {
        return this._list().join(" ");
      },
    };
  }
  // attributes
  getAttribute(n) {
    const v = this.attrs.get(String(n).toLowerCase());
    return v === undefined ? null : v;
  }
  setAttribute(n, v) {
    this.attrs.set(String(n).toLowerCase(), String(v));
  }
  removeAttribute(n) {
    this.attrs.delete(String(n).toLowerCase());
  }
  hasAttribute(n) {
    return this.attrs.has(String(n).toLowerCase());
  }
  toggleAttribute(n, force) {
    const on = force === undefined ? !this.hasAttribute(n) : !!force;
    if (on) this.setAttribute(n, "");
    else this.removeAttribute(n);
    return on;
  }
  get id() {
    return this.getAttribute("id") || "";
  }
  set id(v) {
    this.setAttribute("id", v);
  }
  get className() {
    return this.getAttribute("class") || "";
  }
  set className(v) {
    this.setAttribute("class", v);
  }
  get title() {
    return this.getAttribute("title") || "";
  }
  set title(v) {
    this.setAttribute("title", v);
  }
  get placeholder() {
    return this.getAttribute("placeholder") || "";
  }
  set placeholder(v) {
    this.setAttribute("placeholder", v);
  }
  get hidden() {
    return this.hasAttribute("hidden");
  }
  set hidden(v) {
    this.toggleAttribute("hidden", !!v);
  }
  get disabled() {
    return this.hasAttribute("disabled");
  }
  set disabled(v) {
    this.toggleAttribute("disabled", !!v);
  }
  get type() {
    return this.getAttribute("type") || "";
  }
  // tree
  get children() {
    return this.childNodes.filter((n) => n.nodeType === 1);
  }
  get parentElement() {
    return this.parentNode && this.parentNode.nodeType === 1 ? this.parentNode : null;
  }
  get firstElementChild() {
    return this.children[0] || null;
  }
  get firstChild() {
    return this.childNodes[0] || null;
  }
  appendChild(n) {
    if (n.parentNode) n.parentNode.removeChild(n);
    n.parentNode = this;
    this.childNodes.push(n);
    return n;
  }
  append(...ns) {
    for (const n of ns) this.appendChild(typeof n === "string" ? new TextNode(n) : n);
  }
  removeChild(n) {
    const i = this.childNodes.indexOf(n);
    if (i >= 0) this.childNodes.splice(i, 1);
    n.parentNode = null;
    return n;
  }
  remove() {
    if (this.parentNode) this.parentNode.removeChild(this);
  }
  contains(n) {
    for (let c = n; c; c = c.parentNode) if (c === this) return true;
    return false;
  }
  // content
  get textContent() {
    return this.childNodes.map((c) => c.textContent).join("");
  }
  set textContent(v) {
    for (const c of this.childNodes) c.parentNode = null;
    this.childNodes = [];
    if (v !== "" && v != null) this.appendChild(new TextNode(String(v)));
    this._afterContentChange();
  }
  get innerText() {
    return this.textContent;
  }
  set innerText(v) {
    this.textContent = v;
  }
  get innerHTML() {
    return this.childNodes.map((c) => serialize(c, RAW.has(this.localName))).join("");
  }
  set innerHTML(html) {
    for (const c of this.childNodes) c.parentNode = null;
    this.childNodes = [];
    parseInto(this, String(html ?? ""));
    this._afterContentChange();
    if (H.tracked.has(this.id)) H.writes.push({ t: H.now, id: this.id, html: String(html ?? "") });
  }
  get outerHTML() {
    return serialize(this, false);
  }
  _afterContentChange() {
    if (this.localName === "select") this._selIdx = undefined;
    if (this.localName === "textarea") this._value = undefined;
  }
  // form controls
  get options() {
    return this.localName === "select" ? this.querySelectorAll("option") : [];
  }
  get selectedIndex() {
    if (this.localName !== "select") return -1;
    if (this._selIdx !== undefined) return this._selIdx;
    const opts = this.options;
    const i = opts.findIndex((o) => o.hasAttribute("selected"));
    return i >= 0 ? i : opts.length ? 0 : -1;
  }
  set selectedIndex(i) {
    this._selIdx = Number(i);
  }
  get value() {
    if (this.localName === "select") {
      const o = this.options[this.selectedIndex];
      return o ? o.value : "";
    }
    if (this.localName === "option") return this.hasAttribute("value") ? this.getAttribute("value") : this.textContent;
    if (this.localName === "textarea") return this._value !== undefined ? this._value : this.textContent;
    if (this._value !== undefined) return this._value;
    return this.getAttribute("value") || "";
  }
  set value(v) {
    v = v == null ? "" : String(v);
    if (this.localName === "select") {
      this._selIdx = this.options.findIndex((o) => o.value === v);
      return;
    }
    this._value = v;
  }
  get checked() {
    return this._checked !== undefined ? this._checked : this.hasAttribute("checked");
  }
  set checked(v) {
    this._checked = !!v;
  }
  // selectors
  matches(sel) {
    return selectorMatches(this, sel);
  }
  closest(sel) {
    for (let c = this; c && c.nodeType === 1; c = c.parentNode) if (selectorMatches(c, sel)) return c;
    return null;
  }
  querySelectorAll(sel) {
    const out = [];
    const walk = (n) => {
      for (const c of n.childNodes) {
        if (c.nodeType !== 1) continue;
        if (selectorMatches(c, sel)) out.push(c);
        walk(c);
      }
    };
    walk(this);
    return out;
  }
  querySelector(sel) {
    return this.querySelectorAll(sel)[0] || null;
  }
  getElementsByTagName(t) {
    return this.querySelectorAll(t);
  }
  // events
  addEventListener(type, fn) {
    (this.listeners[type] ||= []).push(fn);
  }
  removeEventListener(type, fn) {
    this.listeners[type] = (this.listeners[type] || []).filter((f) => f !== fn);
  }
  dispatchEvent(ev) {
    return dispatch(this, ev);
  }
  click(opts = {}) {
    return dispatch(this, makeEvent("click", opts));
  }
  focus() {
    document.activeElement = this;
  }
  blur() {
    if (document.activeElement === this) document.activeElement = document.body;
  }
  scrollIntoView() {}
  getBoundingClientRect() {
    return { left: 0, top: 0, right: 100, bottom: 20, width: 100, height: 20, x: 0, y: 0 };
  }
}

function serialize(n, rawParent) {
  if (n.nodeType === 3) return rawParent ? n.data : escText(n.data);
  let s = "<" + n.localName;
  for (const [k, v] of n.attrs) s += v === "" && k !== "value" ? ` ${k}=""` : ` ${k}="${escAttr(v)}"`;
  s += ">";
  if (VOID.has(n.localName)) return s;
  const raw = RAW.has(n.localName);
  s += n.childNodes.map((c) => serialize(c, raw)).join("");
  return s + "</" + n.localName + ">";
}

var RE_START = /<([A-Za-z][\w:-]*)((?:\s+[^\s"'>\/=]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?)*)\s*(\/?)>/y;
var RE_END = /<\/([A-Za-z][\w:-]*)\s*>/y;
var RE_COMMENT = /<!--[\s\S]*?-->/y;
var RE_DECL = /<![^>]*>/y;
var RE_ATTR = /([^\s"'>\/=]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/g;

/** Parse an HTML fragment into `root`. Tolerant tokenizer; no implied end tags except mismatched closes. */
function parseInto(root, html) {
  const stack = [root];
  const top = () => stack[stack.length - 1];
  let i = 0;
  let text = "";
  const flush = () => {
    if (text) top().appendChild(new TextNode(decodeEntities(text)));
    text = "";
  };
  while (i < html.length) {
    const lt = html.indexOf("<", i);
    if (lt < 0) {
      text += html.slice(i);
      break;
    }
    text += html.slice(i, lt);
    i = lt;
    let m;
    RE_COMMENT.lastIndex = i;
    if ((m = RE_COMMENT.exec(html))) {
      flush();
      i = RE_COMMENT.lastIndex;
      continue;
    }
    RE_END.lastIndex = i;
    if ((m = RE_END.exec(html))) {
      flush();
      const tag = m[1].toLowerCase();
      for (let k = stack.length - 1; k > 0; k--) {
        if (stack[k].localName === tag) {
          stack.length = k;
          break;
        }
      }
      i = RE_END.lastIndex;
      continue;
    }
    RE_START.lastIndex = i;
    if ((m = RE_START.exec(html))) {
      flush();
      const el = new Element(m[1]);
      RE_ATTR.lastIndex = 0;
      let a;
      while ((a = RE_ATTR.exec(m[2] || ""))) {
        const name = a[1].toLowerCase();
        const val = a[2] ?? a[3] ?? a[4] ?? "";
        if (!el.attrs.has(name)) el.attrs.set(name, decodeEntities(val));
      }
      top().appendChild(el);
      i = RE_START.lastIndex;
      const tag = el.localName;
      if (RAW.has(tag)) {
        const close = html.toLowerCase().indexOf("</" + tag, i);
        const body = close < 0 ? html.slice(i) : html.slice(i, close);
        if (body) el.appendChild(new TextNode(tag === "textarea" || tag === "title" ? decodeEntities(body) : body));
        i = close < 0 ? html.length : html.indexOf(">", close) + 1;
        continue;
      }
      if (!VOID.has(tag) && !m[3]) stack.push(el);
      continue;
    }
    RE_DECL.lastIndex = i;
    if ((m = RE_DECL.exec(html))) {
      flush();
      i = RE_DECL.lastIndex;
      continue;
    }
    text += "<"; // a '<' that does not start markup is text (HTML tokenizer semantics)
    i += 1;
  }
  flush();
}

// ── selectors: groups ',', combinators ' ' and '>', compounds tag#id.class[attr][attr=val]
var SEL_CACHE = new Map();
function parseSelector(sel) {
  if (SEL_CACHE.has(sel)) return SEL_CACHE.get(sel);
  const groups = [];
  let depth = 0,
    q = null,
    cur = "";
  for (const ch of sel) {
    if (q) {
      if (ch === q) q = null;
    } else if (ch === '"' || ch === "'") q = ch;
    else if (ch === "[") depth++;
    else if (ch === "]") depth--;
    else if (ch === "," && !depth) {
      groups.push(cur.trim());
      cur = "";
      continue;
    }
    cur += ch;
  }
  if (cur.trim()) groups.push(cur.trim());
  const parsed = groups.map((g) => {
    const parts = [];
    const re = /\s*(>)?\s*((?:[A-Za-z][\w-]*|\*)?(?:#[\w-]+|\.[\w-]+|\[[^\]]+\])*)/g;
    let m;
    let comb = " ";
    while ((m = re.exec(g)) && m[0]) {
      if (m[1]) comb = ">";
      if (m[2]) {
        parts.push({ comb, compound: parseCompound(m[2]) });
        comb = " ";
      }
      if (re.lastIndex >= g.length) break;
    }
    return parts;
  });
  SEL_CACHE.set(sel, parsed);
  return parsed;
}
function parseCompound(s) {
  const c = { tag: null, id: null, classes: [], attrs: [] };
  const re = /([A-Za-z][\w-]*|\*)|#([\w-]+)|\.([\w-]+)|\[\s*([\w-]+)\s*(?:([~^$*|]?=)\s*(?:"([^"]*)"|'([^']*)'|([^\]\s]+)))?\s*\]/g;
  let m;
  while ((m = re.exec(s))) {
    if (m[1]) c.tag = m[1] === "*" ? null : m[1].toLowerCase();
    else if (m[2]) c.id = m[2];
    else if (m[3]) c.classes.push(m[3]);
    else if (m[4]) c.attrs.push({ name: m[4].toLowerCase(), op: m[5] || null, val: m[6] ?? m[7] ?? m[8] ?? null });
  }
  return c;
}
function compoundMatches(el, c) {
  if (!el || el.nodeType !== 1) return false;
  if (c.tag && el.localName !== c.tag) return false;
  if (c.id && el.id !== c.id) return false;
  for (const k of c.classes) if (!el.classList.contains(k)) return false;
  for (const a of c.attrs) {
    const v = el.getAttribute(a.name);
    if (v === null) return false;
    if (a.op === "=" && v !== a.val) return false;
    if (a.op === "^=" && !v.startsWith(a.val)) return false;
    if (a.op === "*=" && !v.includes(a.val)) return false;
    if (a.op === "~=" && !v.split(/\s+/).includes(a.val)) return false;
  }
  return true;
}
function complexMatches(el, parts, idx) {
  if (!compoundMatches(el, parts[idx].compound)) return false;
  if (idx === 0) return true;
  const comb = parts[idx].comb;
  if (comb === ">") return complexMatches(el.parentNode, parts, idx - 1);
  for (let p = el.parentNode; p && p.nodeType === 1; p = p.parentNode) if (complexMatches(p, parts, idx - 1)) return true;
  return false;
}
function selectorMatches(el, sel) {
  return parseSelector(sel).some((parts) => parts.length && complexMatches(el, parts, parts.length - 1));
}

// ── events
function makeEvent(type, opts = {}) {
  return Object.assign(
    {
      type,
      bubbles: true,
      defaultPrevented: false,
      _stop: false,
      shiftKey: false,
      metaKey: false,
      ctrlKey: false,
      altKey: false,
      button: 0,
      clientX: 10,
      clientY: 10,
      relatedTarget: null,
      preventDefault() {
        this.defaultPrevented = true;
      },
      stopPropagation() {
        this._stop = true;
      },
      stopImmediatePropagation() {
        this._stop = true;
      },
    },
    opts
  );
}
function dispatch(target, ev) {
  ev.target = target;
  const path = [];
  for (let n = target; n; n = n.parentNode) path.push(n);
  if (path[path.length - 1] !== document) path.push(document);
  path.push(window);
  for (const node of path) {
    ev.currentTarget = node;
    const ls = (node.listeners && node.listeners[ev.type]) || (node === window ? H.windowListeners[ev.type] : null) || [];
    for (const f of [...ls]) track(f.call(node, ev));
    const h = node["on" + ev.type];
    if (typeof h === "function") track(h.call(node, ev));
    if (ev._stop || !ev.bubbles) break;
  }
  return !ev.defaultPrevented;
}

// ───────────────────────── document / window ─────────────────────────
H.windowListeners = {};
var document;
function buildDocument() {
  const src = readFile(UI_DIR + "/index.html");
  const holder = new Element("#document-holder");
  parseInto(holder, src);
  const html = holder.querySelector("html") || holder;
  document = {
    nodeType: 9,
    readyState: "complete",
    visibilityState: "visible",
    documentElement: html,
    head: html.querySelector("head"),
    body: html.querySelector("body"),
    activeElement: null,
    listeners: {},
    parentNode: null,
    addEventListener(t, f) {
      (this.listeners[t] ||= []).push(f);
    },
    removeEventListener(t, f) {
      this.listeners[t] = (this.listeners[t] || []).filter((x) => x !== f);
    },
    querySelector(sel) {
      return selectorMatches(html, sel) ? html : html.querySelector(sel);
    },
    querySelectorAll(sel) {
      return (selectorMatches(html, sel) ? [html] : []).concat(html.querySelectorAll(sel));
    },
    getElementById(id) {
      return html.querySelector("#" + id);
    },
    createElement(t) {
      return new Element(t);
    },
    createTextNode(t) {
      return new TextNode(t);
    },
    dispatchEvent(ev) {
      return dispatch(html, ev);
    },
  };
  html.parentNode = document;
  document.activeElement = document.body;
  globalThis.document = document;
  return src;
}
window.innerWidth = 1400;
window.innerHeight = 900;
window.devicePixelRatio = 2;
window.addEventListener = (t, f) => (H.windowListeners[t] ||= []).push(f);
window.removeEventListener = () => {};
window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
window.getSelection = () => ({ toString: () => H.selectionText, removeAllRanges() {} });
var navigator = { clipboard: { writeText: async (t) => (H.clipboard = t) }, platform: "MacIntel", userAgent: "jsc" };
var _ls = {};
var localStorage = {
  getItem: (k) => (Object.prototype.hasOwnProperty.call(_ls, k) ? _ls[k] : null),
  setItem: (k, v) => {
    _ls[k] = String(v);
  },
  removeItem: (k) => {
    delete _ls[k];
  },
  clear: () => {
    for (const k of Object.keys(_ls)) delete _ls[k];
  },
};
var performance = { now: () => H.now };

// ───────────────────────── fixtures + IPC mock ─────────────────────────
function fnv(s) {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16);
}
function canon(v) {
  if (Array.isArray(v)) {
    if (v.length > 3 && v.every((x) => typeof x === "string")) return `#${v.length}:${fnv(v.join("\n"))}`;
    return v.map(canon);
  }
  if (v && typeof v === "object") {
    const o = {};
    for (const k of Object.keys(v).sort()) if (v[k] !== undefined) o[k] = canon(v[k]);
    return o;
  }
  return v === undefined ? null : v;
}
/** Fixture key = command + the args THIS backend reads (Tauri 2 ignores unknown keys; see harness notes). */
function fixtureKey(cmd, args) {
  const sig = (H.fixture && H.fixture.signatures && H.fixture.signatures[cmd]) || Object.keys(args || {}).sort();
  return cmd + "|" + JSON.stringify(sig.map((k) => canon(args ? args[k] : undefined)));
}
function loadFixture(name) {
  const path = `${FIXTURE_DIR}/${name}.json`;
  let txt = null;
  try {
    txt = readFile(path);
  } catch (e) {
    txt = null;
  }
  H.fixture = txt ? JSON.parse(txt) : { signatures: null, responses: {} };
  H.fixtureName = name;
}
// Newer than every recorded backend (2026-11-18: the «ES» line from the Windows build).
var OPTIONAL_CMDS = new Set(["dictionary_index", "get_dictionary_entry", "strong_gloss", "list_devotional_days", "get_devotional"]);
var UI_ONLY = {
  set_frosted_window: () => ({ ok: "frost-ok" }),
  pick_modules_dir: () => ({ ok: null }),
  set_modules_path: (a) => ({ ok: a.path }),
  clean_text: (a) => ({ ok: a.raw }),
};
function neutral(cmd, args) {
  switch (cmd) {
    case "probe_modules_content":
      return { ok: (args.paths || []).map((p) => ({ path: p, has: false })) };
    case "resolve_word":
      return { ok: { word: args.word, normalized: String(args.word).toLowerCase(), strongs: [], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: false, interlinearHits: [], matchedHit: null } };
    case "lookup_dictionary":
    case "get_commentary":
    case "list_modules":
    case "list_cross_reference_sources":
      return { ok: [] };
    case "get_bible_chapter":
      return { ok: { book: args.book, chapter: args.chapter, module: args.modulePath, abbreviation: "?", title: "", verses: [] } };
    case "dictionary_index":
      return { ok: { total: 0, start: 0, focus: null, exact: false, rows: [] } };
    default:
      return { ok: null };
  }
}
function answer(cmd, args) {
  if (H.overrides[cmd]) {
    const r = H.overrides[cmd](args);
    if (r !== undefined) return r;
  }
  if (UI_ONLY[cmd]) return UI_ONLY[cmd](args);
  const key = fixtureKey(cmd, args);
  const rec = H.fixture.responses[key];
  if (rec) return rec;
  // Commands newer than every recorded backend: neutral reply, never a «missing fixture»
  // (tests that need data give it through H.overrides).
  if (OPTIONAL_CMDS.has(cmd)) return neutral(cmd, args);
  if (MODE === "collect") {
    if (!H.requests.some((r) => r.key === key)) H.requests.push({ key, cmd, args });
  } else if (!H.missing.includes(key)) H.missing.push(key);
  return neutral(cmd, args);
}
function deliver(entry) {
  const r = answer(entry.cmd, entry.args);
  entry.doneAt = H.now;
  entry.result = r;
  if ("err" in r) entry.reject(r.err);
  else entry.resolve(JSON.parse(JSON.stringify(r.ok)));
  drainMicrotasks();
}
function mockInvoke(cmd, args = {}) {
  const entry = { seq: H.calls.length, cmd, args, issuedAt: H.now };
  H.calls.push(entry);
  const p = new Promise((res, rej) => {
    entry.resolve = res;
    entry.reject = rej;
  });
  if (H.delivery === "hold") {
    H.pending.push(entry);
  } else {
    const lat = H.LAT[cmd] ?? 2;
    const start = Math.max(H.now, H.busyUntil);
    H.busyUntil = start + lat;
    setTimeout(() => deliver(entry), H.busyUntil - H.now);
  }
  return p;
}
/** Deliver all currently held replies ("fifo" or "reverse"). */
function release(order = "fifo") {
  const batch = H.pending.splice(0);
  if (order === "reverse") batch.reverse();
  for (const e of batch) deliver(e);
}
/** Deliver held replies matching pred (in issue order). */
function releaseWhere(pred) {
  const keep = [];
  const go = [];
  for (const e of H.pending) (pred(e) ? go : keep).push(e);
  H.pending = keep;
  for (const e of go) deliver(e);
}
/** Run timers and deliver every held reply EXCEPT those matching `keep` (they stay held). */
function settleHolding(keep, { maxSteps = 100000 } = {}) {
  drainMicrotasks();
  for (let i = 0; i < maxSteps; i++) {
    const t = nextTimer();
    if (t) {
      runTimer(t);
      continue;
    }
    if (H.pending.some((e) => !keep(e))) {
      releaseWhere((e) => !keep(e));
      continue;
    }
    return;
  }
  throw new Error("settleHolding: did not become idle");
}
window.__TAURI_INTERNALS__ = {};
window.__TAURI__ = { core: { invoke: mockInvoke } };

// ───────────────────────── boot + helpers for tests ─────────────────────────
// The module library, injected by run.sh / record.sh (jsc cannot read the environment):
// ESPADA_MODULES_DIR, else $HOME/Downloads/for mac espada. Fixture keys use these paths.
var LIB_DIR = typeof HARNESS_LIB_DIR === "string" ? HARNESS_LIB_DIR : "";
var LS_KEYS = {
  theme: "espada37.theme", modulesDir: "espada37.modulesDir", nav: "espada37.nav", study: "espada37.study",
  parallel: "espada37.parallel", history: "espada37.history", notes: "espada37.notes", highlights: "espada37.highlights",
};
/**
 * boot({ storage, fixture, delivery, settleOrder })
 *  storage: { key: value(object|string) } pre-seeded localStorage (keys may be LS_KEYS short names)
 */
function boot(opts = {}) {
  loadFixture(opts.fixture || (typeof HARNESS_FIXTURE === "string" && HARNESS_FIXTURE) || "head");
  for (const [k, v] of Object.entries(opts.storage || {})) {
    localStorage.setItem(LS_KEYS[k] || k, typeof v === "string" ? v : JSON.stringify(v));
  }
  buildDocument();
  // Execute index.html scripts in document order (inline + src), like the webview.
  for (const s of document.querySelectorAll("script")) {
    const src = s.getAttribute("src");
    const file = src && src.split("?")[0];
    // HARNESS_APP (run.sh: APP=/path) swaps app.v7.js for another copy, e.g. to prove a test fails on old code.
    if (file === "app.v7.js" && typeof HARNESS_APP === "string" && HARNESS_APP) load(HARNESS_APP);
    else if (src) load(`${UI_DIR}/${file}`);
    else (0, eval)(s.textContent);
  }
  H.delivery = opts.delivery || "fifo";
  settle({ order: opts.settleOrder || "fifo" });
}
function $id(id) {
  return document.getElementById(id);
}
function clickEl(el, opts = {}) {
  if (!el) throw new Error("clickEl: element not found");
  return dispatch(el, makeEvent("click", opts));
}
function contextMenu(el, opts = {}) {
  return dispatch(el, makeEvent("contextmenu", opts));
}
function changeSelect(sel, value) {
  sel.value = value;
  return dispatch(sel, makeEvent("change"));
}
/** A rendered Bible word span by its visible text (n-th occurrence in verse v). */
function wordEl(text, verse, nth = 0) {
  const root = $id("bible-content");
  const scope = verse == null ? root : root.querySelector(`.verse[data-verse="${verse}"]`);
  if (!scope) throw new Error(`wordEl: verse ${verse} not rendered`);
  const hits = scope.querySelectorAll(".word").filter((w) => w.textContent === text);
  if (!hits[nth]) throw new Error(`wordEl: «${text}» not found in verse ${verse}`);
  return hits[nth];
}
function callsSince(mark, cmd) {
  return H.calls.slice(mark).filter((c) => !cmd || c.cmd === cmd);
}
function writesSince(mark, id = "study-content") {
  return H.writes.slice(mark).filter((w) => w.id === id);
}
function moduleByPath(p) {
  return (state.modules || []).find((m) => m.path === p) || null;
}
function moduleKind(p) {
  return (moduleByPath(p) || {}).kind || "";
}
function findModule(re, kind) {
  return (state.modules || []).find((m) => (!kind || m.kind === kind) && re.test(`${m.filename} ${m.abbreviation}`)) || null;
}
function studyText() {
  return $id("study-content").textContent.replace(/\s+/g, " ").trim();
}
function storedStudy() {
  const v = localStorage.getItem(LS_KEYS.study);
  return v ? JSON.parse(v) : null;
}

// ───────────────────────── assertions + scenario registry ─────────────────────────
var SCENARIOS = [];
function scenario(name, fn, opts = {}) {
  SCENARIOS.push({ name, fn, opts });
}
class AssertionError extends Error {}
function assert(cond, msg) {
  if (!cond) throw new AssertionError(msg || "assertion failed");
}
function eq(actual, expected, msg) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new AssertionError(`${msg || "eq"}: expected ${e}, got ${a}`);
}
function lte(actual, max, msg) {
  if (!(actual <= max)) throw new AssertionError(`${msg || "lte"}: expected ≤ ${max}, got ${actual}`);
}
function includes(hay, needle, msg) {
  if (!String(hay).includes(needle)) throw new AssertionError(`${msg || "includes"}: «${needle}» not in «${String(hay).slice(0, 300)}»`);
}
function excludes(hay, needle, msg) {
  if (String(hay).includes(needle)) throw new AssertionError(`${msg || "excludes"}: «${needle}» found in «${String(hay).slice(0, 300)}»`);
}
