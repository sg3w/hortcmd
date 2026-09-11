// ============================================================
// Settings ▸ Files (TICKET-035).
//
// How files are opened and what F4 means.
// ============================================================

import { usePanes } from "@/store/panesStore";
import { useSettings, useT } from "@/store/settingsStore";
import { Toggle } from "./controls";
import {
  GitBranch,
  EyeOff,
} from "lucide-react";

export function FilesForm() {
  const t = useT();
  const hide = useSettings((s) => s.hideSystemFiles);
  const setHide = useSettings((s) => s.setHideSystemFiles);
  const gitEnabled = useSettings((s) => s.gitEnabled);
  const setGitEnabled = useSettings((s) => s.setGitEnabled);
  const rebuildAll = usePanes((s) => s.rebuildAll);
  const reloadGit = usePanes((s) => s.reloadGit);

  return (
    <div className="flex flex-col gap-4">
      <Toggle
        checked={hide}
        onChange={(v) => {
          setHide(v);
          rebuildAll(); // rebuild the displayed lists immediately.
        }}
        icon={EyeOff}
        label={t("settings.hideSystem")}
        hint={t("settings.hideSystem.hint")}
      />
      <Toggle
        checked={gitEnabled}
        onChange={(v) => {
          setGitEnabled(v);
          reloadGit(); // reload/clear the git status in both windows.
        }}
        icon={GitBranch}
        label={t("settings.git")}
        hint={t("settings.git.hint")}
      />
    </div>
  );
}
