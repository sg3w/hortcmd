// User-triggered clipboard writes, with an explicit failure if both APIs refuse.
export async function writeClipboard(text: string): Promise<void> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }
  } catch {
    // A refused modern API may still permit the user-gesture fallback.
  }
  const focused = document.activeElement;
  const input = focused instanceof HTMLInputElement || focused instanceof HTMLTextAreaElement ? focused : null;
  const inputSelection = input ? {start:input.selectionStart,end:input.selectionEnd,direction:input.selectionDirection} : null;
  const selection = document.getSelection();
  const ranges = selection ? Array.from({length:selection.rangeCount},(_,i)=>selection.getRangeAt(i).cloneRange()) : [];
  const temporary = document.createElement("textarea");
  temporary.value = text;
  temporary.style.position = "fixed";
  temporary.style.opacity = "0";
  temporary.setAttribute("aria-hidden","true");
  document.body.appendChild(temporary);
  try {
    temporary.select();
    if (typeof document.execCommand !== "function" || !document.execCommand("copy")) {
      throw new Error("Clipboard write was refused");
    }
  } catch {
    // Do not include clipboard contents or a potentially sensitive platform error.
    throw new Error("Clipboard write was refused");
  } finally {
    temporary.remove();
    try {
      if (focused instanceof HTMLElement && focused.isConnected) focused.focus({preventScroll:true});
      if (input?.isConnected && inputSelection?.start != null && inputSelection.end != null) {
        input.setSelectionRange(inputSelection.start,inputSelection.end,inputSelection.direction ?? undefined);
      } else if (selection) {
        selection.removeAllRanges();
        for (const range of ranges) if (range.commonAncestorContainer.isConnected) selection.addRange(range);
      }
    } catch {
      // The invoking element may have changed type or disappeared while writing.
    }
  }
}
