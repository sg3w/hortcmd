// ============================================================
// "About hortcmd" (Help menu and native macOS app menu, TICKET-040).
//
// Shows the actual version — built in from `package.json` and, inside
// Tauri, confirmed against the version the bundle reports — plus the
// MIT license note.
// ============================================================

import { useEffect, useState } from "react";
import { Info } from "lucide-react";
import { AppDialog } from "@/components/ui/AppDialog";
import { hasTauri } from "@/ipc/client";
import { useT } from "@/store/settingsStore";
import { useUi } from "@/store/uiStore";

const DEFAULT_SIZE = { w: 420, h: 300 };

export function AboutDialog() {
  const open = useUi((s) => s.aboutOpen);
  const setOpen = useUi((s) => s.setAboutOpen);
  const t = useT();
  const [version, setVersion] = useState(__APP_VERSION__);

  // Inside Tauri the bundle is the authority on the version.
  useEffect(() => {
    if (!open || !hasTauri()) return;
    let alive = true;
    void import("@tauri-apps/api/app")
      .then((m) => m.getVersion())
      .then((v) => alive && setVersion(v))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [open]);

  return (
    <AppDialog
      dialogKey="about"
      open={open}
      onClose={() => setOpen(false)}
      defaultSize={DEFAULT_SIZE}
      fixedSize
      titleBar={
        <>
          <Info size={16} className="text-accent" aria-hidden />
          <span className="font-medium">{t("menu.help.about")}</span>
        </>
      }
    >
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-5">
        <div className="flex flex-col gap-0.5">
          <span className="text-[20px] font-semibold tracking-wide text-text">
            hortcmd
          </span>
          <span className="font-mono text-[12px] text-dim">
            {t("about.version")} {version}
          </span>
        </div>
        <p className="text-[13px] leading-relaxed text-text">
          {t("about.tagline")}
        </p>
        <p className="text-[12px] leading-relaxed text-dim">
          {t("about.license")}
        </p>
      </div>
    </AppDialog>
  );
}
