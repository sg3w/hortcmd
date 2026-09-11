// ============================================================
// The application menu inside the window (TICKET-040).
//
// Renders the shared command definition from `commands.ts` with Radix'
// accessible menubar primitives: arrow keys move between menus and
// entries, Enter opens/executes, Escape closes and restores the focus.
// The menu is identical on macOS, Windows and Linux; only the reduced
// native macOS menu lives in the Rust backend (`src-tauri/src/menu.rs`).
//
// Item states (enabled, checkbox, radio) are read at render time — which
// for a menu content only happens while it is open — and are re-checked
// in `runCommand` immediately before execution, because the folder or a
// remote connection may have changed while the menu stood open.
//
// Narrow windows: categories that no longer fit move into a "more menus"
// category instead of being cut off. Help stays on the right and inside
// the same `Menubar.Root`, so it keeps the shared keyboard navigation.
// ============================================================

import {
  useEffect,
  useLayoutEffect,
  useReducer,
  useRef,
  useState,
  type ReactNode,
} from "react";
import * as Menubar from "@radix-ui/react-menubar";
import { ChevronRight } from "lucide-react";
import {
  COMMANDS,
  MENUS,
  commandState,
  runCommand,
  type Command,
  type CommandId,
  type MenuCategory,
  type MenuNode,
} from "./commands";
import { useNet } from "@/features/net/netStore";
import { usePanes } from "@/store/panesStore";
import { useSettings, useT } from "@/store/settingsStore";
import { useClipboard } from "@/store/clipboardStore";
import { useHistory } from "@/store/historyStore";
import { fillVars } from "@/store/noticeStore";
import { useTransfers } from "@/store/transfersStore";
import { markDialogKey } from "@/lib/keys";
import { preload } from "@/components/ui/LazyFeature";
import { PRELOAD_ON_MENU } from "@/features/lazyFeatures";
import { formatShortcut } from "@/lib/shortcut";
import { cn } from "@/lib/cn";

const contentCls =
  "z-50 max-h-[80vh] min-w-[240px] overflow-y-auto overflow-x-hidden rounded-md border border-edge bg-panel p-1 shadow-xl";

const itemCls =
  "flex cursor-default items-center gap-2 rounded px-2 py-1 text-[12px] text-text outline-none data-[highlighted]:bg-accent-dim data-[disabled]:opacity-40";

const triggerCls =
  "shrink-0 cursor-default whitespace-nowrap rounded px-2 py-0.5 outline-none data-[state=open]:bg-accent-dim data-[highlighted]:bg-accent-dim";

const CATEGORIES = MENUS.filter((c) => c.id !== "help");
const HELP = MENUS.find((c) => c.id === "help")!;

/** Horizontal gap between two triggers (`gap-0.5`). */
const GAP = 2;

/**
 * Forces a re-render whenever a store the menu reads from changes — but
 * only while a menu is open, so a closed menu costs nothing while the
 * cursor runs through a large folder.
 */
function useMenuStateTick(open: boolean): void {
  const [, bump] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    if (!open) return;
    const stores = [
      usePanes,
      useSettings,
      useNet,
      useClipboard,
      useHistory,
      useTransfers,
    ];
    const unsubs = stores.map((store) => store.subscribe(() => bump()));
    return () => unsubs.forEach((off) => off());
  }, [open]);
}

/** Label plus the "…" that marks a command opening a dialog. */
function labelOf(cmd: Command, t: (k: Command["label"]) => string): string {
  return cmd.dialog ? `${t(cmd.label)} …` : t(cmd.label);
}

function Shortcut({ id }: { id: CommandId }) {
  const cmd = COMMANDS[id] as Command;
  if (!cmd.shortcut) return null;
  return (
    <span className="ml-auto pl-6 font-mono text-[11px] text-dim">
      {formatShortcut(cmd.shortcut)}
    </span>
  );
}

