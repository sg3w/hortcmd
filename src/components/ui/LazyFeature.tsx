// ============================================================
// Lazy feature boundaries (TICKET-034).
//
// Markdown rendering, syntax highlighting, terminal emulation and the
// larger dialogs are reachable from the initial dependency graph, which
// means the WebView parses all of them before it can draw two file
// panes. None of them is needed to do that.
//
// This wrapper moves each behind a dynamic import with three rules the
// application depends on:
//
//   * **Nothing mounts until the feature is open.** The wrapper renders
//     `null` while closed, so no invisible modal exists, nothing can
//     steal focus, and a function key cannot be swallowed by a dialog
//     the user did not open.
//   * **Loading is visible but inert.** The fallback is a small,
//     non-modal status line with `role="status"`. It does not trap
//     focus and does not block the panes, so the shell stays usable
//     while a chunk arrives.
//   * **A failed import is recoverable.** A chunk that cannot be
//     fetched (a partial update, a lost dev server) shows a localized
//     message with a retry instead of taking the application down —
//     an unhandled lazy rejection would otherwise unmount the whole
//     React tree.
//
// Chunks are ordinary application assets served by the same origin as
// the document, so the production CSP from TICKET-030 (`script-src
// 'self'`) covers them unchanged.
// ============================================================

import {
  Component,
  Suspense,
  lazy,
  useEffect,
  useRef,
  useState,
  type ComponentType,
  type ReactNode,
} from "react";
import { Loader2, RotateCcw, TriangleAlert } from "lucide-react";
import { useT } from "@/store/settingsStore";

/** What a lazy boundary imports: a module with a default component. */
export type Loader = () => Promise<{ default: ComponentType }>;

/**
 * Starts a feature's chunk without rendering it — used on strong user
 * intent, such as opening the menu that offers the feature. Repeated
 * calls are free: the import cache answers the second one.
 *
 * Deliberately not called at startup for anything; an unconditional
 * preload would put the cost straight back into the launch path.
 */
export function preload(load: Loader): void {
  void load().catch(() => {
    // A failed preload is not an error yet — the real attempt happens
    // when the user opens the feature, and reports properly then.
  });
}

/** Remembers one `lazy()` per loader, so a reopen does not refetch. */
const cache = new WeakMap<Loader, ComponentType>();

function componentFor(load: Loader): ComponentType {
  let component = cache.get(load);
  if (!component) {
    component = lazy(load);
    cache.set(load, component);
  }
  return component;
}

interface Props {
  /** Whether the feature is open. Nothing is imported while false. */
  open: boolean;
  load: Loader;
  /** Name in the loading and error messages ("Einstellungen", …). */
  label?: string;
  /** Rendered instead of the default status line while loading. */
  fallback?: ReactNode;
}

/**
 * Mounts `load`'s component while `open`, and nothing at all otherwise.
 *
 * The component keeps managing its own visibility — every dialog here
 * reads its store and renders a Radix `Dialog.Root`. This only decides
 * whether the module is part of the tree.
 */
export function LazyFeature({ open, load, label, fallback }: Props) {
  // Keep it mounted for one beat after closing so a dialog's close
  // animation and Radix' focus restore can finish; unmounting a Radix
  // dialog while it is still open leaks `pointer-events: none` onto
  // `<body>`.
  const [mounted, setMounted] = useState(open);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    window.clearTimeout(timer.current);
    if (open) {
      setMounted(true);
      return;
    }
    timer.current = window.setTimeout(() => setMounted(false), 400);
    return () => window.clearTimeout(timer.current);
  }, [open]);

  if (!mounted) return null;
  return <LazyBoundary label={label} load={load} fallback={fallback} />;
}

/** Non-modal, announced loading indicator; never takes focus. */
function LoadingNotice({ label }: { label?: string }) {
  const t = useT();
  return (
    <div
      role="status"
      aria-live="polite"
      className="pointer-events-none fixed bottom-3 left-1/2 z-50 flex -translate-x-1/2 items-center gap-2 rounded border border-edge bg-panel px-3 py-1.5 text-[12px] text-dim shadow-lg"
    >
      <Loader2 size={13} className="animate-spin" aria-hidden />
      {label ? `${label} — ${t("lazy.loading")}` : t("lazy.loading")}
    </div>
  );
}

interface BoundaryProps {
  label?: string;
  load: Loader;
  fallback?: ReactNode;
}

/**
 * Catches a rejected dynamic import. React has no hook form of this —
 * an error boundary must be a class — and without one a chunk that
 * fails to arrive unmounts the entire application.
 *
 * The boundary also *creates* the lazy component, rather than receiving
 * it as a child: a retry has to hand React a fresh `lazy()`, and only
 * whoever builds the element can do that. A `lazy()` that already
 * rejected replays that rejection forever.
 */
class LazyBoundary extends Component<
  BoundaryProps,
  { failed: boolean; attempt: number }
> {
  state = { failed: false, attempt: 0 };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  retry = () => {
    cache.delete(this.props.load);
    this.setState((s) => ({ failed: false, attempt: s.attempt + 1 }));
  };

  render() {
    const { label, load, fallback } = this.props;
    if (this.state.failed) return <LoadError label={label} onRetry={this.retry} />;
    const Feature = componentFor(load);
    return (
      <Suspense
        key={this.state.attempt}
        fallback={fallback ?? <LoadingNotice label={label} />}
      >
        <Feature />
      </Suspense>
    );
  }
}

function LoadError({
  label,
  onRetry,
}: {
  label?: string;
  onRetry: () => void;
}) {
  const t = useT();
  return (
    <div
      role="alert"
      className="fixed bottom-3 left-1/2 z-50 flex -translate-x-1/2 items-center gap-2 rounded border border-red-500/40 bg-panel px-3 py-1.5 text-[12px] text-red-400 shadow-lg"
    >
      <TriangleAlert size={13} aria-hidden />
      <span>{label ? `${label} — ${t("lazy.failed")}` : t("lazy.failed")}</span>
      <button
        type="button"
        onClick={onRetry}
        className="flex items-center gap-1 rounded border border-edge px-1.5 py-0.5 text-dim hover:border-accent hover:text-text"
      >
        <RotateCcw size={12} aria-hidden />
        {t("lazy.retry")}
      </button>
    </div>
  );
}
