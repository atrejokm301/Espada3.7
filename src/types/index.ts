export type TabId = "biblia" | "comentario" | "diccionario" | "lexico";

export interface Verse {
  book: string;
  chapter: number;
  verse: number;
  text: string;
  translation: string;
}

export interface CommentaryEntry {
  reference: string;
  source: string;
  title: string;
  text: string;
}

export interface DictionaryEntry {
  term: string;
  source: string;
  title: string;
  definition: string;
}

export interface LexiconEntry {
  strongs: string;
  lemma: string;
  transliteration: string;
  language: "hebrew" | "greek";
  gloss: string;
  definition: string;
  occurrences?: string[];
}

export interface VersionCompare {
  reference: string;
  versions: { translation: string; text: string }[];
}

export interface Bookmark {
  id: string;
  reference: string;
  note?: string;
  createdAt: string;
}
