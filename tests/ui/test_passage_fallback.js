// ISS-40 — passage previews fall back to the reference Bible (RVR1960, ISS-23) when the sidebar
// Bible lacks the book (NT-only) or numbers verses differently (CJ); the label names the Bible
// that answered; 'Ir a pasaje' only when the sidebar Bible itself has the passage.

const RVR = `${LIB_DIR}/00RVR1960.bbli`;
const LBLA_NT = `${LIB_DIR}/00LBLA-NT+.bbli`;
const CJ = `${LIB_DIR}/biblia_corona_de_jerusaln.bbli`;
function preview(sidebar, refText) {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: sidebar } } });
  eq(state.biblePath, sidebar, "sidebar Bible");
  const box = document.createElement("div");
  box.innerHTML = linkifyHtml(refText);
  document.body.appendChild(box);
  const mark = H.calls.length;
  track(openLinkPanel(box.querySelector(".bible-ref")));
  settle();
  return { mark, gp: callsSince(mark, "get_passage"), body: $id("link-panel-body").textContent.replace(/\s+/g, " "), go: $id("btn-goto-passage") };
}

scenario("NT-only sidebar Bible (LBLA-NT+), Gn 1:1 → retried with RVR1960, labelled, no 'Ir a pasaje'", () => {
  const { gp, body, go } = preview(LBLA_NT, "Gn 1:1");
  eq(gp.map((c) => c.args.modulePath), [LBLA_NT, RVR], "sidebar first, then the reference Bible");
  includes(body, "En el principio creó Dios", "RVR1960 text shown");
  includes(body, "RVR1960 — no está en LBLA-NT+", "label names the Bible that answered");
  assert(go.classList.contains("hidden"), "'Ir a pasaje' hidden after a fallback");
});

scenario("Catholic numbering (CJ), Joel 2:28 → RVR1960 retry, 'versículo no encontrado en CJ'", () => {
  const { gp, body, go } = preview(CJ, "Joel 2:28");
  eq(gp.map((c) => c.args.modulePath), [CJ, RVR], "retry with RVR1960");
  includes(body, "derramaré mi Espíritu", "Joel 2:28 text from RVR1960");
  includes(body, "versículo no encontrado en", "numbering note");
  assert(go.classList.contains("hidden"), "'Ir a pasaje' hidden");
});

scenario("Sidebar Bible has the passage → one request, its label, 'Ir a pasaje' shown", () => {
  const { gp, body, go } = preview(LBLA_NT, "Jn 3:16");
  eq(gp.map((c) => c.args.modulePath), [LBLA_NT], "no second request");
  excludes(body, "no está en", "no fallback note");
  assert(!go.classList.contains("hidden"), "'Ir a pasaje' shown");
});

scenario("Sidebar is already RVR1960 and fails → no retry with the same Bible; Spanish message", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: RVR } } });
  H.overrides.get_passage = () => ({ err: "Versículo no encontrado" });
  H.overrides.get_verse_count = () => ({ ok: 0 });
  const box = document.createElement("div");
  box.innerHTML = linkifyHtml("Jn 3:16");
  const mark = H.calls.length;
  track(openLinkPanel(box.querySelector(".bible-ref")));
  settle();
  eq(callsSince(mark, "get_passage").length, 1, "one request only");
  includes($id("link-panel-body").textContent, "no contiene este libro", "clear message");
  assert($id("btn-goto-passage").classList.contains("hidden"), "'Ir a pasaje' hidden after a failure");
});

scenario("Both Bibles fail → 'Versículo no encontrado en «CJ» (numeración distinta)', no button", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: CJ } } });
  H.overrides.get_passage = () => ({ err: "Versículo no encontrado" });
  H.overrides.get_verse_count = () => ({ ok: 25 });
  const box = document.createElement("div");
  box.innerHTML = linkifyHtml("Jn 3:40");
  track(openLinkPanel(box.querySelector(".bible-ref")));
  settle();
  includes($id("link-panel-body").textContent, "Versículo no encontrado en", "numbering message");
  includes($id("link-panel-body").textContent, "(numeración distinta)", "explains why");
  assert($id("btn-goto-passage").classList.contains("hidden"), "no 'Ir a pasaje'");
});

scenario("Hover preview uses the same fallback and label", () => {
  boot({ storage: { nav: { book: 43, chapter: 3, verse: 16, biblePath: LBLA_NT } } });
  const box = document.createElement("div");
  box.innerHTML = linkifyHtml("Gn 1:1");
  let html = "";
  tooltipForTarget(box.querySelector(".bible-ref")).then((h) => (html = h));
  settle();
  includes(html, "RVR1960 — no está en LBLA-NT+", "tooltip label");
  includes(html, "En el principio", "text");
});
