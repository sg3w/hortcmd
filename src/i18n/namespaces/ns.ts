// ============================================================
// How a translation namespace is declared (TICKET-035).
//
// Splitting one 1,500-line dictionary into feature namespaces only
// helps if it does not also lose the two guarantees the single file
// gave: that every key is known at compile time, and that both
// languages carry exactly the same set.
//
// `ns` keeps both, and strengthens the second. German is the source of
// truth — its literal decides the key set — and English is typed as
// `Record<keyof T, string>`, so a missing translation is a type error
// and a stray one is caught by object-literal freshness. Before this,
// the two languages were independent literals and only the *use* of a
// key could reveal a gap.
// ============================================================

/** One feature's translations in both languages, keys locked together. */
export interface Namespace<T extends Record<string, string>> {
  de: T;
  en: Record<keyof T, string>;
}

export function ns<const T extends Record<string, string>>(
  de: T,
  en: Record<keyof T, string>,
): Namespace<T> {
  return { de, en };
}
