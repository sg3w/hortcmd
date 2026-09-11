// ============================================================
// Settings ▸ General (TICKET-035).
//
// Language, theme, font and icon scale, and the terminal program.
// ============================================================

import type { Lang, TransKey } from "@/i18n/dictionaries";
import { useSettings, useT } from "@/store/settingsStore";
import type { Scale, Theme } from "@/store/settingsStore";
import { Field, Segmented } from "./controls";
import {
  Image as ImageIcon,
  Languages,
  Monitor,
  Moon,
  Palette,
  Sun,
  Terminal,
  Type,
} from "lucide-react";

/** Options for the size segmentation (small/medium/large). */
const SCALE_OPTIONS: { value: Scale; key: TransKey }[] = [
  { value: "sm", key: "settings.scale.sm" },
  { value: "md", key: "settings.scale.md" },
  { value: "lg", key: "settings.scale.lg" },
];

export function GeneralForm() {
  const t = useT();
  const language = useSettings((s) => s.language);
  const setLanguage = useSettings((s) => s.setLanguage);
  const theme = useSettings((s) => s.theme);
  const setTheme = useSettings((s) => s.setTheme);
  const fontScale = useSettings((s) => s.fontScale);
  const setFontScale = useSettings((s) => s.setFontScale);
  const iconScale = useSettings((s) => s.iconScale);
  const setIconScale = useSettings((s) => s.setIconScale);
  const terminalProgram = useSettings((s) => s.terminalProgram);
  const setTerminalProgram = useSettings((s) => s.setTerminalProgram);

  const scaleOptions = SCALE_OPTIONS.map((o) => ({
    value: o.value,
    label: t(o.key),
  }));

  return (
    <div className="flex flex-col gap-5">
      <Field icon={Languages} label={t("settings.language")}>
        <Segmented<Lang>
          value={language}
          onChange={setLanguage}
          options={[
            { value: "de", label: "Deutsch" },
            { value: "en", label: "English" },
          ]}
        />
      </Field>

      <Field icon={Palette} label={t("settings.theme")}>
        <Segmented<Theme>
          value={theme}
          onChange={setTheme}
          options={[
            { value: "dark", label: t("settings.theme.dark"), Icon: Moon },
            { value: "light", label: t("settings.theme.light"), Icon: Sun },
            {
              value: "system",
              label: t("settings.theme.system"),
              Icon: Monitor,
            },
          ]}
        />
      </Field>

      <Field icon={Type} label={t("settings.fontScale")}>
        <Segmented<Scale>
          value={fontScale}
          onChange={setFontScale}
          options={scaleOptions}
        />
      </Field>

      <Field icon={ImageIcon} label={t("settings.iconScale")}>
        <Segmented<Scale>
          value={iconScale}
          onChange={setIconScale}
          options={scaleOptions}
        />
      </Field>

      <Field icon={Terminal} label={t("settings.terminal")}>
        <input
          value={terminalProgram}
          onChange={(e) => setTerminalProgram(e.target.value)}
          placeholder={t("settings.terminal.placeholder")}
          spellCheck={false}
          className="w-full rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent"
        />
        <span className="text-[11px] text-dim">{t("settings.terminal.hint")}</span>
      </Field>
    </div>
  );
}
