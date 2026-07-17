// Color/text classes for the Git status of an entry.

const GIT_TEXT: Record<string, string> = {
  modified: "text-amber-400",
  new: "text-emerald-400",
  deleted: "text-red-400 line-through",
  renamed: "text-sky-400",
  conflict: "text-red-500 font-bold",
  ignored: "text-dim italic",
};

/** Text class for a Git status (or undefined when unknown/empty). */
export function gitTextClass(status?: string): string | undefined {
  return status ? GIT_TEXT[status] : undefined;
}
