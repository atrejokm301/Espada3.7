// ISS-25 — plain-text references in study text: Spanish abbreviations, comma notation,
// chapter-only, relative 'v. 5', cross-chapter ranges, no double wrapping, no false positives.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const bootPlain = () => boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
function refs(html, ctx = { book: 43, chapter: 3 }) {
  const box = document.createElement("div");
  box.innerHTML = linkifyHtml(html, ctx);
  normalizeBibleRefElements(box);
  return box.querySelectorAll(".bible-ref").map((r) => ({
    text: r.textContent, book: r.dataset.book, ch: r.dataset.chapter, v: r.dataset.verse, ve: r.dataset.verseEnd,
    che: r.dataset.chapterEnd, ve2: r.dataset.verseEnd2, nested: !!r.querySelector(".bible-ref"),
  }));
}
const one = (html) => {
  const r = refs(html);
  eq(r.length, 1, `exactly one link in «${html}» (got ${JSON.stringify(r)})`);
  return r[0];
};

scenario("Linked: Spanish abbreviations, comma notation, chapter-only and relative refs", () => {
  bootPlain();
  const cases = [
    ["Véase 1 Co 13:4-7 aquí", "1 Co 13:4-7", "1 Co", "13", "4", "7"],
    ["(Éx 3:14)", "Éx 3:14", "Éx", "3", "14", "14"],
    ["Gál 2:20.", "Gál 2:20", "Gál", "2", "20", "20"],
    ["según 2 Ti 3:16,", "2 Ti 3:16", "2 Ti", "3", "16", "16"],
    ["1 P 2:9", "1 P 2:9", "1 P", "2", "9", "9"],
    ["cf. Lc 4,5 y", "Lc 4,5", "Lc", "4", "5", "5"],
    ["el Sal 23 dice", "Sal 23", "Sal", "23", "1", "1"],
    ["Jua 6:32", "Jua 6:32", "Jua", "6", "32", "32"],
    ["Apo 21:4", "Apo 21:4", "Apo", "21", "4", "4"],
  ];
  for (const [html, text, book, ch, v, ve] of cases) {
    const r = one(html);
    eq([r.text, r.book, r.ch, r.v, r.ve], [text, book, ch, v, ve], `«${html}»`);
  }
});

scenario("Relative refs: 'v. 5', 'vv. 3-5', '(v.16)', 'versículos 7-9' → current chapter", () => {
  bootPlain();
  for (const [html, text, v, ve] of [["en el v. 5 ", "v. 5", "5", "5"], ["vv. 3-5", "vv. 3-5", "3", "5"], ["(v.16)", "v.16", "16", "16"], ["los versículos 7-9", "versículos 7-9", "7", "9"]]) {
    const r = one(html);
    eq([r.text, r.book, r.ch, r.v, r.ve], [text, "Juan", "3", v, ve], `«${html}»`);
  }
});

scenario("Regressions still linked: 'Jn. 3:16', 'Ro. 5:8', 'Jn 3:16–18', '1 Juan 4:8', 'Pro 23:24'", () => {
  bootPlain();
  eq([one("Jn. 3:16").book, one("Jn. 3:16").v], ["Jn", "16"], "Jn. 3:16");
  eq(one("Ro. 5:8").text, "Ro. 5:8", "Ro. 5:8");
  eq([one("Jn 3:16–18").v, one("Jn 3:16–18").ve], ["16", "18"], "en dash range");
  eq([one("1 Juan 4:8").book, one("1 Juan 4:8").ch], ["1 Juan", "4"], "1 Juan");
  eq(one("Pro 23:24").text, "Pro 23:24", "one link, not Pro + 23:24");
});

scenario("Cross-chapter 'Efesios 5:21-6:9' → one link (whole text), chapter 5 from v.21, end 6:9", () => {
  bootPlain();
  const r = one("Efesios 5:21-6:9 trata");
  eq([r.text, r.ch, r.v, r.che, r.ve2], ["Efesios 5:21-6:9", "5", "21", "6", "9"], "attributes");
});

