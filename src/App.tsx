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
  onLeftovers,
  onFsCollision,
  onFsDone,
  onFsProgress,
  onGitStatusReady,
  onNetState,
} from "@/ipc/client";
import { panelOf, usePanes } from "@/store/panesStore";
import { useNet } from "@/features/net/netStore";
import { useSession } from "@/store/sessionStore";
import { useTransfers } from "@/store/transfersStore";
import { useOps } from "@/store/opsStore";
import { useSettings, useT } from "@/store/settingsStore";
import { initWindowState } from "@/lib/windowState";
import { recoveredDomains } from "@/store/persisted/gate";
import type { TransKey } from "@/i18n/dictionaries";
import { notify } from "@/store/noticeStore";
import type { ThemeMode } from "@/lib/fileColors";
import { useKeyboard } from "@/features/commander/useKeyboard";
import { useNativeMenu } from "@/features/commander/nativeMenu";
import {
  confirmLeftovers,
  handleExtractDone,
  reloadBoth,
  startNextQueued,
} from "@/features/commander/fileOps";
import { MenuBar } from "@/features/commander/MenuBar";
import { CommandBar } from "@/features/commander/CommandBar";
import { FunctionBar } from "@/features/commander/FunctionBar";
import { Panel } from "@/features/panel/Panel";
// Eager on purpose: an operation prompt can be raised by any action and
// must never wait on a fetch, and the transfer surface reports on work
// that is already running (TICKET-034).
import { OperationDialogs } from "@/components/ui/OperationDialogs";
import { TransferTray, TransferWindow } from "@/components/ui/TransferView";
import { LazyDialogs } from "@/features/LazyDialogs";
import { useFileColorVars } from "@/store/fileColorRules";

import { useRefresh } from "@/features/panel/useRefresh";

import { McpRuntime } from "@/features/settings/McpRuntime";

export default function App() {
  useRefresh();
  const [drives, setDrives] = useState<Drive[]>([]);
  const [ready, setReady] = useState(false);
  const [resolvedTheme, setResolvedTheme] = useState<ThemeMode>("dark");
  const loadDir = usePanes((s) => s.loadDir);
  const activePath = usePanes((s) => panelOf(s, s.active).path);
  const active = usePanes((s) => s.active);
  const theme = useSettings((s) => s.theme);
  const paneSplit = useSettings((s) => s.paneSplit);
  const setPaneSplit = useSettings((s) => s.setPaneSplit);
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
    // Remains of an interrupted failure-safe transfer (TICKET-029).
    // They may hold the only copy of a file, so the user decides.
    const offLeftovers = onLeftovers((l) => confirmLeftovers(l));
    return () => {
      offProgress();
      offCollision();
      offDone();
      offLeftovers();
    };
  }, []);

  // One notice for everything the persistence gate had to recover at
  // startup (TICKET-039). It runs after the first render, because the
  // stores rehydrate while the modules load — long before anything can
  // show a message — and because a shell that works must never be held
  // up by an explanation.
  useEffect(() => {
    const domains = recoveredDomains();
    if (!domains.length) return;
    notify(
      "left",
      "persist.recovered",
      { domains: domains.map((d) => t(`persist.domain.${d}` as TransKey)).join(", ") },
      "error",
    );
  }, [t]);

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

  // The Git status is loaded asynchronously in the background in the backend; as soon as a
  // result arrives, apply it to all matching tabs.
  useEffect(() => {
    return onGitStatusReady((path, git) => {
      usePanes.getState().applyGitStatus(path, git);
    });
  }, []);

  // The authoritative connection lifecycle (TICKET-032). Every
  // transition — attempt started, cancelled, finished, disconnected,
  // connection lost — arrives as one snapshot, so the manager and the
  // location pickers always show the same thing.
  useEffect(() => {
    return onNetState((states) => useNet.getState().applyStates(states));
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

      <McpRuntime />
      <OperationDialogs />
      <TransferWindow />
      <LazyDialogs />
    </div>
  );
}
