/** VFS v1 presentation codec. See docs/vfs-locations.md and the shared golden corpus. */
import type { VfsLocation } from "@/ipc/bindings/VfsLocation";
import type { ProviderPath } from "@/ipc/bindings/ProviderPath";
import type { Scheme } from "@/ipc/bindings/Scheme";

const defaults: Record<string, number> = { sftp: 22, ftp: 21, smb: 445, webdav: 443 };
const control = /[\u0000-\u001f\u007f-\u009f]/u;
function bad(): never { throw new Error("Invalid VFS location"); }
export function encodeComponent(value: string): string {
  return value.replace(/[%/?#@:\[\]\u0000-\u001f\u007f-\u009f]/gu, c => encodeURIComponent(c));
}
function decode(value: string): string {
  const result = decodeURIComponent(value);
  if (control.test(result)) bad();
  return result;
}
export function providerPath(raw: string, encoded = false): ProviderPath {
  if (!raw.startsWith("/") || new TextEncoder().encode(raw).length > 32768 || control.test(raw)) bad();
  const components = raw.slice(1).split("/").map(c => encoded ? decode(c) : c);
  if (components.some(c => c === "." || c === ".." || c.includes("/"))) bad();
  return { components };
}
export function parseLocation(raw: string): VfsLocation {
  if (new TextEncoder().encode(raw).length > 32768 || control.test(raw)) bad();
  const at = raw.indexOf("://");
  if (at < 0 || raw.startsWith("/") || raw.includes(":\\")) return local(raw);
  const scheme = raw.slice(0, at).toLowerCase() as Scheme;
  const rest = raw.slice(at + 3);
  if (scheme === "local" || raw.slice(0, at).toLowerCase() === "file") return local(rest);
  if (!Object.prototype.hasOwnProperty.call(defaults, scheme) || /[?#]/u.test(rest)) bad();
  const slash = rest.indexOf("/");
  const authorityText = slash < 0 ? rest : rest.slice(0, slash);
  const path = providerPath(slash < 0 ? "/" : rest.slice(slash), true);
  if (scheme === "smb" && path.components.some(c => c.includes("\\"))) bad();
  const userAt = authorityText.indexOf("@");
  let user: string | null = null;
  let endpoint = authorityText;
  if (userAt >= 0) {
    const text = authorityText.slice(0, userAt);
    endpoint = authorityText.slice(userAt + 1);
    user = decode(text);
    if (text.includes(":") || user.includes(":") || endpoint.includes("@")) bad();
  }
  let host: string;
  let portText: string | null;
  if (endpoint.startsWith("[")) {
    const end = endpoint.indexOf("]");
    if (end < 0) bad();
    const tail = endpoint.slice(end + 1);
    if (tail && !tail.startsWith(":")) bad();
    // WHATWG's IPv6 serializer supplies compressed lowercase IPv6 only.
    const parsed = new URL(`http://${endpoint.slice(0, end + 1)}/`).hostname;
    host = parsed.slice(1, -1);
    portText = tail ? tail.slice(1) : null;
  } else {
    const colon = endpoint.indexOf(":");
    host = (colon < 0 ? endpoint : endpoint.slice(0, colon)).toLowerCase();
    portText = colon < 0 ? null : endpoint.slice(colon + 1);
    if (!host || /[\s%/\\?#\[\]]/u.test(host)) bad();
    if (/[^a-zA-Z0-9._\-\u0080-\u{10ffff}]/u.test(host)) bad();
  }
  const port = portText === null ? null : Number(portText);
  if (portText !== null && (!/^[0-9]+$/u.test(portText) || !port || port > 65535)) bad();
  return { version: 1, location: { kind: "remote", scheme, authority: { user, host, port }, path } };
}
function local(raw: string): VfsLocation {
  return { version: 1, location: { kind: "local", path: { raw, flavor: raw.startsWith("\\\\") || raw[1] === ":" ? "windows" : "posix" } } };
}
export function locationUri(value: VfsLocation): string {
  if (value.version !== 1) bad();
  const loc = value.location;
  if (loc.kind === "local") return loc.path.raw;
  const a = loc.authority;
  return `${loc.scheme}://${a.user === null ? "" : `${encodeComponent(a.user)}@`}${a.host.includes(":") ? `[${a.host}]` : a.host}${a.port === null ? "" : `:${a.port}`}/${loc.path.components.map(encodeComponent).join("/")}`;
}
export function storeIdentity(value: VfsLocation): string {
  const loc = value.location;
  if (loc.kind === "local") return "local";
  const a = loc.authority;
  return `${loc.scheme}:${encodeComponent(a.user ?? "")}@${a.host.includes(":") ? `[${a.host}]` : a.host.toLowerCase()}:${a.port ?? defaults[loc.scheme]}`;
}
export function locationIdentity(value: VfsLocation): string {
  const loc = value.location;
  return `v1:${storeIdentity(value)}\0${loc.kind === "local" ? loc.path.raw : `/${loc.path.components.join("/")}`}`;
}
export function migrateLegacyLocation(raw: string): string {
  const m = /^(sftp|ftp|smb|webdav):\/\/([^/]*)(\/.*)?$/u.exec(raw);
  return m ? locationUri(parseLocation(`${m[1]}://${m[2]}/${providerPath(m[3] ?? "/").components.map(encodeComponent).join("/")}`)) : raw;
}
