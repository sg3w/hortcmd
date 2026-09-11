// ============================================================
// Connection lifecycle in the frontend (TICKET-032).
//
// The backend owns the state machine; these tests pin down what the UI
// makes of it — which control a row offers, what a running attempt
// disables, how a typed failure reads in both languages, what a
// duplicate click does, and that a dropped connection reaches the
// manager and the location pickers as one snapshot.
// ============================================================

import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  ConnectionStatus,
  ConnectOutcome,
  FailureKind,
  Phase,
} from "@/ipc/bindings";

const calls: string[] = [];
let connectOutcome: () => Promise<ConnectOutcome> = () =>
  Promise.reject(new Error("not configured"));

vi.mock("@/ipc/client", () => ({
  hasTauri: false,
  netConnect: (id: string) => {
    calls.push(`connect:${id}`);
    return connectOutcome();
  },
  netCancelConnect: (attempt: string) => {
    calls.push(`cancel:${attempt}`);
    return Promise.resolve();
  },
  netDisconnect: (id: string) => {
    calls.push(`disconnect:${id}`);
    return Promise.resolve();
  },
  netListProfiles: () => Promise.resolve([]),
  netListConnections: () => Promise.resolve(listed),
  netSaveProfile: () => Promise.resolve(),
  netDeleteProfile: () => Promise.resolve(),
  netForgetHostKey: () => Promise.resolve({ removed: 0, remaining: [] }),
  netTrustHostKey: () => connectOutcome(),
  netCredStatus: () =>
    Promise.resolve({ backend: "keychain", needs_unlock: false, unlocked: true }),
  netCredSetBackend: () => Promise.resolve({}),
  netCredUnlock: () => Promise.resolve({}),
  netCredLock: () => Promise.resolve({}),
  locationCapabilities: () =>
    Promise.resolve({ scheme: "local", store_id: "local", connected: true, caps: [] }),
}));

let listed: ConnectionStatus[] = [];

function status(
  over: Partial<ConnectionStatus> & { phase: Phase },
): ConnectionStatus {
  return {
    id: "p1",
    key: "u@host:22",
    name: "Test",
    connected: over.phase === "connected",
    address: "sftp://u@host:22/srv",
    attempt: null,
    error: null,
    ...over,
  };
}

const { describeConnError, describeFailure, failureKindOf } = await import(
  "./connFailure"
);
const { phaseLabel, rowControls } = await import("./lifecycleUi");
const { useNet } = await import("./netStore");
const { useSettings, translate } = await import("@/store/settingsStore");

beforeEach(() => {
  calls.length = 0;
  listed = [];
  useSettings.setState({ language: "de" });
  useNet.setState({ connections: [], error: null });
});

describe("what a row offers per phase", () => {
  it("offers connect when nothing is running", () => {
    const r = rowControls(status({ phase: "disconnected" }));
    expect(r.action).toBe("connect");
    expect(r.busy).toBe(false);
    expect(r.secondaryDisabled).toBe(false);
    expect(r.status).toBeNull();
  });

  it("replaces connect with a cancel action while an attempt runs", () => {
    const r = rowControls(status({ phase: "connecting", attempt: "conn-1" }));
    expect(r.action).toBe("cancel");
    expect(r.busy).toBe(true);
    // Edit, delete and the host-key action would race the attempt.
    expect(r.secondaryDisabled).toBe(true);
    expect(r.status).toEqual({ key: "net.connecting" });
  });

  it("shows a spinner and no second disconnect while tearing down", () => {
    const r = rowControls(status({ phase: "disconnecting" }));
    expect(r.action).toBe("waiting");
    expect(r.busy).toBe(true);
    expect(r.secondaryDisabled).toBe(true);
  });

  it("offers disconnect once connected", () => {
    const r = rowControls(status({ phase: "connected" }));
    expect(r.action).toBe("disconnect");
    expect(r.busy).toBe(false);
    expect(r.status).toBeNull();
  });

  it("keeps the failure visible and the row usable again", () => {
    const r = rowControls(
      status({
        phase: "failed",
        error: { kind: "timeout", message: "keine Antwort" },
      }),
    );
    expect(r.action).toBe("connect");
    expect(r.busy).toBe(false);
    expect(r.secondaryDisabled).toBe(false);
    expect(r.status).toEqual({
      text: expect.stringContaining(translate("net.fail.timeout")),
    });
  });

  it("treats a profile the backend has not reported on as disconnected", () => {
    expect(rowControls(null).action).toBe("connect");
    expect(rowControls(null).busy).toBe(false);
  });

  it("labels every phase", () => {
    const phases: Phase[] = [
      "disconnected",
      "connecting",
      "connected",
      "disconnecting",
      "failed",
    ];
    for (const phase of phases) {
      expect(translate(phaseLabel(phase)), phase).toBeTruthy();
    }
  });
});

