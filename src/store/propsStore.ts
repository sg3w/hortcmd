// ============================================================
// Small store for the properties/permissions dialog: remembers
// the path of the entry whose permissions are shown (null = closed)
// plus its Git status — only the opening pane knows it, and the
// dialog needs it to resolve the entry's color rule.
// ============================================================

import { create } from "zustand";

interface PropsStore {
  path: string | null;
  /** Git status of the entry, or undefined when unknown/disabled. */
  gitStatus?: string;
  open: (path: string, gitStatus?: string) => void;
  close: () => void;
}

export const usePropsDialog = create<PropsStore>((set) => ({
  path: null,
  gitStatus: undefined,
  open: (path, gitStatus) => set({ path, gitStatus }),
  close: () => set({ path: null, gitStatus: undefined }),
}));

/** Convenience call from outside React. */
export function openProps(path: string, gitStatus?: string): void {
  usePropsDialog.getState().open(path, gitStatus);
}
