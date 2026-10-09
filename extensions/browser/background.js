import { pairing } from "./config.js";
import "./protocol.js";

// Firefox's browser namespace consistently returns Promises; Chromium 116+
// supplies the same asynchronous methods on chrome. Both use one controller.
const extensionApi = globalThis.browser ?? globalThis.chrome;
let socket = null, heartbeat = null, reconnect = null, authenticated = false;
const report = (data) => { if (authenticated && socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(data)); };
const player = DiscoASBrowser.controller(extensionApi.tabs, extensionApi.storage.session, report);
function connect() {
  if (socket && socket.readyState < WebSocket.CLOSING) return;
  clearTimeout(reconnect);
  const candidate = new WebSocket(`ws://127.0.0.1:${pairing.port}/discoas-browser`);
  socket = candidate; authenticated = false;
  candidate.onopen = () => {
    candidate.send(JSON.stringify({ token: pairing.token }));
    // Chromium >=116 keeps the worker alive when messages arrive within 30s.
    // Firefox uses a persistent background page; the heartbeat checks transport.
    heartbeat = setInterval(() => report({ heartbeat: true }), 10000);
  };
  candidate.onmessage = (event) => {
    if (socket !== candidate) return;
    try {
      const request = JSON.parse(event.data);
      if (request.authenticated === true && !authenticated) {
        authenticated = true; player.beginSession().catch(() => {});
      }
      if (authenticated && request.type === "play") player.command(request).catch(() => report({ requestId: request.requestId, rejected: true, error: "错误：视频编号无效" }));
    } catch { /* Ignore malformed transport data. */ }
  };
  candidate.onclose = () => {
    if (socket !== candidate) return;
    socket = null; authenticated = false; clearInterval(heartbeat);
    reconnect = setTimeout(connect, 3000);
  };
  candidate.onerror = () => candidate.close();
}
extensionApi.runtime.onMessage.addListener((message, sender, reply) => {
  player.handle(message, sender).then(reply).catch(() => reply(null));
  return true;
});
extensionApi.tabs.onRemoved.addListener((id) => player.removed(id).catch(() => {}));
extensionApi.alarms.create("discoas-reconnect", { periodInMinutes: 1 });
extensionApi.alarms.onAlarm.addListener((alarm) => { if (alarm.name === "discoas-reconnect") connect(); });
extensionApi.runtime.onStartup.addListener(connect);
extensionApi.runtime.onInstalled.addListener(connect);
connect();
