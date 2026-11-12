// ISS-20 (UI) + ISS-36 (keep backend word spans) + ISS-37 ('Di-s').
// Before the fix: '“Yo' / 'Rabí*' / 'nuevo[a]' were sent as typed (0 of 140 dictionaries),
// '[a]' and morphology codes ('NCFSA', 'P') were tappable words, and the tokenizer re-wrapped
// text inside span.morph / sup.fn / existing span.word.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };

function frag(html) {
  const el = document.createElement("div");
  el.innerHTML = html;
  return el;
}
function words(html) {
  return frag(html).querySelectorAll(".word").map((w) => w.textContent);
}
function visible(html) {
  return frag(html).textContent;
}

scenario("Curly quotes are separators: '“Yo soy”' → Yo, soy", () => {
  boot({ storage: { nav: NAV } });
  const src = "“Yo soy” ‘la luz’";
  const out = tokenizeWords(src);
  eq(words(out), ["Yo", "soy", "la", "luz"], "words");
  eq(visible(out), src, "visible text preserved");
});

scenario("Footnote markers are never part of a word: sup.fn, '*', '[a]', '†'", () => {
  boot({ storage: { nav: NAV } });
  const cases = [
    ['de nuevo<sup class="fn">a</sup> no puede', ["de", "nuevo", "no", "puede"]],
    ["Rabí* sabemos", ["Rabí", "sabemos"]],
    ["[a]", []],
    ["nuevo[a] corazón [12]", ["nuevo", "corazón"]],
    ["Señor† ¶ • °", ["Señor"]],
    ["el año 12 del rey", ["el", "año", "del", "rey"]], // bare numbers are not tappable words
  ];
  for (const [src, want] of cases) {
    const out = tokenizeWords(src);
    eq(words(out), want, `words of «${src}»`);
    eq(visible(out), visible(src), `visible text of «${src}»`);
  }
});

scenario("Morphology codes and Strong links are not words: 'amó VAAI3S G25' → 1 word", () => {
  boot({ storage: { nav: NAV } });
  const src = 'amó <span class="morph">VAAI3S</span> <span class="strong-ref" data-strong="G25">G25</span> Dios';
  const out = tokenizeWords(src);
  eq(words(out), ["amó", "Dios"], "words");
  eq(frag(out).querySelectorAll(".morph .word").length, 0, "nothing wrapped inside .morph");
  eq(frag(out).querySelectorAll(".strong-ref .word").length, 0, "nothing wrapped inside .strong-ref");
  eq(visible(out), visible(src), "visible text preserved");
  // morph nested inside red letters (woc) still skipped, and the woc text after it is wrapped
  const nested = tokenizeWords('<span class="woc">Yo <span class="morph">P</span> soy</span>');
  eq(words(nested), ["Yo", "soy"], "morph inside .woc");
});

scenario("A bare Strong code in verse text becomes a strong-ref: 'principio H7225 creó'", () => {
  boot({ storage: { nav: NAV } });
  const out = tokenizeWords("En el principio H7225 creó");
  const el = frag(out);
  const ref = el.querySelector(".strong-ref");
  assert(ref, "strong-ref span created");
  eq(ref.dataset.strong, "H7225", "data-strong");
  eq(words(out), ["En", "el", "principio", "creó"], "words");
  eq(visible(out), "En el principio H7225 creó", "visible text preserved");
});

scenario("Backend-tagged words (ISS-36) stay exactly as sent, one .word with its data-strong", () => {
  boot({ storage: { nav: NAV } });
  const src =
    'familias de la <span class="morph">A</span> <span class="word" data-strong="H127">tierra</span> <span class="morph">NCFSA</span> <span class="strong-ref" data-strong="H127">H127</span>.';
  const out = tokenizeWords(src);
  const el = frag(out);
  const tagged = el.querySelectorAll('.word[data-strong="H127"]');
  eq(tagged.length, 1, "one tagged word");
  eq(tagged[0].textContent, "tierra", "its text");
  eq(tagged[0].querySelectorAll(".word").length, 0, "not wrapped a second time");
  eq(words(out), ["familias", "de", "la", "tierra"], "all words");
  eq(visible(out), visible(src), "visible text preserved");
});

