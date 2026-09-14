import { describe, expect, it } from "vitest";
import corpus from "../../fixtures/vfs-locations.json";
import { locationIdentity, locationUri, migrateLegacyLocation, parseLocation, storeIdentity } from "./location";
import { parentPath, joinPath, baseName } from "./path";

describe("VFS shared conformance corpus", () => {
  for (const row of corpus.valid) it(row.raw, () => {
    const value = parseLocation(row.raw);
    expect(locationUri(value)).toBe(row.uri);
    expect(storeIdentity(value)).toBe(row.store);
    expect(locationIdentity(value)).toBe(`v1:${row.store}\0${row.path}`);
    expect(parseLocation(locationUri(value))).toEqual(value);
    if (row.parent !== null) expect(parentPath(row.uri)).toBe(row.parent);
  });
  for (const raw of corpus.invalid) it(`rejects ${raw}`, () => expect(() => parseLocation(raw)).toThrow());
  for (const row of corpus.legacy) it(`migrates ${row.raw}`, () => expect(migrateLegacyLocation(row.raw)).toBe(row.uri));
  it("joins literal reserved names without interpreting them as escapes", () => {
    for (const name of ["%20", "a?b#c", "one\\two", "é", "é", "a@b"]) {
      const joined = joinPath("sftp://h/dir", name);
      expect(baseName(joined)).toBe(name);
      expect(parentPath(joined)).toBe("sftp://h/dir");
    }
  });
});

it("keeps POSIX backslashes literal in all presentation helpers", () => {
  expect(joinPath("/tmp/a\\b", "c")).toBe("/tmp/a\\b/c");
  expect(baseName("/tmp/a\\b")).toBe("a\\b");
  expect(parentPath("/tmp/a\\b")).toBe("/tmp");
});
