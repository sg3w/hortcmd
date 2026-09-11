// ============================================================
// Layout shell: menu · two panels · command line · F-bar.
// Initializes drives, start directories, and applies the theme.
// ============================================================

import { useEffect, useRef, useState, type MouseEvent } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { Drive } from "@/ipc/bindings";
import {
  hasTauri,
  homeDir,
  listDrives,
  onFsChanged,
  onFsCollision,
  onFsDone,
  onFsProgress,
  onGitStatusReady,
  setWatched,
} from "@/ipc/client";
import { panelOf, usePanes } from "@/store/panesStore";
import { useSession } from "@/store/sessionStore";
import { useTransfers } from "@/store/transfersStore";
import { useOps } from "@/store/opsStore";
import { useSettings, useT } from "@/store/settingsStore";
import { useUi } from "@/store/uiStore";
import { initWindowState } from "@/lib/windowState";
import { useFileColorVars, type ThemeMode } from "@/lib/fileColors";
import { useKeyboard } from "@/features/commander/useKeyboard";
import { useNativeMenu } from "@/features/commander/nativeMenu";
import {
  handleExtractDone,
  reloadBoth,
  startNextQueued,
} from "@/features/commander/fileOps";
import { MenuBar } from "@/features/commander/MenuBar";
import { CommandBar } from "@/features/commander/CommandBar";
import { FunctionBar } from "@/features/commander/FunctionBar";
import { Panel } from "@/features/panel/Panel";
import { SettingsDialog } from "@/components/ui/SettingsDialog";
import { ShortcutsDialog } from "@/components/ui/ShortcutsDialog";
import { AboutDialog } from "@/components/ui/AboutDialog";
import { OperationDialogs } from "@/components/ui/OperationDialogs";
import { FileBrowserDialog } from "@/components/ui/FileBrowserDialog";
import { ExportDialog } from "@/components/ui/ExportDialog";
import { SelectionFileDialog } from "@/components/ui/SelectionFileDialog";
import { RenameDialog } from "@/components/ui/RenameDialog";
import { CompareDialog } from "@/components/ui/CompareDialog";
import { FileCompareDialog } from "@/components/ui/FileCompareDialog";
import { PreviewDialog } from "@/components/ui/PreviewDialog";
import { PropertiesDialog } from "@/components/ui/PropertiesDialog";
import { SearchDialog } from "@/components/ui/SearchDialog";
import { ConnectionsDialog } from "@/features/net/ConnectionsDialog";
import { SshTerminalWindow } from "@/features/net/SshTerminal";
import { TransferTray, TransferWindow } from "@/components/ui/TransferView";

