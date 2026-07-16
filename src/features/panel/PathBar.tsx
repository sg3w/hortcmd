// ============================================================
// Pfadzeile als Breadcrumb: jedes Segment ist anklickbar und
// springt in den jeweiligen Ordner (bzw. innerhalb eines Archivs).
// ============================================================

import { useEffect, useRef } from "react";
import { ChevronRight, GitBranch, Package } from "lucide-react";
import { panelOf, usePanes, type Side } from "@/store/panesStore";
import { ancestorChain, baseName } from "@/lib/path";
import { cn } from "@/lib/cn";

export function PathBar({ side }: { side: Side }) {
  const path = usePanes((s) => panelOf(s, side).path);
  const archive = usePanes((s) => panelOf(s, side).archive);
  const git = usePanes((s) => panelOf(s, side).git);
  const loadDir = usePanes((s) => s.loadDir);
  const loadArchive = usePanes((s) => s.loadArchive);
  const setActive = usePanes((s) => s.setActive);

  const scrollRef = useRef<HTMLDivElement>(null);
  const segments = ancestorChain(path || "/");

  // Bei tiefen Pfaden das Ende (aktueller Ordner) sichtbar halten.
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollLeft = el.scrollWidth;
  }, [path]);

  const go = (target: string) => {
    if (target === path) return;
    setActive(side);
    if (archive) void loadArchive(side, archive, target);
    else void loadDir(side, target);
  };

  return (
    <div className="flex flex-shrink-0 items-center gap-2 border-b border-edge bg-panel px-2 py-1 text-[12px]">
      {/* Archiv-Badge: links fixiert, damit immer klar ist, dass man in einem
          Archiv navigiert. Klick springt zur Archiv-Wurzel. */}
      {archive && (
        <button
          onClick={() => {
            setActive(side);
            void loadArchive(side, archive, "/");
          }}
          title={archive}
          className="flex shrink-0 items-center gap-1 rounded bg-amber-500/20 px-1.5 py-0.5 text-[11px] text-amber-500 hover:bg-amber-500/30"
        >
          <Package size={11} className="shrink-0" />
          <span className="max-w-[140px] truncate font-mono">
            {baseName(archive)}
          </span>
          <ChevronRight size={11} className="shrink-0 opacity-70" />
        </button>
      )}

      <div
        ref={scrollRef}
        className="flex min-w-0 flex-1 items-center overflow-x-auto font-mono [&::-webkit-scrollbar]:hidden"
      >
        {segments.map((seg, i) => {
          const isLast = i === segments.length - 1;
          return (
            <span key={seg} className="flex shrink-0 items-center">
              {i > 0 && (
                <ChevronRight size={12} className="mx-0.5 shrink-0 text-dim" />
              )}
              <button
                onClick={() => go(seg)}
                title={seg}
                className={cn(
                  "shrink-0 rounded px-1 hover:bg-accent-dim hover:text-text",
                  isLast ? "font-semibold text-accent" : "text-dim",
                )}
              >
                {baseName(seg)}
              </button>
            </span>
          );
        })}
      </div>

      {git?.is_repo && (
        <span
          title={git.branch ? `Git: ${git.branch}` : "Git-Repository"}
          className="flex shrink-0 items-center gap-1 rounded bg-accent-dim px-1.5 py-0.5 text-[11px] text-accent"
        >
          <GitBranch size={11} className="shrink-0" />
          <span className="max-w-[120px] truncate">{git.branch ?? "git"}</span>
        </span>
      )}
    </div>
  );
}
