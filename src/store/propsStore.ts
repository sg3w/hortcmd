// ============================================================
// Small store for the properties/permissions dialog: remembers
// the path of the entry whose permissions are shown (null = closed).
// ============================================================

import { create } from "zustand";

interface PropsStore {
  path: string | null;
  open: (path: string) => void;
  close: () => void;
}

export const usePropsDialog = create<PropsStore>((set) => ({
  path: null,
  open: (path) => set({ path }),
  close: () => set({ path: null }),
}));

/** Convenience call from outside React. */
export function openProps(path: string): void {
  usePropsDialog.getState().open(path);
}
