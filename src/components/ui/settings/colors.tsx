// ============================================================
// Settings ▸ File colours (TICKET-035).
//
// The built-in colour slots (Git status, hidden, read-only, …) and the
// user's own wildcard rules, each with a dark and a light value. The
// picker, the row and the group live here too — they are this page's
// vocabulary and nothing else uses them.
// ============================================================

import { cn } from "@/lib/cn";
import { type CustomColorRule, type FileColorDef, type FileColorOverrides, type ThemeMode, customColorId, fileColorDefs, fileColorLabel, fileColorRef, fileColorValue, normalizeColor } from "@/lib/fileColors";
import { useSettings, useT } from "@/store/settingsStore";
import type { TransKey } from "@/i18n/dictionaries";
import {
  ChevronDown,
  ChevronUp,
  Paintbrush,
  Plus,
  RotateCcw,
  Trash2,
} from "lucide-react";

/** Grid shared by every color row and its header, so columns line up. */
const COLOR_GRID = "grid grid-cols-[1fr_3rem_3rem_1.5rem] items-center gap-2";

const iconBtn =
  "flex h-6 w-6 items-center justify-center rounded text-dim hover:bg-header hover:text-text disabled:opacity-30 disabled:hover:bg-transparent";

/**
 * Translator and store actions are taken once in `ColorsForm` and handed
 * down. Every row here mounts at the same time, and a row that subscribes
 * to the persisted settings store itself is the pattern that produced
 * "Cannot update a component while rendering a different component"
 * elsewhere in this codebase (ticket 354).
 */
type T = (key: TransKey) => string;

function ColorPicker({
  value,
  onChange,
  label,
}: {
  value: string;
  onChange: (v: string) => void;
  label: string;
}) {
  return (
    <input
      type="color"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      aria-label={label}
      title={label}
      className="h-6 w-8 cursor-pointer justify-self-center rounded border border-edge bg-panel p-[2px]"
    />
  );
}

/**
 * One built-in slot: the label is rendered the way the file list renders
 * the color — that doubles as the live preview — plus a picker per theme.
 */
function ColorRow({
  def,
  overrides,
  onChange,
  t,
}: {
  def: FileColorDef;
  overrides: FileColorOverrides;
  onChange: (id: string, theme: ThemeMode, color: string | null) => void;
  t: T;
}) {
  const overridden = !!overrides[def.id];
  return (
    <li className={COLOR_GRID}>
      <span className="flex min-w-0 flex-col leading-tight">
        <span
          className={cn(
            "truncate rounded px-1 py-0.5 font-mono text-[12px]",
            def.previewClass,
            def.extraClass,
          )}
          // A state paints the whole row via previewClass; a rule is text.
          style={{ color: def.previewClass ? undefined : fileColorRef(def.id) }}
        >
          {fileColorLabel(def, t)}
        </span>
        {def.hintKey && (
          <span className="truncate px-1 text-[10px] text-dim">
            {t(def.hintKey)}
          </span>
        )}
      </span>
      <ColorPicker
        value={fileColorValue(def, "dark", overrides)}
        onChange={(v) => onChange(def.id, "dark", v)}
        label={t("settings.colors.dark")}
      />
      <ColorPicker
        value={fileColorValue(def, "light", overrides)}
        onChange={(v) => onChange(def.id, "light", v)}
        label={t("settings.colors.light")}
      />
      <button
        onClick={() => {
          onChange(def.id, "dark", null);
          onChange(def.id, "light", null);
        }}
        disabled={!overridden}
        title={t("settings.colors.resetOne")}
        className={iconBtn}
      >
        <RotateCcw size={12} />
      </button>
    </li>
  );
}

/** A titled group of color rows with the dark/light column header. */
function ColorGroup({
  label,
  defs,
  overrides,
  onChange,
  t,
}: {
  label: string;
  defs: FileColorDef[];
  overrides: FileColorOverrides;
  onChange: (id: string, theme: ThemeMode, color: string | null) => void;
  t: T;
}) {
  if (defs.length === 0) return null;
  return (
    <div className="flex flex-col gap-1">
      <div
        className={cn(
          COLOR_GRID,
          "text-[10px] uppercase tracking-wide text-dim",
        )}
      >
        <span>{label}</span>
        <span className="text-center">{t("settings.colors.dark")}</span>
        <span className="text-center">{t("settings.colors.light")}</span>
        <span />
      </div>
      <ul className="flex flex-col gap-1">
        {defs.map((def) => (
          <ColorRow
            key={def.id}
            def={def}
            overrides={overrides}
            onChange={onChange}
            t={t}
          />
        ))}
      </ul>
    </div>
  );
}

