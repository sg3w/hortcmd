import { localParts, localParent } from "./localPath";
import { parseLocation, locationUri, encodeComponent } from "./location";

// Platform-neutral path helpers (frontend side).
//
// Local paths keep their original behavior verbatim. Remote VFS
// addresses (`scheme://user@host:port/path`, e.g. sftp) are handled
// specially: the `scheme://authority` part is a fixed prefix and the
// `/…` remainder navigates like a POSIX path, so the connection root
// (`scheme://authority/`) is the top and navigation never mangles the
// authority.

/** Splits a remote VFS address into its `scheme://authority` prefix and
 * the POSIX path remainder (always starting with `/`). Returns null for
 * ordinary local paths. */
function splitRemote(p: string): { prefix: string; rest: string } | null {
  const value = parseLocation(p);
  if (value.location.kind === "local") return null;
  const uri = locationUri(value);
  const at = uri.indexOf("/", uri.indexOf("://") + 3);
  return { prefix: uri.slice(0, at), rest: uri.slice(at) };
}

/** Scheme of a remote VFS address (`sftp`, `ftp`, …); null for local paths. */
export function schemeOf(p: string): string | null {
  const m = /^([a-z][a-z0-9+.-]*):\/\//i.exec(p);
  return m ? m[1].toLowerCase() : null;
}

function sep(p: string): string {
  return (p.startsWith("\\\\") || /^[A-Za-z]:/.test(p)) && p.includes("\\") ? "\\" : "/";
}

export function joinPath(base: string, name: string): string {
  const remote = splitRemote(base);
  if (remote) return `${remote.prefix}${remote.rest.endsWith("/") ? remote.rest : remote.rest + "/"}${name.split("/").map(encodeComponent).join("/")}`;
  const s = sep(base);
  return base.endsWith(s) ? base + name : base + s + name;
}

export function parentPath(p: string): string {
  const remote = splitRemote(p);
  if (remote) {
    const trimmed = remote.rest.replace(/\/+$/, "");
    const idx = trimmed.lastIndexOf("/");
    // Already at (or above) the connection root → stay at the root.
    if (idx <= 0) return `${remote.prefix}/`;
    return remote.prefix + trimmed.slice(0, idx);
  }
  return localParent(p);
}

export function isRoot(p: string): boolean {
  const remote = splitRemote(p);
  if (remote) return remote.rest === "/" || remote.rest === "";
  const { root } = localParts(p);
  return root !== "" && (p === root || p === root + localParts(p).separator);
}

/** Root of a path: the connection root for remote addresses, else
 * "/" (POSIX) or "C:\" (Windows). */
export function rootOf(p: string): string {
  const remote = splitRemote(p);
  if (remote) return `${remote.prefix}/`;
  return localParts(p).root || ".";
}

/** Last name component of a path; for the root, the path itself. */
export function baseName(p: string): string {
  if (isRoot(p)) return p;
  const remote = splitRemote(p);
  const target = remote ? remote.rest : p;
  const parts = remote ? target.replace(/\/+$/, "").split("/").map(decodeURIComponent) : sep(target) === "/" ? target.replace(/\/+$/, "").split("/") : target.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || rootOf(p);
}

/** Chain from the root to the path (inclusive), e.g. ["/","/Users","/Users/x"]. */
export function ancestorChain(path: string): string[] {
  const trimmed = sep(path) === "/" ? path.replace(/\/+$/, "") : path.replace(/[\\/]+$/, "");
  // A path that *is* a root keeps the canonical spelling `rootOf` gives
  // it. Trimming alone would turn "sftp://u@host/" into "sftp://u@host",
  // and a crumb built from that no longer matches the pane's own path.
  let cur = !trimmed || isRoot(trimmed) ? rootOf(path) : trimmed;
  const chain = [cur];
  while (!isRoot(cur)) {
    const parent = parentPath(cur);
    if (parent === cur) break;
    chain.push(parent);
    cur = parent;
  }
  return chain.reverse();
}
