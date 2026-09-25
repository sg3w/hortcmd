import { beforeEach, expect, it } from "vitest";
import { useDemo, useTauri } from "@/test/tauri";
import { hasTauri } from "./invoke";
import { isAppError } from "@/lib/appError";
import * as index from "./domains/index";
import * as net from "./domains/net";
import * as recovery from "./domains/recovery";
import * as undo from "./domains/undo";
import type { IndexPolicy } from "./bindings/IndexPolicy";
import type { IndexQuery } from "./bindings/IndexQuery";
import type { Profile } from "./bindings/Profile";

const profile: Profile = {
  id: "id", name: "test", protocol: "webdav", host: "localhost", port: 443,
  user: "", auth: "anonymous", key_path: null, start_path: "/",
  security: { plaintextAcknowledged: false, certificate: null },
};

const policy: IndexPolicy = {
  content: false, hidden: false, ignore: [], allowBattery: true, offline: false, maxContentBytes: 1024,
};
const query: IndexQuery = {
  name: "", path: "", content: "", caseSensitive: false, kind: "any", minSize: null,
  maxSize: null, modifiedAfter: null, after: null, revision: null, limit: 10, adapter: "application",
};
const calls = [
  ["index_create", () => index.indexCreate("/root", policy, null)],
  ["index_unlock", () => index.indexUnlock("id", "secret")],
  ["index_step", () => index.indexStep("id", "token")],
  ["index_cancel", () => index.indexCancel("token")],
  ["index_control", () => index.indexControl("id", "pause")],
  ["index_query", () => index.indexQuery("id", query, "token")],
  ["net_connect", () => net.netConnect("id")],
  ["net_trust_host_key", () => net.netTrustHostKey("id", "token")],
  ["net_cred_unlock", () => net.netCredUnlock("secret", true, false)],
  ["net_certificate_preview", () => net.netCertificatePreview(profile, "/certificate")],
  ["ssh_open_shell", () => net.sshOpenShell("sftp://host/", 80, 24)],
  ["recovery_prepare", () => recovery.prepareRecovery("id", true)],
  ["recovery_cleanup", () => recovery.cleanupRecovery("id")],
  ["recovery_destination", () => recovery.recoveryDestination("id")],
  ["undo_execute", () => undo.executeUndo("id")],
  ["undo_forget", () => undo.forgetUndo("id")],
] as const;

beforeEach(() => useDemo());

it.each(calls)("%s rejects with a structured error without a native runtime", async (command, call) => {
  const error = await call().then(() => { throw new Error("Expected refusal"); }, error => error);
  expect(isAppError(error)).toBe(true);
  expect(error).toMatchObject({ code: "capability.unsupported", params: { scheme: "browser" }, op: command });
  expect(error).not.toBeInstanceOf(TypeError);
  expect(JSON.stringify(error)).not.toContain("secret");
});

it("keeps read-only demo fallbacks usable", async () => {
  expect(await index.indexList()).toEqual([]);
  expect(await net.netListProfiles()).toEqual([]);
  expect(await recovery.listRecovery()).toEqual([]);
  expect(await undo.previewUndo()).toEqual([]);
});

it("forwards native calls and arguments when the runtime exists", async () => {
  const runtime = useTauri({ commands: { index_step: () => ({ id: "native-result" }) } });
  expect(await index.indexStep("scope", "cancel-token")).toEqual({ id: "native-result" });
  expect(runtime.calls).toEqual([{ cmd: "index_step", args: { id: "scope", token: "cancel-token" } }]);
});

it("switches statically imported guarded clients between native and demo modes", async () => {
  expect(hasTauri()).toBe(false);
  expect(await index.indexList()).toEqual([]);
  const runtime = useTauri({ commands: { index_list: () => [{ id: "native" }] } });
  expect(hasTauri()).toBe(true);
  expect(await index.indexList()).toEqual([{ id: "native" }]);
  expect(runtime.calls).toEqual([{ cmd: "index_list", args: {} }]);
  useDemo();
  expect(hasTauri()).toBe(false);
  expect(await index.indexList()).toEqual([]);
});
