// Platform detection (frontend side).

/** true when the app runs on macOS. */
export const isMacOS =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);

/** true when the app runs on Windows. */
export const isWindows =
  typeof navigator !== "undefined" && /Win/.test(navigator.userAgent);

/**
 * true for the platforms with a POSIX rights model (macOS, Linux, BSD)
 * — the frontend counterpart of the backend's `cfg!(unix)` gate. Only
 * used where the answer is needed before the backend has spoken; the
 * authority on what a location supports stays `location_capabilities`.
 */
export const isUnixLike = typeof navigator !== "undefined" && !isWindows;
