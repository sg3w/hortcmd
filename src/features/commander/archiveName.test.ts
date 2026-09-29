import { expect, it } from "vitest";
import { dir, file } from "@/test/fixtures";
import {
  composeArchiveName,
  genericBase,
  proposedArchiveName,
  selectionBase,
  stemRange,
  timestamp,
} from "./archiveName";

const NOW = new Date(2026, 8, 29, 15, 4, 7);
const STAMP = "2026-09-29_15-04-07";

it("formats the local date and time zero-padded and sortable", () => {
  expect(timestamp(NOW)).toBe(STAMP);
});

it("names the archive after a single folder", () => {
  expect(proposedArchiveName([dir("Fotos 2024")], NOW)).toBe(`Fotos 2024_${STAMP}.zip`);
});

it("prefers the first folder in list order over the files around it", () => {
  const entries = [file("notes.txt"), dir("Fotos"), dir("Videos"), file("a.txt")];
  expect(proposedArchiveName(entries, NOW)).toBe(`Fotos_${STAMP}.zip`);
});

it("names the archive after the first file, without its extension", () => {
  expect(proposedArchiveName([file("Bericht.tar.gz"), file("b.txt")], NOW)).toBe(
    `Bericht.tar_${STAMP}.zip`,
  );
});

it("keeps a dotfile's name whole", () => {
  expect(proposedArchiveName([file(".zshrc")], NOW)).toBe(`.zshrc_${STAMP}.zip`);
});

it("replaces what a file name must not carry and drops trailing dots and spaces", () => {
  expect(proposedArchiveName([dir(" a/b:c. ")], NOW)).toBe(`a_b_c_${STAMP}.zip`);
});

it("cuts an over-long base so the whole name stays within the name limit", () => {
  const name = proposedArchiveName([dir("x".repeat(400))], NOW);
  expect(name).toBe(`${"x".repeat(100)}_${STAMP}.zip`);
  expect(name.length).toBeLessThan(255);
});

it("falls back when nothing is packable and when a name sanitizes to nothing", () => {
  expect(proposedArchiveName([], NOW)).toBe(`archiv_${STAMP}.zip`);
  expect(proposedArchiveName([dir("...")], NOW)).toBe(`archiv_${STAMP}.zip`);
});

it("preselects everything but the extension", () => {
  const name = `Fotos_${STAMP}.zip`;
  expect(stemRange(name)).toEqual([0, name.length - 4]);
  expect(name.slice(...stemRange(name))).toBe(`Fotos_${STAMP}`);
  expect(stemRange("noext")).toEqual([0, 5]);
});

// ---------- Composed name (TICKET-392) ----------

const ENTRIES = [file("notes.txt"), dir("Fotos")];

it.each([
  [{ base: "selection", date: true, time: true }, `Fotos_2026-09-29_15-04-07.zip`],
  [{ base: "selection", date: true, time: false }, "Fotos_2026-09-29.zip"],
  [{ base: "selection", date: false, time: true }, "Fotos_15-04-07.zip"],
  [{ base: "selection", date: false, time: false }, "Fotos.zip"],
  [{ base: "generic", date: true, time: true }, `archiv_2026-09-29_15-04-07.zip`],
  [{ base: "generic", date: false, time: false }, "archiv.zip"],
] as const)("composes %o into %s", (options, expected) => {
  expect(composeArchiveName(ENTRIES, options, NOW)).toBe(expected);
});

it("offers both bases the dialog lets the user choose between", () => {
  expect(selectionBase(ENTRIES)).toBe("Fotos");
  expect(selectionBase([])).toBe(genericBase());
  expect(genericBase()).toBe("archiv");
});

it("still proposes the composed default", () => {
  expect(proposedArchiveName(ENTRIES, NOW)).toBe(
    composeArchiveName(ENTRIES, { base: "selection", date: true, time: true }, NOW),
  );
});
