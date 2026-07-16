// ============================================================
// Erkennung unterstützter Archivformate anhand der Dateiendung.
// Muss mit der Backend-Erkennung (`archive_kind` in archive.rs)
// übereinstimmen: ZIP, tar, tar.gz, tar.xz, 7z.
// ============================================================

const ARCHIVE_SUFFIXES = [
  ".zip",
  ".tar",
  ".tar.gz",
  ".tgz",
  ".tar.xz",
  ".txz",
  ".7z",
];

/** Ist `name` ein von hortcmd lesbares Archiv? */
export function isArchive(name: string): boolean {
  const lower = name.toLowerCase();
  return ARCHIVE_SUFFIXES.some((s) => lower.endsWith(s));
}
