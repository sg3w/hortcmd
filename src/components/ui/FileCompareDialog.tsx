// ============================================================
// Dialog „Datei-/Binärvergleich": vergleicht die Cursor-Datei des
// linken Fensters inhaltlich mit der des rechten. Textdateien werden
// zeilenweise (side-by-side) gegenübergestellt, Binärdateien als
// Hex-Dump. Vergleich erledigt das Backend (`compare_files`); die
// Ergebnisliste ist virtualisiert.
// ============================================================

import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { AlertTriangle, Equal, GitCompare, Loader2 } from "lucide-react";
import type { FileDiff } from "@/ipc/bindings";
import { useFileCompareDialog } from "@/store/fileCompareStore";
import { useT } from "@/store/settingsStore";
import { compareFiles } from "@/ipc/client";
import { baseName } from "@/lib/path";
import { formatSize } from "@/lib/format";
import { cn } from "@/lib/cn";
import { AppDialog } from "@/components/ui/AppDialog";

const ROW_H = 20;
const DEFAULT_SIZE = { w: 1100, h: 700 };
const MIN_SIZE = { w: 700, h: 400 };

// Hintergrund je Diff-Status (Textmodus).
const ROW_BG: Record<string, string> = {
  replace: "bg-amber-500/10",
  delete: "bg-red-500/10",
  insert: "bg-emerald-500/10",
};

export function FileCompareDialog() {
  const left = useFileCompareDialog((s) => s.left);
  const right = useFileCompareDialog((s) => s.right);
  const close = useFileCompareDialog((s) => s.close);
  const t = useT();

  const [data, setData] = useState<FileDiff | null>(null);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const open = !!left && !!right;

  useEffect(() => {
    if (!left || !right) return;
    setData(null);
    setError(null);
    let alive = true;
    void compareFiles(left, right)
      .then((d) => alive && setData(d))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [left, right]);

  const count = data
    ? data.mode === "text"
      ? data.lines.length
      : data.hex.length
    : 0;

  const rowVirtualizer = useVirtualizer({
    count,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_H,
    overscan: 20,
  });

  const items = rowVirtualizer.getVirtualItems();

  const summary = useMemo(() => {
    if (!data) return null;
    if (data.identical) return { key: "fcmp.identical", danger: false } as const;
    return { key: "fcmp.different", danger: true } as const;
  }, [data]);

  const titleBar = (
    <>
      <GitCompare size={16} className="text-dim" aria-hidden />
      <span className="text-[13px] font-medium">{t("fcmp.title")}</span>
      {data && (
        <span
          className={cn(
            "ml-2 flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px]",
            data.mode === "binary"
              ? "bg-purple-500/15 text-purple-300"
              : "bg-accent-dim text-accent",
          )}
        >
          {data.mode === "binary" ? t("fcmp.binary") : t("fcmp.text")}
        </span>
      )}
      {summary && (
        <span
          className={cn(
            "flex items-center gap-1 text-[12px]",
            summary.danger ? "text-amber-400" : "text-emerald-400",
          )}
        >
          {!summary.danger && <Equal size={13} aria-hidden />}
          {t(summary.key)}
        </span>
      )}
      {data?.truncated && (
        <span className="flex items-center gap-1 text-[12px] text-amber-400">
          <AlertTriangle size={13} aria-hidden />
          {t("fcmp.truncated")}
        </span>
      )}
    </>
  );

  return (
    <AppDialog
      dialogKey="filecompare"
      open={open}
      onClose={close}
      titleBar={titleBar}
      defaultSize={DEFAULT_SIZE}
      minSize={MIN_SIZE}
    >
      {/* Kopf mit den beiden Dateinamen */}
      <div className="grid grid-cols-2 gap-px border-b border-edge bg-edge text-[12px]">
        {[
          { path: left, size: data?.left_size },
          { path: right, size: data?.right_size },
        ].map((f, i) => (
          <div key={i} className="flex items-baseline gap-2 bg-header px-3 py-1">
            <span className="truncate font-medium text-text" title={f.path ?? ""}>
              {f.path ? baseName(f.path) : ""}
            </span>
            {f.size != null && <span className="text-dim">{formatSize(f.size)}</span>}
          </div>
        ))}
      </div>

      {/* Inhalt */}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
        {!data && !error && (
          <div className="flex items-center gap-2 p-4 text-[13px] text-dim">
            <Loader2 size={14} className="animate-spin" aria-hidden />
            …
          </div>
        )}
        {error && (
          <div className="m-4 rounded border border-red-500/50 bg-red-500/10 px-3 py-2 text-[12px] text-red-400">
            {error}
          </div>
        )}
        {data && count === 0 && !error && (
          <div className="p-4 text-[13px] text-dim">{t("fcmp.empty")}</div>
        )}
        {data && count > 0 && (
          <div
            className="relative w-full font-mono text-[12px]"
            style={{ height: rowVirtualizer.getTotalSize() }}
          >
            {items.map((v) => (
              <div
                key={v.key}
                className="absolute left-0 top-0 flex w-full"
                style={{ height: ROW_H, transform: `translateY(${v.start}px)` }}
              >
                {data.mode === "text" ? (
                  <TextRow row={data.lines[v.index]} />
                ) : (
                  <HexRowView row={data.hex[v.index]} />
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </AppDialog>
  );
}

function TextRow({ row }: { row: FileDiff["lines"][number] }) {
  const bg = ROW_BG[row.tag] ?? "";
  return (
    <>
      <Side no={row.left_no} text={row.left} bg={bg} />
      <div className="w-px shrink-0 bg-edge" />
      <Side no={row.right_no} text={row.right} bg={bg} />
    </>
  );
}

function Side({
  no,
  text,
  bg,
}: {
  no: number | null;
  text: string | null;
  bg: string;
}) {
  return (
    <div className={cn("flex min-w-0 flex-1 items-center", bg)}>
      <span className="w-12 shrink-0 select-none pr-2 text-right text-dim">
        {no ?? ""}
      </span>
      <span
        className={cn(
          "min-w-0 flex-1 whitespace-pre px-2",
          text == null && "bg-black/10",
        )}
      >
        {text ?? ""}
      </span>
    </div>
  );
}

function HexRowView({ row }: { row: FileDiff["hex"][number] }) {
  return (
    <div
      className={cn(
        "flex w-full items-center gap-3 px-2",
        row.differs && "bg-amber-500/10",
      )}
    >
      <span className="w-16 shrink-0 select-none text-dim">
        {row.offset.toString(16).padStart(8, "0")}
      </span>
      <span className="w-[22rem] shrink-0 whitespace-pre text-text">
        {row.left_hex.padEnd(47, " ")}
      </span>
      <span className="w-32 shrink-0 whitespace-pre text-dim">
        {row.left_ascii}
      </span>
      <span className="w-px shrink-0 self-stretch bg-edge" />
      <span className="w-[22rem] shrink-0 whitespace-pre text-text">
        {row.right_hex.padEnd(47, " ")}
      </span>
      <span className="w-32 shrink-0 whitespace-pre text-dim">
        {row.right_ascii}
      </span>
    </div>
  );
}
