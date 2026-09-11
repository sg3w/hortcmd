// ============================================================
// IPC client: typed calls to the Rust backend.
// Falls back to demo data when no Tauri runtime is present
// (e.g. in the pure browser during layout development).
// ============================================================

import type {
  Capability,
  Checksums,
  CollisionReq,
  ConnectionStatus,
  ConnectOutcome,
  CredConfig,
  CredStatus,
  DiffEntry,
  DirEntry,
  DirListing,
  DirSize,
  DirSizeStats,
  Drive,
  FileDiff,
  FileProps,
  ForgetResult,
  GitStatus,
  GitStatusEvent,
  Leftovers,
  LocationCapabilities,
  LevelDiff,
  LevelEntry,
  OpDone,
  OpProgress,
  OpResult,
  Preview,
  Profile,
  SearchHit,
  SearchSummary,
  SelEntry,
  SelectionFile,
  Tag,
  TermClosed,
  TermData,
  VolumeInfo,
} from "./bindings";
import { isMacOS, isUnixLike } from "@/lib/platform";

export const hasTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

// Lazy: the Tauri core is only loaded on the first call
// (no top-level await → compatible with the build target).
let invokePromise: Promise<InvokeFn> | null = null;

function getInvoke(): Promise<InvokeFn> {
  if (!invokePromise) {
    invokePromise = import("@tauri-apps/api/core").then(
      (mod) => mod.invoke as InvokeFn,
    );
  }
  return invokePromise;
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const fn = await getInvoke();
  return fn<T>(cmd, args);
}

export async function listDir(path: string): Promise<DirListing> {
  if (!hasTauri) return demoListDir(path);
  return invoke<DirListing>("list_dir", { path });
}

export async function listDrives(): Promise<Drive[]> {
  if (!hasTauri) return demoDrives();
  return invoke<Drive[]>("list_drives");
}

export async function homeDir(): Promise<string> {
  if (!hasTauri) return "/Users/demo";
  return invoke<string>("home_dir");
}

// ---------- Remote connections (SFTP, ticket 021/022) ----------

export async function netListProfiles(): Promise<Profile[]> {
  if (!hasTauri) return [];
  return invoke<Profile[]>("net_list_profiles");
}

export async function netSaveProfile(
  profile: Profile,
  secret: string | null,
): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("net_save_profile", { profile, secret });
}

export async function netDeleteProfile(id: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("net_delete_profile", { id });
}

export async function netConnect(id: string): Promise<ConnectOutcome> {
  return invoke<ConnectOutcome>("net_connect", { id });
}

/** Confirms a host key shown by `net_connect` and retries (ticket 028). */
export async function netTrustHostKey(
  id: string,
  token: string,
): Promise<ConnectOutcome> {
  return invoke<ConnectOutcome>("net_trust_host_key", { id, token });
}

/** Drops the host keys hortcmd stores for a profile's endpoint. */
export async function netForgetHostKey(id: string): Promise<ForgetResult> {
  if (!hasTauri) return { removed: 0, remaining: [] };
  return invoke<ForgetResult>("net_forget_host_key", { id });
}

export async function netDisconnect(id: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("net_disconnect", { id });
}

export async function netListConnections(): Promise<ConnectionStatus[]> {
  if (!hasTauri) return [];
  return invoke<ConnectionStatus[]>("net_list_connections");
}

// ---------- Credential backend (keychain / KeePass, ticket 027) ----------

/** Demo default in the browser: the OS keychain, nothing to unlock. */
const DEMO_CRED: CredStatus = {
  config: { backend: "keychain", kdbx_path: "", key_file: "", master_in_keychain: false },
  unlocked: false,
  needs_unlock: false,
};

export async function netCredStatus(): Promise<CredStatus> {
  if (!hasTauri) return DEMO_CRED;
  return invoke<CredStatus>("net_cred_status");
}

export async function netCredSetBackend(config: CredConfig): Promise<CredStatus> {
  if (!hasTauri) return DEMO_CRED;
  return invoke<CredStatus>("net_cred_set_backend", { config });
}

/** Opens the `.kdbx`. `remember` keeps it open for the session,
 *  `create` starts a new database at the configured path. */
export async function netCredUnlock(
  password: string,
  remember: boolean,
  create: boolean,
): Promise<CredStatus> {
  return invoke<CredStatus>("net_cred_unlock", { password, remember, create });
}

export async function netCredLock(): Promise<CredStatus> {
  if (!hasTauri) return DEMO_CRED;
  return invoke<CredStatus>("net_cred_lock");
}

// ---------- SSH terminal (ticket 026) ----------

/** Opens an interactive shell on the SSH connection behind `address`,
 *  starting in that address' remote directory. Returns the session id. */
export async function sshOpenShell(
  address: string,
  cols: number,
  rows: number,
): Promise<string> {
  return invoke<string>("ssh_open_shell", { address, cols, rows });
}

/** Sends keyboard input to a shell session. */
export async function sshSend(id: string, data: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("ssh_send", { id, data });
}

/** Reports the terminal size to the remote side. */
export async function sshResize(
  id: string,
  cols: number,
  rows: number,
): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("ssh_resize", { id, cols, rows });
}

/** Closes a shell session (unknown ids are ignored by the backend). */
export async function sshClose(id: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("ssh_close", { id });
}

/**
 * Sums a folder recursively in the background and streams intermediate
 * totals over `onProgress`. `token` identifies the task for `cancelDirSize`.
 */