scenario("'Di-s' is one word (ISS-37); other hyphens and dashes still split", () => {
  boot({ storage: { nav: NAV } });
  eq(words(tokenizeWords("En el principio creó Di-s los cielos")), ["En", "el", "principio", "creó", "Di-s", "los", "cielos"], "Di-s");
  eq(words(tokenizeWords("—dijo Él— a Bet-el.")), ["dijo", "Él", "a", "Bet", "el"], "dashes and Bet-el unchanged");
  eq(words(tokenizeWords("Di-sé")), ["Di", "sé"], "no general hyphen joining");
});

scenario("Inner apostrophes stay inside the word; edge ones are text", () => {
  boot({ storage: { nav: NAV } });
  eq(words(tokenizeWords("yeshu’ah y ’Elohim")), ["yeshu’ah", "y", "Elohim"], "apostrophes");
});

scenario("cleanQueryWord trims edge punctuation, symbols and a trailing footnote group", () => {
  boot({ storage: { nav: NAV } });
  const cases = [
    ["“Yo", "Yo"],
    ["nuevo[a]", "nuevo"],
    ["nuevo[a].", "nuevo"],
    ["Rabí*", "Rabí"],
    ["¿Dónde", "Dónde"],
    ["—dijo", "dijo"],
    ["faltará.", "faltará"],
    ["’Elohim", "Elohim"],
    ["yeshu’ah", "yeshu’ah"],
    ["Di-s", "Di-s"],
    ["G25", "G25"],
    ["אֱלֹהִים׃", "אֱלֹהִים"],
    ["[a]", ""],
    ["•", ""],
  ];
  for (const [inp, want] of cases) eq(cleanQueryWord(inp), want, `cleanQueryWord(«${inp}»)`);
});

scenario("A tap next to a quote sends the clean word: '“Yo' → resolve_word 'Yo'", () => {
  boot({ storage: { nav: NAV } });
  H.overrides.get_bible_chapter = (a) =>
    a.modulePath === RVR
      ? { ok: { book: 43, chapter: 3, module: RVR, abbreviation: "RVR1960", title: "", verses: [{ verse: 16, html: "Jesús dijo: “Yo soy la luz<sup class=\"fn\">a</sup>”.", plain: "" }] } }
      : undefined;
  reloadChapter();
  settle();
  const yo = wordEl("Yo", 16);
  const mark = H.calls.length;
  clickEl(yo);
  settle();
  const rw = callsSince(mark, "resolve_word");
  eq(rw.length, 1, "one resolve_word");
  eq(rw[0].args.word, "Yo", "word sent");
  eq($id("dict-query").value, "Yo", "search box shows the clean word");
  const bodies = $id("bible-content").querySelectorAll(".verse-body .word").map((w) => w.textContent);
  eq(bodies, ["Jesús", "dijo", "Yo", "soy", "la", "luz"], "the footnote 'a' is not a word");
});

scenario("Invariant over every recorded chapter (HEAD, wp3, tagged): visible text identical, no word inside morph/fn/strong-ref", () => {
  boot({ storage: { nav: NAV } });
  let verses = 0;
  let wordsSeen = 0;
  for (const name of ["head", "wp3", "tagged"]) {
    const fx = JSON.parse(readFile(`${FIXTURE_DIR}/${name}.json`)).responses;
    for (const [key, rec] of Object.entries(fx)) {
      if (!key.startsWith("get_bible_chapter|") || !rec.ok) continue;
      for (const v of rec.ok.verses || []) {
        const out = frag(tokenizeWords(v.html));
        const src = frag(v.html);
        if (out.textContent !== src.textContent) throw new AssertionError(`${name} ${key} v${v.verse}: visible text changed`);
        if (out.querySelectorAll(".morph .word, sup.fn .word, .strong-ref .word, .word .word").length) {
          throw new AssertionError(`${name} ${key} v${v.verse}: word wrapped inside morph/fn/strong-ref/word`);
        }
        wordsSeen += out.querySelectorAll(".word").length;
        verses++;
      }
    }
  }
  assert(verses > 200, `checked ${verses} verses`);
  assert(wordsSeen > 3000, `checked ${wordsSeen} words`);
});
