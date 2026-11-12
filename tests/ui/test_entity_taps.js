// ISS-05 / ISS-02 / ISS-20 — words written with HTML entities, as real modules store them
// (verse text copied from the library, read-only): BTX3 'cre&oacute;', Nuestro Pueblo
// 'Señor&ndash;', LBLA '&quot;No', Versión Hispano Americana '&laquo;No matarás&raquo;'.
// A tap sends the DECODED word, entities between words are separators, the occurrence index
// counts encoded and plain spellings of a word as the same word, and an older backend that
// sends the same verse already decoded gives exactly the same words.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NELSON = `${LIB_DIR}/Nelson.dcti`;
const STRONGES = `${LIB_DIR}/04STRONGes.lexi`;
const SAVED = { diccionario: NELSON, lexico: STRONGES };

// Real verses (book, chapter, verse, html as stored in the module).
const BTX3_GN_1_1 = `En principio<span style="color:#4D80B3;">&deg;</span> cre&oacute; 'Elohim<span style="color:#4D80B3;">&deg;</span> los cielos y la tierra.`;
const NP_EZ = `Esto dice el Se&ntilde;or&ndash;, &ndash;or&aacute;culo del Se&ntilde;or&ndash;: volved.`;
const LBLA_GN_3_1 = `y dijo a la mujer: ¿Conque Dios os ha dicho: &quot;No comeréis de ningún árbol del huerto&quot;?`;
const VHA_MT_5_21 = `Habéis oído que fué dicho a los antiguos: &laquo;No matarás&raquo;; y cualquiera que matare, será culpable.`;
const MIXED = `y él dijo: &iquest;dónde está &eacute;l?`;

function decodeAll(html) {
  return html.replace(/(<[^>]+>)|([^<]+)/g, (m, tag, text) => tag || escText(decodeEntities(text)).replace(/&nbsp;/g, " "));
}

/** Boot on Biblia showing one synthetic verse 1 in Gn 1 (served for RVR's path). */
function bootVerse(html) {
  boot({ storage: { nav: { book: 1, chapter: 1, verse: 1, biblePath: RVR }, study: { workspace: "biblia", modulesByKind: SAVED } } });
  H.overrides.get_bible_chapter = (a) =>
    a.modulePath === RVR && a.book === 1 && a.chapter === 1
      ? { ok: { book: 1, chapter: 1, module: RVR, abbreviation: "RVR1960", title: "", verses: [{ verse: 1, html, plain: decodeEntities(html.replace(/<[^>]+>/g, "")) }] } }
      : undefined;
  H.overrides.resolve_word = (a) => ({ ok: { word: a.word, normalized: a.word.toLowerCase(), strongs: [], hebrew: [], greek: [], dictionary: [], lexicon: [], interlinearUsed: true, interlinearHits: [], matchedHit: null } });
  H.overrides.lookup_dictionary = () => ({ ok: [] });
  H.overrides.probe_modules_content = (a) => ({ ok: a.paths.map((p) => ({ path: p, has: false })) });
  changeSelect($id("sel-chapter"), "1");
  settle();
}
const words = () => $id("bible-content").querySelector('.verse[data-verse="1"]').querySelectorAll(".word").map((w) => w.textContent);
const tap = (text, nth = 0) => {
  const mark = H.calls.length;
  clickEl(wordEl(text, 1, nth));
  settle();
  const rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, `one resolve for «${text}»`);
  return rw[0].args;
};

scenario("BTX3 Gn 1:1 'cre&oacute;' is one word; a tap sends «creó», the search box shows «creó»", () => {
  bootVerse(BTX3_GN_1_1);
  eq(words(), ["En", "principio", "creó", "Elohim", "los", "cielos", "y", "la", "tierra"], "words (° and the edge ' are not part of any word)");
  const args = tap("creó");
  eq(args.word, "creó", "decoded word sent to resolve_word");
  eq($id("dict-query").value, "creó", "search box");
  includes(studyText(), "«creó»", "dictionary banner");
});

scenario("Nuestro Pueblo 'Se&ntilde;or&ndash;, &ndash;or&aacute;culo': dashes split, letters join; 2nd «Señor» → occurrence 1", () => {
  bootVerse(NP_EZ);
  eq(words(), ["Esto", "dice", "el", "Señor", "oráculo", "del", "Señor", "volved"], "words");
  const a1 = tap("Señor", 1);
  eq(a1.word, "Señor", "decoded");
  eq(a1.occurrence, 1, "second «Señor» in the verse");
  const a0 = tap("oráculo");
  eq(a0.occurrence, 0, "first «oráculo»");
});

scenario("LBLA '&quot;No comeréis…&quot;': the quote entity is never part of the tapped word", () => {
  bootVerse(LBLA_GN_3_1);
  assert(words().includes("No"), "«No» is a word");
  assert(words().every((w) => !/["&;]/.test(w)), `no quote or entity residue in words: ${words().join("|")}`);
  eq(tap("No").word, "No", "sent «No»");
  eq(tap("huerto").word, "huerto", "the word before the closing quote");
});

scenario("Versión Hispano Americana '&laquo;No matarás&raquo;;' → «No» and «matarás» are clean words", () => {
  bootVerse(VHA_MT_5_21);
  eq(tap("matarás").word, "matarás", "sent «matarás»");
  assert(words().every((w) => !/[«»&;]/.test(w)), "no guillemets in words");
});

scenario("Occurrence counts 'él' and '&eacute;l' as the same word; '&iquest;' is a separator", () => {
  bootVerse(MIXED);
  eq(words(), ["y", "él", "dijo", "dónde", "está", "él"], "words");
  eq(tap("él", 1).occurrence, 1, "the encoded «él» is the 2nd «él»");
  eq(tap("él", 0).occurrence, 0, "the plain one is the 1st");
});

scenario("Both backends: the same verse sent encoded (raw) or already decoded gives identical words and text", () => {
  boot({ storage: {} });
  for (const html of [BTX3_GN_1_1, NP_EZ, LBLA_GN_3_1, VHA_MT_5_21, MIXED]) {
    const enc = document.createElement("div");
    enc.innerHTML = tokenizeWords(html);
    const dec = document.createElement("div");
    dec.innerHTML = tokenizeWords(decodeAll(html));
    eq(enc.querySelectorAll(".word").map((w) => w.textContent), dec.querySelectorAll(".word").map((w) => w.textContent), `words: ${html.slice(0, 40)}`);
    eq(enc.textContent, dec.textContent, `visible text: ${html.slice(0, 40)}`);
  }
});
