/** Lexical local presentation, mirrored by vfs/local_path.rs and golden fixtures. */
export function localParts(raw: string): { root: string; separator: string; windows: boolean } {
  const windows = raw.startsWith("\\\\") || raw[1] === ":";
  if (!windows) return { root: raw.startsWith("/") ? "/" : "", separator: "/", windows };
  const separator = raw.includes("\\") ? "\\" : "/";
  const drive = /^(?:\\\\\?\\)?[A-Za-z]:[\\/]?/u.exec(raw);
  if (drive) return { root: drive[0], separator, windows };
  const unc = /^(?:\\\\\?\\UNC\\|\\\\)[^\\/]+[\\/][^\\/]+/u.exec(raw);
  return { root: unc?.[0] ?? raw, separator, windows };
}
export function localParent(raw: string): string {
  const { root, separator, windows } = localParts(raw);
  const trimmed = raw.replace(windows ? /[\\/]+$/u : /\/+$/u, "");
  if (trimmed.length <= root.length) return root;
  const at = windows ? Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\")) : trimmed.lastIndexOf(separator);
  return at >= root.length ? trimmed.slice(0, at) : root || ".";
}
