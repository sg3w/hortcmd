// ============================================================
// The settings dialog's shared control vocabulary (TICKET-035).
//
// Every page is built from these four: a labelled `Field`, a
// `Segmented` choice, a `Toggle` with an optional hint, and the small
// button style the list rows use. Keeping them here is what lets a page
// be read as a list of settings rather than as markup.
// ============================================================

import { consumeDialogKey } from "@/lib/keys";
import { normalizeExt } from "@/store/settingsStore";
import { useId, useState } from "react";
import {
  X,
  type LucideIcon,
} from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";
import { useSettings, useT } from "@/store/settingsStore";

/**
 * The mark on a setting whose feature is not finished yet (TICKET-391).
 *
 * It sits next to the label rather than in it, so the label stays the
 * accessible name of the switch and screen readers are not made to read
 * "BETA" as part of every announcement of the control's state.
 */
export function Beta() {
  const t = useT();
  return (
    <span className="rounded border border-amber-500/40 bg-amber-500/10 px-1 py-px text-[9px] font-semibold tracking-wide text-amber-400">
      {t("settings.beta")}
    </span>
  );
}

export function Toggle({
  checked,
  onChange,
  icon: Icon,
  label,
  hint,
  badge,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  icon: LucideIcon;
  label: string;
  hint?: string;
  /** Rendered after the label, e.g. `<Beta />`. */
  badge?: ReactNode;
}) {
  const hintId = useId();
  return (
    <label className="flex cursor-pointer items-start gap-3">
      <button
        role="switch"
        aria-checked={checked}
        aria-label={label}
        aria-describedby={hint ? hintId : undefined}
        onClick={() => onChange(!checked)}
        className={cn(
          "mt-0.5 flex h-5 w-9 shrink-0 items-center rounded-full p-0.5 transition",
          checked ? "justify-end bg-accent" : "justify-start bg-cursor-inactive",
        )}
      >
        <span className="h-4 w-4 rounded-full bg-white" />
      </button>
      <span className="flex items-center gap-2">
        <Icon size={15} className="text-dim" />
        <span className="flex flex-col">
          <span className="flex items-center gap-1.5 text-[13px] text-text">
            {label}
            {badge}
          </span>
          {hint && <span id={hintId} className="text-[11px] text-dim">{hint}</span>}
        </span>
      </span>
    </label>
  );
}

export const smallBtn =
  "flex items-center gap-1 rounded border border-edge bg-panel px-2 py-1 text-[12px] text-text hover:border-accent";

/** Program selection (native select over the configured programs). */
export function ProgramSelect({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const t = useT();
  const programs = useSettings((s) => s.programs);
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="min-w-0 flex-1 rounded border border-edge bg-panel px-1.5 py-1 text-[12px] text-text outline-none focus:border-accent"
    >
      <option value="">{t("settings.editor.none")}</option>
      {programs.map((p) => (
        <option key={p.id} value={p.id}>
          {p.name}
        </option>
      ))}
    </select>
  );
}

/** A program's extension chips with add/remove. */
export function ExtensionTags({
  exts,
  onAdd,
  onRemove,
}: {
  exts: string[];
  onAdd: (ext: string) => void;
  onRemove: (ext: string) => void;
}) {
  const t = useT();
  const [val, setVal] = useState("");
  const add = () => {
    const ext = normalizeExt(val);
    if (ext) onAdd(ext);
    setVal("");
  };
  return (
    <div className="flex flex-wrap items-center gap-1">
      {exts.map((ext) => (
        <span
          key={ext}
          className="flex items-center gap-1 rounded bg-header px-1.5 py-0.5 font-mono text-[11px] text-text"
        >
          .{ext}
          <button
            onClick={() => onRemove(ext)}
            className="text-dim hover:text-red-400"
            title={t("settings.editor.remove")}
          >
            <X size={11} />
          </button>
        </span>
      ))}
      <input
        value={val}
        onChange={(e) => setVal(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            consumeDialogKey(e);
            add();
          }
        }}
        placeholder={t("settings.editor.extPlaceholder")}
        spellCheck={false}
        className="w-16 rounded border border-edge bg-panel px-1.5 py-0.5 font-mono text-[11px] text-text outline-none focus:border-accent"
      />
    </div>
  );
}

export function Field({
  icon: Icon,
  label,
  children,
}: {
  icon: LucideIcon;
  label: string;
  children: ReactNode;
}) {
  return (
    <fieldset className="flex min-w-0 flex-col gap-2">
      <legend className="flex items-center gap-2 text-[13px] text-text">
        <Icon size={15} className="text-dim" />
        {label}
      </legend>
      {children}
    </fieldset>
  );
}

export function Segmented<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string; Icon?: LucideIcon }[];
}) {
  return (
    <div className="inline-flex overflow-hidden rounded border border-edge">
      {options.map((o) => (
        <button
          key={o.value}
          aria-pressed={value === o.value}
          onClick={() => onChange(o.value)}
          className={cn(
            "flex items-center gap-1.5 px-3 py-1 text-[12px]",
            value === o.value
              ? "bg-accent text-white"
              : "bg-panel text-dim hover:text-text",
          )}
        >
          {o.Icon && <o.Icon size={13} />}
          {o.label}
        </button>
      ))}
    </div>
  );
}
