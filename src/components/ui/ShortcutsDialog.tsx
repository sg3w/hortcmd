// ============================================================
// "Keyboard shortcuts" overview (Help menu, TICKET-040).
//
// Generated from the same command definition the menu bar uses, so it
// can never list a shortcut the application does not actually have — and
// a command that gains or loses a shortcut updates here automatically.
// ============================================================

import { Keyboard } from "lucide-react";
import { AppDialog } from "@/components/ui/AppDialog";
import {
  COMMANDS,
  MENUS,
  shortcutsOf,
  type Command,
} from "@/features/commander/commands";
import { formatShortcut } from "@/lib/shortcut";
import { useT } from "@/store/settingsStore";
import { useUi } from "@/store/uiStore";

const DEFAULT_SIZE = { w: 560, h: 560 };
const MIN_SIZE = { w: 420, h: 320 };

export function ShortcutsDialog() {
  const open = useUi((s) => s.shortcutsOpen);
  const setOpen = useUi((s) => s.setShortcutsOpen);
  const t = useT();

  // Read while the dialog is open — the set of visible commands is
  // platform-dependent (Quit and Quick Look).
  const groups = open
    ? MENUS.map((cat) => ({ cat, ids: shortcutsOf(cat) })).filter(
        (g) => g.ids.length > 0,
      )
    : [];

  return (
    <AppDialog
      dialogKey="shortcuts"
      open={open}
      onClose={() => setOpen(false)}
      defaultSize={DEFAULT_SIZE}
      minSize={MIN_SIZE}
      titleBar={
        <>
          <Keyboard size={16} className="text-accent" aria-hidden />
          <span className="font-medium">{t("menu.help.keys")}</span>
        </>
      }
    >
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
        {groups.map(({ cat, ids }) => (
          <section key={cat.id} className="flex flex-col gap-1">
            <h3 className="text-[12px] font-medium uppercase tracking-wide text-dim">
              {t(cat.label)}
            </h3>
            <dl className="flex flex-col">
              {ids.map((id) => {
                const cmd = COMMANDS[id] as Command;
                return (
                  <div
                    key={id}
                    className="flex items-baseline gap-3 border-b border-edge/40 py-1 last:border-b-0"
                  >
                    <dt className="min-w-0 flex-1 truncate text-[13px] text-text">
                      {cmd.dialog ? `${t(cmd.label)} …` : t(cmd.label)}
                    </dt>
                    <dd className="shrink-0 rounded border border-edge bg-header px-1.5 py-0.5 font-mono text-[11px] text-text">
                      {formatShortcut(cmd.shortcut!)}
                    </dd>
                  </div>
                );
              })}
            </dl>
          </section>
        ))}
      </div>
    </AppDialog>
  );
}
