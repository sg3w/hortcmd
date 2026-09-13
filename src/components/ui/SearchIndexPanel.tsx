import { useEffect, useRef, useState } from "react";
import { indexList, indexCreate, indexUnlock, indexStep, indexCancel, indexControl, indexQuery } from "@/ipc/client";
import type { IndexInfo } from "@/ipc/bindings/IndexInfo";
import type { IndexPage } from "@/ipc/bindings/IndexPage";
import { useT } from "@/store/settingsStore";
import { describeErrorFull } from "@/store/errorText";
import type { TransKey } from "@/i18n/dictionaries";
const states: Record<string, TransKey> = {
  locked: "index.locked", paused: "index.paused", building: "index.building", stale: "index.stale",
  snapshot: "index.snapshot", resource_pause: "index.resource", limited: "index.limited", offline: "index.offline", incomplete: "index.incomplete",
};
let serial = 0;
export function SearchIndexPanel({ initialRoot, onReveal }: { initialRoot: string; onReveal: (path: string) => void }) {
  const t = useT();
  const [root, setRoot] = useState(initialRoot);
  const [scopes, setScopes] = useState<IndexInfo[]>([]);
  const [selected, setSelected] = useState("");
  const [content, setContent] = useState(false);
  const [hidden, setHidden] = useState(false);
  const [offline, setOffline] = useState(false);
  const [battery, setBattery] = useState(false);
  const [ignore, setIgnore] = useState("node_modules target dist .cache");
  const [persistent, setPersistent] = useState(false);
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [text, setText] = useState("");
  const [path, setPath] = useState("");
  const [sensitive, setSensitive] = useState(false);
  const [kind, setKind] = useState("any");
  const [minimum, setMinimum] = useState("");
  const [maximum, setMaximum] = useState("");
  const [modified, setModified] = useState("");
  const [page, setPage] = useState<IndexPage | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const active = useRef<string | null>(null);
  const crawl = useRef<string | null>(null);
  const generation = useRef(0);
  const scope = scopes.find(s => s.id === selected);
  const reload = async () => setScopes(await indexList());
  useEffect(() => { void reload().catch(e => setError(describeErrorFull(e))); }, []);
  useEffect(() => {
    generation.current += 1; setPage(null); setText(""); setError("");
    if (active.current) { void indexCancel(active.current).catch(() => {}); active.current = null; }
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      try {
        const list = await indexList(); if (disposed) return;
        setScopes(list);
        const current = list.find(s => s.id === selected);
        if (!current) { setSelected(""); setPage(null); return; }
        if (current && ["building", "stale", "resource_pause"].includes(current.state)) {
          const token = `index-crawl:${++serial}`; crawl.current = token;
          await indexStep(selected, token); crawl.current = null;
        }
      } catch (e) { if (!disposed) setError(describeErrorFull(e)); }
      if (!disposed) timer = setTimeout(tick, 1500);
    };
    if (selected) void tick();
    return () => { disposed = true; clearTimeout(timer); if (crawl.current) { void indexCancel(crawl.current).catch(() => {}); crawl.current = null; } };
  }, [selected]);
  useEffect(() => () => { generation.current += 1; if (active.current) void indexCancel(active.current).catch(() => {}); }, []);
  const perform = async (work: () => Promise<void>) => {
    setBusy(true); setError(""); try { await work(); await reload(); } catch (e) { setError(describeErrorFull(e)); } finally { setBusy(false); }
  };
  const query = async (next = false) => {
    if (!selected) return;
    if (active.current) await indexCancel(active.current);
    const token = `index-query:${++serial}`; active.current = token;
    const run = ++generation.current;
    setBusy(true); setError("");
    try {
      const result = await indexQuery(selected, { name, path, content: text, caseSensitive: sensitive, kind,
        minSize: minimum ? Number(minimum) : null, maxSize: maximum ? Number(maximum) : null,
        modifiedAfter: modified ? Math.floor(new Date(modified).getTime() / 1000) : null,
        after: next ? page?.next ?? null : null, revision: next ? page?.revision ?? null : null, limit: 200, adapter: "application" }, token);
      if (generation.current === run) setPage(result);
    } catch (e) { if (generation.current === run) setError(describeErrorFull(e)); }
    finally { if (generation.current === run) { setBusy(false); active.current = null; } }
  };
  const field = "rounded border border-edge bg-panel px-2 py-1 text-text";
  return <div className="min-h-0 flex-1 overflow-y-auto p-3 text-[12px] text-text">
    <p className="mb-3 text-dim">{t("index.privacy")}</p>
    <fieldset className="flex flex-col gap-2 rounded border border-edge p-3" disabled={busy}>
      <legend>{t("index.approve")}</legend>
      <label>{t("search.root")} <input className={field} value={root} onChange={e => setRoot(e.target.value)} /></label>
      <label><input type="checkbox" checked={content} onChange={e => setContent(e.target.checked)} /> {t("index.content")}</label>
      <label><input type="checkbox" checked={hidden} onChange={e => setHidden(e.target.checked)} /> {t("index.hidden")}</label>
      <label>{t("search.ignore")} <input className={field} value={ignore} onChange={e => setIgnore(e.target.value)} /></label>
      <label><input type="checkbox" checked={offline} onChange={e => setOffline(e.target.checked)} /> {t("index.offlineOpt")}</label>
      <label><input type="checkbox" checked={battery} onChange={e => setBattery(e.target.checked)} /> {t("index.battery")}</label>
      <label><input type="checkbox" checked={persistent} onChange={e => setPersistent(e.target.checked)} /> {t("index.persist")}</label>
      {persistent && <label>{t("index.password")} <input className={field} type="password" autoComplete="new-password" value={password} onChange={e => setPassword(e.target.value)} /></label>}
      <button className={field} disabled={!root.trim() || (persistent && password.length < 12)} onClick={() => void perform(async () => {
        const created = await indexCreate(root, { content, hidden, offline, allowBattery: battery, ignore: ignore.split(/\s+/).filter(Boolean), maxContentBytes: 1024 * 1024 }, persistent ? password : null);
        setPassword(""); setSelected(created.id);
      })}>{t("index.approve")}</button>
    </fieldset>
    <label className="my-3 block">{t("index.scopes")} <select className={field} value={selected} onChange={e => { setSelected(e.target.value); setBusy(false); }}>
      <option value="">{t("index.choose")}</option>
      {scopes.map(item => <option key={item.id} value={item.id}>{item.root ?? item.id} ({item.id.slice(0, 8)}) — {t(states[item.state] ?? "index.stale")}</option>)}
    </select></label>
    {scope && <>
      <p role="status">{t(states[scope.state] ?? "index.stale")} · {scope.entries} {t("index.entries")} · {scope.bytes} {t("index.bytes")} · {scope.unreadable} {t("index.skipped")}</p>
      {scope.state === "locked" ? <div className="my-2 flex gap-2">
        <label>{t("index.password")} <input type="password" className={field} value={password} onChange={e => setPassword(e.target.value)} /></label>
        <button className={field} disabled={busy} onClick={() => void perform(async () => { await indexUnlock(selected, password); setPassword(""); })}>{t("index.unlock")}</button>
      </div> : <div className="my-2 flex gap-2">
        {(["pause", "resume", "rebuild"] as const).map(action => <button key={action} className={field} disabled={busy} onClick={() => void perform(async () => {
          if (crawl.current) await indexCancel(crawl.current);
          await indexControl(selected, action); setPage(null);
        })}>{t(`index.${action}`)}</button>)}
      </div>}
      <button className={field} disabled={busy} onClick={() => void perform(async () => { if (crawl.current) await indexCancel(crawl.current); await indexControl(selected, "delete"); setSelected(""); setPage(null); })}>{t("index.delete")}</button>
      {scope.state !== "locked" && <fieldset disabled={busy} className="my-3 flex flex-wrap gap-2">
        <legend>{t("index.query")}</legend>
        <label>{t("search.name")} <input className={field} value={name} onChange={e => { setName(e.target.value); setPage(null); }} /></label>
        <label>{t("index.path")} <input className={field} value={path} onChange={e => { setPath(e.target.value); setPage(null); }} /></label>
        <label>{t("search.content")} <input className={field} disabled={!scope.policy?.content} value={text} onChange={e => { setText(e.target.value); setPage(null); }} /></label>
        <label><input type="checkbox" checked={sensitive} onChange={e => { setSensitive(e.target.checked); setPage(null); }} /> {t("search.case")}</label>
        <label>{t("index.kind")} <select className={field} value={kind} onChange={e => { setKind(e.target.value); setPage(null); }}><option value="any">{t("index.any")}</option><option value="file">{t("search.mode.files")}</option><option value="directory">{t("index.directory")}</option></select></label>
        <label>{t("index.minBytes")} <input className={field} type="number" min="0" value={minimum} onChange={e => { setMinimum(e.target.value); setPage(null); }} /></label>
        <label>{t("index.maxBytes")} <input className={field} type="number" min="0" value={maximum} onChange={e => { setMaximum(e.target.value); setPage(null); }} /></label>
        <label>{t("index.modified")} <input className={field} type="date" value={modified} onChange={e => { setModified(e.target.value); setPage(null); }} /></label>
        <button className={field} onClick={() => void query()}>{t("search.run")}</button>
      </fieldset>}
    </>}
    {active.current && <button className={field} onClick={() => { if (active.current) void indexCancel(active.current); }}>{t("search.stop")}</button>}
    {error && <p role="alert" className="my-2 text-red-400">{error}</p>}
    {page && <div className="my-3">
      <p role="status">{t(states[page.freshness] ?? "index.stale")} — {t("index.provenance")}</p>
      <ul>{page.hits.map(hit => <li key={hit.id}><button className="py-1 text-left disabled:opacity-50" disabled={!hit.address} onClick={() => hit.address && onReveal(hit.address)}>{hit.relative}</button> · {hit.size} {t("index.bytes")} · {t(hit.provenance === "content" ? "index.contentMatch" : "index.metadataMatch")}</li>)}</ul>
      {page.next && <button className={field} disabled={busy} onClick={() => void query(true)}>{t("index.next")}</button>}
    </div>}
  </div>;
}
