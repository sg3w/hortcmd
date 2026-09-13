// ============================================================
// What a connection row offers, derived from its lifecycle
// (TICKET-032).
//
// The decisions — which control is shown, what is disabled, what the
// status line says — live here rather than inside the component, so
// they can be stated once and tested directly. `ConnectionsDialog`
// renders this and adds nothing of its own.
// ============================================================

import type { ConnectionStatus, Phase } from "@/ipc/bindings";
import type { TransKey } from "@/i18n/dictionaries";
import { describeFailure } from "./connFailure";

/** The action a row's primary button performs right now. */
export type RowAction = "connect" | "cancel" | "disconnect" | "waiting";

export interface RowControls {
  phase: Phase;
  /** Which primary control the row shows. */
  action: RowAction;
  /** A spinner belongs on the row (an attempt or a teardown is running). */
  busy: boolean;
  /** Edit, delete and host-key actions would race the running work. */
  secondaryDisabled: boolean;
  /** Label under the name: the running phase, or the last failure. */
  status: { key: TransKey } | { text: string } | null;
}

const PHASE_LABEL: Record<Phase, TransKey> = {
  connected: "net.connected",
  connecting: "net.connecting",
  disconnecting: "net.disconnecting",
  failed: "net.failed",
  disconnected: "net.disconnected",
};

/** The label for a phase — also what the status dot's tooltip reads. */
export function phaseLabel(phase: Phase): TransKey {
  return PHASE_LABEL[phase];
}

/**
 * Everything the row needs, from the backend's authoritative status.
 *
 * A profile the backend has not reported on yet counts as disconnected —
 * the honest starting point — and a running attempt or a recorded
 * failure is rendered as such, which is what keeps a reopened dialog
 * from resetting itself.
 */
export function rowControls(status: ConnectionStatus | null): RowControls {
  const phase: Phase = status?.phase ?? "disconnected";
  const connecting = phase === "connecting";
  const disconnecting = phase === "disconnecting";
  const busy = connecting || disconnecting;

  const action: RowAction = connecting
    ? "cancel"
    : disconnecting
      ? "waiting"
      : phase === "connected"
        ? "disconnect"
        : "connect";

  // While something runs, the phase is the news; otherwise the last
  // failure is — and a connected or freshly disconnected profile has
  // nothing to add.
  const statusLine = busy
    ? { key: PHASE_LABEL[phase] }
    : status?.error
      ? { text: describeFailure(status.error) }
      : null;

  return {
    phase,
    action,
    busy,
    secondaryDisabled: busy,
    status: statusLine,
  };
}
