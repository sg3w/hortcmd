/** A displayed place, including the container for an archive's inner path. */
export interface NavigationLocation {
  path: string;
  archive: string | null;
}
/** Strings are accepted only for compatibility with an older in-memory session. */
export type HistoryEntry = NavigationLocation | string;
/** true appends, false refreshes, a number revisits an existing history entry. */
export type HistoryNavigation = boolean | number;

export function historyLocation(entry: HistoryEntry): NavigationLocation {
  return typeof entry === "string" ? { path: entry, archive: null } : entry;
}

export function recordNavigation(
  tab: { history: HistoryEntry[]; historyIndex: number },
  location: NavigationLocation,
  navigation: HistoryNavigation,
): { history: NavigationLocation[]; historyIndex: number } {
  let history = tab.history.map(historyLocation);
  let historyIndex = tab.historyIndex;
  if (typeof navigation === "number") {
    if (Number.isInteger(navigation) && navigation >= 0 && navigation < history.length) {
      historyIndex = navigation;
      history[historyIndex] = location;
    }
  } else if (navigation) {
    const previous = history[historyIndex];
    if (previous?.path !== location.path || previous.archive !== location.archive) {
      history = [...history.slice(0, historyIndex + 1), location];
      historyIndex = history.length - 1;
    }
  }
  return { history, historyIndex };
}