export async function dirSize(
  path: string,
  token: string,
  onProgress: (stats: DirSizeStats) => void,
): Promise<DirSize> {
  if (!hasTauri) return demoDirSize(onProgress);
  const { Channel } = await import("@tauri-apps/api/core");
  const channel = new Channel<DirSizeStats>();
  channel.onmessage = onProgress;
  return invoke<DirSize>("dir_size", { path, token, onProgress: channel });
}

/** Stops a running `dirSize`; unknown tokens are ignored. */
export async function cancelDirSize(token: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("cancel_dir_size", { token });
}

/** Capacity/usage of the volume `path` lies on; null if none matches. */
export async function volumeInfo(path: string): Promise<VolumeInfo | null> {
  if (!hasTauri) return demoVolume();
  return invoke<VolumeInfo | null>("volume_info", { path });
}

// ---------- Location capabilities (TICKET-031) ----------

/**
 * What the location behind `address` supports right now. Resolved
 * through the same parser and session registry as the file-system
 * commands, so the answer describes the location the commands would
 * actually act on — including "nothing" for a dropped connection.
 *
 * The browser demo has no backend and therefore one deterministic
 * answer: the local set, with the same platform gates the Rust matrix
 * applies (Quick Look and Finder tags on macOS, rights editing on
 * Unix — the demo counts macOS as Unix, which is what it runs on).
 */
export async function locationCapabilities(
  address: string,
): Promise<LocationCapabilities> {
  if (!hasTauri) return demoCapabilities();
  return invoke<LocationCapabilities>("location_capabilities", { address });
}

// ---------- File operations ----------

export async function makeDir(path: string): Promise<string> {
  if (!hasTauri) return path;
  return invoke<string>("make_dir", { path });
}

/**
 * Writes text into a file (e.g. an exported file list).
 * Without `overwrite`, an existing file is not overwritten; the
 * return value `false` indicates the name conflict (`true` = written).
 */
export async function writeTextFile(
  path: string,
  contents: string,
  overwrite: boolean,
): Promise<boolean> {
  if (!hasTauri) {
    console.log("[demo] write_text_file", path, `${contents.length} B`, { overwrite });
    return true;
  }
  return invoke<boolean>("write_text_file", { path, contents, overwrite });
}

/** Renames an entry (full source and target path). */
export async function renameEntry(from: string, to: string): Promise<void> {
  if (!hasTauri) {
    console.log("[demo] rename_entry", from, to);
    return;
  }
  return invoke<void>("rename_entry", { from, to });
}

/**
 * Renames several entries in `dir` (batch rename). The target names are
 * computed by the frontend; the backend executes them safely (two-phase,
 * conflict check) and returns the success count + error list.
 */
export async function renameBatch(
  dir: string,
  renames: [string, string][],
): Promise<OpResult> {
  if (!hasTauri) {
    console.log("[demo] rename_batch", dir, `${renames.length} Einträge`);
    return { ok: renames.length, errors: [] };
  }
  return invoke<OpResult>("rename_batch", { dir, renames });
}

export async function deleteEntries(paths: string[]): Promise<OpResult> {
  if (!hasTauri) return { ok: paths.length, errors: [] };
  return invoke<OpResult>("delete_entries", { paths });
}

/** Moves entries to the system trash (restorable). */
export async function trashEntries(paths: string[]): Promise<OpResult> {
  if (!hasTauri) {
    console.log("[demo] trash_entries", paths);
    return { ok: paths.length, errors: [] };
  }
  return invoke<OpResult>("trash_entries", { paths });
}

/** Reads an excerpt of a file for the preview (F3). */
export async function readPreview(
  path: string,
  maxBytes: number,
): Promise<Preview> {
  if (!hasTauri) return demoPreview(path);
  return invoke<Preview>("read_preview", { path, maxBytes });
}

function demoArchive(_archive: string, inner: string): DirListing {
  const trimmed = inner.replace(/^\/+|\/+$/g, "");
  const mk = (name: string, is_dir: boolean, size: number): DirEntry => ({
    name,
    is_dir,
    is_symlink: false,
    size,
    modified: 0,
    mode: null,
    hidden: name.startsWith("."),
    readonly: false,
    executable: false,
  });
  // Fictional archive structure: the root contains "src/" and files.
  const entries =
    trimmed === ""
      ? [
          mk("src", true, 0),
          mk("README.md", false, 1024),
          mk("data.json", false, 8192),
        ]
      : trimmed === "src"
        ? [mk("main.rs", false, 4096), mk("lib.rs", false, 2048)]
        : [];
  return { path: `/${trimmed}`, entries };
}

