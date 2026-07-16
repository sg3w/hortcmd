// ============================================================
// Speicherort-Auswahl als Radix Select (gestylt im TC-Look).
// Ersetzt die frühere Button-Leiste.
// ============================================================

import * as Select from "@radix-ui/react-select";
import { Check, ChevronDown, HardDrive, House, Star } from "lucide-react";
import type { Drive } from "@/ipc/bindings";
import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { formatSize } from "@/lib/format";
import { useSettings, useT } from "@/store/settingsStore";
import { cn } from "@/lib/cn";

interface Props {
  side: Side;
  drives: Drive[];
}

export function DriveSelect({ side, drives }: Props) {
  const path = usePanes((s) => panelOf(s, side).path);
  const loadDir = usePanes((s) => s.loadDir);
  const setActive = usePanes((s) => s.setActive);
  const favorites = useSettings((s) => s.favorites);
  const t = useT();

  // Längster passender Mount = aktueller Speicherort.
  const current = [...drives]
    .filter((d) => path.startsWith(d.mount))
    .sort((a, b) => b.mount.length - a.mount.length)[0];
  const free = current ? current.free : 0;

  return (
    <div className="flex flex-shrink-0 items-center gap-2 border-b border-edge bg-header px-2 py-1">
      <Select.Root
        value={current?.mount ?? ""}
        onValueChange={(value) => {
          setActive(side);
          // Favoriten tragen ein "fav:"-Präfix, um mit Laufwerks-Mounts
          // nicht zu kollidieren.
          loadDir(side, value.startsWith("fav:") ? value.slice(4) : value);
        }}
      >
        <Select.Trigger
          className="flex min-w-[150px] items-center gap-1.5 rounded border border-edge bg-panel px-2 py-0.5 font-mono text-text outline-none hover:border-accent data-[state=open]:border-accent"
          aria-label="Speicherort"
        >
          <HardDrive size={13} className="shrink-0 text-accent" />
          <Select.Value placeholder={t("drive.placeholder")} />
          <Select.Icon className="ml-auto">
            <ChevronDown size={13} className="text-dim" />
          </Select.Icon>
        </Select.Trigger>

        <Select.Portal>
          <Select.Content
            position="popper"
            sideOffset={4}
            className="z-50 max-h-[320px] min-w-[240px] overflow-hidden rounded-md border border-edge bg-panel shadow-xl"
          >
            <Select.Viewport className="p-1">
              {favorites.length > 0 && (
                <Select.Group>
                  <Select.Label className="px-2 py-1 text-[10px] uppercase tracking-wide text-dim">
                    {t("settings.cat.favorites")}
                  </Select.Label>
                  {favorites.map((f) => (
                    <Select.Item
                      key={`fav:${f.path}`}
                      value={`fav:${f.path}`}
                      className={cn(
                        "flex cursor-default select-none items-center gap-2 rounded px-2 py-1 text-[12px] text-text outline-none",
                        "data-[highlighted]:bg-accent-dim",
                      )}
                    >
                      <Star size={14} className="shrink-0 text-amber-400" />
                      <div className="flex min-w-0 flex-col leading-tight">
                        <Select.ItemText>{f.name}</Select.ItemText>
                        <span className="truncate font-mono text-[10px] text-dim">
                          {f.path}
                        </span>
                      </div>
                    </Select.Item>
                  ))}
                  <Select.Separator className="my-1 h-px bg-edge" />
                </Select.Group>
              )}
              {drives.map((d) => {
                const isHome = d.name === "~" || d.name === "Home";
                return (
                  <Select.Item
                    key={d.mount}
                    value={d.mount}
                    className={cn(
                      "flex cursor-default select-none items-center gap-2 rounded px-2 py-1 text-[12px] text-text outline-none",
                      "data-[highlighted]:bg-accent-dim data-[state=checked]:text-accent",
                    )}
                  >
                    {isHome ? (
                      <House size={14} className="shrink-0 text-accent" />
                    ) : (
                      <HardDrive size={14} className="shrink-0 text-dim" />
                    )}
                    <div className="flex min-w-0 flex-col leading-tight">
                      <Select.ItemText>{d.name}</Select.ItemText>
                      <span className="truncate font-mono text-[10px] text-dim">
                        {d.mount}
                      </span>
                    </div>
                    {d.total > 0 && (
                      <span className="ml-auto whitespace-nowrap font-mono text-[10px] text-dim">
                        {formatSize(d.free)} {t("drive.free")}
                      </span>
                    )}
                    <Select.ItemIndicator className="ml-1">
                      <Check size={12} className="text-accent" />
                    </Select.ItemIndicator>
                  </Select.Item>
                );
              })}
            </Select.Viewport>
          </Select.Content>
        </Select.Portal>
      </Select.Root>

      {free > 0 && (
        <span className="ml-auto whitespace-nowrap font-mono text-[11px] text-dim">
          {formatSize(free)} {t("drive.free")}
        </span>
      )}
    </div>
  );
}
