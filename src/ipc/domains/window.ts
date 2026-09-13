import { invoke } from "../invoke";

/** Page zoom is presentation state; Tauri applies it to the calling WebView. */
export function setPageZoom(factor: number): Promise<void> {
  return invoke("plugin:webview|set_webview_zoom", { value: factor });
}
