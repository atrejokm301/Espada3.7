/** Catálogo de libros (offline). Por ahora solo Génesis tiene texto de muestra. */
export interface BookMeta {
  id: number;
  name: string;
  abbr: string;
  chapters: number;
  testament: "AT" | "NT";
  available: boolean;
}

export const BOOKS: BookMeta[] = [
  { id: 1, name: "Génesis", abbr: "Gn", chapters: 50, testament: "AT", available: true },
  { id: 2, name: "Éxodo", abbr: "Éx", chapters: 40, testament: "AT", available: false },
  { id: 3, name: "Levítico", abbr: "Lv", chapters: 27, testament: "AT", available: false },
  { id: 4, name: "Números", abbr: "Nm", chapters: 36, testament: "AT", available: false },
  { id: 5, name: "Deuteronomio", abbr: "Dt", chapters: 34, testament: "AT", available: false },
  { id: 19, name: "Salmos", abbr: "Sal", chapters: 150, testament: "AT", available: false },
  { id: 23, name: "Isaías", abbr: "Is", chapters: 66, testament: "AT", available: false },
  { id: 40, name: "Mateo", abbr: "Mt", chapters: 28, testament: "NT", available: false },
  { id: 43, name: "Juan", abbr: "Jn", chapters: 21, testament: "NT", available: false },
  { id: 45, name: "Romanos", abbr: "Ro", chapters: 16, testament: "NT", available: false },
];

export const TRANSLATIONS = [
  { id: "RV1960", name: "Reina-Valera 1960", available: true },
  { id: "BTX3", name: "Biblia Textual 3ª", available: true },
  { id: "Kadosh", name: "Kadosh Israelita", available: true },
] as const;
