import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { useTauri } from "@/test/tauri";
import { useSettings } from "@/store/settingsStore";
import type { Profile } from "@/ipc/bindings/Profile";
import type { CertificatePreview } from "@/ipc/bindings/CertificatePreview";
import { CertificateTrustForm } from "./CertificateTrustForm";

vi.mock("@/ipc/invoke", async original => ({ ...await original<typeof import("@/ipc/invoke")>(), hasTauri: () => true }));
const profile: Profile = { id: "p", name: "Example", host: "example.test", port: 443, protocol: "webdav", user: "test", auth: "password", key_path: null, start_path: "/", security: { plaintextAcknowledged: false, certificate: null } };
const preview: CertificatePreview = { trust: { path: "/certificate.pem", fingerprint: "abcd1234", profileId: "p", endpoint: "endpoint", isCa: false }, subject: "CN=example.test", issuer: "CN=Private CA", validFrom: "2026-01-01", validUntil: "2027-01-01", hostnames: ["example.test"], algorithm: "ECDSA-SHA256" };

it.each(["de", "en"] as const)("shows certificate evidence before explicit approval in %s", async language => {
  const runtime = useTauri({ commands: { net_certificate_preview: () => preview } });
  useSettings.setState({ language });
  const onChange = vi.fn();
  render(<CertificateTrustForm profile={profile} onChange={onChange} />);
  const user = userEvent.setup();
  await user.click(screen.getByText(language === "de" ? "Zertifikat für dieses Profil" : "Certificate for this profile"));
  await user.type(screen.getByRole("textbox"), "/certificate.pem");
  await user.click(screen.getByRole("button", { name: language === "de" ? "Zertifikat prüfen" : "Inspect certificate" }));
  expect(await screen.findByText("abcd1234")).toBeInTheDocument();
  expect(screen.getByText("CN=Private CA")).toBeInTheDocument();
  expect(screen.getByText("2027-01-01")).toBeInTheDocument();
  expect(onChange).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: language === "de" ? "Für dieses Profil freigeben (mit Profil speichern)" : "Approve for this profile (saved with profile)" }));
  expect(onChange).toHaveBeenCalledWith({ ...profile.security, certificate: preview.trust });
  expect(runtime.calls.map(c => c.cmd)).toEqual(["net_certificate_preview"]);
});

it("ignores an old preview after the profile endpoint changes", async () => {
  let complete!: (preview: CertificatePreview) => void;
  useTauri({ commands: { net_certificate_preview: () => new Promise<CertificatePreview>(resolve => { complete = resolve; }) } });
  useSettings.setState({ language: "en" });
  const onChange = vi.fn();
  const view = render(<CertificateTrustForm profile={profile} onChange={onChange} />);
  const user = userEvent.setup();
  await user.click(screen.getByText("Certificate for this profile"));
  await user.type(screen.getByRole("textbox"), "/certificate.pem");
  await user.click(screen.getByRole("button", { name: "Inspect certificate" }));
  await waitFor(() => expect(complete).toBeTypeOf("function"));
  view.rerender(<CertificateTrustForm profile={{ ...profile, host: "changed.example" }} onChange={onChange} />);
  await act(async () => complete(preview));
  expect(screen.queryByText("abcd1234")).toBeNull();
  expect(onChange).not.toHaveBeenCalled();
});
