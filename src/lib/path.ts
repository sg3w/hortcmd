// Plattformneutrale Pfad-Helfer (Frontend-Seite).

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

/** Wurzel eines Pfads: "/" (POSIX) bzw. "C:\" (Windows). */
export function rootOf(p: string): string {
  const win = /^([A-Za-z]:)[\\/]?/.exec(p);
  return win ? `${win[1]}\\` : "/";
}

/** Letzter Namensbestandteil eines Pfads; für die Wurzel der Pfad selbst. */
export function baseName(p: string): string {
  if (isRoot(p)) return p;
  const parts = p.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || rootOf(p);
}

/** Kette von der Wurzel bis zum Pfad (inklusive), z. B. ["/","/Users","/Users/x"]. */
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
