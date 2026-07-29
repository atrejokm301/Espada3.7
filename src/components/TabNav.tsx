import { BookOpen, MessageSquareText, BookMarked, Languages } from "lucide-react";
import type { TabId } from "../types";

const TABS: { id: TabId; label: string; icon: typeof BookOpen }[] = [
  { id: "biblia", label: "Biblia", icon: BookOpen },
  { id: "comentario", label: "Comentario", icon: MessageSquareText },
  { id: "diccionario", label: "Diccionario", icon: BookMarked },
  { id: "lexico", label: "Léxico", icon: Languages },
];

interface Props {
  active: TabId;
  onChange: (tab: TabId) => void;
}

export function TabNav({ active, onChange }: Props) {
  return (
    <nav className="flex gap-1 border-b border-slate-700/80 bg-slate-900/80 px-2 pt-1">
      {TABS.map(({ id, label, icon: Icon }) => {
        const isActive = active === id;
        return (
          <button
            key={id}
            type="button"
            onClick={() => onChange(id)}
            className={`flex items-center gap-2 rounded-t-lg px-4 py-2.5 text-sm font-medium transition-colors ${
              isActive
                ? "bg-slate-800 text-amber-300 border border-b-0 border-slate-700"
                : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
            }`}
          >
            <Icon size={16} strokeWidth={2} />
            {label}
          </button>
        );
      })}
    </nav>
  );
}
