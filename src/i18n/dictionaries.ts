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

import { preflight } from "./namespaces/preflight";
import { undo } from "./namespaces/undo";
import { recovery } from "./namespaces/recovery";
import { chrome } from "./namespaces/chrome";
import { compare } from "./namespaces/compare";
import { errors } from "./namespaces/errors";
import type { backup } from "./namespaces/backup";
import type { exporting } from "./namespaces/exporting";
import { net } from "./namespaces/net";
import { persist } from "./namespaces/persist";
import { operations } from "./namespaces/operations";
import type { preview } from "./namespaces/preview";
import { properties } from "./namespaces/properties";
import type { pack } from "./namespaces/pack";
import type { rename } from "./namespaces/rename";
import { search } from "./namespaces/search";
import { settings } from "./namespaces/settings";

export type Lang = "de" | "en";

/** Core and mixed-use namespaces needed before any dialog opens. */
const CORE_NAMESPACES = [
  preflight,
  undo,
  recovery,
  chrome,
  errors,
  operations,
  net,
  search,
  compare,
  properties,
  settings,
  persist,
] as const;

/** The key union includes unloaded features without importing their values. */
type Keys<T> = T extends {de: infer D} ? keyof D : never;
export type TransKey = Keys<(typeof CORE_NAMESPACES)[number]>
  | keyof typeof rename.de | keyof typeof exporting.de | keyof typeof preview.de
  | keyof typeof backup.de | keyof typeof pack.de;

export { DICT } from "./registry";
import { registerTranslations } from "./registry";
for (const namespace of CORE_NAMESPACES) registerTranslations(namespace);

export { CORE_NAMESPACES };