/** Check/radio marker with a fixed width so all labels stay aligned. */
function Mark({ checked }: { checked?: boolean }) {
  return (
    <span
      aria-hidden
      className={cn(
        "w-3.5 shrink-0 text-center",
        checked ? "text-accent" : "text-transparent",
      )}
    >
      ✓
    </span>
  );
}

/** A single command entry (plain or checkbox). */
function CommandItem({ id }: { id: CommandId }) {
  const cmd = COMMANDS[id] as Command;
  const st = commandState(id);
  const t = useT();
  if (st.hidden) return null;

  const label = labelOf(cmd, t);
  // A disabled entry must explain itself (TICKET-031): the reason goes
  // into the accessible name, so a screen reader reads it with the
  // label, and into the tooltip, so a pointer user sees the same words.
  const why =
    st.disabled && st.reason
      ? fillVars(t(st.reason), st.reasonVars)
      : undefined;
  const aria = why ? `${label} – ${why}` : label;

  if (cmd.role === "check") {
    return (
      <Menubar.CheckboxItem
        className={itemCls}
        checked={!!st.checked}
        disabled={st.disabled}
        aria-label={aria}
        title={why}
        onSelect={() => runCommand(id)}
      >
        <Mark checked={st.checked} />
        <span>{label}</span>
        <Shortcut id={id} />
      </Menubar.CheckboxItem>
    );
  }

  return (
    <Menubar.Item
      className={itemCls}
      disabled={st.disabled}
      aria-label={aria}
      title={why}
      onSelect={() => runCommand(id)}
    >
      <Mark />
      <span>{label}</span>
      <Shortcut id={id} />
    </Menubar.Item>
  );
}

function RadioGroup({ node }: { node: Extract<MenuNode, { kind: "radio" }> }) {
  const t = useT();
  const current = node.current();
  return (
    <Menubar.RadioGroup
      value={current}
      onValueChange={(value) => {
        const id = node.ids.find(
          (i) => (COMMANDS[i] as Command).value === value,
        );
        if (id) runCommand(id);
      }}
    >
      {node.ids.map((id) => {
        const cmd = COMMANDS[id] as Command;
        const st = commandState(id);
        if (st.hidden) return null;
        return (
          <Menubar.RadioItem
            key={id}
            value={cmd.value ?? id}
            disabled={st.disabled}
            className={itemCls}
          >
            <Mark checked={cmd.value === current} />
            <span>{t(cmd.label)}</span>
            <Shortcut id={id} />
          </Menubar.RadioItem>
        );
      })}
    </Menubar.RadioGroup>
  );
}

/** Dynamically built entries (favorites, connections, transfers …). */
function DynamicList({ node }: { node: Extract<MenuNode, { kind: "list" }> }) {
  const t = useT();
  const items = node.build();
  if (items.length === 0) {
    // Deliberately not a menu item: there is nothing to execute here.
    return (
      <div className="px-2 py-1.5 pl-[1.65rem] text-[12px] text-dim">
        {t(node.empty)}
      </div>
    );
  }
  return (
    <>
      {items.map((entry) => (
        <Menubar.Item
          key={entry.id}
          className={itemCls}
          disabled={entry.disabled || !entry.run}
          onSelect={() => entry.run?.()}
        >
          <Mark />
          <span className="flex min-w-0 flex-col leading-tight">
            <span className="truncate">{entry.label}</span>
            {entry.detail && (
              <span className="truncate font-mono text-[10px] text-dim">
                {entry.detail}
              </span>
            )}
          </span>
        </Menubar.Item>
      ))}
    </>
  );
}

/** True when a node produces nothing visible, so separators can collapse. */
function isEmptyNode(node: MenuNode): boolean {
  switch (node.kind) {
    case "item":
      return !!commandState(node.id).hidden;
    case "radio":
      return node.ids.every((id) => commandState(id).hidden);
    case "sub":
      return node.nodes.every(isEmptyNode);
    default:
      // Separators and list nodes always render (the latter possibly as
      // their empty placeholder).
      return false;
  }
}

