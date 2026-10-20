(() => {
  const extensionApi = globalThis.browser ?? globalThis.chrome;
  const send = (data) => extensionApi.runtime.sendMessage(data).catch(() => null);
  const post = (data) => window.postMessage({ source: "discoas-apple", dir: "to-main", ...data }, "*");
  window.addEventListener("message", (event) => {
    if (event.source !== window) return;
    const data = event.data;
    if (!data || data.source !== "discoas-apple" || data.dir !== "to-bridge") return;
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
  extensionApi.runtime.onMessage.addListener((message) => {
    if (message.type === "discoas-bind" || message.type === "discoas-pause")
      post({ type: message.type, binding: message.binding });
  });
})();
