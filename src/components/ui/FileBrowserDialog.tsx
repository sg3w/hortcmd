// ============================================================
// Reusable file browser modal (Radix Dialog).
// Opened from anywhere via the fileBrowserStore and returns the
// chosen path through onPick.
// ============================================================

import { useCallback, useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { ArrowUp, FolderSearch, House } from "lucide-react";
import type { DirEntry, Drive } from "@/ipc/bindings";
import { homeDir, listDir, listDrives } from "@/ipc/client";
import {
  useFileBrowser,
  type SelectMode,
} from "@/store/fileBrowserStore";
import { useT } from "@/store/settingsStore";
import { formatSize } from "@/lib/format";
import { RowIcon } from "@/lib/fileIcon";
import { isRoot, joinPath, parentPath } from "@/lib/path";
import { cn } from "@/lib/cn";

/** macOS app bundle (…​.app) – treated like a selectable file. */
function isAppBundle(entry: DirEntry): boolean {
  return entry.is_dir && /\.app$/i.test(entry.name);
}

/** Whether an entry may be selected in the given mode. */
function isSelectable(entry: DirEntry, mode: SelectMode): boolean {
  if (mode === "any") return true;
  if (mode === "folder") return entry.is_dir;
  // file: files or app bundles
  return !entry.is_dir || isAppBundle(entry);
}

export function FileBrowserDialog() {
  const request = useFileBrowser((s) => s.request);
  const close = useFileBrowser((s) => s.close);
  const t = useT();

  const [path, setPath] = useState("");
  const [pathInput, setPathInput] = useState("");
  const [entries, setEntries] = useState<DirEntry[]>([]);
  const [drives, setDrives] = useState<Drive[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const mode = request?.selectMode ?? "file";

  const navigate = useCallback(async (target: string) => {
    try {
      const res = await listDir(target);
      const sorted = [...res.entries].sort((a, b) => {
        if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
        return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
      });
      setPath(res.path);
      setPathInput(res.path);
      setEntries(sorted);
      setSelected(null);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Initialize on open: load the start folder + fetch the drives.
  useEffect(() => {
    if (!request) return;
    setSelected(null);
    void listDrives().then(setDrives);
    void (async () => {
      const start = request.initialPath || (await homeDir());
      await navigate(start);
    })();
  }, [request, navigate]);

  const rows: DirEntry[] = [
    ...(isRoot(path)
      ? []
      : [
          {
            name: "..",
            is_dir: true,
            is_symlink: false,
            size: 0,
            modified: 0,
            mode: null,
          } as DirEntry,
        ]),
    ...entries,
  ];

  const activate = (entry: DirEntry) => {
    if (entry.name === "..") return void navigate(parentPath(path));
    const treatAsLeaf = isAppBundle(entry) && mode !== "folder";
    if (entry.is_dir && !treatAsLeaf) {
      return void navigate(joinPath(path, entry.name));
    }
    if (isSelectable(entry, mode)) pick(joinPath(path, entry.name));
  };

  const pick = (full: string) => {
    if (!request) return;
    request.onPick(full);
    close();
  };

  const confirm = () => {
    if (selected) pick(joinPath(path, selected));
    else if (mode !== "file") pick(path); // choose the current folder
  };

  const canConfirm = selected !== null || mode !== "file";

  return (
    // Dialog.Root stays mounted permanently and is only controlled via
    // `open`. If the component unmounted while in the `open` state (formerly
    // `if (!request) return null`), Radix would leave `pointer-events: none`
    // on <body> → the entire app stops responding after closing.
    <Dialog.Root open={!!request} onOpenChange={(o) => !o && close()}>
      {request && (
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[70] bg-black/50" />
        <Dialog.Content
          className="fixed left-1/2 top-1/2 z-[71] flex h-[460px] w-[560px] max-w-[92vw] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-edge bg-panel shadow-2xl"
          aria-describedby={undefined}
        >
          <Dialog.Title className="flex items-center gap-2 border-b border-edge bg-header px-4 py-2 text-text">
            <FolderSearch size={15} className="text-accent" />
            {request.title ?? t("fb.title")}
          </Dialog.Title>

          {/* Navigation bar */}
          <div className="flex items-center gap-1.5 border-b border-edge bg-header px-2 py-1.5">
            <button
              onClick={() => navigate(parentPath(path))}
              disabled={isRoot(path)}
              title={t("fb.up")}
              className="rounded p-1 text-dim hover:bg-panel hover:text-text disabled:opacity-40"
            >
              <ArrowUp size={15} />
            </button>
            <button
              onClick={() => homeDir().then(navigate)}
              title={t("fb.home")}
              className="rounded p-1 text-dim hover:bg-panel hover:text-text"
            >
              <House size={15} />
            </button>
            {drives.length > 0 && (
              <select
                value=""
                onChange={(e) => e.target.value && navigate(e.target.value)}
                className="max-w-[130px] rounded border border-edge bg-panel px-1 py-0.5 text-[12px] text-text outline-none focus:border-accent"
                title={t("drive.placeholder")}
              >
                <option value="">💾</option>
                {drives.map((d) => (
                  <option key={d.mount} value={d.mount}>
                    {d.name}
                  </option>
                ))}
              </select>
            )}
            <input
              value={pathInput}
              onChange={(e) => setPathInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") navigate(pathInput);
              }}
              spellCheck={false}
              autoComplete="off"
              className="min-w-0 flex-1 rounded border border-edge bg-panel-inactive px-2 py-0.5 font-mono text-[12px] text-text outline-none focus:border-accent"
            />
          </div>

          {/* Entries */}
          <div className="min-h-0 flex-1 overflow-y-auto">
            {error ? (
              <p className="px-3 py-2 text-[12px] text-red-400">{error}</p>
            ) : (
              rows.map((entry) => {
                const selectable = entry.name !== ".." && isSelectable(entry, mode);
                const isSel = selected === entry.name;
                return (
                  <div
                    key={entry.name}
                    onClick={() =>
                      setSelected(selectable ? entry.name : null)
                    }
                    onDoubleClick={() => activate(entry)}
                    className={cn(
                      "flex cursor-default items-center gap-2 px-3 py-1 text-[12px]",
                      isSel ? "bg-accent-dim text-text" : "text-text hover:bg-row-alt",
                      !selectable && entry.name !== ".." && "text-dim",
                    )}
                  >
                    <RowIcon row={entry} size={15} />
                    <span className="truncate">{entry.name}</span>
                    {!entry.is_dir && (
                      <span className="ml-auto shrink-0 font-mono text-[10px] text-dim">
                        {formatSize(entry.size)}
                      </span>
                    )}
                  </div>
                );
              })
            )}
          </div>

          {/* Footer */}
          <div className="flex items-center gap-2 border-t border-edge bg-header px-4 py-2">
            <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-dim">
              {selected ? joinPath(path, selected) : path}
            </span>
            <Dialog.Close className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent">
              {t("fb.cancel")}
            </Dialog.Close>
            <button
              onClick={confirm}
              disabled={!canConfirm}
              className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
            >
              {request.confirmLabel ?? t("fb.select")}
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
      )}
    </Dialog.Root>
  );
}