/** Renders a node list without hidden entries and dangling separators. */
function Nodes({ nodes }: { nodes: MenuNode[] }): ReactNode {
  const t = useT();
  const visible = nodes.filter((n) => n.kind === "sep" || !isEmptyNode(n));
  const cleaned = visible.filter((node, i) => {
    if (node.kind !== "sep") return true;
    const before = visible.slice(0, i).some((n) => n.kind !== "sep");
    const after = visible.slice(i + 1).some((n) => n.kind !== "sep");
    return before && after && visible[i - 1]?.kind !== "sep";
  });

  return (
    <>
      {cleaned.map((node, i) => {
        switch (node.kind) {
          case "sep":
            return (
              <Menubar.Separator key={`sep${i}`} className="my-1 h-px bg-edge" />
            );
          case "item":
            return <CommandItem key={node.id} id={node.id} />;
          case "radio":
            return <RadioGroup key={`radio${i}`} node={node} />;
          case "list":
            return <DynamicList key={`list${i}`} node={node} />;
          case "sub":
            return (
              <Menubar.Sub key={node.label}>
                <Menubar.SubTrigger className={itemCls}>
                  <Mark />
                  <span>{t(node.label)}</span>
                  <ChevronRight
                    size={13}
                    className="ml-auto shrink-0 text-dim"
                    aria-hidden
                  />
                </Menubar.SubTrigger>
                <Menubar.Portal>
                  <Menubar.SubContent
                    onKeyDownCapture={markDialogKey}
                    className={contentCls}
                  >
                    <Nodes nodes={node.nodes} />
                  </Menubar.SubContent>
                </Menubar.Portal>
              </Menubar.Sub>
            );
        }
      })}
    </>
  );
}

interface CategoryProps {
  cat: MenuCategory;
  className?: string;
  triggerRef?: (el: HTMLButtonElement | null) => void;
}

function Category({ cat, className, triggerRef }: CategoryProps) {
  const t = useT();
  // Opening (or even just pointing at) a category is strong intent:
  // warm the chunks its heavyweight actions need, so the fetch hides
  // behind the time it takes to read the menu (TICKET-034).
  const warm = () => {
    for (const load of PRELOAD_ON_MENU[cat.id] ?? []) preload(load);
  };
  return (
    <Menubar.Menu value={cat.id}>
      <Menubar.Trigger
        ref={triggerRef}
        onPointerEnter={warm}
        onFocus={warm}
        className={cn(triggerCls, className)}
      >
        {t(cat.label)}
      </Menubar.Trigger>
      <Menubar.Portal>
        <Menubar.Content
          align="start"
          sideOffset={4}
          onKeyDownCapture={markDialogKey}
          className={contentCls}
        >
          <Nodes nodes={cat.nodes} />
        </Menubar.Content>
      </Menubar.Portal>
    </Menubar.Menu>
  );
}

/**
 * Number of categories that fit into `width`. `widths` holds the measured
 * trigger widths; `reserve` is what Help (and, when needed, the overflow
 * trigger) take away.
 */
function fittingCount(
  widths: Record<string, number>,
  width: number,
  reserve: number,
  moreWidth: number,
): number {
  const total = CATEGORIES.reduce(
    (sum, c) => sum + (widths[c.id] ?? 0) + GAP,
    0,
  );
  if (total <= width - reserve) return CATEGORIES.length;

  const budget = width - reserve - moreWidth - GAP;
  let used = 0;
  let n = 0;
  for (const cat of CATEGORIES) {
    used += (widths[cat.id] ?? 0) + GAP;
    if (used > budget) break;
    n++;
  }
  // At least one real category stays reachable directly; everything else
  // is still reachable through the overflow menu.
  return Math.max(1, n);
}

