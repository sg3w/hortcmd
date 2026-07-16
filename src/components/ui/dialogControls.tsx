// ============================================================
// Kleine, wiederverwendbare Formular-Bausteine für Dialoge
// (Export, Massenumbenennen …): Abschnitt, Checkbox, Radio.
// ============================================================

import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

export function Section({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-[12px] font-medium uppercase tracking-wide text-dim">
        {label}
      </span>
      {children}
    </div>
  );
}

export function Check({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: () => void;
  label: string;
}) {
  return (
    <label className="flex cursor-pointer items-center gap-2 text-[13px] text-text">
      <input
        type="checkbox"
        checked={checked}
        onChange={onChange}
        className="accent-[color:var(--accent)]"
      />
      {label}
    </label>
  );
}

export function Radio({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: () => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <label
      className={cn(
        "flex items-center gap-2 text-[13px]",
        disabled
          ? "cursor-default text-dim opacity-50"
          : "cursor-pointer text-text",
      )}
    >
      <input
        type="radio"
        checked={checked}
        onChange={onChange}
        disabled={disabled}
        className="accent-[color:var(--accent)]"
      />
      {label}
    </label>
  );
}
