// Injected before the plugin document. No Tauri APIs are exposed to this frame.
(() => {
  let next = 0;
  const pending = new Map();
  const listeners = new Set();
  window.hortcmd = Object.freeze({
    request(request) {
      if (pending.size >= 32) return Promise.reject(new Error("Too many pending plugin requests"));
      const id = ++next;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => { pending.delete(id); reject(new Error("Plugin request timed out")); }, 15000);
        pending.set(id, {resolve, reject, timer});
        parent.postMessage({channel:"hortcmd:request", id, request}, "*");
      });
    },
    onEvent(listener) { listeners.add(listener); return () => listeners.delete(listener); }
  });
  addEventListener("message", event => {
    if (event.source !== parent || !event.data) return;
    const data = event.data;
    if (data.channel === "hortcmd:response") {
      const call = pending.get(data.id);
      if (!call) return;
      pending.delete(data.id); clearTimeout(call.timer);
      if (data.error) call.reject(new Error(data.error)); else call.resolve(data.value);
    } else if (data.channel === "hortcmd:event") {
      for (const listener of listeners) { try { listener(data.event); } catch (error) { console.error(error); } }
    }
  });
})();
