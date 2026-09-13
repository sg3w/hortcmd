// ============================================================
// Every lazily loaded dialog, mounted only while it is open
// (TICKET-034).
//
// Before this, all sixteen dialogs were rendered unconditionally and
// self-managed their visibility, which meant the WebView parsed
// Markdown rendering, syntax highlighting and a terminal emulator
// before it could draw two file panes.
//
// Here each one is paired with the store flag that opens it. While that
// flag is false nothing is imported and nothing is in the tree — so no
// invisible modal exists and no dialog can claim a function key the
// user never aimed at it.
//
// This component subscribes to a handful of small booleans. It
// re-renders when one flips, which is exactly when something has to
// mount or unmount anyway.
// ============================================================

import { LazyFeature } from "@/components/ui/LazyFeature";
import { LAZY } from "./lazyFeatures";
import { useRecovery } from "@/store/recoveryStore";
import { useT } from "@/store/settingsStore";
import { useUi } from "@/store/uiStore";
import { useOps } from "@/store/opsStore";
import { useCompareDialog } from "@/store/compareStore";
import { useExportDialog } from "@/store/exportStore";
import { useFileBrowser } from "@/store/fileBrowserStore";
import { useFileCompareDialog } from "@/store/fileCompareStore";
import { usePropsDialog } from "@/store/propsStore";
import { useRenameDialog } from "@/store/renameStore";
import { useSearchDialog } from "@/store/searchStore";
import { useSelectionFile } from "@/store/selectionFileStore";
import { useNet } from "@/features/net/netStore";
import { useTerminals } from "@/features/net/terminalStore";

export function LazyDialogs() {
  const t = useT();

  const recoveryOpen = useRecovery(s => s.open);
  const settingsOpen = useUi((s) => s.settingsOpen);
  const shortcutsOpen = useUi((s) => s.shortcutsOpen);
  const aboutOpen = useUi((s) => s.aboutOpen);
  const previewOpen = useOps((s) => s.preview !== null);
  const browserOpen = useFileBrowser((s) => s.request !== null);
  const renameOpen = useRenameDialog((s) => s.side !== null);
  const compareOpen = useCompareDialog((s) => s.open);
  const exportOpen = useExportDialog((s) => s.side !== null);
  const selectionOpen = useSelectionFile((s) => s.mode !== null);
  const fileCompareOpen = useFileCompareDialog((s) => s.left !== null);
  const propsOpen = usePropsDialog((s) => s.path !== null);
  const searchOpen = useSearchDialog((s) => s.side !== null);
  const connectionsOpen = useNet((s) => s.managerOpen);
  const terminalOpen = useTerminals((s) => s.session !== null);

  return (
    <>
      <LazyFeature open={recoveryOpen} load={LAZY.recovery} label={t("recovery.title")} />
      <LazyFeature
        open={settingsOpen}
        load={LAZY.settings}
        label={t("menu.cfg.settings")}
      />
      <LazyFeature
        open={shortcutsOpen}
        load={LAZY.shortcuts}
        label={t("menu.help.keys")}
      />
      <LazyFeature open={aboutOpen} load={LAZY.about} label={t("menu.help.about")} />
      <LazyFeature
        open={browserOpen}
        load={LAZY.fileBrowser}
        label={t("ctx.openWithOther")}
      />
      <LazyFeature
        open={renameOpen}
        load={LAZY.rename}
        label={t("menu.file.batchRename")}
      />
      <LazyFeature
        open={compareOpen}
        load={LAZY.compareDirs}
        label={t("menu.cmd.compareDirs")}
      />
      <LazyFeature open={exportOpen} load={LAZY.export} label={t("menu.mark.export")} />
      <LazyFeature
        open={selectionOpen}
        load={LAZY.selectionFile}
        label={t("menu.mark.saveFile")}
      />
      <LazyFeature
        open={fileCompareOpen}
        load={LAZY.compareFiles}
        label={t("menu.file.compareFiles")}
      />
      <LazyFeature
        open={previewOpen}
        load={LAZY.preview}
        label={t("menu.file.view")}
      />
      <LazyFeature
        open={propsOpen}
        load={LAZY.properties}
        label={t("menu.file.props")}
      />
      <LazyFeature open={searchOpen} load={LAZY.search} label={t("menu.cmd.search")} />
      <LazyFeature
        open={connectionsOpen}
        load={LAZY.connections}
        label={t("net.title")}
      />
      <LazyFeature
        open={terminalOpen}
        load={LAZY.terminal}
        label={t("menu.net.terminal")}
      />
    </>
  );
}
