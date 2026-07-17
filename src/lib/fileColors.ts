// ============================================================
// Central registry of the file colors (TICKET-009).
//
// Every color the file list can apply is registered here as a slot with
// a semantic id, a label, and a default per theme. Slots of kind "rule"
// additionally carry a matcher and a priority: the first matching rule
// (lowest priority value first) colors the entry — and that is exactly
// the rule the properties dialog explains.
//
// Colors reach the DOM as CSS variables (`--fc-<id>`, published by
// `applyFileColors`). File list, properties dialog, and settings preview
// all reference the same variable, so they cannot drift apart, and
// changing a color repaints without re-rendering the list.
//
// Extending: `registerFileColor()` adds a slot at module load (e.g. from
// a plugin). The settings UI renders whatever the registry contains, so
// a new rule needs no UI change.
// ============================================================

import { useEffect, useMemo } from "react";
import type { TransKey } from "@/i18n/dictionaries";
import { globToRegExp } from "@/lib/glob";
import { useSettings } from "@/store/settingsStore";

export type ThemeMode = "dark" | "light";

/** Everything a rule may look at; `DirEntry` and `FileProps` both fit. */
export interface FileColorContext {
  name: string;
  is_dir: boolean;
  is_symlink: boolean;
  hidden: boolean;
  readonly: boolean;
  executable: boolean;
  /** Git status of the entry, or undefined when unknown/disabled. */
  gitStatus?: string;
}

/** "rule" colors an entry by its state, "state" colors an interaction. */
export type FileColorKind = "rule" | "state";

export interface FileColorDef {
  id: string;
  kind: FileColorKind;
  /** Built-ins translate `labelKey`; custom rules carry a plain `label`. */
  labelKey?: TransKey;
  label?: string;
  /** Optional explanation shown below the label in the settings. */
  hintKey?: TransKey;
  /** Color per theme; used when the settings hold no valid override. */
  defaults: Record<ThemeMode, string>;
  /** Lower value = matched first. */
  priority: number;
  /** Rules only: does this color apply to `ctx`? */
  match?: (ctx: FileColorContext) => boolean;
  /** Classes applied together with the color (e.g. strikethrough). */
  extraClass?: string;
  /**
   * How the settings preview renders this slot. Rules simply paint their
   * color as text and need nothing here; a state paints the row instead,
   * which only its own CSS can express.
   */
  previewClass?: string;
  /** Published with this alpha (0..1) instead of opaque. */
  alpha?: number;
}

/** Persisted overrides: slot id → color per theme. */
export type FileColorOverrides = Record<
  string,
  Partial<Record<ThemeMode, string>>
>;

/** A user-defined rule (Settings → File colors). */
export interface CustomColorRule {
  id: string;
  /** Label in the settings and the properties dialog. */
  name: string;
  /** Wildcard pattern(s), e.g. "*.log *.tmp". */
  pattern: string;
  dark: string;
  light: string;
}

/** Only `#rrggbb` is accepted — that is what `<input type="color">` yields. */
const HEX = /^#[0-9a-f]{6}$/i;
/** Last resort when even a default is unusable (hand-edited config). */
const FALLBACK_COLOR = "#9aa0a6";
/** Custom rules are matched before the built-ins (explicit user intent). */
const CUSTOM_PRIORITY_BASE = -1000;

/** Built-in rule for one Git status. */
function gitRule(
  status: string,
  priority: number,
  defaults: Record<ThemeMode, string>,
  extraClass?: string,
): FileColorDef {
  const id = `git.${status}`;
  return {
    id,
    kind: "rule",
    labelKey: `color.${id}` as TransKey,
    priority,
    defaults,
    extraClass,
    match: (c) => c.gitStatus === status,
  };
}

const BUILTIN: FileColorDef[] = [
  // ----- Git status (codes come from commands/fs/git.rs) -----
  gitRule("conflict", 10, { dark: "#ef4444", light: "#b91c1c" }, "font-bold"),
  gitRule("deleted", 20, { dark: "#f87171", light: "#dc2626" }, "line-through"),
  gitRule("renamed", 30, { dark: "#38bdf8", light: "#0369a1" }),
  gitRule("untracked", 40, { dark: "#34d399", light: "#047857" }),
  gitRule("staged", 50, { dark: "#a78bfa", light: "#6d28d9" }),
  gitRule("modified", 60, { dark: "#fbbf24", light: "#b45309" }),
  gitRule("ignored", 70, { dark: "#9aa0a6", light: "#6b7178" }, "italic"),

  // ----- File attributes (flags come from commands/fs/attrs.rs) -----
  {
    id: "symlink",
    kind: "rule",
    labelKey: "color.symlink",
    priority: 80,
    defaults: { dark: "#22d3ee", light: "#0e7490" },
    match: (c) => c.is_symlink,
  },
  {
    id: "executable",
    kind: "rule",
    labelKey: "color.executable",
    priority: 90,
    defaults: { dark: "#7ee787", light: "#15803d" },
    match: (c) => c.executable,
  },
  {
    id: "readonly",
    kind: "rule",
    labelKey: "color.readonly",
    priority: 100,
    defaults: { dark: "#94a3b8", light: "#475569" },
    match: (c) => c.readonly && !c.is_dir,
  },
  {
    id: "hidden",
    kind: "rule",
    labelKey: "color.hidden",
    priority: 110,
    defaults: { dark: "#6e7681", light: "#9ca3af" },
    match: (c) => c.hidden,
  },

  // ----- Interaction states (no matcher; consumed by index.css) -----
  {
    id: "selection",
    kind: "state",
    labelKey: "color.selection",
    hintKey: "color.selection.hint",
    priority: 200,
    defaults: { dark: "#4a3b0f", light: "#fde68a" },
    previewClass: "bg-selection text-selection-text",
  },
  {
    id: "cursor.active",
    kind: "state",
    labelKey: "color.cursorActive",
    hintKey: "color.cursorActive.hint",
    priority: 210,
    defaults: { dark: "#3b82f6", light: "#2563eb" },
    previewClass:
      "bg-accent-dim outline outline-1 -outline-offset-1 outline-cursor",
  },
  {
    id: "cursor.inactive",
    kind: "state",
    labelKey: "color.cursorInactive",
    hintKey: "color.cursorInactive.hint",
    priority: 220,
    defaults: { dark: "#9ac27b", light: "#9ac27b" },
    alpha: 0.4,
    previewClass:
      "bg-[color:var(--cursor-inactive-marker)] outline outline-1 -outline-offset-1 outline-[color:var(--cursor-inactive)]",
  },
];

