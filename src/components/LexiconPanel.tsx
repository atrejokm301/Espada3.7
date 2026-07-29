import { Search } from "lucide-react";
import type { LexiconEntry } from "../types";

interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  entries: LexiconEntry[];
}

export function LexiconPanel({ query, onQueryChange, entries }: Props) {
  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <div className="relative">
        <Search
          size={16}
          className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-slate-500"
        />
        <input
          type="search"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          placeholder="Strong’s, lema o glosa (H1254, bara, Elohim…)"
          className="w-full rounded-lg border border-slate-700 bg-slate-900 py-2.5 pl-9 pr-3 text-sm text-slate-100 placeholder:text-slate-500 focus:border-amber-500/60 focus:outline-none focus:ring-1 focus:ring-amber-500/40"
        />
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto">
        {entries.length === 0 ? (
          <p className="p-6 text-center text-sm text-slate-500">Sin resultados en el léxico de muestra.</p>
        ) : (
          entries.map((entry) => (
            <article
              key={entry.strongs}
              className="rounded-xl border border-slate-800 bg-slate-900/60 p-5"
            >
              <div className="flex flex-wrap items-baseline gap-2">
                <span className="rounded bg-violet-500/20 px-2 py-0.5 font-mono text-sm font-semibold text-violet-300">
                  {entry.strongs}
                </span>
                <span className="text-xl text-slate-100" lang="he">
                  {entry.lemma}
                </span>
                <span className="text-sm italic text-slate-400">{entry.transliteration}</span>
                <span className="rounded-full bg-slate-800 px-2 py-0.5 text-[10px] uppercase tracking-wide text-slate-400">
                  {entry.language === "hebrew" ? "Hebreo" : "Griego"}
                </span>
              </div>
              <p className="mt-2 text-sm font-medium text-amber-300/90">{entry.gloss}</p>
              <p className="mt-2 text-[15px] leading-relaxed text-slate-300">{entry.definition}</p>
              {entry.occurrences && entry.occurrences.length > 0 && (
                <p className="mt-3 text-xs text-slate-500">
                  Ejemplos: {entry.occurrences.join(" · ")}
                </p>
              )}
            </article>
          ))
        )}
      </div>
    </div>
  );
}
