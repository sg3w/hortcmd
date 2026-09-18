/* No Tauri or filesystem APIs: the injected SDK owns the host connection. */
(() => {
  const api = window.hortcmd;
  const element = id => document.getElementById(id);
  let context = null;
  let copying = false;
  let refreshGeneration = 0;
  let disposed = false;
  const status = message => { element("status").textContent = message; };
  const buttons = () => {
    element("copy-path").disabled = !context?.path;
    element("copy-selection").disabled = copying || !context?.selection?.length;
  };
  function show(value) {
    context = value;
    element("side").textContent = value?.side ?? "Unavailable";
    element("path").textContent = value?.path ?? "Unavailable";
    element("folder").textContent = value?.folderPath ?? "Unavailable";
    element("file").textContent = value?.filePath ?? "None";
    element("limit").textContent = value ? value.copyLimit ? `${value.copyLimit} KiB/s` : "Unlimited" : "Unavailable";
    element("clipboard-path").value = value?.path ?? "";
    element("selection").replaceChildren(...(value?.selection ?? []).map(path => {
      const item = document.createElement("li"); item.textContent = path; return item;
    }));
    buttons();
  }
  async function refresh() {
    const generation = ++refreshGeneration;
    try {
      const value = await api.request({type:"context:get"});
      if (!disposed && generation === refreshGeneration) { show(value); status(value ? "Context refreshed." : "No context is available."); }
    } catch (error) { if (!disposed && generation === refreshGeneration) status(String(error)); }
  }
  element("refresh").onclick = refresh;
  element("copy-path").onclick = async () => {
    if (!context?.path) return;
    // This user gesture writes only the displayed path, never reads the clipboard.
    const input = element("clipboard-path"); input.focus(); input.select();
    try {
      if (document.execCommand?.("copy")) { status("Path copied."); return; }
      if (navigator.clipboard?.writeText) { await navigator.clipboard.writeText(input.value); status("Path copied."); return; }
      throw new Error("Clipboard access is unavailable. The path is selected; use the system Copy shortcut.");
    } catch (error) { status(String(error)); }
  };
  element("copy-form").onsubmit = async event => {
    event.preventDefault();
    if (copying || !context?.selection?.length) return;
    const destination = element("destination").value.trim();
    if (!destination) return;
    copying = true; buttons();
    try {
      const result = await api.request({type:"op:copy",sources:[...context.selection],destination});
      if (!disposed) status(`Copy request ${result.operationId}: review it in the commander. Current limit: ${context.copyLimit || "unlimited"} KiB/s.`);
    } catch (error) { if (!disposed) status(String(error)); }
    finally { copying = false; if (!disposed) buttons(); }
  };
  if (!api) { status("Open this plugin through hortcmd plugin management."); return; }
  const unsubscribe = api.onEvent(event => {
    if (event.type === "view:open") show(event.data.context);
    else if (event.type === "view:path" || event.type === "context:changed") void refresh();
    else if (event.type === "view:close") { disposed = true; ++refreshGeneration; unsubscribe(); }
  });
  api.request({type:"view:ready"}).then(refresh).catch(error => status(String(error)));
})();
