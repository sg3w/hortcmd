import { create } from "zustand";
export const useUndo = create<{ open: boolean; setOpen: (open: boolean) => void }>((set) => ({ open: false, setOpen: open => set({ open }) }));
export function openUndo(): void { useUndo.getState().setOpen(true); }