export function MenuBar() {
  const [open, setOpen] = useState("");
  const t = useT();
  const language = useSettings((s) => s.language);
  const fontScale = useSettings((s) => s.fontScale);

  const rootRef = useRef<HTMLDivElement>(null);
  const helpRef = useRef<HTMLButtonElement>(null);
  const moreProbeRef = useRef<HTMLSpanElement>(null);
  const triggers = useRef(new Map<string, HTMLButtonElement>());
  const [widths, setWidths] = useState<Record<string, number> | null>(null);
  const [shown, setShown] = useState(CATEGORIES.length);

  useMenuStateTick(open !== "");

  // The connection list decides whether the remote commands are available.
  // Refresh it when a menu opens, so a connection dropped elsewhere is
  // already reflected while the menu is being read.
  useEffect(() => {
    if (open) void useNet.getState().refresh();
  }, [open]);

  // Label widths change with the language and the font size — drop the
  // measurement so every trigger is rendered once more and re-measured.
  useLayoutEffect(() => {
    setWidths(null);
    setShown(CATEGORIES.length);
  }, [language, fontScale]);

  useLayoutEffect(() => {
    if (widths) return;
    const measured: Record<string, number> = {};
    triggers.current.forEach((el, id) => {
      measured[id] = el.offsetWidth;
    });
    if (CATEGORIES.some((c) => !measured[c.id])) return; // not laid out yet
    measured.__help = helpRef.current?.offsetWidth ?? 0;
    measured.__more = moreProbeRef.current?.offsetWidth ?? 0;
    setWidths(measured);
  }, [widths]);

  // Recompute the visible categories on every window resize (the menubar
  // spans the full window width, so nothing else changes it).
  useEffect(() => {
    if (!widths) return;
    const recompute = () => {
      const width = rootRef.current?.clientWidth ?? 0;
      if (width === 0) return;
      setShown(
        fittingCount(widths, width, widths.__help ?? 0, widths.__more ?? 0),
      );
    };
    recompute();
    window.addEventListener("resize", recompute);
    return () => window.removeEventListener("resize", recompute);
  }, [widths]);

  // While measuring (widths === null) everything is rendered, so the
  // browser can lay the triggers out at their natural width.
  const visible = widths ? CATEGORIES.slice(0, shown) : CATEGORIES;
  const overflow = widths ? CATEGORIES.slice(shown) : [];

  const moreCategory: MenuCategory = {
    id: "more",
    label: "menu.more",
    nodes: overflow.map((cat) => ({
      kind: "sub" as const,
      label: cat.label,
      nodes: cat.nodes,
    })),
  };

  return (
    <header className="flex flex-shrink-0 items-center border-b border-edge bg-header px-2 py-[3px]">
      {/* One Root for all categories: Radix keeps a single roving tabindex
          per Root, so Help on the right stays in the same keyboard cycle. */}
      <Menubar.Root
        ref={rootRef}
        value={open}
        onValueChange={setOpen}
        // Keystrokes inside the menu (type-ahead, arrows, Enter) must not
        // reach the global TC key handler — it would otherwise fill the
        // quick filter or run a file action behind the open menu.
        onKeyDownCapture={markDialogKey}
        className="flex min-w-0 flex-1 items-center gap-0.5"
      >
        {visible.map((cat) => (
          <Category
            key={cat.id}
            cat={cat}
            triggerRef={(el) => {
              if (el) triggers.current.set(cat.id, el);
            }}
          />
        ))}
        {overflow.length > 0 && <Category cat={moreCategory} />}
        <Category cat={HELP} className="ml-auto" triggerRef={(el) => {
          helpRef.current = el;
        }} />
      </Menubar.Root>

      {/* Off-screen probe: gives the overflow trigger a measurable width
          before it is ever shown, so the calculation cannot oscillate. */}
      <span
        ref={moreProbeRef}
        aria-hidden
        className={cn(triggerCls, "pointer-events-none absolute -top-96 left-0 opacity-0")}
      >
        {t("menu.more")}
      </span>
    </header>
  );
}
