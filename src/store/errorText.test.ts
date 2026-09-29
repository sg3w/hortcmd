// ============================================================
// Reading a backend error (TICKET-036).
//
// The contract has three properties worth holding: control flow uses
// codes (never message text), every code the backend can send has a
// translation in both languages, and a code this build does not know
// still produces something a user can report.
// ============================================================

import { beforeEach, describe, expect, it } from "vitest";
import type { AppError, ErrorAction, ItemError } from "@/ipc/bindings";
import { ERROR_CODES } from "@/lib/errorCodes.generated";
import { asAppError, isAppError, isCancelled, isError, isOneOf } from "@/lib/appError";
import {
  describeError,
  describeErrorFull,
  describeItemErrors,
  errorActionLabel,
  errorReference,
} from "./errorText";
import { DICT, type TransKey } from "@/i18n/dictionaries";
import { translate, useSettings } from "@/store/settingsStore";

function err(code: string, over: Partial<AppError> = {}): AppError {
  return {
    version: 1,
    code,
    params: {},
    op: null,
    provider: null,
    retryable: false,
    action: null,
    detail: null,
    id: "e-test",
    ...over,
  };
}

beforeEach(() => useSettings.setState({ language: "de" }));

describe("recognizing a backend error", () => {
  it("accepts the real shape and rejects everything else", () => {
    expect(isAppError(err("fs.io"))).toBe(true);
    for (const other of [
      null,
      undefined,
      "a string",
      42,
      new Error("boom"),
      { code: "fs.io" }, // no id, no version
    ]) {
      expect(isAppError(other), String(other)).toBe(false);
    }
  });

  it("unwraps one, or answers null", () => {
    expect(asAppError(err("fs.io"))?.code).toBe("fs.io");
    expect(asAppError(new Error("boom"))).toBeNull();
  });
});

describe("control flow uses codes", () => {
  it("matches a code exactly, never a substring", () => {
    expect(isError(err("net.auth"), "net.auth")).toBe(true);
    // A prefix must not match — that is the whole point.
    expect(isError(err("net.authSomethingElse"), "net.auth")).toBe(false);
    expect(isError(err("net.auth"), "net")).toBe(false);
  });

  it("matches any of several codes", () => {
    const e = err("archive.passwordWrong");
    expect(isOneOf(e, "archive.passwordRequired", "archive.passwordWrong")).toBe(
      true,
    );
    expect(isOneOf(e, "net.auth")).toBe(false);
  });

  it("keeps cancellation distinct from failure", () => {
    expect(isCancelled(err("cancelled"))).toBe(true);
    expect(isCancelled(err("fs.io"))).toBe(false);
  });

  it("answers false for something that is not a backend error", () => {
    expect(isError("cancelled", "cancelled")).toBe(false);
    expect(isCancelled(new Error("cancelled"))).toBe(false);
  });
});

describe("localization", () => {
  it("translates every code the backend can send, in both languages", () => {
    // Walks the generated list, so a new backend code without a
    // translation fails here rather than reaching a user as a generic
    // message.
    for (const code of ERROR_CODES) {
      const key = `error.${code}` as TransKey;
      expect(DICT.de[key], `de/${code}`).toBeTruthy();
      expect(DICT.en[key], `en/${code}`).toBeTruthy();
    }
  });

  it("gives each code its own wording", () => {
    const texts = ERROR_CODES.map((c) => translate(`error.${c}` as TransKey));
    // Nothing collapses into one generic sentence.
    expect(new Set(texts).size).toBe(ERROR_CODES.length);
  });

  it("uses the same placeholders in both languages", () => {
    const marks = (s: string) => (s.match(/\{[a-zA-Z]+\}/g) ?? []).sort();
    for (const code of ERROR_CODES) {
      const key = `error.${code}` as TransKey;
      expect(marks(DICT.en[key]), code).toEqual(marks(DICT.de[key]));
    }
  });

  it("follows the language setting without anything being re-fetched", () => {
    const e = err("net.timeout");
    useSettings.setState({ language: "de" });
    const de = describeError(e);
    useSettings.setState({ language: "en" });
    expect(describeError(e)).not.toBe(de);
    expect(describeError(e)).toBe(translate("error.net.timeout"));
  });

  it("fills the parameters the backend attached", () => {
    const text = describeError(
      err("net.unreachable", { params: { host: "nas.local" } }),
    );
    expect(text).toContain("nas.local");
    expect(text).not.toContain("{host}");
  });

  it("leaves a placeholder alone when its parameter is missing", () => {
    // Better a visible gap than a sentence that silently lost a value.
    expect(describeError(err("net.unreachable"))).toContain("{host}");
  });

  it("names the archive limit that was exceeded", () => {
    // TICKET-395: "a limit was exceeded" is only reportable; the bound and
    // its value are what tells the user what to raise or split.
    const many = describeError(err("archive.tooManyEntries", { params: { limit: "1000000" } }));
    expect(many).toContain("1000000");
    const large = describeError(err("archive.tooLarge", { params: { limit: "4.0 TB" } }));
    expect(large).toContain("4.0 TB");
  });

  it("passes a non-backend error through as its own text", () => {
    expect(describeError("etwas ging schief")).toBe("etwas ging schief");
  });
});