scenario("Clicking 'Efesios 5:21-6:9' previews chapter 5 from v.21 and says where it stops", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR }, study: { workspace: "comentario" } } });
  const content = $id("study-content");
  content.innerHTML += `<div class="study-body">${linkifyHtml("Efesios 5:21-6:9")}</div>`;
  const mark = H.calls.length;
  clickEl(content.querySelector('.bible-ref[data-chapter-end="6"]'));
  settle();
  const gp = callsSince(mark, "get_passage");
  eq(gp.length, 1, "one passage request");
  eq([gp[0].args.book, gp[0].args.chapter, gp[0].args.verseStart], [49, 5, 21], "chapter 5 from v.21");
  assert(gp[0].args.verseEnd > 21, "to the end of the chapter");
  const body = $id("link-panel-body").textContent.replace(/\s+/g, " ");
  includes(body, "Efesios 5:21–6:9", "label keeps the whole reference");
  includes(body, "Se muestra hasta 5:33", "says the preview stops at the end of chapter 5");
  excludes(body, "5:6", "not the old wrong 5:6–21 range");
});

scenario("'Jn 3:16-18. 4 …' does not swallow the '4'", () => {
  bootPlain();
  const r = one("Jn 3:16-18. 4 Luego");
  eq([r.text, r.v, r.ve], ["Jn 3:16-18", "16", "18"], "range ends at 18");
});

scenario("Not linked: 'caps. 4,5', 'son 5', 'a 3,4 km', 'Dan 3 pasos', 'mi 2', 'He 3 hijos', 'Sal 151'", () => {
  bootPlain();
  for (const html of ["los caps. 4,5", "que son 5", "a 3,4 km", "Dan 3 pasos", "en mi 2ª visita", "He 3 hijos", "Sal 151"]) {
    eq(refs(html), [], `«${html}» must stay text`);
  }
});

scenario("No double wrapping inside backend links; raw cross-chapter refs are normalised", () => {
  bootPlain();
  const backend = `<span class="bible-ref" data-book="Jua" data-chapter="2" data-verse="25" data-verse-end="25" title="Ver pasaje">Jua 2:25</span>`;
  const r = refs(`ver ${backend} y G26`);
  eq(r.length, 1, "one link");
  eq(r[0].nested, false, "no link inside the link");
  const raw = `<span class="bible-ref bible-ref-raw" data-raw="Jn 3:16-4:2" title="Ver pasaje">Jn 3:16-4:2</span>`;
  const n = refs(raw);
  eq(n.length, 1, "raw span stays one link");
  eq([n[0].ch, n[0].v, n[0].che, n[0].ve2], ["3", "16", "4", "2"], "cross-chapter attrs from data-raw");
  const box = document.createElement("div");
  box.innerHTML = linkifyHtml("ver G26");
  eq(box.querySelectorAll(".strong-ref").length, 1, "Strong numbers still linked");
});

scenario("Every recorded study text: links never nest and keep the visible text", () => {
  bootPlain();
  let n = 0;
  for (const [k, v] of Object.entries(H.fixture.responses)) {
    if (!/^(get_commentary|lookup_dictionary|lookup_lexicon)\|/.test(k) || !v.ok) continue;
    for (const e of [].concat(v.ok)) {
      if (!e || !e.html) continue;
      const a = document.createElement("div");
      a.innerHTML = e.html;
      const b = document.createElement("div");
      b.innerHTML = linkifyHtml(e.html, { book: 43, chapter: 3 });
      eq(b.textContent, a.textContent, `visible text changed (${k.slice(0, 80)})`);
      assert(!b.querySelector(".bible-ref .bible-ref"), `nested link (${k.slice(0, 80)})`);
      n++;
    }
  }
  assert(n > 50, `enough recorded entries checked (${n})`);
});

scenario("Older backend that cannot resolve 'Jua' → the UI's own table still finds Juan", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  H.overrides.resolve_book_ref = () => ({ ok: null });
  const box = document.createElement("div");
  box.innerHTML = linkifyHtml("Jua 6:32");
  let res = null;
  resolveRefElement(box.querySelector(".bible-ref")).then((r) => (res = r));
  settle();
  eq(res && res.bookNum, 43, "Jua → 43");
  eq(bookNumberOf("1 Co"), 46, "1 Co");
  eq(bookNumberOf("2Ti"), 55, "2Ti");
  eq(bookNumberOf("3 Jn"), 64, "3 Jn");
  eq(bookNumberOf("3 Co"), null, "3 Co does not exist");
});