function demoPreview(path: string): Preview {
  const name = path.split(/[\\/]/).pop() ?? path;
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
  const base = {
    name,
    hex: null as string | null,
    data_url: null as string | null,
    exif: [] as Preview["exif"],
    truncated: false,
  };

  if (["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"].includes(ext)) {
    // The placeholder embeds a file name, so it is escaped like any
    // other markup (TICKET-030). An SVG behind `<img src>` is a passive
    // context that cannot run scripts, but malformed markup from an odd
    // name would still break the preview.
    const label = name.replace(/[<>&'"]/g, (c) => `&#${c.charCodeAt(0)};`);
    const dot =
      "data:image/svg+xml;base64," +
      btoa(
        `<svg xmlns='http://www.w3.org/2000/svg' width='320' height='200'><rect width='100%' height='100%' fill='#2b2d31'/><text x='50%' y='50%' fill='#9aa0a6' font-family='sans-serif' font-size='14' text-anchor='middle'>${label}</text></svg>`,
      );
    return {
      ...base,
      kind: "image",
      size: 2_359_296,
      text: null,
      data_url: dot,
      exif: [
        { name: "Make", value: "DemoCam" },
        { name: "Model", value: "RC-1000" },
        { name: "ExposureTime", value: "1/125 s" },
        { name: "FNumber", value: "f/2.8" },
        { name: "ISOSpeedRatings", value: "200" },
        { name: "DateTimeOriginal", value: "2026-07-01 12:34:56" },
      ],
    };
  }

  // Extension-dependent demo content so the render modes are testable.
  const text = demoContent(name, ext);
  return { ...base, kind: "text", size: text.length, text };
}

function demoContent(name: string, ext: string): string {
  if (ext === "md" || ext === "markdown") {
    return [
      `# ${name}`,
      "",
      "Eine **Demo-Markdown**-Datei mit `Code`, [Link](https://example.com) und Liste:",
      "",
      "- Punkt eins",
      "- Punkt zwei",
      "",
      "```js",
      "const answer = 42;",
      "```",
    ].join("\n");
  }
  if (ext === "csv") {
    return "Name,Rolle,Ort\nAnna,Admin,Berlin\nBén,\"Dev, Sr.\",Wien\nCem,Design,Zürich";
  }
  if (ext === "tsv") {
    return "Name\tRolle\tOrt\nAnna\tAdmin\tBerlin\nCem\tDesign\tZürich";
  }
  if (ext === "json") {
    return '{\n  "name": "demo",\n  "count": 42,\n  "tags": ["a", "b"],\n  "active": true\n}';
  }
  if (ext === "yaml" || ext === "yml") {
    return "name: demo\ncount: 42\ntags:\n  - a\n  - b\nactive: true";
  }
  if (ext === "html" || ext === "htm") {
    return '<!doctype html>\n<html>\n  <body>\n    <h1>Hallo</h1>\n    <p class="x">Welt</p>\n  </body>\n</html>';
  }
  return [
    `# ${name}`,
    "",
    "Dies ist eine Demo-Vorschau (Browser ohne Tauri).",
    "In der nativen App zeigt F3 den echten Dateiinhalt.",
    "",
    ...Array.from({ length: 30 }, (_, i) => `Zeile ${i + 1}: Lorem ipsum dolor sit amet.`),
  ].join("\n");
}

// ---------- Transfers (copy/move, concurrent) ----------
//
// Internal event bus fed by both real Tauri events and the
// demo simulator. This keeps the store decoupled from Tauri.

const progressSubs = new Set<(p: OpProgress) => void>();
const doneSubs = new Set<(d: OpDone) => void>();
const collisionSubs = new Set<(c: CollisionReq) => void>();
const changedSubs = new Set<(dir: string) => void>();
const leftoverSubs = new Set<(l: Leftovers) => void>();
const gitReadySubs = new Set<(path: string, status: GitStatus) => void>();
const termDataSubs = new Set<(d: TermData) => void>();
const termClosedSubs = new Set<(c: TermClosed) => void>();
const nativeMenuSubs = new Set<(id: string) => void>();
let tauriWired = false;

async function ensureTauriEvents(): Promise<void> {
  if (!hasTauri || tauriWired) return;
  tauriWired = true;
  const { listen } = await import("@tauri-apps/api/event");
  listen<OpProgress>("fs-progress", (e) =>
    progressSubs.forEach((f) => f(e.payload)),
  );
  listen<OpDone>("fs-done", (e) => doneSubs.forEach((f) => f(e.payload)));
  listen<CollisionReq>("fs-collision", (e) =>
    collisionSubs.forEach((f) => f(e.payload)),
  );
  listen<string>("fs-changed", (e) =>
    changedSubs.forEach((f) => f(e.payload)),
  );
  listen<Leftovers>("fs-leftovers", (e) =>
    leftoverSubs.forEach((f) => f(e.payload)),
  );
  listen<GitStatusEvent>("git-support-ready", (e) =>
    gitReadySubs.forEach((f) => f(e.payload.path, e.payload.status)),
  );
  listen<TermData>("ssh-term-data", (e) =>
    termDataSubs.forEach((f) => f(e.payload)),
  );
  listen<TermClosed>("ssh-term-closed", (e) =>
    termClosedSubs.forEach((f) => f(e.payload)),
  );
  listen<string>("native-menu", (e) =>
    nativeMenuSubs.forEach((f) => f(e.payload)),
  );
}

/** Rebuilds the reduced native macOS menu in `lang` (TICKET-040).
 *  Only the frontend knows the persisted language setting. */
export function setMenuLanguage(lang: string): void {
  if (!hasTauri) return;
  void invoke<void>("set_menu_language", { lang });
}

/** Subscribes to the remains of interrupted failure-safe transfers found
 *  in a destination folder (TICKET-029). They are only reported — the
 *  frontend asks before anything is deleted. */
export function onLeftovers(cb: (l: Leftovers) => void): () => void {
  leftoverSubs.add(cb);
  void ensureTauriEvents();
  return () => leftoverSubs.delete(cb);
}

/** Subscribes to clicks in the reduced native macOS menu (TICKET-040). */
export function onNativeMenu(cb: (id: string) => void): () => void {
  nativeMenuSubs.add(cb);
  void ensureTauriEvents();
  return () => nativeMenuSubs.delete(cb);
}

/** Subscribes to shell output (base64 chunks) of all SSH terminals. */
export function onSshTermData(cb: (d: TermData) => void): () => void {
  termDataSubs.add(cb);
  void ensureTauriEvents();
  return () => termDataSubs.delete(cb);
}

/** Subscribes to the end of an SSH terminal session. */
export function onSshTermClosed(cb: (c: TermClosed) => void): () => void {
  termClosedSubs.add(cb);
  void ensureTauriEvents();
  return () => termClosedSubs.delete(cb);
}

export function onFsProgress(cb: (p: OpProgress) => void): () => void {
  progressSubs.add(cb);
  void ensureTauriEvents();
  return () => progressSubs.delete(cb);
}

export function onFsDone(cb: (d: OpDone) => void): () => void {
  doneSubs.add(cb);
  void ensureTauriEvents();
  return () => doneSubs.delete(cb);
}

export function onFsCollision(cb: (c: CollisionReq) => void): () => void {
  collisionSubs.add(cb);
  void ensureTauriEvents();
  return () => collisionSubs.delete(cb);
}

/** Subscribes to external folder changes (directory watcher). */
export function onFsChanged(cb: (dir: string) => void): () => void {
  changedSubs.add(cb);
  void ensureTauriEvents();
  return () => changedSubs.delete(cb);
}

/** Defines which folders the watcher observes (the currently displayed ones). */
export function setWatched(paths: string[]): void {
  if (!hasTauri) return;
  void invoke<void>("set_watched", { paths });
}

export type TransferOp = "copy" | "move" | "extract" | "pack";

/** Starts a copy/move transfer (fire-and-forget).
 *  `limit` = bytes/s (0 = unlimited), `verify` = checksums after copying,
 *  `safe` = stage and commit instead of writing to the final name
 *  (TICKET-029). The caller passes the value captured when the transfer
 *  was queued, so a waiting operation keeps the policy it was started with. */
export function startTransfer(
  op: "copy" | "move",
  id: string,
  sources: string[],
  destDir: string,
  limit = 0,
  verify = false,
  bufKb = 256,
  threads = 1,
  safe = false,
): void {
  if (!hasTauri) {
    demoTransfer(op, id, sources, destDir, true);
    return;
  }
  const cmd = op === "copy" ? "copy_entries" : "move_entries";
  void invoke<void>(cmd, {
    id,
    sources,
    destDir,
    limit,
    verify,
    bufKb,
    threads,
    safe,
  });
}

/** Pauses or resumes a running transfer. */
export function pauseTransfer(id: string, paused: boolean): void {
  if (!hasTauri) {
    demoPause(id, paused);
    return;
  }
  void invoke<void>("pause_transfer", { id, paused });
}

/** Extracts entries from an archive (fire-and-forget). */
export function startExtract(
  id: string,
  archive: string,
  base: string,
  names: string[],
  destDir: string,
  password?: string,
): void {
  if (!hasTauri) {
    demoTransfer("extract", id, names.length ? names : ["(alles)"], destDir, false);
    return;
  }
  void invoke<void>("extract_entries", {
    id,
    archive,
    base,
    names,
    destDir,
    password: password ?? null,
  });
}

/** Packs sources into a new ZIP archive (fire-and-forget). */
export function startPack(id: string, sources: string[], destZip: string): void {
  if (!hasTauri) {
    demoTransfer("pack", id, sources, destZip, false);
    return;
  }
  void invoke<void>("create_archive", { id, sources, destZip });
}

/**
 * Opens a file with the default program (F4). Rejects when the path cannot be
 * opened — a dangling symlink, for instance — so the caller can report it
 * instead of failing silently (TICKET-018).
 */
export async function openPath(path: string): Promise<void> {
  if (!hasTauri) {
    console.log("[demo] open_path", path);
    return;
  }
  return invoke<void>("open_path", { path });
}

/** Opens an `http(s)` link from rendered content in the system browser
 *  (TICKET-030). The backend validates the scheme again and refuses
 *  anything else, so this is never a general launcher. */
export async function openExternal(url: string): Promise<void> {
  if (!hasTauri) {
    console.log("[demo] open_external", url);
    return;
  }
  return invoke<void>("open_external", { url });
}

/** Native macOS Quick Look preview (Space). */
export function quickLook(path: string): void {
  if (!hasTauri) {
    console.log("[demo] quick_look", path);
    return;
  }
  void invoke<void>("quick_look", { path });
}

/**
 * Opens the operating system's own information/properties window for a
 * local entry (Finder "Get Info", the Explorer properties sheet, the
 * desktop file manager's properties dialog). Resolves once the window
 * has been handed to the system; it outlives hortcmd's own dialog.
 */
export async function openSystemProperties(path: string): Promise<void> {
  if (!hasTauri) {
    console.log("[demo] open_system_properties", path);
    return;
  }
  return invoke<void>("open_system_properties", { path });
}

/** Opens a file with a specific program (empty = system default). */
export function openWith(path: string, program: string): void {
  if (!hasTauri) {
    console.log("[demo] open_with", path, program);
    return;
  }
  void invoke<void>("open_with", { path, program });
}

// ---------- Directory comparison / synchronization ----------

/**
 * Compares two directory trees (by size + modification time) and delivers
 * the results **streaming per directory** over `onBatch`, so large
 * trees don't appear only at the end. Resolves with `true` if the result
 * was truncated at the upper limit (200,000 files).
 */
export async function compareDirs(
  left: string,
  right: string,
  recursive: boolean,
  onBatch: (entries: DiffEntry[]) => void,
): Promise<boolean> {
  if (!hasTauri) {
    const rows = demoCompare(recursive);
    const top = rows.filter((r) => !r.rel.includes("/"));
    const nested = rows.filter((r) => r.rel.includes("/"));
    onBatch(top);
    if (nested.length) {
      await new Promise((res) => setTimeout(res, 250));
      onBatch(nested);
    }
    return false;
  }
  const { Channel } = await import("@tauri-apps/api/core");
  const channel = new Channel<DiffEntry[]>();
  channel.onmessage = onBatch;
  return invoke<boolean>("compare_dirs", {
    left,
    right,
    recursive,
    onBatch: channel,
  });
}

/**
 * Compares the two directory levels the panes already hold (TICKET-059).
 * No directory is scanned again, so the same call serves local folders,
 * every remote provider and the inside of an archive alike.
 */
export async function compareLevels(
  left: LevelEntry[],
  right: LevelEntry[],
): Promise<LevelDiff[]> {
  if (!hasTauri) return demoCompareLevels(left, right);
  return invoke<LevelDiff[]>("compare_levels", { left, right });
}

/**
 * Writes a selection file. Like `write_text_file`, `false` reports an
 * existing target instead of overwriting it.
 */
export async function saveSelection(
  path: string,
  location: string,
  entries: SelEntry[],
  overwrite: boolean,
): Promise<boolean> {
  if (!hasTauri) {
    console.log("[demo] save_selection", path, `${entries.length} Eintrag/Einträge`);
    return true;
  }
  return invoke<boolean>("save_selection", { path, location, entries, overwrite });
}

/** Reads and validates a selection file (format checks live in Rust). */
export async function loadSelection(path: string): Promise<SelectionFile> {
  if (!hasTauri) {
    return { version: 1, location: "/demo", entries: [{ name: "readme.md", is_dir: false }] };
  }
  return invoke<SelectionFile>("load_selection", { path });
}

/** Copies files source→target (pairs of absolute paths). */
export async function syncCopy(
  items: [string, string][],
): Promise<OpResult> {
  if (!hasTauri) {
    console.log("[demo] sync_copy", `${items.length} Datei(en)`);
    return { ok: items.length, errors: [] };
  }
  return invoke<OpResult>("sync_copy", { items });
}

/** Compares two files by content (text diff or hex comparison). */
export async function compareFiles(
  left: string,
  right: string,
): Promise<FileDiff> {
  if (!hasTauri) return demoFileDiff(left, right);
  return invoke<FileDiff>("compare_files", { left, right });
}

// ---------- Search ----------

export type SearchMode = "files" | "duplicates" | "empty_dirs" | "large_files";

export interface SearchOptions {
  mode: SearchMode;
  name: string;
  name_regex: boolean;
  content: string;
  content_regex: boolean;
  case_sensitive: boolean;
  ignore_dirs: string[];
  min_size: number;
}

/**
 * Searches `root` and streams matches in batches. `token` identifies the
 * run for `cancelSearch`. Only `status: "complete"` means the returned
 * list is the full answer – see `SearchStatus`.
 */
export async function search(
  root: string,
  options: SearchOptions,
  token: string,
  onBatch: (hits: SearchHit[]) => void,
): Promise<SearchSummary> {
  if (!hasTauri) {
    const rows = demoSearch(options);
    onBatch(rows);
    return {
      status: "complete",
      hits: rows.length,
      files: rows.length,
      dirs: 0,
      unreadable: 0,
    };
  }
  const { Channel } = await import("@tauri-apps/api/core");
  const channel = new Channel<SearchHit[]>();
  channel.onmessage = onBatch;
  return invoke<SearchSummary>("search", { root, options, token, onBatch: channel });
}

/** Stops a running `search`; unknown tokens are ignored. */
export async function cancelSearch(token: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("cancel_search", { token });
}

function demoSearch(o: SearchOptions): SearchHit[] {
  const now = Math.floor(Date.now() / 1000);
  const mk = (
    path: string,
    size: number,
    detail = "",
    group = 0,
    is_dir = false,
  ): SearchHit => ({
    path,
    name: path.split("/").pop() ?? path,
    is_dir,
    size,
    modified: now,
    detail,
    group,
  });
  if (o.mode === "duplicates") {
    return [
      mk("/Users/demo/a/logo.png", 20480, "2 × 20.0 KB", 1),
      mk("/Users/demo/b/logo-copy.png", 20480, "2 × 20.0 KB", 1),
      mk("/Users/demo/notes.txt", 2048, "3 × 2.0 KB", 2),
      mk("/Users/demo/backup/notes.txt", 2048, "3 × 2.0 KB", 2),
      mk("/Users/demo/old/notes.txt", 2048, "3 × 2.0 KB", 2),
    ];
  }
  if (o.mode === "empty_dirs") {
    return [
      mk("/Users/demo/tmp", 0, "", 0, true),
      mk("/Users/demo/Projects/empty", 0, "", 0, true),
    ];
  }
  if (o.mode === "large_files") {
    return [
      mk("/Users/demo/photo.jpg", 2_359_296),
      mk("/Users/demo/archive.zip", 10_485_760),
    ].filter((h) => h.size >= o.min_size);
  }
  // files: name/content search
  const base = [
    mk("/Users/demo/notes.txt", 2048, o.content ? "12: TODO: aufräumen" : ""),
    mk("/Users/demo/readme.md", 4096, o.content ? "3: siehe TODO oben" : ""),
    mk("/Users/demo/src/main.rs", 8192),
  ];
  return base;
}

function demoFileDiff(left: string, _right: string): FileDiff {
  const isBinary = /\.(zip|jpg|png|pdf|bin)$/i.test(left);
  if (isBinary) {
    return {
      mode: "binary",
      identical: false,
      truncated: false,
      left_size: 5,
      right_size: 5,
      lines: [],
      hex: [
        {
          offset: 0,
          left_hex: "00 01 02 03 ff",
          left_ascii: ".....",
          right_hex: "00 01 09 03 ff",
          right_ascii: ".....",
          differs: true,
        },
      ],
    };
  }
  return {
    mode: "text",
    identical: false,
    truncated: false,
    left_size: 42,
    right_size: 45,
    lines: [
      { tag: "equal", left_no: 1, left: "erste Zeile", right_no: 1, right: "erste Zeile" },
      { tag: "replace", left_no: 2, left: "alter Wert", right_no: 2, right: "neuer Wert" },
      { tag: "delete", left_no: 3, left: "nur links", right_no: null, right: null },
      { tag: "insert", left_no: null, left: null, right_no: 3, right: "nur rechts" },
      { tag: "equal", left_no: 4, left: "letzte Zeile", right_no: 4, right: "letzte Zeile" },
    ],
    hex: [],
  };
}

/** Demo mirror of the backend classification (browser-only preview). */
function demoCompareLevels(left: LevelEntry[], right: LevelEntry[]): LevelDiff[] {
  const index = (v: LevelEntry[]) => new Map(v.map((e) => [e.name, e]));
  const l = index(left);
  const r = index(right);
  const names = [...new Set([...l.keys(), ...r.keys()])].sort();
  return names.map((name) => {
    const a = l.get(name);
    const b = r.get(name);
    let status = "unknown";
    if (a && !b) status = "left_only";
    else if (b && !a) status = "right_only";
    else if (a && b) {
      const bad = (e: LevelEntry) => e.is_symlink || e.modified === 0;
      if (a.is_dir !== b.is_dir || bad(a) || bad(b)) status = "unknown";
      else if (a.size === b.size && a.modified === b.modified) status = "same";
      else if (a.modified === b.modified) status = "different";
      else if (a.is_dir && b.is_dir) status = "different";
      else status = a.modified > b.modified ? "newer_left" : "newer_right";
    }
    return { name, left: !!a, right: !!b, status };
  });
}

function demoCompare(recursive: boolean): DiffEntry[] {
  const now = Math.floor(Date.now() / 1000);
  const mk = (
    rel: string,
    status: string,
    left: boolean,
    right: boolean,
    ls: number,
    rs: number,
    lm: number,
    rm: number,
  ): DiffEntry => ({
    rel,
    name: rel.split("/").pop() ?? rel,
    left,
    right,
    left_size: ls,
    right_size: rs,
    left_modified: lm,
    right_modified: rm,
    status,
  });
  const rows = [
    mk("notes.txt", "same", true, true, 2048, 2048, now - 200, now - 200),
    mk("report.pdf", "newer_left", true, true, 40000, 39000, now - 10, now - 500),
    mk("config.json", "newer_right", true, true, 320, 512, now - 800, now - 12),
    mk("data.csv", "different", true, true, 256, 300, now - 60, now - 60),
    mk("src/main.rs", "left_only", true, false, 4096, 0, now - 30, 0),
    mk("src/lib.rs", "left_only", true, false, 2048, 0, now - 30, 0),
    mk("archive.zip", "right_only", false, true, 0, 1048576, 0, now - 900),
  ];
  // Many synthetic entries so the virtualization is testable.
  const STATI = ["different", "newer_left", "newer_right", "same"] as const;
  for (let i = 0; i < 1500; i++) {
    const s = STATI[i % STATI.length];
    rows.push(
      mk(
        `logs/entry-${String(i).padStart(4, "0")}.log`,
        s,
        true,
        true,
        1000 + i,
        1000 + i + (s === "different" ? 7 : 0),
        now - i,
        now - i - (s === "newer_right" ? 100 : 0),
      ),
    );
  }
  // Without recursion, only top-level entries (no "/" in the path).
  return recursive ? rows : rows.filter((r) => !r.rel.includes("/"));
}

/** Requests the Git status of a folder; runs asynchronously in the backend
 *  in the background (blocks neither folder opening nor other IPC calls) and
 *  delivers the result via `onGitStatusReady` (event `git-support-ready`). */
export function requestGitStatus(path: string): void {
  if (!hasTauri) {
    // Demo: deliver the result asynchronously as well, like the original.
    queueMicrotask(() =>
      gitReadySubs.forEach((f) => f(path, demoGitStatus(path))),
    );
    return;
  }
  void invoke<void>("git_status_watch", { path });
}

/** Subscribes to Git status results (see requestGitStatus). */
export function onGitStatusReady(
  cb: (path: string, status: GitStatus) => void,
): () => void {
  gitReadySubs.add(cb);
  void ensureTauriEvents();
  return () => gitReadySubs.delete(cb);
}

function demoGitStatus(path: string): GitStatus {
  // Demo only: present /Users/demo as a repo with mixed statuses.
  if (!path.includes("/Users/demo")) {
    return { is_repo: false, branch: null, entries: {} };
  }
  return {
    is_repo: true,
    branch: "main",
    entries: {
      "notes.txt": "modified",
      "readme.md": "untracked",
      "report.pdf": "deleted",
      "photo.jpg": "renamed",
      "config.json": "staged",
      "data.csv": "conflict",
      Projects: "modified",
      // Also executable — shows that the Git rule has the higher priority.
      "script.sh": "ignored",
    },
  };
}

/** Opens a terminal in the folder (empty `program` = system default). */
export function openTerminal(path: string, program?: string): void {
  const prog = program?.trim() ? program.trim() : null;
  if (!hasTauri) {
    console.log("[demo] open_terminal", path, prog);
    return;
  }
  void invoke<void>("open_terminal", { path, program: prog });
}

// ---------- Permissions & properties ----------

/** Reads permissions, owner, extended attributes, and ACL of an entry. */
export async function fileProps(path: string): Promise<FileProps> {
  if (!hasTauri) return demoProps(path);
  return invoke<FileProps>("file_props", { path });
}

/** Sets the access rights (chmod); `mode` are the permission bits (0..0o7777). */
export async function setPermissions(path: string, mode: number): Promise<void> {
  if (!hasTauri) {
    console.log("[demo] set_permissions", path, mode.toString(8));
    return;
  }
  return invoke<void>("set_permissions", { path, mode });
}

/** Changes owner/group (chown); name or numeric ID, null = unchanged. */
export async function setOwner(
  path: string,
  owner: string | null,
  group: string | null,
): Promise<void> {
  if (!hasTauri) {
    console.log("[demo] set_owner", path, owner, group);
    return;
  }
  return invoke<void>("set_owner", { path, owner, group });
}

/** Computes MD5, SHA-1, and SHA-256 of a file. */
export async function fileChecksums(path: string): Promise<Checksums> {
  if (!hasTauri) {
    return {
      md5: "d41d8cd98f00b204e9800998ecf8427e",
      sha1: "da39a3ee5e6b4b0d3255bfef95601890afd80709",
      sha256:
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    };
  }
  return invoke<Checksums>("file_checksums", { path });
}

/** Reads the Finder tags of an entry (macOS; otherwise empty). */
export async function getTags(path: string): Promise<Tag[]> {
  if (!hasTauri) return demoTags;
  return invoke<Tag[]>("get_tags", { path });
}

/** Writes the Finder tags (an empty list removes them). */
export async function setTags(path: string, tags: Tag[]): Promise<void> {
  if (!hasTauri) {
    demoTags = tags;
    console.log("[demo] set_tags", path, tags);
    return;
  }
  return invoke<void>("set_tags", { path, tags });
}

// Remembers demo tags across an open/edit cycle in the browser.
let demoTags: Tag[] = [
  { name: "Wichtig", color: 6 },
  { name: "Projekt", color: 4 },
];

function demoProps(path: string): FileProps {
  const name = path.split("/").pop() || path;
  // Mirror the flags of demoListDir, so both show the same color rule.
  return {
    path,
    name,
    is_dir: false,
    is_symlink: name === "latest",
    size: 4096,
    modified: Math.floor(Date.now() / 1000),
    mode: 0o100644,
    uid: 501,
    gid: 20,
    owner: "demo",
    group: "staff",
    xattrs: [{ name: "com.apple.quarantine", size: 42, value: "0081;…" }],
    acl: ["user:demo:allow read,write"],
    unix: true,
    hidden: name.startsWith("."),
    readonly: name === "license.txt",
    executable: name === "script.sh",
  };
}

/** Lists the contents of an archive at level `inner` (with a password if needed). */
export async function listArchive(
  archive: string,
  inner: string,
  password?: string,
): Promise<DirListing> {
  if (!hasTauri) return demoArchive(archive, inner);
  return invoke<DirListing>("list_archive", {
    archive,
    inner,
    password: password ?? null,
  });
}

/** Cancels a running transfer. */
export function cancelTransfer(id: string): void {
  if (!hasTauri) {
    demoCancelled.add(id);
    return;
  }
  void invoke<void>("cancel_transfer", { id });
}

/** Answers a name conflict. */
export function resolveCollision(
  reqId: string,
  action: "overwrite" | "rename" | "skip",
  applyAll: boolean,
): void {
  if (!hasTauri) {
    const resume = demoCollisionResume.get(reqId);
    demoCollisionResume.delete(reqId);
    resume?.();
    return;
  }
  void invoke<void>("resolve_collision", { reqId, action, applyAll });
}

// ---------- Demo simulator (only without Tauri) ----------

const demoCancelled = new Set<string>();
const demoCollisionResume = new Map<string, () => void>();
const demoPaused = new Set<string>();

function demoPause(id: string, paused: boolean): void {
  if (paused) demoPaused.add(id);
  else demoPaused.delete(id);
}

function demoTransfer(
  op: TransferOp,
  id: string,
  sources: string[],
  _destDir: string,
  collide: boolean,
): void {
  const filesTotal = Math.max(1, sources.length);
  const perFile = 4_000_000; // fiktive Dateigröße
  const bytesTotal = filesTotal * perFile;
  const steps = 40;
  let step = 0;
  let paused = false;
  let collisionShown = false;

  const timer = setInterval(() => {
    if (demoCancelled.has(id)) {
      clearInterval(timer);
      demoCancelled.delete(id);
      doneSubs.forEach((f) =>
        f({ id, op, ok: 0, errors: [], cancelled: true, mode: "" }),
      );
      return;
    }
    if (paused) return;

    // Pause from the UI: send the current state as "paused" and wait.
    if (demoPaused.has(id)) {
      const frac = step / steps;
      progressSubs.forEach((f) =>
        f({
          id,
          op,
          file_name: sources[Math.min(filesTotal - 1, step)] ?? "",
          file_done: 0,
          file_total: perFile,
          files_done: Math.floor(frac * filesTotal),
          files_total: filesTotal,
          bytes_done: Math.floor(frac * bytesTotal),
          bytes_total: bytesTotal,
          paused: true,
        }),
      );
      return;
    }

    // Simulate a name conflict once (copy/move only).
    if (collide && !collisionShown && step === 3 && sources.length > 0) {
      collisionShown = true;
      paused = true;
      const reqId = `demo-${id}`;
      demoCollisionResume.set(reqId, () => {
        paused = false;
      });
      collisionSubs.forEach((f) =>
        f({ transfer_id: id, req_id: reqId, path: sources[0], is_dir: false }),
      );
      return;
    }

    step++;
    const frac = step / steps;
    const filesDone = Math.min(filesTotal, Math.floor(frac * filesTotal));
    const idx = Math.min(filesTotal - 1, filesDone);
    const fileFrac = (frac * filesTotal) % 1;
    progressSubs.forEach((f) =>
      f({
        id,
        op,
        file_name: sources[idx] ?? "",
        file_done: Math.floor(fileFrac * perFile),
        file_total: perFile,
        files_done: filesDone,
        files_total: filesTotal,
        bytes_done: Math.floor(frac * bytesTotal),
        bytes_total: bytesTotal,
        paused: false,
      }),
    );
    if (step >= steps) {
      clearInterval(timer);
      doneSubs.forEach((f) =>
        f({ id, op, ok: filesTotal, errors: [], cancelled: false, mode: "" }),
      );
    }
  }, 130);
}

function demoDrives(): Drive[] {
  return [
    { name: "/", mount: "/", total: 500_000_000_000, free: 210_000_000_000 },
    { name: "~", mount: "/Users/demo", total: 500_000_000_000, free: 210_000_000_000 },
  ];
}

/**
 * The demo's capability set — the local matrix of `vfs::caps`, so the
 * browser build disables and enables exactly the actions the packaged
 * app does on the same platform.
 */
function demoCapabilities(): LocationCapabilities {
  const caps: Capability[] = [
    "browse",
    "preview",
    "copyFrom",
    "copyTo",
    "move",
    "create",
    "deletePermanent",
    "trash",
    "archiveBrowse",
    "archiveExtractFrom",
    "archiveExtractTo",
    "archivePackFrom",
    "archivePackTo",
    "stat",
    "recursiveSize",
    "checksums",
    "search",
    "compareDirs",
    "compareFiles",
    "gitStatus",
    "volumeInfo",
    "systemOpen",
    "openWith",
    "localTerminal",
  ];
  if (isUnixLike) caps.push("permissions");
  if (isMacOS) caps.push("quickLook", "finderTags");
  return { scheme: "local", store_id: "local", connected: true, caps };
}

function demoVolume(): VolumeInfo {
  const total = 500_000_000_000;
  const free = 210_000_000_000;
  return {
    name: "Macintosh HD",
    mount: "/",
    fs: "apfs",
    total,
    used: total - free,
    free,
    used_pct: ((total - free) * 100) / total,
    free_pct: (free * 100) / total,
  };
}

/** Counts up in a few steps, so the loading state is visible in the browser. */
async function demoDirSize(
  onProgress: (stats: DirSizeStats) => void,
): Promise<DirSize> {
  const final: DirSizeStats = {
    bytes: 1_374_000,
    files: 128,
    dirs: 12,
    errors: 1,
  };
  for (const step of [0.2, 0.6]) {
    await new Promise((res) => setTimeout(res, 300));
    onProgress({
      bytes: Math.round(final.bytes * step),
      files: Math.round(final.files * step),
      dirs: Math.round(final.dirs * step),
      errors: 0,
    });
  }
  await new Promise((res) => setTimeout(res, 300));
  return { stats: final, cancelled: false };
}

function demoListDir(path: string): DirListing {
  const now = Date.now();
  const mk = (
    name: string,
    is_dir: boolean,
    size: number,
    ageDays: number,
    extra: Partial<DirEntry> = {},
  ): DirEntry => ({
    name,
    is_dir,
    is_symlink: false,
    size,
    modified: Math.floor((now - ageDays * 86_400_000) / 1000),
    mode: null,
    hidden: name.startsWith("."),
    readonly: false,
    executable: false,
    ...extra,
  });
  return {
    path,
    entries: [
      mk(".config", true, 0, 40),
      mk(".gitignore", false, 128, 15),
      mk("Documents", true, 0, 3),
      mk("Downloads", true, 0, 1),
      mk("Pictures", true, 0, 12),
      mk("Projects", true, 0, 0),
      mk("notes.txt", false, 2048, 2),
      mk("report.pdf", false, 384_512, 5),
      mk("archive.zip", false, 10_485_760, 20),
      mk("script.sh", false, 512, 8, { executable: true }),
      mk("photo.jpg", false, 2_359_296, 30),
      mk("readme.md", false, 4096, 1),
      mk("data.csv", false, 256, 4),
      mk("config.json", false, 320, 6),
      // Cover the remaining color rules in demo mode as well.
      mk("latest", false, 12, 2, { is_symlink: true }),
      mk("license.txt", false, 1024, 90, { readonly: true }),
      mk("build.log", false, 65_536, 0),
    ],
  };
}
