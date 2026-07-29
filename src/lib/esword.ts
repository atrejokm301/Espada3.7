/** Invoke usando el global de Tauri (withGlobalTauri) — sin imports dinámicos. */
function getInvoke(): (
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<unknown> {
  const g = globalThis as unknown as {
    __TAURI__?: {
      core?: {
        invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown>;
      };
    };
    __TAURI_INTERNALS__?: {
      invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown>;
    };
  };

  if (g.__TAURI__?.core?.invoke) {
    return (cmd, args) => g.__TAURI__!.core!.invoke(cmd, args);
  }
  if (g.__TAURI_INTERNALS__?.invoke) {
    return (cmd, args) => g.__TAURI_INTERNALS__!.invoke(cmd, args);
  }

  throw new Error(
    "API de Tauri no disponible. Abrí la ventana de escritorio de la app (no el navegador).",
  );
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getInvoke()(cmd, args) as Promise<T>;
}

export interface ModuleInfo {
  filename: string;
  path: string;
  moduleType: string;
  title: string;
  abbreviation: string;
  sizeMb: number;
}

export interface BookInfo {
  number: number;
  name: string;
  abbr: string;
  chapters: number;
  testament: string;
}

export interface VerseRow {
  book: string;
  bookNumber: number;
  chapter: number;
  verse: number;
  text: string;
  translation: string;
}

export interface ChapterResult {
  book: string;
  bookNumber: number;
  chapter: number;
  translation: string;
  moduleTitle: string;
  verses: VerseRow[];
}

export interface CommentaryResult {
  reference: string;
  source: string;
  title: string;
  text: string;
  level: string;
  module: string;
}

export interface DictionaryResult {
  term: string;
  source: string;
  title: string;
  definition: string;
  module: string;
}

export interface LexiconResult {
  term: string;
  source: string;
  title: string;
  definition: string;
  module: string;
}

export async function listModules(path?: string): Promise<ModuleInfo[]> {
  return invoke("list_esword_modules", { path: path ?? null });
}

export async function listBooks(): Promise<BookInfo[]> {
  return invoke("list_bible_books");
}

export async function getChapter(
  modulePath: string,
  bookNumber: number,
  chapter: number,
): Promise<ChapterResult> {
  return invoke("get_bible_chapter", {
    modulePath,
    bookNumber,
    chapter,
  });
}

export async function getCommentaries(
  modulePath: string,
  bookNumber: number,
  chapter: number,
  verse: number,
): Promise<CommentaryResult[]> {
  return invoke("get_verse_commentaries", {
    modulePath,
    bookNumber,
    chapter,
    verse,
  });
}

export async function searchDictionary(
  modulePath: string,
  term: string,
  limit = 20,
): Promise<DictionaryResult[]> {
  return invoke("search_dictionary", { modulePath, term, limit });
}

export async function searchLexicon(
  modulePath: string,
  term: string,
  limit = 20,
): Promise<LexiconResult[]> {
  return invoke("search_lexicon", { modulePath, term, limit });
}

/** Prefer RV1960 when available. */
export function pickDefaultBible(bibles: ModuleInfo[]): ModuleInfo | null {
  if (!bibles.length) return null;
  const prefer = [
    /reina valera \(?1960\)?/i,
    /rv\s*1960/i,
    /rv\s*60/i,
    /rvr1960/i,
  ];
  for (const re of prefer) {
    const hit = bibles.find(
      (b) => re.test(b.title) || re.test(b.abbreviation) || re.test(b.filename),
    );
    if (hit) return hit;
  }
  const plain = bibles.find(
    (b) => !/interlineal|strong|\+/i.test(b.title + b.abbreviation),
  );
  return plain ?? bibles[0];
}
