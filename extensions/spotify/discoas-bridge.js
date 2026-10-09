/* Optional DiscoAS bridge. Export a paired copy from DiscoAS settings before installation.
 * API reference: https://spicetify.app/docs/development/api-wrapper/methods/player
 * This template contains no Spotify account credentials. Never publish a paired copy.
 */
(function discoasBridge() {
  "use strict";
  const endpoint = "ws://127.0.0.1:__DISCOAS_PORT__/discoas";
  const pairingToken = "__DISCOAS_TOKEN__";
  let socket = null;
  let authenticated = false;
  let requestId = 0;
  let retryTimer = null;
  let pollTimer = null;
  let listenersAttached = false;

  function report(rejected = false, id = requestId) {
    if (!authenticated || socket?.readyState !== WebSocket.OPEN) return;
    socket.send(JSON.stringify({
      requestId: id,
      uri: Spicetify.Player.data?.item?.uri ?? "",
      playing: Boolean(Spicetify.Player.isPlaying()),
      rejected,
    }));
  }

  async function play(message) {
    if (!Number.isSafeInteger(message.requestId) || message.requestId < requestId
      || !/^spotify:track:[A-Za-z0-9]{22}$/.test(message.uri ?? "")) return;
    requestId = message.requestId;
    const current = requestId;
    try {
      await Spicetify.Player.playUri(message.uri);
      if (current === requestId) report();
    } catch {
      if (current === requestId) report(true, current);
    }
  }

  function connect() {
    clearTimeout(retryTimer);
    try { socket = new WebSocket(endpoint); }
    catch { retryTimer = setTimeout(connect, 2000); return; }
    authenticated = false;
    socket.onopen = () => socket.send(JSON.stringify({ token: pairingToken }));
    socket.onmessage = ({ data }) => {
      let message;
      try { message = JSON.parse(data); } catch { return; }
      if (message.authenticated === true) {
        authenticated = true;
        requestId = 0;
        report();
        clearInterval(pollTimer);
        pollTimer = setInterval(() => report(), 750);
        if (!listenersAttached) {
          listenersAttached = true;
          Spicetify.Player.addEventListener("songchange", () => report());
          Spicetify.Player.addEventListener("onplaypause", () => report());
        }
        return;
      }
      if (authenticated) void play(message);
    };
    socket.onclose = () => {
      authenticated = false;
      clearInterval(pollTimer);
      retryTimer = setTimeout(connect, 2000);
    };
    socket.onerror = () => { /* onclose schedules reconnection. */ };
  }

  const ready = setInterval(() => {
    if (!window.Spicetify?.Player?.playUri || !Spicetify.Player.isPlaying) return;
    clearInterval(ready);
    connect();
  }, 500);
})();
