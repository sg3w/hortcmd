// Funktionstastenleiste F3–F8 im Total-Commander-Stil.

import {
  Copy,
  Eye,
  FolderInput,
  FolderPlus,
  LogOut,
  Pencil,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { usePanes } from "@/store/panesStore";
import { useT } from "@/store/settingsStore";
import type { TransKey } from "@/i18n/dictionaries";
import { runAction, type ActionId } from "./actions";

const KEYS: { num: string; label: TransKey; action: ActionId; Icon: LucideIcon }[] = [
  { num: "F3", label: "fn.view", action: "view", Icon: Eye },
  { num: "F4", label: "fn.edit", action: "edit", Icon: Pencil },
  { num: "F5", label: "fn.copy", action: "copy", Icon: Copy },
  { num: "F6", label: "fn.move", action: "move", Icon: FolderInput },
  { num: "F7", label: "fn.mkdir", action: "mkdir", Icon: FolderPlus },
  { num: "F8", label: "fn.delete", action: "delete", Icon: Trash2 },
  { num: "Alt+F4", label: "fn.quit", action: "quit", Icon: LogOut },
];

export function FunctionBar() {
  const active = usePanes((s) => s.active);
  const t = useT();
  return (
    <footer className="flex flex-shrink-0 gap-0.5 border-t border-edge bg-header px-1.5 py-1">
      {KEYS.map((k) => (
        <button
          key={k.num}
          onClick={() => runAction(k.action, active)}
          className="flex flex-1 items-center justify-center gap-1.5 whitespace-nowrap rounded border border-edge bg-panel px-1.5 py-1 hover:border-accent hover:bg-accent-dim"
        >
          <k.Icon size={14} className="shrink-0 text-accent" aria-hidden />
          <span className="text-[11px] font-bold text-accent">{k.num}</span>
          <span className="text-text">{t(k.label)}</span>
        </button>
      ))}
    </footer>
  );
}
