import { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight, CloudOff, Loader2, Sparkles } from "lucide-react";
import { TabNav } from "./components/TabNav";
import { BiblePanel } from "./components/BiblePanel";
import { CommentaryPanel } from "./components/CommentaryPanel";
import { DictionaryPanel } from "./components/DictionaryPanel";
import { LexiconPanel } from "./components/LexiconPanel";
import {
  getChapter,
  getCommentaries,
  listBooks,
  listModules,
  pickDefaultBible,
  searchDictionary,
  searchLexicon,
  type BookInfo,
  type CommentaryResult,
  type DictionaryResult,
  type LexiconResult,
  type ModuleInfo,
  type VerseRow,
} from "./lib/esword";
import { loadBookmarks, loadNotes, saveBookmarks, saveNotes } from "./lib/storage";
import type { Bookmark, TabId, Verse } from "./types";

function App() {
  const [tab, setTab] = useState<TabId>("biblia");
  const [modules, setModules] = useState<ModuleInfo[]>([]);
  const [books, setBooks] = useState<BookInfo[]>([]);
  const [bibleMod, setBibleMod] = useState<ModuleInfo | null>(null);
  const [cmtMod, setCmtMod] = useState<ModuleInfo | null>(null);
  const [dictMod, setDictMod] = useState<ModuleInfo | null>(null);
  const [lexMod, setLexMod] = useState<ModuleInfo | null>(null);

  const [bookNumber, setBookNumber] = useState(1);
  const [chapter, setChapter] = useState(1);
  const [selectedVerse, setSelectedVerse] = useState<number | null>(1);
  const [search, setSearch] = useState("");
  const [dictQuery, setDictQuery] = useState("");
  const [lexQuery, setLexQuery] = useState("H1254");

  const [verses, setVerses] = useState<VerseRow[]>([]);
  const [chapterTitle, setChapterTitle] = useState("");
  const [loadingChapter, setLoadingChapter] = useState(false);
  const [chapterError, setChapterError] = useState<string | null>(null);

  const [commentaries, setCommentaries] = useState<CommentaryResult[]>([]);
  const [dictEntries, setDictEntries] = useState<DictionaryResult[]>([]);
  const [lexEntries, setLexEntries] = useState<LexiconResult[]>([]);
  const [bootError, setBootError] = useState<string | null>(null);
  const [booting, setBooting] = useState(true);

  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const [notes, setNotes] = useState<Record<string, string>>({});

  const bibles = useMemo(
    () => modules.filter((m) => m.moduleType === "bible"),
    [modules],
  );
  const commentariesMods = useMemo(
    () => modules.filter((m) => m.moduleType === "commentary"),
    [modules],
  );
  const dictionaries = useMemo(
    () => modules.filter((m) => m.moduleType === "dictionary"),
    [modules],
  );
  const lexicons = useMemo(
    () => modules.filter((m) => m.moduleType === "lexicon"),
    [modules],
  );

  const bookMeta = books.find((b) => b.number === bookNumber);
  const maxChapter = bookMeta?.chapters ?? 1;
  const bookName = bookMeta?.name ?? "Génesis";

  // Boot: load modules + books (with timeout so we never stick forever)
  useEffect(() => {
    setBookmarks(loadBookmarks());
    setNotes(loadNotes());

    let cancelled = false;
    const timeout = window.setTimeout(() => {
      if (!cancelled) {
        setBootError(
          "Timeout al escanear e-Sword (20s). ¿La carpeta existe y no está bloqueada?",
        );
        setBooting(false);
      }
    }, 20000);

    (async () => {
      try {
        const [mods, bk] = await Promise.all([listModules(), listBooks()]);
        if (cancelled) return;
        setModules(mods);
        setBooks(bk);
        const defBible = pickDefaultBible(mods.filter((m) => m.moduleType === "bible"));
        setBibleMod(defBible);
        setCmtMod(mods.find((m) => m.moduleType === "commentary") ?? null);
        const dicts = mods.filter((m) => m.moduleType === "dictionary");
        setDictMod(
          dicts.find((d) => /vine|strong|expositivo|pik/i.test(d.title + d.filename)) ??
            dicts[0] ??
            null,
        );
        const lexs = mods.filter((m) => m.moduleType === "lexicon");
        setLexMod(
          lexs.find((l) => /strong/i.test(l.title + l.filename)) ?? lexs[0] ?? null,
        );
        if (!mods.length) {
          setBootError(
            "No se encontraron módulos en C:\\Program Files (x86)\\e-Sword",
          );
        }
      } catch (e) {
        if (!cancelled) {
          setBootError(e instanceof Error ? e.message : String(e));
        }
      } finally {
        window.clearTimeout(timeout);
        if (!cancelled) setBooting(false);
      }
    })();

    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
    };
  }, []);

  // Load chapter when bible/book/chapter changes
  useEffect(() => {
    if (!bibleMod) return;
    let cancelled = false;
    setLoadingChapter(true);
    setChapterError(null);
    getChapter(bibleMod.path, bookNumber, chapter)
      .then((res) => {
        if (cancelled) return;
        setVerses(res.verses);
        setChapterTitle(res.moduleTitle);
        setSelectedVerse(res.verses[0]?.verse ?? null);
      })
      .catch((e) => {
        if (cancelled) return;
        setVerses([]);
        setChapterError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoadingChapter(false);
      });
    return () => {
      cancelled = true;
    };
  }, [bibleMod, bookNumber, chapter]);

  // Load commentaries for selected verse
  useEffect(() => {
    if (!cmtMod || selectedVerse == null) {
      setCommentaries([]);
      return;
    }
    let cancelled = false;
    getCommentaries(cmtMod.path, bookNumber, chapter, selectedVerse)
      .then((res) => {
        if (!cancelled) setCommentaries(res);
      })
      .catch(() => {
        if (!cancelled) setCommentaries([]);
      });
    return () => {
      cancelled = true;
    };
  }, [cmtMod, bookNumber, chapter, selectedVerse]);

  // Dictionary search (debounced light)
  useEffect(() => {
    if (!dictMod) return;
    const q = dictQuery.trim();
    if (!q) {
      setDictEntries([]);
      return;
    }
    const t = setTimeout(() => {
      searchDictionary(dictMod.path, q, 25)
        .then(setDictEntries)
        .catch(() => setDictEntries([]));
    }, 250);
    return () => clearTimeout(t);
  }, [dictMod, dictQuery]);

  // Lexicon search
  useEffect(() => {
    if (!lexMod) return;
    const q = lexQuery.trim();
    if (!q) {
      setLexEntries([]);
      return;
    }
    const t = setTimeout(() => {
      searchLexicon(lexMod.path, q, 25)
        .then(setLexEntries)
        .catch(() => setLexEntries([]));
    }, 250);
    return () => clearTimeout(t);
  }, [lexMod, lexQuery]);

  const filteredVerses = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return verses;
    return verses.filter(
      (v) => v.text.toLowerCase().includes(q) || String(v.verse).includes(q),
    );
  }, [verses, search]);

  const displayVerses: Verse[] = filteredVerses.map((v) => ({
    book: v.book,
    chapter: v.chapter,
    verse: v.verse,
    text: v.text,
    translation: v.translation,
  }));

  const reference =
    selectedVerse != null
      ? `${bookName} ${chapter}:${selectedVerse}`
      : `${bookName} ${chapter}`;

  const noteKey =
    selectedVerse != null ? `${bookNumber}|${chapter}|${selectedVerse}` : "";

  const bookmarkedVerses = useMemo(() => {
    const set = new Set<number>();
    for (const b of bookmarks) {
      const m = b.reference.match(/^(.+?)\s+(\d+):(\d+)$/);
      if (m && m[1] === bookName && Number(m[2]) === chapter) {
        set.add(Number(m[3]));
      }
    }
    return set;
  }, [bookmarks, bookName, chapter]);

  const toggleBookmark = useCallback(
    (verse: number) => {
      const ref = `${bookName} ${chapter}:${verse}`;
      setBookmarks((prev) => {
        const exists = prev.find((b) => b.reference === ref);
        const next = exists
          ? prev.filter((b) => b.reference !== ref)
          : [
              ...prev,
              {
                id: crypto.randomUUID(),
                reference: ref,
                createdAt: new Date().toISOString(),
              },
            ];
        saveBookmarks(next);
        return next;
      });
    },
    [bookName, chapter],
  );

  const handleNoteChange = useCallback(
    (text: string) => {
      if (!noteKey) return;
      setNotes((prev) => {
        const next = { ...prev };
        if (!text.trim()) delete next[noteKey];
        else next[noteKey] = text;
        saveNotes(next);
        return next;
      });
    },
    [noteKey],
  );

  const goChapter = (delta: number) => {
    setChapter((c) => Math.min(maxChapter, Math.max(1, c + delta)));
    setSearch("");
  };

  if (booting) {
    return (
      <div
        className="flex h-full flex-col items-center justify-center gap-3 bg-slate-950 text-slate-300"
        style={{ minHeight: "100vh", padding: 24 }}
      >
        <Loader2 className="animate-spin text-amber-400" size={28} />
        <p className="text-base font-medium text-amber-200">
          Escaneando módulos e-Sword…
        </p>
        <p className="text-xs text-slate-500">
          C:\Program Files (x86)\e-Sword
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-col bg-slate-950 text-slate-100">
      <header className="flex shrink-0 flex-wrap items-center gap-3 border-b border-slate-800 bg-gradient-to-r from-slate-950 via-slate-900 to-slate-950 px-4 py-3">
        <div className="flex items-center gap-2">
          <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-amber-500/20 ring-1 ring-amber-500/40">
            <Sparkles size={18} className="text-amber-300" />
          </div>
          <div>
            <h1 className="text-base font-bold tracking-tight text-slate-50">
              Asignación del Cielo
            </h1>
            <p className="text-[11px] text-slate-500">
              e-Sword local · {modules.length} módulos
            </p>
          </div>
        </div>

        <div className="ml-auto flex flex-wrap items-center gap-2">
          <select
            value={bookNumber}
            onChange={(e) => {
              setBookNumber(Number(e.target.value));
              setChapter(1);
            }}
            className="max-w-[10rem] rounded-lg border border-slate-700 bg-slate-900 px-2 py-1.5 text-sm focus:border-amber-500/50 focus:outline-none"
          >
            {books.map((b) => (
              <option key={b.number} value={b.number}>
                {b.name}
              </option>
            ))}
          </select>

          <div className="flex items-center rounded-lg border border-slate-700 bg-slate-900">
            <button
              type="button"
              onClick={() => goChapter(-1)}
              disabled={chapter <= 1}
              className="px-2 py-1.5 text-slate-400 hover:text-white disabled:opacity-30"
            >
              <ChevronLeft size={18} />
            </button>
            <span className="min-w-[3rem] text-center text-sm font-medium">
              Cap. {chapter}
            </span>
            <button
              type="button"
              onClick={() => goChapter(1)}
              disabled={chapter >= maxChapter}
              className="px-2 py-1.5 text-slate-400 hover:text-white disabled:opacity-30"
            >
              <ChevronRight size={18} />
            </button>
          </div>

          <select
            value={bibleMod?.path ?? ""}
            onChange={(e) => {
              const m = bibles.find((b) => b.path === e.target.value) ?? null;
              setBibleMod(m);
            }}
            className="max-w-[14rem] rounded-lg border border-slate-700 bg-slate-900 px-2 py-1.5 text-sm focus:border-amber-500/50 focus:outline-none"
            title="Biblia"
          >
            {bibles.map((b) => (
              <option key={b.path} value={b.path}>
                {b.abbreviation || b.title}
              </option>
            ))}
          </select>

          <span className="flex items-center gap-1.5 rounded-full bg-emerald-500/10 px-2.5 py-1 text-[11px] font-medium text-emerald-400 ring-1 ring-emerald-500/30">
            <CloudOff size={12} />
            Offline
          </span>
        </div>
      </header>

      {bootError && (
        <div className="border-b border-red-900/50 bg-red-950/40 px-4 py-2 text-sm text-red-300">
          {bootError}
        </div>
      )}

      <TabNav active={tab} onChange={setTab} />

      {/* Module selectors for current tab */}
      {(tab === "comentario" || tab === "diccionario" || tab === "lexico") && (
        <div className="flex shrink-0 items-center gap-2 border-b border-slate-800 bg-slate-900/40 px-4 py-2">
          <span className="text-xs text-slate-500">Módulo:</span>
          {tab === "comentario" && (
            <select
              value={cmtMod?.path ?? ""}
              onChange={(e) =>
                setCmtMod(
                  commentariesMods.find((m) => m.path === e.target.value) ?? null,
                )
              }
              className="max-w-md flex-1 rounded border border-slate-700 bg-slate-900 px-2 py-1 text-sm"
            >
              {commentariesMods.map((m) => (
                <option key={m.path} value={m.path}>
                  {m.abbreviation} — {m.title}
                </option>
              ))}
            </select>
          )}
          {tab === "diccionario" && (
            <select
              value={dictMod?.path ?? ""}
              onChange={(e) =>
                setDictMod(dictionaries.find((m) => m.path === e.target.value) ?? null)
              }
              className="max-w-md flex-1 rounded border border-slate-700 bg-slate-900 px-2 py-1 text-sm"
            >
              {dictionaries.map((m) => (
                <option key={m.path} value={m.path}>
                  {m.abbreviation} — {m.title}
                </option>
              ))}
            </select>
          )}
          {tab === "lexico" && (
            <select
              value={lexMod?.path ?? ""}
              onChange={(e) =>
                setLexMod(lexicons.find((m) => m.path === e.target.value) ?? null)
              }
              className="max-w-md flex-1 rounded border border-slate-700 bg-slate-900 px-2 py-1 text-sm"
            >
              {lexicons.map((m) => (
                <option key={m.path} value={m.path}>
                  {m.abbreviation} — {m.title}
                </option>
              ))}
            </select>
          )}
        </div>
      )}

      <main className="min-h-0 flex-1 p-4">
        {tab === "biblia" && (
          <>
            {loadingChapter && (
              <div className="mb-2 flex items-center gap-2 text-sm text-slate-400">
                <Loader2 size={14} className="animate-spin" /> Cargando capítulo…
              </div>
            )}
            {chapterError && (
              <div className="mb-2 rounded-lg border border-amber-800/50 bg-amber-950/30 px-3 py-2 text-sm text-amber-200">
                {chapterError}
              </div>
            )}
            <BiblePanel
              verses={displayVerses}
              selectedVerse={selectedVerse}
              onSelectVerse={setSelectedVerse}
              search={search}
              onSearchChange={setSearch}
              compare={null}
              bookmarked={bookmarkedVerses}
              onToggleBookmark={toggleBookmark}
              note={noteKey ? (notes[noteKey] ?? "") : ""}
              onNoteChange={handleNoteChange}
            />
          </>
        )}
        {tab === "comentario" && (
          <CommentaryPanel
            reference={reference}
            entries={commentaries.map((c) => ({
              reference: c.reference,
              source: `${c.source} (${c.level})`,
              title: c.title,
              text: c.text,
            }))}
          />
        )}
        {tab === "diccionario" && (
          <DictionaryPanel
            query={dictQuery}
            onQueryChange={setDictQuery}
            entries={dictEntries.map((d) => ({
              term: d.term,
              source: d.source,
              title: d.title,
              definition: d.definition,
            }))}
          />
        )}
        {tab === "lexico" && (
          <LexiconPanel
            query={lexQuery}
            onQueryChange={setLexQuery}
            entries={lexEntries.map((e) => ({
              strongs: e.term,
              lemma: e.term,
              transliteration: e.source,
              language: e.term.toUpperCase().startsWith("G") ? "greek" : "hebrew",
              gloss: e.title,
              definition: e.definition,
            }))}
          />
        )}
      </main>

      <footer className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-slate-800 bg-slate-950 px-4 py-2 text-[11px] text-slate-500">
        <span>
          {reference}
          {verses.length > 0 ? ` · ${verses.length} vv.` : ""}
          {bibleMod ? ` · ${bibleMod.abbreviation}` : ""}
          {chapterTitle ? ` · ${chapterTitle.slice(0, 40)}` : ""}
        </span>
        <span>
          B:{bibles.length} C:{commentariesMods.length} D:{dictionaries.length} L:
          {lexicons.length} · C:\Program Files (x86)\e-Sword
        </span>
      </footer>
    </div>
  );
}

export default App;
