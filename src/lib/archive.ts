// ============================================================
// Detection of supported archive formats by file extension.
// Must match the backend detection (`archive_kind` in archive.rs):
// ZIP, tar, tar.gz, tar.xz, 7z.
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

/** Is `name` an archive readable by hortcmd? */
export function isArchive(name: string): boolean {
  const lower = name.toLowerCase();
  return ARCHIVE_SUFFIXES.some((s) => lower.endsWith(s));
}