const registry = new Map<string, FileColorDef>(BUILTIN.map((d) => [d.id, d]));

/** Registers an additional color slot (e.g. from a plugin) at module load. */
export function registerFileColor(def: FileColorDef): void {
  registry.set(def.id, def);
}

/** All registered slots, ordered as the settings show them. */
export function fileColorDefs(): FileColorDef[] {
  return [...registry.values()].sort((a, b) => a.priority - b.priority);
}

/** Slot id of a custom rule. */
export function customColorId(ruleId: string): string {
  return `custom.${ruleId}`;
}

/** Persisted custom rules → defs (each pattern compiled once). */
export function customColorDefs(rules: CustomColorRule[]): FileColorDef[] {
  return rules.map((rule, i) => {
    const pattern = rule.pattern.trim();
    // An empty pattern would match everything — treat it as inactive.
    const re = pattern ? globToRegExp(pattern) : null;
    return {
      id: customColorId(rule.id),
      kind: "rule",
      label: rule.name.trim() || pattern,
      priority: CUSTOM_PRIORITY_BASE + i,
      defaults: { dark: rule.dark, light: rule.light },
      match: (c) => re != null && re.test(c.name),
    };
  });
}

/** Label of a slot; built-ins are translated, custom rules are user text. */
export function fileColorLabel(
  def: FileColorDef,
  t: (key: TransKey) => string,
): string {
  return def.labelKey ? t(def.labelKey) : def.label || def.id;
}

/**
 * Configured color of a slot. Falls back to the slot default when the
 * stored value is missing or not a `#rrggbb` literal, so a broken or
 * hand-edited configuration still renders.
 */
export function fileColorValue(
  def: FileColorDef,
  theme: ThemeMode,
  overrides: FileColorOverrides,
): string {
  const stored = overrides[def.id]?.[theme];
  return stored && HEX.test(stored)
    ? stored
    : normalizeColor(def.defaults[theme]);
}

/** `value` if it is a usable `#rrggbb`, otherwise `fallback`. */
export function normalizeColor(
  value: string | undefined,
  fallback = FALLBACK_COLOR,
): string {
  return value && HEX.test(value) ? value : fallback;
}

/** Name of the CSS variable a slot is published under. */
export function fileColorVar(id: string): string {
  return `--fc-${id.replace(/\./g, "-")}`;
}

/** `var(--fc-…)` reference for inline styles. */
export function fileColorRef(id: string): string {
  return `var(${fileColorVar(id)})`;
}

/** `#rrggbb` → `rgba()` with the given alpha. */
function withAlpha(hex: string, alpha: number): string {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

/** The value published for a slot (respects `alpha`). */
export function fileColorCss(
  def: FileColorDef,
  theme: ThemeMode,
  overrides: FileColorOverrides,
): string {
  const hex = fileColorValue(def, theme, overrides);
  return def.alpha == null ? hex : withAlpha(hex, def.alpha);
}

/** Variables published on the last run, so removed rules get cleaned up. */
let publishedVars: string[] = [];

/** Publishes every color as a CSS variable on `<html>`. */
export function applyFileColors(
  theme: ThemeMode,
  overrides: FileColorOverrides,
  custom: CustomColorRule[],
): void {
  const style = document.documentElement.style;
  const defs = [...fileColorDefs(), ...customColorDefs(custom)];
  const vars = defs.map((d) => fileColorVar(d.id));
  for (const name of publishedVars) {
    if (!vars.includes(name)) style.removeProperty(name);
  }
  for (const def of defs) {
    style.setProperty(fileColorVar(def.id), fileColorCss(def, theme, overrides));
  }
  publishedVars = vars;
}

/**
 * The rule that colors `ctx`, or undefined when none applies (the entry
 * then keeps the default folder/file color). `rules` must be sorted by
 * priority — use `useFileColorRules()`.
 */
export function resolveFileColor(
  rules: FileColorDef[],
  ctx: FileColorContext,
): FileColorDef | undefined {
  return rules.find((r) => r.match?.(ctx));
}

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