describe("typed failures", () => {
  const kinds: FailureKind[] = [
    "timeout",
    "cancelled",
    "auth",
    "hostKey",
    "unreachable",
    "protocol",
    "keychain",
  ];

  it("reads every kind in German and English", () => {
    for (const lang of ["de", "en"] as const) {
      useSettings.setState({ language: lang });
      const texts = kinds.map((kind) => describeFailure({ kind, message: "" }));
      for (const [i, text] of texts.entries()) {
        expect(text, `${lang}/${kinds[i]}`).toBeTruthy();
        // A missing translation would surface as the bare key.
        expect(text, `${lang}/${kinds[i]}`).not.toContain("net.fail.");
      }
      // Each kind says something of its own — nothing collapses into a
      // single generic "connection failed".
      expect(new Set(texts).size, lang).toBe(kinds.length);
    }
    // …and the two languages really differ.
    useSettings.setState({ language: "de" });
    const de = describeFailure({ kind: "timeout", message: "" });
    useSettings.setState({ language: "en" });
    expect(describeFailure({ kind: "timeout", message: "" })).not.toBe(de);
  });

  it("keeps the server's own words as a detail", () => {
    const text = describeFailure({ kind: "auth", message: "530 Login incorrect" });
    expect(text).toContain(translate("net.fail.auth"));
    expect(text).toContain("530 Login incorrect");
  });

  it("recognizes the marker on a thrown error", () => {
    expect(failureKindOf("CONN_TIMEOUT: zu langsam")).toBe("timeout");
    expect(failureKindOf("CONN_UNREACHABLE: weg")).toBe("unreachable");
    expect(describeConnError("CONN_AUTH: 530")).toContain(
      translate("net.fail.auth"),
    );
  });

  it("passes an untagged error through unchanged", () => {
    expect(failureKindOf("Profil p1 nicht gefunden")).toBeNull();
    expect(describeConnError("Profil p1 nicht gefunden")).toBe(
      "Profil p1 nicht gefunden",
    );
  });
});

describe("the store's connect path", () => {
  it("navigates on success", async () => {
    const connected = status({ phase: "connected" });
    connectOutcome = () =>
      Promise.resolve({ kind: "connected", data: connected });
    const address = await useNet.getState().connect("p1");
    expect(address).toBe("sftp://u@host:22/srv");
  });

  it("adopts the running attempt on a duplicate request", async () => {
    const pending = status({ phase: "connecting", attempt: "conn-7" });
    connectOutcome = () => Promise.resolve({ kind: "pending", data: pending });
    const address = await useNet.getState().connect("p1");
    // No navigation, no error — the attempt already running owns it.
    expect(address).toBeNull();
    expect(useNet.getState().error).toBeNull();
    expect(useNet.getState().connections[0].attempt).toBe("conn-7");
  });

  it("reports a typed failure in the user's language", async () => {
    connectOutcome = () =>
      Promise.reject(new Error("CONN_UNREACHABLE: host weg"));
    expect(await useNet.getState().connect("p1")).toBeNull();
    expect(useNet.getState().error).toContain(translate("net.fail.unreachable"));
  });

  it("cancels the attempt the backend reported, and only that one", async () => {
    useNet.setState({
      connections: [
        status({ phase: "connecting", attempt: "conn-9" }),
        status({ id: "p2", phase: "connecting", attempt: "conn-10" }),
      ],
    });
    await useNet.getState().cancelConnect("p1");
    expect(calls).toEqual(["cancel:conn-9"]);
  });

  it("does nothing when there is no attempt to cancel", async () => {
    useNet.setState({ connections: [status({ phase: "connected" })] });
    await useNet.getState().cancelConnect("p1");
    expect(calls).toHaveLength(0);
  });

  it("adopts a pushed snapshot, so a lost connection reaches every view", () => {
    useNet.setState({ connections: [status({ phase: "connected" })] });
    useNet.getState().applyStates([
      status({
        phase: "failed",
        error: { kind: "protocol", message: "Verbindung zum Server verloren" },
      }),
    ]);
    const [now] = useNet.getState().connections;
    expect(now.connected).toBe(false);
    expect(now.phase).toBe("failed");
    // `connectionFor` — and with it the location pickers and the Netz
    // menu — filters on `connected`, so they follow the same snapshot.
    expect(rowControls(now).action).toBe("connect");
  });
});