describe("an unknown code from a newer backend", () => {
  const newer = err("net.somethingAddedLater", { id: "e-9f2" });

  it("renders a safe generic message naming the code", () => {
    const text = describeError(newer);
    expect(text).toContain("net.somethingAddedLater");
    expect(text).toBe(
      translate("error.unknown").replace("{code}", "net.somethingAddedLater"),
    );
  });

  it("exposes the correlation id so it can be reported", () => {
    expect(errorReference(newer)).toContain("e-9f2");
    expect(describeErrorFull(newer)).toContain("e-9f2");
  });

  it("does not append a reference to a code it does know", () => {
    expect(describeErrorFull(err("fs.notFound"))).toBe(
      translate("error.fs.notFound"),
    );
  });

  it("tolerates unknown extra fields", () => {
    const withExtras = {
      ...newer,
      somethingNew: { nested: true },
      version: 99,
    } as unknown;
    expect(isAppError(withExtras)).toBe(true);
    expect(describeError(withExtras)).toContain("net.somethingAddedLater");
  });
});

describe("technical detail", () => {
  it("is never part of the displayed message", () => {
    // It may carry a server's own words, and its own language.
    const e = err("net.auth", { detail: "530 Login incorrect" });
    expect(describeError(e)).not.toContain("530");
    expect(describeErrorFull(e)).not.toContain("530");
  });
});

describe("what the user can do", () => {
  it("labels every action", () => {
    const actions: ErrorAction[] = [
      "retry",
      "reconnect",
      "authenticate",
      "confirmHostKey",
      "providePassword",
      "freeSpace",
      "checkPermissions",
      "unsupported",
      "report",
    ];
    for (const action of actions) {
      expect(errorActionLabel(err("fs.io", { action })), action).toBeTruthy();
    }
  });

  it("offers nothing when the error suggests nothing", () => {
    expect(errorActionLabel(err("cancelled"))).toBeNull();
  });
});

describe("a batch with mixed results", () => {
  const item = (name: string, code: string): ItemError => ({
    item: name,
    error: err(code),
  });

  it("names every failing item and keeps them all", () => {
    const lines = describeItemErrors([
      item("/a.txt", "fs.permission"),
      item("/b.txt", "fs.notFound"),
    ]);
    expect(lines).toHaveLength(2);
    expect(lines[0]).toContain("/a.txt");
    expect(lines[0]).toContain(translate("error.fs.permission"));
    expect(lines[1]).toContain("/b.txt");
    expect(lines[1]).toContain(translate("error.fs.notFound"));
  });

  it("drops cancelled items — a cancel is not a failure", () => {
    const lines = describeItemErrors([
      item("/a.txt", "cancelled"),
      item("/b.txt", "fs.io"),
    ]);
    expect(lines).toHaveLength(1);
    expect(lines[0]).toContain("/b.txt");
  });

  it("reports an operation-level failure without an item name", () => {
    const lines = describeItemErrors([item("", "transfer.noStaging")]);
    expect(lines).toEqual([translate("error.transfer.noStaging")]);
  });

  it("says nothing at all when everything succeeded", () => {
    expect(describeItemErrors([])).toEqual([]);
  });
});
