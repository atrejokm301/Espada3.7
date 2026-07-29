import type { CommentaryEntry } from "../types";

interface Props {
  reference: string;
  entries: CommentaryEntry[];
}

export function CommentaryPanel({ reference, entries }: Props) {
  return (
    <div className="flex h-full min-h-0 flex-col gap-3 overflow-y-auto">
      <div className="rounded-lg border border-slate-800 bg-slate-900/50 px-4 py-3">
        <p className="text-sm text-slate-400">
          Comentario para{" "}
          <span className="font-medium text-amber-300">{reference}</span>
        </p>
        <p className="mt-1 text-xs text-slate-500">
          Desde tus módulos e-Sword (.cmti). Cambia el módulo en la barra superior.
        </p>
      </div>

      {entries.length === 0 ? (
        <div className="rounded-xl border border-dashed border-slate-700 p-8 text-center text-sm text-slate-500">
          No hay comentario de muestra para este versículo.
          <br />
          Prueba Génesis 1:1, 1:3 o 1:26–27.
        </div>
      ) : (
        entries.map((entry, i) => (
          <article
            key={`${entry.source}-${i}`}
            className="rounded-xl border border-slate-800 bg-slate-900/60 p-5"
          >
            <header className="mb-3 border-b border-slate-800 pb-3">
              <h2 className="text-base font-semibold text-slate-100">{entry.title}</h2>
              <p className="mt-1 text-xs text-sky-400/90">
                {entry.source} · {entry.reference}
              </p>
            </header>
            <div className="whitespace-pre-wrap text-[15px] leading-relaxed text-slate-300">
              {entry.text}
            </div>
          </article>
        ))
      )}
    </div>
  );
}
