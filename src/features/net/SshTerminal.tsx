// ============================================================
// SSH terminal window (ticket 026).
//
// Shows the shell session of `terminalStore` in a dialog with an
// xterm.js terminal: keystrokes go to the backend via `ssh_send`,
// output arrives as base64 chunks over the `ssh-term-data` event, and
// every size change is reported to the remote side (`ssh_resize`).
// Session end and connection loss are written into the terminal itself
// before the window reports it as closed.
// ============================================================

import { useEffect, useRef } from "react";
import { Terminal as TerminalIcon } from "lucide-react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import {
  onSshTermClosed,
  onSshTermData,
  sshClose,
  sshOpenShell,
  sshResize,
  sshSend,
} from "@/ipc/client";
import { AppDialog } from "@/components/ui/AppDialog";
import { useT } from "@/store/settingsStore";
import { useTerminals, type TermSession } from "@/features/net/terminalStore";

/** Decodes a base64 output chunk into the raw bytes xterm expects
 *  (multi-byte characters may straddle chunk boundaries). */
function decode(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** Reads a theme token off `<html>` (xterm needs literal colors). */
function cssVar(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || fallback;
}

function terminalTheme() {
  return {
    background: cssVar("--panel-bg", "#2b2d31"),
    foreground: cssVar("--text", "#dcddde"),
    cursor: cssVar("--accent", "#3b82f6"),
    selectionBackground: cssVar("--accent-dim", "#2c5aa0"),
  };
}

export function SshTerminalWindow() {
  const session = useTerminals((s) => s.session);
  const close = useTerminals((s) => s.close);
  const t = useT();

  const status =
    session?.status === "closed"
      ? (session.reason ?? t("term.closed"))
      : session?.status === "opening"
        ? t("term.opening")
        : t("term.hint");

  return (
    <AppDialog
      dialogKey="ssh-terminal"
      open={session !== null}
      onClose={close}
      keepEscape
      titleBar={
        <>
          <TerminalIcon size={15} className="text-accent" />
          {t("term.title")}
          <span className="truncate font-mono text-[11px] text-dim">
            {session?.title}
          </span>
        </>
      }
      defaultSize={{ w: 760, h: 460 }}
      minSize={{ w: 420, h: 240 }}
      footer={
        <div className="flex flex-shrink-0 items-center gap-2 border-t border-edge bg-header px-3 py-1.5">
          <span className="truncate text-[11px] text-dim">{status}</span>
          <button
            onClick={close}
            className="ml-auto rounded border border-edge bg-panel px-3 py-0.5 text-[12px] text-text hover:border-accent"
          >
            {t("term.close")}
          </button>
        </div>
      }
    >
      <div className="relative min-h-0 flex-1 bg-panel">
        {/* Keyed by uid so a new session always starts with a fresh
            terminal instead of inheriting the previous scrollback. */}
        {session && <TerminalPane key={session.uid} session={session} />}
      </div>
    </AppDialog>
  );
}

function TerminalPane({ session }: { session: TermSession }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  // Backend session id — read inside event handlers, hence a ref.
  const sshIdRef = useRef<string | null>(null);

  // Create the terminal and open the remote shell for its initial size.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const term = new Terminal({
      fontFamily: cssVar("--font-mono", "monospace"),
      fontSize: 12,
      cursorBlink: true,
      scrollback: 5000,
      theme: terminalTheme(),
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;

    let disposed = false;
    void (async () => {
      try {
        const id = await sshOpenShell(session.address, term.cols, term.rows);
        // The window may have been closed while the shell was opening.
        if (disposed) {
          void sshClose(id);
          return;
        }
        sshIdRef.current = id;
        useTerminals.getState().attach(session.uid, id);
        term.focus();
      } catch (e) {
        if (disposed) return;
        term.write(`\r\n\x1b[31m${String(e)}\x1b[0m\r\n`);
        useTerminals.getState().fail(session.uid, String(e));
      }
    })();

    const input = term.onData((data) => {
      const id = sshIdRef.current;
      if (id) void sshSend(id, data);
    });

    return () => {
      disposed = true;
      input.dispose();
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [session.uid, session.address]);

  // Shell output and session end for exactly this session.
  useEffect(() => {
    const offData = onSshTermData((d) => {
      if (d.id !== sshIdRef.current) return;
      termRef.current?.write(decode(d.data));
    });
    const offClosed = onSshTermClosed((c) => {
      if (c.id !== sshIdRef.current) return;
      sshIdRef.current = null;
      termRef.current?.write(`\r\n\x1b[33m${c.reason}\x1b[0m\r\n`);
      useTerminals.getState().markClosed(c.id, c.reason);
    });
    return () => {
      offData();
      offClosed();
    };
  }, []);

  // Follow the window size and report every change to the remote side.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const apply = () => {
      const term = termRef.current;
      if (!term) return;
      fitRef.current?.fit();
      const id = sshIdRef.current;
      if (id) void sshResize(id, term.cols, term.rows);
    };
    const observer = new ResizeObserver(apply);
    observer.observe(host);
    return () => observer.disconnect();
  }, [session.status]);

  // Follow the app theme (light/dark switch while a shell is open).
  useEffect(() => {
    const observer = new MutationObserver(() => {
      const term = termRef.current;
      if (term) term.options.theme = terminalTheme();
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    return () => observer.disconnect();
  }, []);

  return (
    <div
      ref={hostRef}
      onMouseDown={() => termRef.current?.focus()}
      className="absolute inset-0 p-1.5"
    />
  );
}
