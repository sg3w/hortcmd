import { useEffect, useRef, useState } from "react";
import { commandStart, commandStatus, commandCancel, hasTauri } from "@/ipc/client";
import type { CommandOutput } from "@/ipc/bindings/CommandOutput";
import { useCapability } from "@/store/capsStore";
import { useT } from "@/store/settingsStore";
import { describeErrorFull } from "@/store/errorText";
import { isDialogKey } from "@/lib/keys";

export default function CommandLine({ path }: { path: string }) {
  const t = useT();
  const local = useCapability(path, "localTerminal");
  const [command, setCommand] = useState("");
  const [id, setId] = useState<number | null>(null);
  const [output, setOutput] = useState<CommandOutput | null>(null);
  const [error, setError] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [starting, setStarting] = useState(false);
  const busy = useRef(false);
  const input = useRef<HTMLInputElement>(null);
  const running = starting || (id !== null && !output?.done);

  useEffect(() => {
    if (id === null || output?.done) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const next = await commandStatus(id!);
        if (disposed) return;
        setOutput(next);
        setError("");
        if (next.done) { busy.current = false; return; }
      } catch (e) { if (!disposed) setError(describeErrorFull(e)); }
      if (!disposed) timer = setTimeout(poll, 250);
    }
    void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, [id, output?.done]);

  async function start() {
    if (busy.current || !local || !hasTauri || !command.trim()) return;
    busy.current = true;
    setStarting(true);
    setId(null);
    setOutput(null);
    setError("");
    setSubmitted(`${path} > ${command}`);
    try { setId(await commandStart(path, command)); }
    catch (e) { setError(describeErrorFull(e)); busy.current = false; setId(null); }
    finally { setStarting(false); }
  }
  async function cancel() {
    if (id === null) return;
    try { await commandCancel(id); }
    catch (e) { setError(describeErrorFull(e)); }
  }

  return <div className="min-w-0 flex-1 text-[12px]">
    <div className="flex items-center gap-2">
      <input ref={input} aria-label={t("command.input")} title={t(local && hasTauri ? "command.hint" : "command.localOnly")}
        disabled={!local || !hasTauri || running} value={command} onChange={e => setCommand(e.target.value)}
        onKeyDown={e => {
          if (e.repeat || e.nativeEvent.isComposing || e.defaultPrevented || isDialogKey(e.nativeEvent) || document.querySelector('[role="dialog"], [role="menu"], [role="alertdialog"]')) return;
          if (e.key === "Enter" && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) { e.preventDefault(); e.stopPropagation(); void start(); }
          if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); setCommand(""); }
        }} type="text" spellCheck={false} autoComplete="off" className="min-w-0 flex-1 bg-transparent font-mono text-text outline-none" />
      {running && <button disabled={starting} onClick={() => void cancel()}>{t("command.stop")}</button>}
      {!running && submitted && <button onClick={() => { setSubmitted(""); setOutput(null); setId(null); setError(""); input.current?.focus(); }}>{t("command.clear")}</button>}
    </div>
    {submitted && <section aria-label={t("command.output")} className="max-h-48 overflow-auto border-t border-edge">
      <div className="break-all font-mono text-dim">{submitted}</div>
      <div role="status">{running ? t("command.running") : output?.cancelled ? t("command.cancelled") : output?.done ? t("command.exit").replace("{code}", String(output.exit_code ?? "—")) : ""}</div>
      {output?.stdout && <pre className="whitespace-pre-wrap break-all">{output.stdout}</pre>}
      {output?.stderr && <pre className="whitespace-pre-wrap break-all text-danger">{output.stderr}</pre>}
      {output?.truncated && <p>{t("command.truncated")}</p>}
      {(error || output?.error) && <p role="alert">{error || describeErrorFull(output?.error)}</p>}
    </section>}
  </div>;
}
