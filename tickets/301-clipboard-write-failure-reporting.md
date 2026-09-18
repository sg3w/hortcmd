# TICKET-301: Report failed system clipboard writes

## Finding

While preparing ticket 107, `src/lib/clipboard.ts` was found to ignore both a
false return from `document.execCommand("copy")` and an exception from that call.
If the modern Clipboard API is absent or denied, `writeClipboard` therefore
resolves successfully even though nothing was copied. The commander's callers
cannot display their existing error notifications.

## Requirements

- Resolve only after a confirmed clipboard write.
- Use the fallback when the modern API is unavailable or rejects.
- Reject when the fallback returns false or throws; always remove its temporary
  textarea and preserve the invoking focus/selection where possible.
- Cover modern success, fallback success, denial and thrown failure with tests.
- Keep clipboard contents out of error messages and diagnostics.
- Run `npm run check`.