/** One user rule: name (in its own color), pattern, colors, order. */
function CustomRuleRow({
  rule,
  first,
  last,
  t,
  update,
  remove,
  move,
}: {
  rule: CustomColorRule;
  first: boolean;
  last: boolean;
  t: T;
  update: (id: string, patch: Partial<CustomColorRule>) => void;
  remove: (id: string) => void;
  move: (id: string, delta: number) => void;
}) {
  return (
    <li className="flex flex-col gap-2 rounded border border-edge bg-panel-inactive p-2">
      <div className={COLOR_GRID}>
        <input
          value={rule.name}
          onChange={(e) => update(rule.id, { name: e.target.value })}
          placeholder={t("settings.colors.custom.name")}
          spellCheck={false}
          style={{ color: fileColorRef(customColorId(rule.id)) }}
          className="min-w-0 rounded border border-edge bg-panel px-2 py-0.5 font-mono text-[12px] outline-none focus:border-accent"
        />
        <ColorPicker
          value={normalizeColor(rule.dark)}
          onChange={(v) => update(rule.id, { dark: v })}
          label={t("settings.colors.dark")}
        />
        <ColorPicker
          value={normalizeColor(rule.light)}
          onChange={(v) => update(rule.id, { light: v })}
          label={t("settings.colors.light")}
        />
        <button
          onClick={() => remove(rule.id)}
          title={t("settings.colors.custom.remove")}
          className={cn(iconBtn, "hover:bg-red-500/20 hover:text-red-400")}
        >
          <Trash2 size={12} />
        </button>
      </div>
      <div className="flex items-center gap-2">
        <input
          value={rule.pattern}
          onChange={(e) => update(rule.id, { pattern: e.target.value })}
          placeholder={t("settings.colors.custom.pattern")}
          spellCheck={false}
          className="min-w-0 flex-1 rounded border border-edge bg-panel px-2 py-0.5 font-mono text-[11px] text-text outline-none focus:border-accent"
        />
        <button
          onClick={() => move(rule.id, -1)}
          disabled={first}
          title={t("settings.colors.custom.up")}
          className={iconBtn}
        >
          <ChevronUp size={12} />
        </button>
        <button
          onClick={() => move(rule.id, 1)}
          disabled={last}
          title={t("settings.colors.custom.down")}
          className={iconBtn}
        >
          <ChevronDown size={12} />
        </button>
      </div>
    </li>
  );
}

export function ColorsForm() {
  const t = useT();
  const overrides = useSettings((s) => s.fileColors);
  const setFileColor = useSettings((s) => s.setFileColor);
  const resetFileColors = useSettings((s) => s.resetFileColors);
  const customRules = useSettings((s) => s.customColorRules);
  const addColorRule = useSettings((s) => s.addColorRule);
  const updateColorRule = useSettings((s) => s.updateColorRule);
  const removeColorRule = useSettings((s) => s.removeColorRule);
  const moveColorRule = useSettings((s) => s.moveColorRule);

  // Whatever the registry holds: a newly registered rule appears here
  // without any change to this component.
  const defs = fileColorDefs();

  return (
    <div className="flex flex-col gap-5">
      <p className="text-[11px] text-dim">{t("settings.colors.hint")}</p>

      <div className="flex flex-col gap-3">
        <ColorGroup
          label={t("settings.colors.rules")}
          defs={defs.filter((d) => d.kind === "rule")}
          overrides={overrides}
          onChange={setFileColor}
          t={t}
        />
        <ColorGroup
          label={t("settings.colors.states")}
          defs={defs.filter((d) => d.kind === "state")}
          overrides={overrides}
          onChange={setFileColor}
          t={t}
        />
        <button
          onClick={resetFileColors}
          disabled={Object.keys(overrides).length === 0}
          className="flex items-center gap-1.5 self-start rounded border border-edge bg-panel px-3 py-1 text-[12px] text-text hover:border-accent disabled:opacity-40"
        >
          <RotateCcw size={13} />
          {t("settings.colors.reset")}
        </button>
      </div>

      {/* User-defined rules — matched before the built-in ones */}
      <div className="flex flex-col gap-2 border-t border-edge pt-3">
        <span className="flex items-center gap-2 text-[13px] text-text">
          <Paintbrush size={15} className="text-dim" />
          {t("settings.colors.custom")}
        </span>
        <span className="text-[11px] text-dim">
          {t("settings.colors.custom.hint")}
        </span>
        {customRules.length === 0 ? (
          <p className="text-[12px] text-dim">
            {t("settings.colors.custom.empty")}
          </p>
        ) : (
          <ul className="flex flex-col gap-2">
            {customRules.map((r, i) => (
              <CustomRuleRow
                key={r.id}
                rule={r}
                first={i === 0}
                last={i === customRules.length - 1}
                t={t}
                update={updateColorRule}
                remove={removeColorRule}
                move={moveColorRule}
              />
            ))}
          </ul>
        )}
        <button
          onClick={addColorRule}
          className="flex items-center justify-center gap-1.5 rounded border border-dashed border-edge px-2 py-1.5 text-[12px] text-dim hover:border-accent hover:text-text"
        >
          <Plus size={13} />
          {t("settings.colors.custom.add")}
        </button>
      </div>
    </div>
  );
}
