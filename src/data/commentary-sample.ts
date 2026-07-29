import type { CommentaryEntry } from "../types";

/**
 * Comentarios de muestra offline.
 * Fuente: extractos de tu corpus e-Sword (Adam Clarke / estilo estudio clásico).
 * Luego se reemplazan por carga real de .cmti / .cmtx.
 */
export const COMMENTARIES: CommentaryEntry[] = [
  {
    reference: "Génesis 1:1",
    source: "Adam Clarke",
    title: "Adam Clarke's Commentary on the Bible",
    text: `בראשית ברא אלהים את השמים ואת הארץ — Bereshith bara Elohim eth hashamayim ve'eth ha'arets.

"En el principio creó Dios los cielos y la tierra."

Muchas definiciones se han intentado del término Dios. La definición general de esta gran Causa Primera, en la medida en que las palabras humanas se atreven, es: El Ser eterno, independiente y autoexistente; cuyo propósito y acciones brotan de sí mismo, sin motivo ni influencia ajenos; de dominio absoluto; la esencia más pura, simple y espiritual; infinitamente benevolente, veraz y santo; causa del ser y sustentador de todas las cosas.

La palabra original אֱלֹהִים Elohim es forma plural de אֵל El / אֱלוֹהַּ Eloah. Desde la antigüedad, hombres piadosos y eruditos han visto en ella una alusión a la pluralidad de personas en la naturaleza divina, unida a la unidad de esencia. El verbo בָּרָא (bara') —crear— se usa en la Escritura con Dios como sujeto, subrayando un acto creativo que solo Él realiza.

Este versículo anuncia el origen de todo lo visible: no es mito eterno de materia, sino creación por la palabra y el poder del Dios vivo.`,
  },
  {
    reference: "Génesis 1:1",
    source: "Notas de estudio (muestra local)",
    title: "Asignación del Cielo — notas de muestra",
    text: `Estructura del versículo:
1) Tiempo: "En el principio" (bereshith) — no hay "antes" narrativo en el texto; Dios inicia la historia.
2) Acción: "creó" (bara') — verbo teológico fuerte; sujeto exclusivo de Dios en el AT.
3) Sujeto: "Dios" (Elohim) — nombre de majestad y poder.
4) Objeto: "los cielos y la tierra" — merismo: todo el cosmos.

Aplicación de estudio: al abrir la Biblia, el primer acto no es del hombre buscando a Dios, sino de Dios creando un mundo donde se revelará. Toda teología posterior (alianza, redención, escatología) presupone este versículo.

⚠️ Disclaimer: estos comentarios son herramientas de estudio. La autoridad final es la Escritura; contraste siempre con el texto y con la sana doctrina.`,
  },
  {
    reference: "Génesis 1:3",
    source: "Notas de estudio (muestra local)",
    title: "Asignación del Cielo — notas de muestra",
    text: `"Y dijo Dios: Sea la luz; y fue la luz."

La creación por la palabra (fiat) anticipa el tema de la Palabra eficaz a lo largo de la Escritura (Sal 33:6, 9; Jn 1:1-3; Heb 11:3). La luz aparece antes de sol y luna (vv. 14-16): Dios mismo es la fuente de orden y revelación; los luminarias son instrumentos, no ídolos.

Pregunta de estudio: ¿cómo se relaciona esta luz con "Dios es luz" (1 Jn 1:5) y con Cristo "luz del mundo" (Jn 8:12)?`,
  },
  {
    reference: "Génesis 1:26-27",
    source: "Notas de estudio (muestra local)",
    title: "Asignación del Cielo — notas de muestra",
    text: `"Hagamos al hombre a nuestra imagen…" / "Y creó Dios al hombre a su imagen…"

- "Hagamos" (plural deliberativo) ha sido leído en la tradición cristiana en continuidad con la revelación trinitaria; el texto hebreo también se ha explicado como plural de majestad o consulta divina.
- Imagen y semejanza: el ser humano recibe dignidad y vocación de dominio responsable (no explotación destructiva) sobre la creación.
- Varón y hembra: la imagen de Dios se afirma de ambos, en la complementariedad del diseño creador.

Punto de contraste con cosmovisiones paganas: el ser humano no es accidente cósmico ni esclavo de dioses caprichosos, sino imagen del Creador con propósito.`,
  },
];

export function getCommentariesForReference(reference: string): CommentaryEntry[] {
  const ref = reference.trim().toLowerCase();
  // Exact match first
  const exact = COMMENTARIES.filter((c) => c.reference.toLowerCase() === ref);
  if (exact.length) return exact;

  // Partial: e.g. "Génesis 1:1" matches entries for that verse
  return COMMENTARIES.filter((c) => {
    const cr = c.reference.toLowerCase();
    return cr.includes(ref) || ref.includes(cr.split("–")[0] ?? cr);
  });
}

export function getCommentariesForVerse(
  book: string,
  chapter: number,
  verse: number,
): CommentaryEntry[] {
  const key = `${book} ${chapter}:${verse}`;
  return COMMENTARIES.filter((c) => {
    const r = c.reference;
    if (r === key) return true;
    // Ranges like "Génesis 1:26-27"
    const m = r.match(/^(.+?)\s+(\d+):(\d+)(?:-(\d+))?$/);
    if (!m) return false;
    const [, b, ch, v1, v2] = m;
    if (b !== book || Number(ch) !== chapter) return false;
    const from = Number(v1);
    const to = v2 ? Number(v2) : from;
    return verse >= from && verse <= to;
  });
}
