// Platform detection (frontend side).

/** true when the app runs on macOS. */
export const isMacOS =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);
