import { createContext, useContext, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import type { ReactNode } from "react";
import type { Row } from "@/store/panesStore";
import { translate, useSettings, useT } from "@/store/settingsStore";
import type { PersistedSettings } from "@/store/settingsSchema";
import { iconFor } from "@/lib/fileIcon";
import { joinPath } from "@/lib/path";
import { parseLocation } from "@/lib/location";
import { hasTauri } from "@/ipc/client";
import type { FileIconRequest } from "@/ipc/bindings/FileIconRequest";

/** Null means type-only (archives and callers without a local entry identity). */
export const FileIconContext = createContext<string | null>(null);

/**
 * The settings a row icon needs, subscribed to **once** for the whole tree.
 *
 * `RowIcon` is mounted per visible row (plus overscan) and per tree node, so
 * dozens of instances exist at the same time. Each one subscribing to the
 * persisted `useSettings` store is the pattern that produced "Cannot update a
 * component while rendering a different component" for `dialogSizeStore` +
 * `AppDialog`: a store update — or its persist rehydration — notifying every
 * subscriber while React is mid-render of a sibling row is enough. One
 * provider subscribes and the rows read a plain context value instead
 * (ticket 354).
 */
interface RowIconSettings {
  mode: PersistedSettings["fileIcons"];
  linkLabel: string;
}
const RowIconSettingsContext = createContext<RowIconSettings | null>(null);

export function RowIconSettingsProvider({ children }: { children: ReactNode }) {
  const mode = useSettings(s => s.fileIcons);
  const t = useT();
  const linkLabel = t("settings.fileIcons.link");
  const value = useMemo(() => ({ mode, linkLabel }), [mode, linkLabel]);
  return <RowIconSettingsContext.Provider value={value}>{children}</RowIconSettingsContext.Provider>;
}

/** Context value, or a one-off read for a `RowIcon` rendered outside the app tree. */
function useRowIconSettings(): RowIconSettings {
  const provided = useContext(RowIconSettingsContext);
  const [fallback] = useState(() => ({
    mode: useSettings.getState().fileIcons,
    linkLabel: translate("settings.fileIcons.link"),
  }));
  return provided ?? fallback;
}
const listeners = new Set<() => void>();
let environment = "";
let stop: (() => void) | undefined;
function snapshot() { return environment; }
function subscribe(listener: () => void) {
  listeners.add(listener);
  const unsubscribe = () => {
    listeners.delete(listener);
    if (!listeners.size) { stop?.(); stop = undefined; }
  };
  if (listeners.size !== 1) return unsubscribe;
  const update = () => {
    environment = `${document.documentElement.dataset.theme}:${window.devicePixelRatio}:${Math.floor(Date.now() / 5000)}`;
    listeners.forEach(fn => fn());
  };
  const observer = new MutationObserver(update);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  window.addEventListener("resize", update);
  window.addEventListener("focus", update);
  const timer = setInterval(update, 5000);
  update();
  stop = () => {
    observer.disconnect(); clearInterval(timer);
    window.removeEventListener("resize", update); window.removeEventListener("focus", update);
  };
  return unsubscribe;
}

export function iconRequest(row: Row, path: string | null, size: number, scale: number, theme: string): FileIconRequest {
  let local: string | null = null;
  if (path && !row.is_symlink) {
    const location = parseLocation(path).location;
    if (location.kind === "local") local = joinPath(location.path.raw, row.name);
  }
  const pixels = Math.max(12, Math.min(128, Math.round(size * scale)));
  const type = row.is_dir ? "folder" : row.name.slice(row.name.lastIndexOf(".") + 1).toLowerCase();
  return {
    id: JSON.stringify([local ?? type, local ? row.modified : 0, row.is_dir, pixels, theme]),
    name: row.name, path: local, is_dir: row.is_dir, is_symlink: row.is_symlink, pixels,
  };
}

/** Space is reserved before loading. Only intersecting rows (+80px) subscribe. */
export function RowIcon({ row, size = 14, directory }: { row: Row; size?: number; directory?: string | null }) {
  const context = useContext(FileIconContext);
  const { mode, linkLabel } = useRowIconSettings();
  const env = useSyncExternalStore(subscribe, snapshot);
  const element = useRef<HTMLSpanElement>(null);
  const [visible, setVisible] = useState(false);
  const [loaded, setLoaded] = useState<{ key: string; image: string | null }>();
  const request = iconRequest(row, directory === undefined ? context : directory, size,
    window.devicePixelRatio || 1, env.split(":")[0]);
  const key = request.id;
  const enabled = mode === "system" && hasTauri() && !row.parent;
  useEffect(() => {
    if (!enabled || !element.current || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(entries => setVisible(entries[0].isIntersecting), { rootMargin: "80px" });
    observer.observe(element.current);
    return () => observer.disconnect();
  }, [enabled]);
  useEffect(() => {
    if (!enabled || !visible) return;
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;
    void import("./nativeIconQueue").then(({ queue }) => {
      if (!cancelled) unsubscribe = queue.subscribe(request, image => setLoaded({ key, image }));
    }).catch(() => { /* Keep the simple fallback if the lazy chunk is unavailable. */ });
    return () => { cancelled = true; unsubscribe?.(); };
    // env periodically checks TTL, including short negative-cache retries.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, env, enabled, visible]);
  const image = enabled && loaded?.key === key ? loaded.image : null;
  const { Icon, className } = iconFor(row);
  return <span ref={element} className="relative inline-flex shrink-0 align-middle" style={{ width: size, height: size }}>
    {image ? <img src={image} alt="" width={size} height={size} onError={() => setLoaded({ key, image: null })} />
      : <Icon size={size} className={className} aria-hidden />}
    {row.is_symlink && !row.parent && <span role="img" aria-label={linkLabel}
      className="absolute bottom-0 left-0 rounded-sm bg-panel text-text" style={{ width: Math.max(8, size * .4), height: Math.max(8, size * .4), lineHeight: 0 }}>
      <svg viewBox="0 0 10 10" aria-hidden><path d="M2 8V5h6M5 2l3 3-3 3" fill="none" stroke="currentColor" strokeWidth="1.5" /></svg>
    </span>}
  </span>;
}
