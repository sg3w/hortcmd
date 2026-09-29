// ============================================================
// "Pack": the dialog that composes the archive name (TICKET-392).
//
// Base, date and time are switches rather than characters to delete out of
// a proposed string. Editing the field by hand takes it out of the options'
// hands; one checkbox hands it back.
//
// The switches are a setting, not a form: the dialog opens on what was chosen
// last (TICKET-394), because archives are made in series.
//
// The name field comes first (TICKET-395). It is what the dialog focuses and
// pre-selects, and in the common case the only control touched; the switches
// that compose it read as its options rather than as the form above it.
// ============================================================

import "@/i18n/namespaces/pack";
import { useEffect, useId, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { FileArchive } from "lucide-react";
import {
  composeArchiveName,
  genericBase,
  selectionBase,
  stemRange,
  type ArchiveNameOptions,
} from "@/features/commander/archiveName";
import { useOps } from "@/store/opsStore";
import { useSettings, useT } from "@/store/settingsStore";
import { consumeDialogKey } from "@/lib/keys";
import { SimpleDialog } from "./SimpleDialog";

const FIELD =
  "w-full rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[13px] text-text outline-none focus:border-accent disabled:opacity-40";

export function PackDialog() {
  const inputId = useId();
  const baseId = useId();
  const pack = useOps((s) => s.pack);
  const closeDialogs = useOps((s) => s.closeDialogs);
  const t = useT();
  const inputRef = useRef<HTMLInputElement>(null);
  // Captured once per opening, so switching date or time off and on again
  // yields the same name instead of a drifting one.
  const openedAt = useRef(new Date());

  // The options are a setting, not a form: read when the dialog opens and
  // written as they change (TICKET-394). Read through `getState` rather than
  // subscribed — a change made here must not re-run the opening effect and
  // throw away what is in the field.
  const [options, setOptions] = useState<ArchiveNameOptions>(
    () => useSettings.getState().packNameOptions,
  );
  const [value, setValue] = useState("");
  /** False once the field was typed into: the options no longer describe it. */
  const [auto, setAuto] = useState(true);

  const entries = pack?.entries ?? [];

  useEffect(() => {
    if (!pack) return;
    openedAt.current = new Date();
    const stored = useSettings.getState().packNameOptions;
    setOptions(stored);
    setAuto(true);
    const initial = composeArchiveName(pack.entries, stored, openedAt.current);
    setValue(initial);
    requestAnimationFrame(() => {
      const el = inputRef.current;
      if (!el) return;
      el.focus();
      const [from, to] = stemRange(initial);
      el.setSelectionRange(from, to);
    });
  }, [pack]);

  /** Applies an option change and rebuilds the name it describes. */
  const changeOptions = (next: Partial<ArchiveNameOptions>) => {
    const merged = { ...options, ...next };
    setOptions(merged);
    // Stored on the change, not on a successful pack: the next opening shows
    // what was last chosen, whether or not that archive was created.
    useSettings.getState().setPackNameOptions(merged);
    setValue(composeArchiveName(entries, merged, openedAt.current));
  };

  /** The takeover: back to the composed name, discarding what was typed. */
  const takeOver = (on: boolean) => {
    setAuto(on);
    if (on) setValue(composeArchiveName(entries, options, openedAt.current));
  };

  const canSubmit = !!pack && value.trim().length > 0;
  const submit = () => {
    if (!canSubmit) {
      inputRef.current?.focus();
      return;
    }
    // Close first, then execute, so `onSubmit` can open a follow-up dialog.
    const fn = pack?.onSubmit;
    closeDialogs();
    fn?.(value);
  };

  const withBase = (key: "archive.base.selection" | "archive.base.generic", base: string) =>
    t(key).replace("{base}", base);

  return (
    <SimpleDialog
      open={!!pack}
      onOpenChange={(o) => !o && closeDialogs()}
      overlayZ="z-[60]"
      contentZ="z-[61]"
      // Roomier than the shared prompt: three options and a name that carries
      // a timestamp need the width to stay readable (TICKET-392).
      className="w-[600px]"
      noDescription
    >
      <Dialog.Title className="flex items-center gap-2 border-b border-edge px-4 py-2.5 text-text">
        <FileArchive size={16} className="text-accent" />
        {t("archive.title")}
      </Dialog.Title>
      <div className="px-4 py-3">
        <label htmlFor={inputId} className="mb-1 block text-[12px] text-dim">
          {t("archive.label")}
        </label>
        <input
          id={inputId}
          ref={inputRef}
          type="text"
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
            setAuto(false);
          }}
          onKeyDown={(e) => {
            // Consume both keys here (TICKET-014): submitting or closing
            // unmounts this input synchronously, and without stopping the
            // event the global pane handler would act on it afterwards.
            if (e.key === "Enter") {
              consumeDialogKey(e);
              submit();
            } else if (e.key === "Escape") {
              consumeDialogKey(e);
              closeDialogs();
            }
          }}
          spellCheck={false}
          autoComplete="off"
          className={FIELD}
        />

        <label className="mt-2.5 flex cursor-pointer items-center gap-2 text-[12px] text-text">
          <input
            type="checkbox"
            checked={auto}
            onChange={(e) => takeOver(e.target.checked)}
            className="accent-accent"
          />
          {t("archive.auto")}
        </label>
        {!auto && <p className="mt-1 text-[12px] text-dim">{t("archive.auto.hint")}</p>}

        <label htmlFor={baseId} className="mb-1 mt-3 block text-[12px] text-dim">
          {t("archive.base")}
        </label>
        <select
          id={baseId}
          value={options.base}
          disabled={!auto}
          onChange={(e) => changeOptions({ base: e.target.value as ArchiveNameOptions["base"] })}
          className={FIELD}
        >
          <option value="selection">{withBase("archive.base.selection", selectionBase(entries))}</option>
          <option value="generic">{withBase("archive.base.generic", genericBase())}</option>
        </select>

        <div className="mt-2.5 flex gap-6">
          {([["archive.addDate", "date"], ["archive.addTime", "time"]] as const).map(([label, key]) => (
            <label
              key={key}
              className="flex cursor-pointer items-center gap-2 text-[12px] text-text has-[:disabled]:cursor-default has-[:disabled]:opacity-40"
            >
              <input
                type="checkbox"
                checked={options[key]}
                disabled={!auto}
                onChange={(e) => changeOptions({ [key]: e.target.checked })}
                className="accent-accent"
              />
              {t(label)}
            </label>
          ))}
        </div>

      </div>
      <div className="flex justify-end gap-2 border-t border-edge px-4 py-2.5">
        <button
          onClick={closeDialogs}
          className="rounded border border-edge bg-panel px-3 py-1 text-[13px] text-text hover:border-accent"
        >
          {t("op.cancel")}
        </button>
        <button
          onClick={submit}
          disabled={!canSubmit}
          onPointerDown={(e) => {
            if (!canSubmit) {
              e.preventDefault();
              inputRef.current?.focus();
            }
          }}
          className="rounded bg-accent px-3 py-1 text-[13px] text-white hover:brightness-110 disabled:opacity-40"
        >
          {t("op.create")}
        </button>
      </div>
    </SimpleDialog>
  );
}
