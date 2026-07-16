// Wandelt ein einfaches Wildcard-Muster (* und ?) in eine RegExp um.
// Mehrere Muster durch Leerzeichen oder ";" trennbar (z. B. "*.txt *.md").

export function globToRegExp(pattern: string): RegExp {
  const parts = pattern
    .split(/[;\s]+/)
    .map((p) => p.trim())
    .filter(Boolean);
  const alts = (parts.length ? parts : ["*"]).map(oneGlob);
  return new RegExp(`^(?:${alts.join("|")})$`, "i");
}

function oneGlob(glob: string): string {
  return glob
    .replace(/[.+^${}()|[\]\\]/g, "\\$&") // Regex-Sonderzeichen escapen
    .replace(/\*/g, ".*")
    .replace(/\?/g, ".");
}
