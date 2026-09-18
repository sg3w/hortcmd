import { useEffect, useRef, useState } from "react";
import { indexList, indexCreate, indexUnlock, indexStep, indexCancel, indexControl, indexQuery } from "@/ipc/client";
import type { IndexInfo } from "@/ipc/bindings/IndexInfo";
import { indexScopeKey, readIndexSearchSession, saveIndexSearchSession, type IndexQueryStatus } from "@/store/indexSearchSession";
import { useT } from "@/store/settingsStore";
import { describeErrorFull } from "@/store/errorText";
import type { TransKey } from "@/i18n/dictionaries";
const states: Record<string, TransKey> = {
  locked: "index.locked", paused: "index.paused", building: "index.building", stale: "index.stale",
  snapshot: "index.snapshot", resource_pause: "index.resource", limited: "index.limited", offline: "index.offline", incomplete: "index.incomplete",
};
let serial = 0;
type Props = { initialRoot: string; onReveal: (path: string) => void };
export function SearchIndexPanel(props: Props) {
  return <IndexPanel key={props.initialRoot} {...props} />;
}
function IndexPanel({ initialRoot, onReveal }: Props) {
  const t = useT();
  const [saved] = useState(() => readIndexSearchSession(initialRoot));
  const [root, setRoot] = useState(saved.root);
  const [selected, setSelected] = useState(saved.selected);
  const [content, setContent] = useState(saved.content);
  const [hidden, setHidden] = useState(saved.hidden);
  const [offline, setOffline] = useState(saved.offline);
  const [battery, setBattery] = useState(saved.battery);
  const [ignore, setIgnore] = useState(saved.ignore);
  const [persistent, setPersistent] = useState(saved.persistent);
  const [name, setName] = useState(saved.name);
  const [text, setText] = useState(saved.text);
  const [path, setPath] = useState(saved.path);
  const [sensitive, setSensitive] = useState(saved.sensitive);
  const [kind, setKind] = useState(saved.kind);
  const [minimum, setMinimum] = useState(saved.minimum);
  const [maximum, setMaximum] = useState(saved.maximum);
  const [modified, setModified] = useState(saved.modified);
  const [page, setPage] = useState(saved.page);
  const [error, setError] = useState(saved.error);
  const [password, setPassword] = useState("");
  const [scopes, setScopes] = useState<IndexInfo[]>([]);
  const [scopeKey, setScopeKey] = useState(saved.scopeKey);
  const [validated, setValidated] = useState<string | null>(null);
  const [status, setStatus] = useState<IndexQueryStatus>(saved.status === "running" ? "cancelled" : saved.status);
  const [busy, setBusy] = useState(false);
  const active = useRef<string | null>(null);
  const crawl = useRef<string | null>(null);
  const generation = useRef(0);
  const mounted = useRef(true);
  const selectedRef = useRef(selected);
  const identity = useRef(scopeKey);
  const latest = useRef(saved);
  const scope = scopes.find(s => s.id === selected);
  const cancel = (token: string | null) => { if (token) void indexCancel(token).catch(() => {}); };
  const invalidate = () => {
    generation.current += 1;
    cancel(active.current); active.current = null;
    setPage(null); setText(""); setError(""); setStatus("idle"); setBusy(false);
    latest.current = { ...latest.current, page: null, text: "", error: "", status: "idle", scopeKey: null };
    saveIndexSearchSession(latest.current);
  };
  const choose = (id: string) => {
    if (id === selectedRef.current) return;
    invalidate(); setPassword(""); setValidated(null); setScopeKey(null); identity.current = null;
    selectedRef.current = id; setSelected(id);
    latest.current = { ...latest.current, selected: id };
    saveIndexSearchSession(latest.current);
  };
  const acceptScopes = (list: IndexInfo[]) => {
    const current = list.find(item => item.id === selectedRef.current);
    const key = indexScopeKey(current);
    if (identity.current !== key || (!key && (latest.current.page !== null || active.current !== null))) invalidate();
    identity.current = key; setScopeKey(key); setValidated(key); setScopes(list);
    if (!current && selectedRef.current) choose("");
    return key;
  };
  useEffect(() => {
    latest.current = { initialRoot, root, selected, content, hidden, offline, battery, ignore, persistent,
      name, text, path, sensitive, kind, minimum, maximum, modified, page, error, scopeKey, status };
    saveIndexSearchSession(latest.current);
  }, [initialRoot, root, selected, content, hidden, offline, battery, ignore, persistent,
    name, text, path, sensitive, kind, minimum, maximum, modified, page, error, scopeKey, status]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false; generation.current += 1;
      saveIndexSearchSession({ ...latest.current, status: active.current ? "cancelled" : latest.current.status });
      cancel(active.current); active.current = null;
      cancel(crawl.current); crawl.current = null;
    };
  }, []);
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      try {
        const list = await indexList(); if (disposed || selectedRef.current !== selected) return;
        acceptScopes(list);
        const current = list.find(item => item.id === selected);
        if (current && ["building", "stale", "resource_pause"].includes(current.state)) {
          const token = `index-crawl:${++serial}`; crawl.current = token;
          await indexStep(selected, token);
          if (crawl.current === token) crawl.current = null;
        }
      } catch (e) {
        if (!disposed) { invalidate(); setValidated(null); setError(describeErrorFull(e)); }
      }
      if (!disposed && selected) timer = setTimeout(tick, 1500);
    };
    void tick();
    return () => { disposed = true; clearTimeout(timer); cancel(crawl.current); crawl.current = null; };
  }, [selected]);
  const currentRun = (run: number, id: string) => mounted.current && generation.current === run && selectedRef.current === id;
  const perform = async (work: (current: () => boolean) => Promise<void>) => {
    if (selectedRef.current !== selected) return;
    const run = ++generation.current; const id = selectedRef.current;
    const current = () => currentRun(run, id);
    setBusy(true); setError("");
    try {
      await work(current);
      if (!mounted.current || generation.current !== run) return;
      const list = await indexList();
      if (mounted.current && generation.current === run) acceptScopes(list);
    } catch (e) { if (current()) setError(describeErrorFull(e)); }
    finally { if (mounted.current && generation.current === run) setBusy(false); }
  };
  const query = async (next = false) => {
    const key = identity.current;
    if (!selected || !validated || !key) return;
    const previous = active.current;
    const token = `index-query:${++serial}`; active.current = token;
    const run = ++generation.current;
    setBusy(true); setError(""); setStatus("running");
    try {
      if (previous) await indexCancel(previous);
      if (!currentRun(run, selected)) return;
      const result = await indexQuery(selected, { name, path, content: text, caseSensitive: sensitive, kind,
        minSize: minimum ? Number(minimum) : null, maxSize: maximum ? Number(maximum) : null,
        modifiedAfter: modified ? Math.floor(new Date(modified).getTime() / 1000) : null,
        after: next ? page?.next ?? null : null, revision: next ? page?.revision ?? null : null, limit: 200, adapter: "application" }, token);
      if (!currentRun(run, selected)) return;
      const list = await indexList();
      if (!currentRun(run, selected)) return;
      if (acceptScopes(list) === key && currentRun(run, selected)) { setPage(result); setStatus("complete"); }
    } catch (e) { if (currentRun(run, selected)) { setError(describeErrorFull(e)); setStatus("error"); } }
    finally { if (currentRun(run, selected)) { setBusy(false); active.current = null; } }
  };
  const stop = () => {
    generation.current += 1; cancel(active.current); active.current = null;
    setBusy(false); setStatus("cancelled");
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
      <button className={field} disabled={!root.trim() || (persistent && password.length < 12)} onClick={() => void perform(async current => {
        const created = await indexCreate(root, { content, hidden, offline, allowBattery: battery, ignore: ignore.split(/\s+/).filter(Boolean), maxContentBytes: 1024 * 1024 }, persistent ? password : null);
        if (current()) { setPassword(""); choose(created.id); }
      })}>{t("index.approve")}</button>
    </fieldset>
    <label className="my-3 block">{t("index.scopes")} <select className={field} value={selected} onChange={e => choose(e.target.value)}>
      <option value="">{t("index.choose")}</option>
      {scopes.map(item => <option key={item.id} value={item.id}>{item.root ?? item.id} ({item.id.slice(0, 8)}) — {t(states[item.state] ?? "index.stale")}</option>)}
    </select></label>
    {scope && <>
      <p role="status">{t(states[scope.state] ?? "index.stale")} · {scope.entries} {t("index.entries")} · {scope.bytes} {t("index.bytes")} · {scope.unreadable} {t("index.skipped")}</p>
      {scope.state === "locked" ? <div className="my-2 flex gap-2">
        <label>{t("index.password")} <input type="password" className={field} value={password} onChange={e => setPassword(e.target.value)} /></label>
        <button className={field} disabled={busy} onClick={() => void perform(async current => { await indexUnlock(selected, password); if (current()) setPassword(""); })}>{t("index.unlock")}</button>
      </div> : <div className="my-2 flex gap-2">
        {(["pause", "resume", "rebuild"] as const).map(action => <button key={action} className={field} disabled={busy} onClick={() => void perform(async current => {
          if (crawl.current) await indexCancel(crawl.current);
          if (!current()) return;
          await indexControl(selected, action); if (current()) { setPage(null); setStatus("idle"); }
        })}>{t(`index.${action}`)}</button>)}
      </div>}
      <button className={field} disabled={busy} onClick={() => void perform(async current => { if (crawl.current) await indexCancel(crawl.current); if (!current()) return; await indexControl(selected, "delete"); if (current()) choose(""); })}>{t("index.delete")}</button>
      {scope.state !== "locked" && <fieldset disabled={busy || !validated} className="my-3 flex flex-wrap gap-2">
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
    {active.current && <button className={field} onClick={stop}>{t("search.stop")}</button>}
    {status === "cancelled" && validated && <p role="status">{t("search.cancelled")}</p>}
    {error && (!selected || validated) && <p role="alert" className="my-2 text-red-400">{error}</p>}
    {page && validated && validated === scopeKey && <div className="my-3">
      <p role="status">{t(states[page.freshness] ?? "index.stale")} — {t("index.provenance")}</p>
      <ul>{page.hits.map(hit => <li key={hit.id}><button className="py-1 text-left disabled:opacity-50" disabled={!hit.address} onClick={() => hit.address && onReveal(hit.address)}>{hit.relative}</button> · {hit.size} {t("index.bytes")} · {t(hit.provenance === "content" ? "index.contentMatch" : "index.metadataMatch")}</li>)}</ul>
      {page.next && <button className={field} disabled={busy} onClick={() => void query(true)}>{t("index.next")}</button>}
    </div>}
  </div>;
}
