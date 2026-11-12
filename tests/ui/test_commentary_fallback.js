// ISS-16 (user decision 4) — a commentary with no note for the verse shows its CHAPTER note,
// clearly labelled «Comentario del capítulo»; book introductions only when «Libro» is ticked.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const WEO = `${LIB_DIR}/WEO-ES - Bosquejos Expositivos de la Biblia Warren W Wiersbe.cmti`;
const CMH = `${LIB_DIR}/Comentario Matthew Henry.cmti`;
const NAV = { book: 43, chapter: 3, verse: 16, biblePath: RVR };
const T_VERSE = { verse: true, chapter: false, book: false };
const T_CHAPTER = { verse: false, chapter: true, book: false };
const on = (primary, filters = T_VERSE) => ({ nav: NAV, study: { workspace: "comentario", filters, modulesByKind: { comentario: primary } } });
const primaryCalls = (p) => H.calls.filter((c) => c.cmd === "get_commentary" && c.args.modulePath === p);

scenario("Wiersbe (chapter outlines only), verse filter → second call for the chapter, labelled", () => {
  boot({ storage: on(WEO) });
  const calls = primaryCalls(WEO);
  eq(calls.map((c) => c.args.filters), [T_VERSE, T_CHAPTER], "verse request, then the chapter fallback");
  const text = studyText();
  includes(text, "Comentario · Juan 3:16", "banner keeps the verse");
  includes(text, "Comentario del capítulo", "fallback clearly labelled");
  excludes(text, "Sin nota para", "not the empty message");
  assert($id("study-content").querySelectorAll(".study-entry.is-chapter-fallback").length >= 1, "chapter entries rendered");
});

scenario("A commentary with a note for the verse → no second call, no fallback label", () => {
  boot({ storage: on(CMH) });
  eq(primaryCalls(CMH).map((c) => c.args.filters), [T_VERSE], "only the verse request");
  excludes(studyText(), "Comentario del capítulo", "no fallback label");
});

scenario("Book introductions are never auto-shown (the fallback asks for chapter scope only)", () => {
  boot({ storage: on(WEO) });
  for (const c of primaryCalls(WEO)) eq(c.args.filters.book, false, "book scope not requested");
});

scenario("«Capítulo» ticked by the user → normal request, no extra fallback call", () => {
  const filters = { verse: true, chapter: true, book: false };
  boot({ storage: on(WEO, filters) });
  eq(primaryCalls(WEO).map((c) => c.args.filters), [filters], "one request with the user's filters");
  excludes(studyText(), "Comentario del capítulo", "no fallback label when the user asked for chapters");
});

scenario("Edge: neither verse nor chapter note → helpful Spanish message naming the module", () => {
  boot({ storage: on(CMH) });
  H.overrides.get_commentary = (a) => (a.modulePath === CMH ? { ok: [] } : undefined);
  clickEl($id("btn-verse-pick"));
  settle();
  clickEl($id("verse-grid").querySelector('button[data-v="17"]'));
  settle();
  eq(primaryCalls(CMH).slice(-2).map((c) => c.args.filters), [T_VERSE, T_CHAPTER], "fallback tried");
  const text = studyText();
  includes(text, "Sin nota para Jn 3:17 en CMH", "names verse and module");
  includes(text, "Active «Capítulo» o «Libro»", "says how to see more");
});

scenario("Error path: the fallback request fails → the empty message, nothing thrown", () => {
  boot({ storage: on(CMH) });
  H.overrides.get_commentary = (a) =>
    a.modulePath !== CMH ? undefined : a.filters.chapter ? { err: "no such table: ChapterCommentary" } : { ok: [] };
  clickEl($id("btn-verse-pick"));
  settle();
  clickEl($id("verse-grid").querySelector('button[data-v="18"]'));
  settle();
  excludes(studyText(), "Cargando", "render finished");
  assert(/Sin nota|No se pudo/.test(studyText()), `a clear message is shown: ${studyText().slice(0, 200)}`);
});
