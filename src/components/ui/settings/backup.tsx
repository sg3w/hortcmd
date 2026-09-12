// ============================================================
// Settings ▸ General ▸ Back up settings (TICKET-039).
//
// Export, import and reset for everything this application keeps in
// `localStorage`. Three buttons, and two of them destroy something, so
// both say exactly what before they do it:
//
//   * Import shows how many settings a file carries and where it came
//     from, and refuses the file outright — before touching anything —
//     when it is not ours, comes from a newer schema, or contains a
//     field whose name suggests a secret.
//   * Reset lists the domains it will clear *and* the ones it will not,
//     because "reset settings" is exactly ambiguous enough that
//     somebody will fear for their connection profiles.
// ============================================================

import { useState } from "react";
import { Download, RotateCcw, Upload } from "lucide-react";
import { readPreview, writeTextFile } from "@/ipc/client";
import { openFileBrowser } from "@/store/fileBrowserStore";
import { useSettings, useT } from "@/store/settingsStore";
import { PERSISTED_KEYS, type PersistedDomain } from "@/store/persisted/gate";
import { buildExport, readImport, type ImportRefusal } from "@/store/persisted/transfer";
import { fillVars } from "@/store/noticeStore";
import { useOps } from "@/store/opsStore";
import type { TransKey } from "@/i18n/dictionaries";

/** The default file name an export is offered under. */
const FILE_NAME = "hortcmd-settings.json";

/** Big enough for any plausible preferences file, small enough to refuse a disk image. */
const MAX_IMPORT_BYTES = 1024 * 1024;

/** What a reset clears, in the order the confirmation lists them. */
const RESET_DOMAINS: PersistedDomain[] = [
  "settings",
  "session",
  "history",
  "dialogSizes",
  "window",
];

export function BackupSection() {
  const t = useT();
  const [message, setMessage] = useState<{ text: string; bad: boolean } | null>(null);
  const requestConfirm = useOps((s) => s.requestConfirm);

  const say = (key: TransKey, vars?: Record<string, string | number>, bad = false) =>
    setMessage({ text: fillVars(t(key), vars), bad });

  const doExport = () => {
    openFileBrowser({
      title: t("persist.export"),
      selectMode: "folder",
      confirmLabel: t("persist.export"),
      onPick: (folder) => {
        void (async () => {
          const sep = folder.includes("\\") ? "\\" : "/";
          const path = `${folder.replace(/[/\\]$/, "")}${sep}${FILE_NAME}`;
          try {
            // The store's own state, minus the setters — the same shape
            // that goes into localStorage.
            const { ...state } = useSettings.getState();
            const data: Record<string, unknown> = {};
            for (const [key, value] of Object.entries(state)) {
              if (typeof value !== "function") data[key] = value;
            }
            await writeTextFile(path, buildExport(data), true);
            say("persist.export.done", { path });
          } catch (e) {
            say("persist.export.failed", { error: String(e) }, true);
          }
        })();
      },
    });
  };

  const refusalText = (refusal: ImportRefusal): string => {
    switch (refusal.reason) {
      case "notJson":
        return t("persist.import.notJson");
      case "notOurs":
        return t("persist.import.notOurs");
      case "tooNew":
        return fillVars(t("persist.import.tooNew"), { version: refusal.version });
      case "hasSecrets":
        return fillVars(t("persist.import.hasSecrets"), {
          fields: refusal.fields.slice(0, 3).join(", "),
        });
    }
  };

  const doImport = () => {
    openFileBrowser({
      title: t("persist.import"),
      selectMode: "file",
      confirmLabel: t("persist.import"),
      onPick: (path) => {
        void (async () => {
          let text: string;
          try {
            const preview = await readPreview(path, MAX_IMPORT_BYTES);
            text = preview.text ?? "";
          } catch (e) {
            say("persist.import.failed", { reason: String(e) }, true);
            return;
          }

          const result = readImport(text);
          if (!result.ok) {
            say("persist.import.failed", { reason: refusalText(result.refusal) }, true);
            return;
          }
          // Nothing has been applied yet: the confirmation is shown
          // against a value that is already validated, so saying yes
          // cannot fail halfway.
          requestConfirm({
            title: t("persist.import.confirm.title"),
            message: fillVars(t("persist.import.confirm.body"), {
              count: result.count,
              path,
            }),
            confirmLabel: t("persist.import"),
            icon: Upload,
            onConfirm: () => {
              useSettings.setState(result.settings);
              say("persist.import.done", { count: result.count });
            },
          });
        })();
      },
    });
  };

  const doReset = () => {
    for (const domain of RESET_DOMAINS) {
      try {
        localStorage.removeItem(PERSISTED_KEYS[domain]);
      } catch {
        /* storage unavailable — there was nothing to clear */
      }
    }
    say("persist.reset.done");
    // A reload is what makes the reset complete: the stores are already
    // holding the old values, and re-deriving every one of them by hand
    // is how a reset ends up half applied.
    window.location.reload();
  };

  const askReset = () =>
    requestConfirm({
      title: t("persist.reset.title"),
      message: [
        t("persist.reset.body"),
        ...RESET_DOMAINS.map((d) => `  • ${t(`persist.domain.${d}` as TransKey)}`),
        "",
        t("persist.reset.keeps"),
      ].join("\n"),
      confirmLabel: t("persist.reset.confirm"),
      danger: true,
      onConfirm: doReset,
    });

  return (
    <div className="flex flex-col gap-2">
      <span className="text-[11px] text-dim">{t("persist.section.hint")}</span>
      <div className="flex flex-wrap gap-2">
        <SectionButton icon={Download} label={t("persist.export")} onClick={doExport} />
        <SectionButton icon={Upload} label={t("persist.import")} onClick={doImport} />
        <SectionButton
          icon={RotateCcw}
          label={t("persist.reset")}
          onClick={askReset}
          danger
        />
      </div>

      {message && (
        <span
          role="status"
          className={`text-[11px] ${message.bad ? "text-red-400" : "text-dim"}`}
        >
          {message.text}
        </span>
      )}
    </div>
  );
}

function SectionButton({
  icon: Icon,
  label,
  onClick,
  danger,
}: {
  icon: typeof Download;
  label: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex items-center gap-1.5 rounded border px-2 py-1 text-[12px] ${
        danger
          ? "border-red-500/40 text-red-400 hover:bg-red-500/10"
          : "border-edge text-text hover:bg-panel-inactive"
      }`}
    >
      <Icon size={13} />
      {label}
    </button>
  );
}
