import type { DictionaryEntry } from "../types";

/** Diccionario de muestra offline (términos de Génesis 1). */
export const DICTIONARY: DictionaryEntry[] = [
  {
    term: "crear",
    source: "Vine (extracto de corpus local)",
    title: "Diccionario Expositivo — W. E. Vine",
    definition: `bara' (בָּרָא, H1254), «crear, hacer». Este verbo tiene un significado teológico muy profundo, puesto que su único sujeto es Dios. Solo él puede «crear» en el sentido que está implícito en bara. El verbo expresa creación de la nada (ex nihilo), una idea que se percibe con claridad en los pasajes relacionados con la creación en escala cósmica: «En el principio creó Dios los cielos y la tierra» (Gén 1:1; cf. Gén 2:3; Is 40:26; Is 42:5).

Todos los demás verbos que significan «creación» permiten una gama de significados mucho más amplia; tienen sujetos divinos y humanos y se usan en contextos que no tienen que ver con la creación de la vida.

Bara' se usa a menudo en paralelo con: ’asah («hacer»), yatsar («formar») y kûn («establecer»). Véase Isaías 45:18.`,
  },
  {
    term: "Dios",
    source: "Notas de estudio (muestra local)",
    title: "Espada 3.7 — diccionario de muestra",
    definition: `En Génesis 1 el nombre principal es אֱלֹהִים (Elohim). Destaca el poder creador y la soberanía sobre el cosmos. Más adelante en el Pentateuco aparece también יהוה (YHWH), el nombre del pacto.

No confundir: "Dios" en español traduce distintos términos hebreos/griegos según el contexto. Para estudio léxico, preferir el número de Strong y el texto original.`,
  },
  {
    term: "cielos",
    source: "Notas de estudio (muestra local)",
    title: "Espada 3.7 — diccionario de muestra",
    definition: `Hebreo שָׁמַיִם (shamayim), dual/plural de forma. En Gn 1:1 forma merismo con "tierra": la totalidad de lo creado. En el relato puede referirse a la bóveda/expansión (raqia') o al dominio celestial según el versículo.

En teología bíblica posterior, "cielos" también designa la morada de Dios (Sal 11:4; Mt 6:9), sin anular el sentido cósmico del término.`,
  },
  {
    term: "tierra",
    source: "Notas de estudio (muestra local)",
    title: "Espada 3.7 — diccionario de muestra",
    definition: `Hebreo אֶרֶץ (erets). Puede significar: (1) la tierra como planeta/suelo en contraste con los cielos; (2) territorio o país; (3) el suelo cultivable. En Gn 1:1 es el polo cósmico opuesto a "cielos". En 1:10 Dios nombra "Tierra" a lo seco.

Importante para la teología de la creación y de la tierra prometida: el mismo vocablo viaja del cosmos al pacto.`,
  },
  {
    term: "luz",
    source: "Notas de estudio (muestra local)",
    title: "Espada 3.7 — diccionario de muestra",
    definition: `Hebreo אוֹר (or). Primera palabra creativa explícita en forma de mandato ("Sea la luz"). En el AT y el NT la luz se asocia a la presencia, verdad y salvación de Dios (Sal 27:1; Is 9:2; Jn 1:4-9).

Distinguir de "lumbreras" (me'orot, Gn 1:14-16): la luz como realidad ordenadora precede a los cuerpos celestes asignados al día y la noche.`,
  },
  {
    term: "imagen",
    source: "Notas de estudio (muestra local)",
    title: "Espada 3.7 — diccionario de muestra",
    definition: `Hebreo צֶלֶם (tselem) en Gn 1:26-27. El ser humano es creado "a imagen de Dios": dignidad, representación y vocación. No es una efigie material de Dios, sino que refleja su gobierno y carácter de forma limitada y creada.

Junto a "semejanza" (demut). En el NT, Cristo es la imagen perfecta del Dios invisible (Col 1:15), y el creyente es conformado a esa imagen (Ro 8:29).`,
  },
];

export function lookupDictionary(query: string): DictionaryEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return DICTIONARY;
  return DICTIONARY.filter(
    (d) =>
      d.term.toLowerCase().includes(q) ||
      d.definition.toLowerCase().includes(q) ||
      d.source.toLowerCase().includes(q),
  );
}