export default function App() {
  const [drives, setDrives] = useState<Drive[]>([]);
  const [ready, setReady] = useState(false);
  const [resolvedTheme, setResolvedTheme] = useState<ThemeMode>("dark");
  const loadDir = usePanes((s) => s.loadDir);
  const activePath = usePanes((s) => panelOf(s, s.active).path);
  const active = usePanes((s) => s.active);
  const theme = useSettings((s) => s.theme);
  const paneSplit = useSettings((s) => s.paneSplit);
  const setPaneSplit = useSettings((s) => s.setPaneSplit);
  const settingsOpen = useUi((s) => s.settingsOpen);
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const t = useT();
  const mainRef = useRef<HTMLDivElement>(null);

  useKeyboard();
  useNativeMenu();

  const startSplitDrag = (e: MouseEvent) => {
    e.preventDefault();
    const onMove = (ev: globalThis.MouseEvent) => {
      const box = mainRef.current?.getBoundingClientRect();
      if (box) setPaneSplit((ev.clientX - box.left) / box.width);
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  // Resolve the theme. "system" follows the OS appearance live.
  useEffect(() => {
    if (theme !== "system") {
      setResolvedTheme(theme);
      return;
    }
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => setResolvedTheme(mq.matches ? "dark" : "light");
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [theme]);

  // Apply the resolved theme to <html> and publish the file colors for it.
  useEffect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
  }, [resolvedTheme]);
  useFileColorVars(resolvedTheme);

  // Forward transfer events from the backend to the transfers store.
  useEffect(() => {
    const offProgress = onFsProgress((p) => useTransfers.getState().apply(p));
    const offCollision = onFsCollision((c) =>
      useOps.getState().pushCollision(c),
    );
    const offDone = onFsDone((d) => {
      // Encrypted archive without/with a wrong password → ask for the password.
      if (handleExtractDone(d)) return;
      const store = useTransfers.getState();
      store.finish(d);
      void reloadBoth();
      // Start the next waiting operation of the copy queue.
      startNextQueued();
      // Remove successful/cancelled operations after showing them briefly.
      if (d.errors.length === 0) {
        setTimeout(() => useTransfers.getState().remove(d.id), 900);
      }
    });
    return () => {
      offProgress();
      offCollision();
      offDone();
    };
  }, []);

  useEffect(() => {
    (async () => {
      void initWindowState(); // Fensterposition/-größe wiederherstellen (Tauri)
      const ds = await listDrives();
      setDrives(ds);
      // Restore the saved session (last tabs), otherwise load home.
      const saved = useSession.getState().session;
      const hasTabs =
        saved && (saved.left.tabs.length > 0 || saved.right.tabs.length > 0);
      if (hasTabs) {
        await usePanes.getState().restoreSession(saved);
      } else {
        const home = await homeDir();
        await Promise.all([loadDir("left", home), loadDir("right", home)]);
      }
      setReady(true);
    })();
  }, [loadDir]);

  // Continuously save the tab layout of both panes to the session (debounced).
  useEffect(() => {
    if (!ready) return;
    let timer: number | undefined;
    const capture = () => useSession.getState().capture();
    const schedule = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(capture, 500);
    };
    const unsub = usePanes.subscribe(schedule);
    window.addEventListener("beforeunload", capture);
    return () => {
      unsub();
      window.removeEventListener("beforeunload", capture);
      window.clearTimeout(timer);
    };
  }, [ready]);

  // Directory watcher: observe the displayed real folders and reload
  // automatically on external changes (debounced, cursor-preserving).
  useEffect(() => {
    const timers = new Map<string, number>();

    const watchedPaths = () => {
      const s = usePanes.getState();
      const paths = (["left", "right"] as const)
        .map((side) => panelOf(s, side))
        .filter((t) => !t.archive && t.path)
        .map((t) => t.path);
      return [...new Set(paths)];
    };

    let lastKey = "";
    const syncWatched = () => {
      const paths = watchedPaths();
      const key = paths.join("|");
      if (key !== lastKey) {
        lastKey = key;
        setWatched(paths);
      }
    };
    syncWatched();
    const unsub = usePanes.subscribe(syncWatched);

    const offChanged = onFsChanged((dir) => {
      const s = usePanes.getState();
      (["left", "right"] as const).forEach((side) => {
        const t = panelOf(s, side);
        if (t.archive || t.path !== dir) return;
        window.clearTimeout(timers.get(side));
        timers.set(
          side,
          window.setTimeout(() => usePanes.getState().refresh(side), 250),
        );
      });
    });

    return () => {
      unsub();
      offChanged();
      timers.forEach((id) => window.clearTimeout(id));
    };
  }, []);

  // The Git status is loaded asynchronously in the background in the backend; as soon as a
  // result arrives, apply it to all matching tabs.
  useEffect(() => {
    return onGitStatusReady((path, git) => {
      usePanes.getState().applyGitStatus(path, git);
    });
  }, []);

  // Intercept window closing during running transfers (TICKET-004): instead
  // of aborting silently, a modal forces a decision.
  useEffect(() => {
    if (!hasTauri) return;
    let unlisten = () => {};
    (async () => {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      unlisten = await getCurrentWindow().onCloseRequested((event) => {
        // Only genuinely running or queued operations may block
        // the close. A transfer that neither reports progress nor
        // waits (e.g. a cancelled password extraction that never becomes
        // "done") must not hold the window at the X permanently.
        const active = useTransfers
          .getState()
          .transfers.some(
            (t) => !t.done && !t.cancelled && (t.queued || t.fileTotal > 0),
          );
        if (active) {
          event.preventDefault();
          useOps.getState().setForceClose(true);
        }
      });
    })();
    return () => unlisten();
  }, []);

  if (!ready) {
    return (
      <div className="flex h-full items-center justify-center text-dim">Lade …</div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Anwendungsmenü (TICKET-040) */}
      <MenuBar />

      {/* Commandbar (Sonderfunktionen + Ansichtsumschalter) */}
      <CommandBar />

      {/* Zwei-Fenster-Bereich (Splitter verschiebbar) */}
      <main ref={mainRef} className="flex min-h-0 flex-1">
        <div
          className="flex min-w-0 shrink-0"
          style={{ width: `${paneSplit * 100}%` }}
        >
          <Panel side="left" drives={drives} />
        </div>
        <div
          onMouseDown={startSplitDrag}
          className="relative w-1 flex-shrink-0 cursor-col-resize bg-edge hover:bg-accent"
        >
          {/* Arbeitsrichtung: Pfeil vom aktiven Fenster zum Zielfenster.
              Kopieren/Verschieben laufen stets in diese Richtung; das Badge
              dient zugleich als Zieh-Handle für die Fenstertrennung. */}
          <div
            onMouseDown={startSplitDrag}
            title={t("dir.hint")}
            className="absolute left-1/2 top-1/2 z-10 flex h-6 w-6 -translate-x-1/2 -translate-y-1/2 cursor-col-resize items-center justify-center rounded-full border border-edge bg-accent text-white shadow-md hover:brightness-110"
          >
            {active === "left" ? (
              <ChevronRight size={15} />
            ) : (
              <ChevronLeft size={15} />
            )}
          </div>
        </div>
        <div className="flex min-w-0 flex-1">
          <Panel side="right" drives={drives} />
        </div>
      </main>

      {/* Kommandozeile */}
      <div className="flex flex-shrink-0 items-center gap-1.5 border-t border-edge bg-panel px-2 py-1">
        <span className="font-mono text-[12px] text-exec">{activePath || "~"}</span>
        <input
          type="text"
          spellCheck={false}
          autoComplete="off"
          className="min-w-0 flex-1 bg-transparent font-mono text-[12px] text-text outline-none"
        />
        {/* Minimierte Transfers erscheinen hier in der Statusleiste */}
        <TransferTray />
      </div>

      <FunctionBar />

      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
      <ShortcutsDialog />
      <AboutDialog />
      <OperationDialogs />
      <FileBrowserDialog />
      <RenameDialog />
      <CompareDialog />
      <ExportDialog />
      <SelectionFileDialog />
      <FileCompareDialog />
      <PreviewDialog />
      <PropertiesDialog />
      <SearchDialog />
      <ConnectionsDialog />
      <SshTerminalWindow />
      <TransferWindow />
    </div>
  );
}
