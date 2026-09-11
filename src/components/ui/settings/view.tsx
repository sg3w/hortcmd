// ============================================================
// Settings ▸ File view (TICKET-035).
//
// What the list shows and how it sorts: hidden files, folders first,
// the extension and permission columns, date and size formats.
// ============================================================

import { usePanes } from "@/store/panesStore";
import { useSettings, useT } from "@/store/settingsStore";
import type { DateFormat, SizeFormat } from "@/store/settingsStore";
import { Field, Segmented, Toggle } from "./controls";
import {
  FolderTree,
  FileType,
  Lock,
  Ruler,
  Calendar,
} from "lucide-react";

export function ViewForm() {
  const t = useT();
  const foldersFirst = useSettings((s) => s.foldersFirst);
  const setFoldersFirst = useSettings((s) => s.setFoldersFirst);
  const showExt = useSettings((s) => s.showExtColumn);
  const setShowExt = useSettings((s) => s.setShowExtColumn);
  const showPerms = useSettings((s) => s.showPermissions);
  const setShowPerms = useSettings((s) => s.setShowPermissions);
  const sizeFormat = useSettings((s) => s.sizeFormat);
  const setSizeFormat = useSettings((s) => s.setSizeFormat);
  const dateFormat = useSettings((s) => s.dateFormat);
  const setDateFormat = useSettings((s) => s.setDateFormat);
  const rebuildAll = usePanes((s) => s.rebuildAll);

  return (
    <div className="flex flex-col gap-5">
      <Toggle
        checked={foldersFirst}
        onChange={(v) => {
          setFoldersFirst(v);
          rebuildAll(); // rebuild the sorting immediately.
        }}
        icon={FolderTree}
        label={t("settings.foldersFirst")}
        hint={t("settings.foldersFirst.hint")}
      />
      <Toggle
        checked={showExt}
        onChange={setShowExt}
        icon={FileType}
        label={t("settings.showExt")}
        hint={t("settings.showExt.hint")}
      />
      <Toggle
        checked={showPerms}
        onChange={setShowPerms}
        icon={Lock}
        label={t("settings.showPerms")}
        hint={t("settings.showPerms.hint")}
      />

      <Field icon={Ruler} label={t("settings.sizeFormat")}>
        <Segmented<SizeFormat>
          value={sizeFormat}
          onChange={setSizeFormat}
          options={[
            { value: "auto", label: t("settings.sizeFormat.auto") },
            { value: "bytes", label: t("settings.sizeFormat.bytes") },
          ]}
        />
      </Field>

      <Field icon={Calendar} label={t("settings.dateFormat")}>
        <Segmented<DateFormat>
          value={dateFormat}
          onChange={setDateFormat}
          options={[
            { value: "medium", label: t("settings.dateFormat.medium") },
            { value: "short", label: t("settings.dateFormat.short") },
            { value: "iso", label: t("settings.dateFormat.iso") },
          ]}
        />
      </Field>
    </div>
  );
}
