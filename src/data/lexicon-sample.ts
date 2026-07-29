import type { LexiconEntry } from "../types";

/** Léxico Strong's de muestra (términos clave de Génesis 1). */
export const LEXICON: LexiconEntry[] = [
  {
    strongs: "H7225",
    lemma: "רֵאשִׁית",
    transliteration: "reshith",
    language: "hebrew",
    gloss: "principio, primicia, comienzo",
    definition:
      "Lo primero en tiempo, orden o rango; el comienzo. En Gn 1:1 (bereshith) marca el inicio de la creación narrada. También se usa de primicias y de lo principal (Pr 1:7; 4:7).",
    occurrences: ["Génesis 1:1", "Proverbios 1:7", "Proverbios 4:7"],
  },
  {
    strongs: "H1254",
    lemma: "בָּרָא",
    transliteration: "bara'",
    language: "hebrew",
    gloss: "crear",
    definition:
      "Crear; en contextos teológicos, acto de Dios. En la creación cósmica suele entenderse como producción de lo que no existía de esa forma (ex nihilo en la teología clásica). Sujeto típico: Dios. Cf. Vine H1254 en el diccionario de muestra.",
    occurrences: ["Génesis 1:1", "Génesis 1:21", "Génesis 1:27", "Isaías 40:26"],
  },
  {
    strongs: "H430",
    lemma: "אֱלֹהִים",
    transliteration: "Elohim",
    language: "hebrew",
    gloss: "Dios; dioses (según contexto)",
    definition:
      "Forma plural de El/Eloah. En la gran mayoría de usos del AT con verbos en singular se refiere al Dios verdadero (como en Gn 1). En otros contextos puede referirse a dioses paganos, jueces o seres celestiales. No se reduce a una sola glosa; el contexto manda.",
    occurrences: ["Génesis 1:1", "Génesis 1:2", "Éxodo 20:3", "Salmos 82:6"],
  },
  {
    strongs: "H8064",
    lemma: "שָׁמַיִם",
    transliteration: "shamayim",
    language: "hebrew",
    gloss: "cielos, cielo",
    definition:
      "Cielos / cielo. Forma dual. Puede denotar el firmamento visible, el cielo como morada de Dios, o el polo cósmico junto a erets en merismos.",
    occurrences: ["Génesis 1:1", "Génesis 1:8", "Salmos 19:1"],
  },
  {
    strongs: "H776",
    lemma: "אֶרֶץ",
    transliteration: "erets",
    language: "hebrew",
    gloss: "tierra, país, suelo",
    definition:
      "Tierra (como creación), país/territorio, o suelo. Extremadamente frecuente en el AT. En Gn 1:1 es el mundo creado; en 1:10 recibe el nombre de lo seco.",
    occurrences: ["Génesis 1:1", "Génesis 1:10", "Génesis 12:1"],
  },
  {
    strongs: "H7307",
    lemma: "רוּחַ",
    transliteration: "ruach",
    language: "hebrew",
    gloss: "espíritu, aliento, viento",
    definition:
      "Aliento, viento o espíritu, según contexto. En Gn 1:2, «el Espíritu de Dios» (ruach Elohim) se mueve sobre las aguas: presencia activa de Dios en la obra creadora. El mismo vocablo cubre un campo semántico amplio; no forzar siempre «Espíritu Santo» sin mirar el contexto.",
    occurrences: ["Génesis 1:2", "Génesis 6:3", "Ezequiel 37:9-10"],
  },
  {
    strongs: "H216",
    lemma: "אוֹר",
    transliteration: "or",
    language: "hebrew",
    gloss: "luz",
    definition:
      "Luz. Primera realidad llamada a existir por mandato en el relato de los seis días (Gn 1:3). Base de la polaridad luz/tinieblas y de la simbología de revelación y vida en el resto de la Escritura.",
    occurrences: ["Génesis 1:3-5", "Salmos 27:1", "Isaías 9:2"],
  },
  {
    strongs: "H6754",
    lemma: "צֶלֶם",
    transliteration: "tselem",
    language: "hebrew",
    gloss: "imagen, figura",
    definition:
      "Imagen o representación. En Gn 1:26-27 el ser humano es creado a tselem de Dios. En otros textos puede referirse a ídolos (imágenes talladas): el contraste teológico es deliberado — el hombre representa a Dios; no debe fabricar dioses a su imagen.",
    occurrences: ["Génesis 1:26-27", "Génesis 9:6", "Éxodo 20:4 (contraste)"],
  },
];

export function lookupLexicon(query: string): LexiconEntry[] {
  const q = query.trim().toLowerCase().replace(/^h/, "h");
  if (!q) return LEXICON;
  return LEXICON.filter(
    (e) =>
      e.strongs.toLowerCase().includes(q) ||
      e.lemma.includes(query.trim()) ||
      e.transliteration.toLowerCase().includes(q) ||
      e.gloss.toLowerCase().includes(q) ||
      e.definition.toLowerCase().includes(q),
  );
}

export function getLexiconByStrongs(num: string): LexiconEntry | undefined {
  const n = num.toUpperCase().startsWith("H") || num.toUpperCase().startsWith("G")
    ? num.toUpperCase()
    : `H${num}`;
  return LEXICON.find((e) => e.strongs.toUpperCase() === n);
}
