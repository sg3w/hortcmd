// ============================================================
// The dictionary, after the namespace split (TICKET-035).
//
// Two guarantees the single file used to give implicitly, now checked:
// both languages carry the same keys (also enforced at compile time by
// `ns`), and no two namespaces claim the same key — a duplicate would
// be silently won by whichever is spread last, which is exactly the
// kind of bug a merge cannot show you.
// ============================================================

import { describe, expect, it } from "vitest";
import { CORE_NAMESPACES, DICT } from "./dictionaries";
import { rename } from "./namespaces/rename";
import { exporting } from "./namespaces/exporting";
import { preview } from "./namespaces/preview";
import { pack } from "./namespaces/pack";
const ALL_NAMESPACES = [...CORE_NAMESPACES, rename, exporting, preview, pack];

describe("language parity", () => {
  it("has the same keys in German and English", () => {
    const de = Object.keys(DICT.de).sort();
    const en = Object.keys(DICT.en).sort();
    expect(en).toEqual(de);
  });

  it("has a non-empty translation for every key in both languages", () => {
    for (const [key, value] of Object.entries(DICT.de)) {
      expect(value.length, `de/${key}`).toBeGreaterThan(0);
    }
    for (const [key, value] of Object.entries(DICT.en)) {
      expect(value.length, `en/${key}`).toBeGreaterThan(0);
    }
  });

  it("keeps the placeholders of a key identical across languages", () => {
    // "{n} Dateien" and "Files" would lose the count silently.
    const placeholders = (s: string) =>
      (s.match(/\{[a-zA-Z]+\}/g) ?? []).sort();
    for (const key of Object.keys(DICT.de) as (keyof typeof DICT.de)[]) {
      expect(placeholders(DICT.en[key]), key).toEqual(
        placeholders(DICT.de[key]),
      );
    }
  });
});

describe("namespaces", () => {
  it("claims each key exactly once", () => {
    const seen = new Map<string, number>();
    for (const [i, namespace] of ALL_NAMESPACES.entries()) {
      for (const key of Object.keys(namespace.de)) {
        const first = seen.get(key);
        expect(
          first,
          `"${key}" is declared in namespace ${first} and again in ${i}`,
        ).toBeUndefined();
        seen.set(key, i);
      }
    }
    expect(seen.size).toBe(Object.keys(DICT.de).length);
  });

  it("loses nothing in the merge", () => {
    const fromNamespaces = ALL_NAMESPACES.reduce(
      (n, ns) => n + Object.keys(ns.de).length,
      0,
    );
    expect(Object.keys(DICT.de)).toHaveLength(fromNamespaces);
  });
});
