// ============================================================
// Batch-rename rules and conflict detection (TICKET-033).
//
// The preview is the only thing standing between a rule set and a
// batch of irreversible renames on disk, so the conflict statuses —
// duplicate, exists, invalid — matter as much as the names themselves.
// ============================================================

import { describe, expect, it } from "vitest";
import type { Row } from "@/store/panesStore";
import { row as makeRow } from "@/test/fixtures";
import {
  buildPreview,
  buildRenamer,
  DEFAULT_RULES,
  type RenameRules,
} from "./renameRules";

function row(name: string, isDir = false): Row {
  return makeRow(name, { is_dir: isDir, size: 0 });
}

function renamer(over: Partial<RenameRules> = {}) {
  const { rename, error } = buildRenamer({ ...DEFAULT_RULES, ...over });
  if (!rename) throw new Error(`unexpected rule error: ${error}`);
  return rename;
}

describe("placeholders", () => {
  it("keeps the name unchanged with the default masks", () => {
    expect(renamer()(row("a.txt"), 0)).toBe("a.txt");
    expect(renamer()(row("no-extension"), 0)).toBe("no-extension");
  });

  it("expands [N], [E] and [C]", () => {
    const rename = renamer({ nameMask: "shot-[C]-[N]", extMask: "[E]" });
    expect(rename(row("bild.png"), 0)).toBe("shot-1-bild.png");
    expect(rename(row("bild.png"), 4)).toBe("shot-5-bild.png");
  });

  it("can move the extension into the name and vice versa", () => {
    expect(renamer({ nameMask: "[E]", extMask: "[N]" })(row("a.txt"), 0)).toBe(
      "txt.a",
    );
  });

  it("drops the extension when its mask is empty", () => {
    expect(renamer({ extMask: "" })(row("a.txt"), 0)).toBe("a");
  });

  it("leaves an unknown placeholder alone", () => {
    expect(renamer({ nameMask: "[X]-[N]" })(row("a.txt"), 0)).toBe("[X]-a.txt");
  });

  it("treats a folder as all name and no extension", () => {
    // Otherwise "my.folder" would be split at the dot and renamed wrong.
    expect(renamer({ nameMask: "[N]", extMask: "[E]" })(row("my.folder", true), 0))
      .toBe("my.folder");
  });
});

describe("the counter", () => {
  it("starts, steps and pads", () => {
    const rename = renamer({
      nameMask: "[C]",
      extMask: "",
      counterStart: 10,
      counterStep: 5,
      counterDigits: 4,
    });
    expect(rename(row("a"), 0)).toBe("0010");
    expect(rename(row("a"), 2)).toBe("0020");
  });

  it("puts the sign in front of the padding", () => {
    const rename = renamer({
      nameMask: "[C]",
      extMask: "",
      counterStart: -3,
      counterDigits: 3,
    });
    expect(rename(row("a"), 0)).toBe("-003");
  });
});

describe("search and replace", () => {
  it("replaces every occurrence, case-sensitively by default", () => {
    expect(renamer({ search: "a", replace: "X" })(row("banana.txt"), 0)).toBe(
      "bXnXnX.txt",
    );
    expect(renamer({ search: "A", replace: "X" })(row("banana.txt"), 0)).toBe(
      "banana.txt",
    );
  });

  it("can ignore case", () => {
    expect(
      renamer({ search: "A", replace: "X", caseSensitive: false })(
        row("Banana.txt"),
        0,
      ),
    ).toBe("BXnXnX.txt");
  });

  it("supports regular expressions with capture groups", () => {
    const rename = renamer({
      search: "(\\d+)-(\\d+)",
      replace: "$2-$1",
      regex: true,
    });
    expect(rename(row("img-12-34.png"), 0)).toBe("img-34-12.png");
  });

  it("reports an invalid expression instead of renaming", () => {
    const { rename, error } = buildRenamer({
      ...DEFAULT_RULES,
      search: "([",
      regex: true,
    });
    expect(rename).toBeUndefined();
    expect(error).toBeTruthy();
  });

  it("treats a plain search as literal text, not a pattern", () => {
    expect(renamer({ search: ".", replace: "_" })(row("a.b.txt"), 0)).toBe(
      "a_b_txt",
    );
  });
});

describe("case conversion", () => {
  it("applies to the name part only", () => {
    expect(renamer({ caseMode: "upper" })(row("aBc.TxT"), 0)).toBe("ABC.TxT");
    expect(renamer({ caseMode: "lower" })(row("aBc.TxT"), 0)).toBe("abc.TxT");
  });

  it("title-cases each word", () => {
    expect(renamer({ caseMode: "title" })(row("my long name.txt"), 0)).toBe(
      "My Long Name.txt",
    );
  });
});

describe("the preview's conflict detection", () => {
  const rename = renamer({ nameMask: "x", extMask: "[E]" });

  it("marks an unchanged name rather than renaming it", () => {
    const items = buildPreview([row("x.txt")], new Set(["x.txt"]), rename);
    expect(items[0].status).toBe("unchanged");
  });

  it("marks every member of a collision as duplicate", () => {
    const items = buildPreview(
      [row("a.txt"), row("b.txt")],
      new Set(["a.txt", "b.txt"]),
      rename,
    );
    expect(items.map((i) => i.status)).toEqual(["duplicate", "duplicate"]);
  });

  it("marks a target that another file in the folder already holds", () => {
    const items = buildPreview(
      [row("a.txt")],
      new Set(["a.txt", "x.txt"]),
      rename,
    );
    expect(items[0].status).toBe("exists");
  });

  it("allows a rotation among the renamed entries themselves", () => {
    // a→b and b→a: both targets are taken, but only by sources that are
    // being renamed too, so the batch is valid.
    const swap = (entry: Row) => (entry.name === "a" ? "b" : "a");
    const items = buildPreview([row("a"), row("b")], new Set(["a", "b"]), swap);
    expect(items.map((i) => i.status)).toEqual(["ok", "ok"]);
  });

  it("rejects an empty name and one carrying a path separator", () => {
    const items = buildPreview(
      [row("a"), row("b"), row("c")],
      new Set(),
      (entry) => (entry.name === "a" ? "" : entry.name === "b" ? "x/y" : "x\\y"),
    );
    expect(items.map((i) => i.status)).toEqual([
      "invalid",
      "invalid",
      "invalid",
    ]);
  });

  it("keeps the folder flag so the caller can tell them apart", () => {
    const items = buildPreview([row("d", true)], new Set(["d"]), () => "e");
    expect(items[0]).toMatchObject({ from: "d", to: "e", isDir: true, status: "ok" });
  });
});
