import { Bookmark, BookmarkCheck, Search } from "lucide-react";
import type { Verse, VersionCompare } from "../types";

interface Props {
  verses: Verse[];
  selectedVerse: number | null;
  onSelectVerse: (verse: number) => void;
  search: string;
  onSearchChange: (q: string) => void;
  compare: VersionCompare | null;
  bookmarked: Set<number>;
  onToggleBookmark: (verse: number) => void;
  note: string;
  onNoteChange: (note: string) => void;
}

export function BiblePanel({
  verses,
  selectedVerse,
  onSelectVerse,
  search,
  onSearchChange,
  compare,
  bookmarked,
  onToggleBookmark,
  note,
  onNoteChange,
}: Props) {
  return (
    <div className="flex h-full min-h-0 flex-col gap-3 lg:flex-row">
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="mb-3 flex items-center gap-2">
          <div className="relative flex-1">
            <Search
              size={16}
              className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-slate-500"
            />
            <input
              type="search"
              value={search}
              onChange={(e) => onSearchChange(e.target.value)}
              placeholder="Buscar en el capítulo…"
              className="w-full rounded-lg border border-slate-700 bg-slate-900 py-2 pl-9 pr-3 text-sm text-slate-100 placeholder:text-slate-500 focus:border-amber-500/60 focus:outline-none focus:ring-1 focus:ring-amber-500/40"
            />
          </div>
          <span className="shrink-0 rounded-md bg-slate-800 px-2 py-1 text-xs text-slate-400">
            {verses.length} vv.
          </span>
        </div>

        <div className="min-h-0 flex-1 space-y-1 overflow-y-auto rounded-xl border border-slate-800 bg-slate-900/40 p-3">
          {verses.length === 0 ? (
            <p className="p-6 text-center text-sm text-slate-500">
              No hay versículos para este libro/capítulo en el módulo seleccionado.
              <br />
              Prueba otra Biblia o un libro que esté en ese módulo (p. ej. solo NT).
            </p>
          ) : (
            verses.map((v) => {
              const selected = selectedVerse === v.verse;
              const isBm = bookmarked.has(v.verse);
              return (
                <div
                  key={v.verse}
                  role="button"
                  tabIndex={0}
                  onClick={() => onSelectVerse(v.verse)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") onSelectVerse(v.verse);
                  }}
                  className={`group flex gap-3 rounded-lg px-3 py-2.5 text-left transition-colors ${
                    selected
                      ? "bg-amber-500/15 ring-1 ring-amber-500/40"
                      : "hover:bg-slate-800/80"
                  }`}
                >
                  <span className="mt-0.5 w-6 shrink-0 text-right text-sm font-semibold text-amber-400/90">
                    {v.verse}
                  </span>
                  <p className="flex-1 text-[15px] leading-relaxed text-slate-100">
                    {v.text}
                  </p>
                  <button
                    type="button"
                    title={isBm ? "Quitar marcador" : "Marcar"}
                    onClick={(e) => {
                      e.stopPropagation();
                      onToggleBookmark(v.verse);
                    }}
                    className={`shrink-0 self-start rounded p-1 opacity-0 transition-opacity group-hover:opacity-100 ${
                      isBm ? "text-amber-400 opacity-100" : "text-slate-500 hover:text-amber-300"
                    }`}
                  >
                    {isBm ? <BookmarkCheck size={16} /> : <Bookmark size={16} />}
                  </button>
                </div>
              );
            })
          )}
        </div>
      </div>

      <aside className="flex w-full shrink-0 flex-col gap-3 lg:w-80">
        {compare && selectedVerse === 1 && (
          <div className="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
            <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-amber-400/90">
              Comparar versiones · {compare.reference}
            </h3>
            <ul className="space-y-3">
              {compare.versions.map((ver) => (
                <li key={ver.translation}>
                  <span className="text-xs font-medium text-sky-400">{ver.translation}</span>
                  <p className="mt-0.5 text-sm leading-relaxed text-slate-300">{ver.text}</p>
                </li>
              ))}
            </ul>
          </div>
        )}

        <div className="flex min-h-[140px] flex-1 flex-col rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400">
            Nota personal
            {selectedVerse != null ? ` · v.${selectedVerse}` : ""}
          </h3>
          {selectedVerse == null ? (
            <p className="text-sm text-slate-500">Selecciona un versículo para anotar.</p>
          ) : (
            <textarea
              value={note}
              onChange={(e) => onNoteChange(e.target.value)}
              placeholder="Escribe tu nota de estudio (se guarda en este equipo)…"
              className="min-h-[100px] flex-1 resize-none rounded-lg border border-slate-700 bg-slate-950/50 p-3 text-sm text-slate-200 placeholder:text-slate-600 focus:border-amber-500/50 focus:outline-none"
            />
          )}
        </div>
      </aside>
    </div>
  );
}
