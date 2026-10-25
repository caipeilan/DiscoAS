(() => {
  const extensionApi = globalThis.browser ?? globalThis.chrome;
  const send = (data) => extensionApi.runtime.sendMessage(data).catch(() => null);
  const pauses = new Map();
  const post = (data) => window.postMessage({ source: "discoas-apple", dir: "to-main", ...data }, "*");
  window.addEventListener("message", (event) => {
    if (event.source !== window) return;
    const data = event.data;
    if (!data || data.source !== "discoas-apple" || data.dir !== "to-bridge") return;
    if (data.type === "paused") { pauses.get(data.requestId)?.(data.paused === true); return; }
    if (data.type === "ready") {
      send({ type: "discoas-ready" }).then((binding) => {
        if (binding) post({ type: "discoas-bind", binding });
      });
      return;
    }
    if (data.type !== "state" && data.type !== "observation") return;
    const { source, dir, type, ...payload } = data;
    send({ type: type === "state" ? "discoas-state" : "discoas-observation", ...payload });
  });
  extensionApi.runtime.onMessage.addListener((message, _sender, reply) => {
    if (message.type === "discoas-observe") post({ type: message.type, enabled: message.enabled });
    if (message.type === "discoas-bind") post({ type: message.type, binding: message.binding });
    if (message.type === "discoas-pause") {
      if (message.requestId === undefined) { post({ type: message.type }); return; }
      const timeout = setTimeout(() => { pauses.delete(message.requestId); reply({ paused: false }); }, 1200);
      pauses.set(message.requestId, paused => { clearTimeout(timeout); pauses.delete(message.requestId); reply({ paused }); });
      post({ type: message.type, requestId: message.requestId });
      return true;
    }
  });
})();
