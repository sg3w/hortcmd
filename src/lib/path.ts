// Platform-neutral path helpers (frontend side).

function sep(p: string): string {
  return p.includes("\\") ? "\\" : "/";
}

export function joinPath(base: string, name: string): string {
  const s = sep(base);
  return base.endsWith(s) ? base + name : base + s + name;
}

export function parentPath(p: string): string {
  const s = sep(p);
  const trimmed = p.replace(/[\\/]+$/, "");
  const idx = trimmed.lastIndexOf(s);
  if (idx <= 0) return s;
  return trimmed.slice(0, idx) || s;
}

export function isRoot(p: string): boolean {
  return p === "/" || /^[A-Za-z]:\\?$/.test(p);
}

/** Root of a path: "/" (POSIX) or "C:\" (Windows). */
export function rootOf(p: string): string {
  const win = /^([A-Za-z]:)[\\/]?/.exec(p);
  return win ? `${win[1]}\\` : "/";
}

/** Last name component of a path; for the root, the path itself. */
export function baseName(p: string): string {
  if (isRoot(p)) return p;
  const parts = p.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || rootOf(p);
}

/** Chain from the root to the path (inclusive), e.g. ["/","/Users","/Users/x"]. */
export function ancestorChain(path: string): string[] {
  let cur = path.replace(/[\\/]+$/, "") || rootOf(path);
  const chain = [cur];
  while (!isRoot(cur)) {
    const parent = parentPath(cur);
    if (parent === cur) break;
    chain.push(parent);
    cur = parent;
  }
  return chain.reverse();
}
