// Converts a simple wildcard pattern (* and ?) into a RegExp.
// Multiple patterns can be separated by spaces or ";" (e.g. "*.txt *.md").

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
