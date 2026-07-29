import { Search } from "lucide-react";
import type { DictionaryEntry } from "../types";

interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  entries: DictionaryEntry[];
}

export function DictionaryPanel({ query, onQueryChange, entries }: Props) {
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
          placeholder="Buscar término (crear, Dios, luz, imagen…)"
          className="w-full rounded-lg border border-slate-700 bg-slate-900 py-2.5 pl-9 pr-3 text-sm text-slate-100 placeholder:text-slate-500 focus:border-amber-500/60 focus:outline-none focus:ring-1 focus:ring-amber-500/40"
        />
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto">
        {entries.length === 0 ? (
          <p className="p-6 text-center text-sm text-slate-500">Sin resultados.</p>
        ) : (
          entries.map((entry) => (
            <article
              key={`${entry.term}-${entry.source}`}
              className="rounded-xl border border-slate-800 bg-slate-900/60 p-5"
            >
              <h2 className="text-lg font-semibold capitalize text-amber-300">{entry.term}</h2>
              <p className="mt-0.5 text-xs text-sky-400/90">{entry.title}</p>
              <div className="mt-3 whitespace-pre-wrap text-[15px] leading-relaxed text-slate-300">
                {entry.definition}
              </div>
            </article>
          ))
        )}
      </div>
    </div>
  );
}
