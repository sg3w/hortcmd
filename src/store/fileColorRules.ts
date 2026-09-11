// ============================================================
// File colours as the app sees them (TICKET-035).
//
// `lib/fileColors.ts` owns what a colour *is*: the slot registry, the
// matching rules, the priority order and how a value becomes CSS. It
// says nothing about where the user's configuration comes from, which
// is what keeps it a leaf utility — reusable, and testable without a
// store.
//
// These two hooks are the other half: they read the persisted settings
// and hand the pure functions their input. They live here, beside the
// store they depend on, rather than inside the utility that must not.
// ============================================================

import { useEffect, useMemo } from "react";
import {
  applyFileColors,
  customColorDefs,
  fileColorDefs,
  type FileColorDef,
  type ThemeMode,
} from "@/lib/fileColors";
import { useSettings } from "./settingsStore";

/** All active rules (custom first), sorted by priority. */
export function useFileColorRules(): FileColorDef[] {
  const custom = useSettings((s) => s.customColorRules);
  return useMemo(
    () =>
      [...fileColorDefs(), ...customColorDefs(custom)]
        .filter((d) => d.kind === "rule")
        .sort((a, b) => a.priority - b.priority),
    [custom],
  );
}

/** Keeps the published CSS variables in sync with theme and settings. */
export function useFileColorVars(theme: ThemeMode): void {
  const overrides = useSettings((s) => s.fileColors);
  const custom = useSettings((s) => s.customColorRules);
  useEffect(() => {
    applyFileColors(theme, overrides, custom);
  }, [theme, overrides, custom]);
}
