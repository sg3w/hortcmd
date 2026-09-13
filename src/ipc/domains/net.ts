// ============================================================
// Connections, credentials and SSH terminals (TICKET-035).
//
// Profiles and their lifecycle (ticket 032), the credential store
// behind them (ticket 027), and the shell sessions that ride on an SSH
// connection (ticket 026).
// ============================================================

import type {
  ConnectOutcome,
  ConnectionStatus,
  CredConfig,
  CredStatus,
  ForgetResult,
  Profile,
} from "../bindings";
import { hasTauri, invoke } from "../invoke";
import * as demo from "../demo";

export async function netListProfiles(): Promise<Profile[]> {
  if (!hasTauri) return [];
  return invoke<Profile[]>("net_list_profiles");
}

export async function netSaveProfile(
  profile: Profile,
  secret: string | null,
): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("net_save_profile", { profile, secret });
}

export async function netDeleteProfile(id: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("net_delete_profile", { id });
}

export async function netConnect(id: string): Promise<ConnectOutcome> {
  return invoke<ConnectOutcome>("net_connect", { id });
}

/** Confirms a host key shown by `net_connect` and retries (ticket 028). */
export async function netTrustHostKey(
  id: string,
  token: string,
): Promise<ConnectOutcome> {
  return invoke<ConnectOutcome>("net_trust_host_key", { id, token });
}

/** Drops the host keys hortcmd stores for a profile's endpoint. */
export async function netForgetHostKey(id: string): Promise<ForgetResult> {
  if (!hasTauri) return { removed: 0, remaining: [] };
  return invoke<ForgetResult>("net_forget_host_key", { id });
}

/** Stops a running connection attempt (TICKET-032). Idempotent: an
 *  already finished or unknown attempt is a no-op, never an error. */
export async function netCancelConnect(attempt: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("net_cancel_connect", { attempt });
}

export async function netDisconnect(id: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("net_disconnect", { id });
}

export async function netListConnections(): Promise<ConnectionStatus[]> {
  if (!hasTauri) return [];
  return invoke<ConnectionStatus[]>("net_list_connections");
}

export async function netCredStatus(): Promise<CredStatus> {
  if (!hasTauri) return demo.DEMO_CRED;
  return invoke<CredStatus>("net_cred_status");
}

export async function netCredSetBackend(config: CredConfig): Promise<CredStatus> {
  if (!hasTauri) return demo.DEMO_CRED;
  return invoke<CredStatus>("net_cred_set_backend", { config });
}

/** Opens the `.kdbx`. `remember` keeps it open for the session,
 *  `create` starts a new database at the configured path. */
export async function netCredUnlock(
  password: string,
  remember: boolean,
  create: boolean,
): Promise<CredStatus> {
  return invoke<CredStatus>("net_cred_unlock", { password, remember, create });
}

export async function netCredLock(): Promise<CredStatus> {
  if (!hasTauri) return demo.DEMO_CRED;
  return invoke<CredStatus>("net_cred_lock");
}

/** Opens an interactive shell on the SSH connection behind `address`,
 *  starting in that address' remote directory. Returns the session id. */
export async function sshOpenShell(
  address: string,
  cols: number,
  rows: number,
): Promise<string> {
  return invoke<string>("ssh_open_shell", { address, cols, rows });
}

/** Sends keyboard input to a shell session. */
export async function sshSend(id: string, data: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("ssh_send", { id, data });
}

/** Reports the terminal size to the remote side. */
export async function sshResize(
  id: string,
  cols: number,
  rows: number,
): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("ssh_resize", { id, cols, rows });
}

/** Closes a shell session (unknown ids are ignored by the backend). */
export async function sshClose(id: string): Promise<void> {
  if (!hasTauri) return;
  return invoke<void>("ssh_close", { id });
}

/** Rebuilds the reduced native macOS menu in `lang` (TICKET-040).
 *  Only the frontend knows the persisted language setting. */
export function setMenuLanguage(lang: string): void {
  if (!hasTauri) return;
  void invoke<void>("set_menu_language", { lang });
}

export async function netTransportPolicy(): Promise<import("../bindings/TransportPolicy").TransportPolicy> {
  if (!hasTauri) return { allowPlaintext: false };
  return invoke("net_transport_policy");
}

export async function netSetTransportPolicy(policy: import("../bindings/TransportPolicy").TransportPolicy): Promise<void> {
  if (!hasTauri) return;
  return invoke("net_set_transport_policy", { policy });
}

export async function netCertificatePreview(profile: Profile, path: string): Promise<import("../bindings/CertificatePreview").CertificatePreview> {
  return invoke("net_certificate_preview", { profile, path });
}
