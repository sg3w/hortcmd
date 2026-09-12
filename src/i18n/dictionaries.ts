// ============================================================
// The translation dictionary (TICKET-035).
//
// Composed from feature namespaces rather than written as one literal:
// a 1,500-line file where every feature's wording sat next to every
// other's meant that changing one string put the whole dictionary in
// the diff, and that German and English drifted apart silently.
//
// The namespaces are documented in `namespaces/`, one per feature area.
// `ns()` locks each one's two languages to the same key set at compile
// time; `TransKey` is still the union of every key, so a typo in a call
// to `t()` is still a type error.
//
// File and folder *names* stay untranslated — those are data.
// ============================================================

import { chrome } from "./namespaces/chrome";
import { compare } from "./namespaces/compare";
import { errors } from "./namespaces/errors";
import { exporting } from "./namespaces/exporting";
import { net } from "./namespaces/net";
import { operations } from "./namespaces/operations";
import { preview } from "./namespaces/preview";
import { properties } from "./namespaces/properties";
import { rename } from "./namespaces/rename";
import { search } from "./namespaces/search";
import { settings } from "./namespaces/settings";

export type Lang = "de" | "en";

/** Every namespace, in the order they are merged. */
const NAMESPACES = [
  chrome,
  errors,
  operations,
  net,
  search,
  compare,
  properties,
  rename,
  exporting,
  preview,
  settings,
] as const;

export const DICT = {
  de: {
    ...chrome.de,
    ...errors.de,
    ...operations.de,
    ...net.de,
    ...search.de,
    ...compare.de,
    ...properties.de,
    ...rename.de,
    ...exporting.de,
    ...preview.de,
    ...settings.de,
  },
  en: {
    ...chrome.en,
    ...errors.en,
    ...operations.en,
    ...net.en,
    ...search.en,
    ...compare.en,
    ...properties.en,
    ...rename.en,
    ...exporting.en,
    ...preview.en,
    ...settings.en,
  },
} as const;

export type TransKey = keyof (typeof DICT)["de"];

/**
 * The namespaces as a list, for the test that checks no two of them
 * claim the same key — a duplicate would be silently won by whichever
 * is spread last.
 */
export const ALL_NAMESPACES: readonly {
  de: Record<string, string>;
  en: Record<string, string>;
}[] = NAMESPACES;
