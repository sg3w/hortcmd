/** Lazy feature modules register both languages before their first render.
 * This registry never imports a feature, so loading core text cannot pull one in. */
export const DICT: Record<"de" | "en", Record<string, string>> = { de: {}, en: {} };
export function registerTranslations(namespace: { de: Record<string,string>; en: Record<string,string> }): void {
  Object.assign(DICT.de, namespace.de);
  Object.assign(DICT.en, namespace.en);
}
