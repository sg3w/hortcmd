// Text in die System-Zwischenablage schreiben (mit Fallback ohne Clipboard-API).

export async function writeClipboard(text: string): Promise<void> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }
  } catch {
    // Fällt auf die execCommand-Variante zurück.
  }
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  try {
    document.execCommand("copy");
  } catch {
    // ignorieren – ohne Zwischenablage-Zugriff nicht möglich
  }
  document.body.removeChild(ta);
}
