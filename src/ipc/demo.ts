// ============================================================
// The browser demo (TICKET-035).
//
// Without a Tauri runtime the app still has to be usable — that is how
// the layout is developed and how the frontend suite runs. Every demo
// answer lives here, in one module, rather than interleaved with the
// commands it stands in for: a demo reply that drifts from the real
// one is a bug nobody notices, and side by side they are at least
// comparable.
//
// Everything here is typed against the *generated* bindings, so a field
// the Rust side adds breaks the demo at compile time too.
// ============================================================

import type {
  Capability,
  CredStatus,
  DiffEntry,
  DirEntry,
  DirListing,
  DirSize,
  DirSizeStats,
  Drive,
  FileDiff,
  FileProps,
  GitStatus,
  LevelDiff,
  LevelEntry,
  LocationCapabilities,
  Preview,
  SearchHit,
  Tag,
  VolumeInfo,
} from "./bindings";
import { isMacOS, isUnixLike } from "@/lib/platform";
import type { SearchOptions, TransferOp } from "./types";
import {
  collisionSubs,
  doneSubs,
  progressSubs,
} from "./events";

/** Demo default in the browser: the OS keychain, nothing to unlock. */
export const DEMO_CRED: CredStatus = {
  config: { backend: "keychain", kdbx_path: "", key_file: "", master_in_keychain: false },
  unlocked: false,
  needs_unlock: false,
};

export function demoArchive(_archive: string, inner: string): DirListing {
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

export function demoPreview(path: string): Preview {
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

export function demoContent(name: string, ext: string): string {
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

export function demoSearch(o: SearchOptions): SearchHit[] {
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

export function demoFileDiff(left: string, _right: string): FileDiff {
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
export function demoCompareLevels(left: LevelEntry[], right: LevelEntry[]): LevelDiff[] {
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

export function demoCompare(recursive: boolean): DiffEntry[] {
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

export function demoGitStatus(path: string): GitStatus {
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

// Remembers demo tags across an open/edit cycle in the browser.
let demoTags: Tag[] = [
  { name: "Wichtig", color: 6 },
  { name: "Projekt", color: 4 },
];

export function demoProps(path: string): FileProps {
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

export const demoCancelled = new Set<string>();

export const demoCollisionResume = new Map<string, () => void>();

export const demoPaused = new Set<string>();

export function demoPause(id: string, paused: boolean): void {
  if (paused) demoPaused.add(id);
  else demoPaused.delete(id);
}

export function demoTransfer(
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

export function demoDrives(): Drive[] {
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
export function demoCapabilities(): LocationCapabilities {
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

export function demoVolume(): VolumeInfo {
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
export async function demoDirSize(
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

export function demoListDir(path: string): DirListing {
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

/** The demo's Finder tags, and the way to replace them. Exposed as a
 *  pair rather than a mutable binding so the tag state stays owned by
 *  the demo module. */
export function demoGetTags(): Tag[] {
  return demoTags;
}

export function demoSetTags(next: Tag[]): void {
  demoTags = next;
}
