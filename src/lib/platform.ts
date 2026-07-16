// Plattform-Erkennung (Frontend-Seite).

/** true, wenn die App unter macOS läuft. */
export const isMacOS =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);
