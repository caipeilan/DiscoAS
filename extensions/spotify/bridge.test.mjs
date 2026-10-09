import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createContext, runInContext } from "node:vm";

const source = readFileSync(new URL("./discoas-bridge.js", import.meta.url), "utf8")
  .replace("__DISCOAS_PORT__", "12345").replace("__DISCOAS_TOKEN__", "paired-secret");
const trackA = "spotify:track:0123456789ABCDEFGHIJKL";
const trackB = "spotify:track:ABCDEFGHIJKL0123456789";

function setup(playUri = async () => {}) {
  const intervals = new Map();
  let timer = 0;
  const sockets = [];
  class Socket {
    static OPEN = 1;
    readyState = 1;
    sent = [];
    constructor(url) { this.url = url; sockets.push(this); }
    send(text) { this.sent.push(JSON.parse(text)); }
  }
  const Player = {
    data: { item: { uri: trackA } }, isPlaying: () => true,
    addEventListener: () => {}, playUri,
  };
  const context = createContext({
    window: { Spicetify: { Player } }, Spicetify: { Player }, WebSocket: Socket,
    setInterval(fn) { intervals.set(++timer, fn); return timer; },
    clearInterval(id) { intervals.delete(id); }, setTimeout: () => ++timer, clearTimeout: () => {},
  });
  runInContext(source, context);
  intervals.get(1)();
  const socket = sockets[0];
  socket.onopen();
  return {
    socket, Player,
    authenticate() { socket.onmessage({ data: JSON.stringify({ authenticated: true }) }); },
    send(message) { socket.onmessage({ data: JSON.stringify(message) }); },
  };
}

test("the optional bridge authenticates before exposing track state", () => {
  const bridge = setup();
  assert.equal(bridge.socket.url, "ws://127.0.0.1:12345/discoas");
  assert.deepEqual(bridge.socket.sent, [{ token: "paired-secret" }]);
  bridge.authenticate();
  assert.equal(bridge.socket.sent[1].uri, trackA);
  assert.equal(bridge.socket.sent[1].requestId, 0);
});

test("invalid URIs are ignored and a rejected request reports its own ID", async () => {
  const calls = [];
  const bridge = setup(async (uri) => { calls.push(uri); throw new Error("unavailable"); });
  bridge.authenticate();
  bridge.send({ requestId: 1, uri: "spotify:track:a?context=evil" });
  bridge.send({ requestId: 2, uri: trackB });
  bridge.send({ requestId: 1, uri: trackA });
  await new Promise(setImmediate);
  assert.deepEqual(calls, [trackB]);
  assert.equal(bridge.socket.sent.at(-1).requestId, 2);
  assert.equal(bridge.socket.sent.at(-1).rejected, true);
});

test("a superseded request cannot publish a delayed failure for a newer selection", async () => {
  let rejectOld;
  const bridge = setup((uri) => uri === trackA
    ? new Promise((_, reject) => { rejectOld = reject; }) : Promise.resolve());
  bridge.authenticate();
  bridge.send({ requestId: 1, uri: trackA });
  bridge.Player.data.item.uri = trackB;
  bridge.send({ requestId: 2, uri: trackB });
  await new Promise(setImmediate);
  rejectOld(new Error("late old failure"));
  await new Promise(setImmediate);
  assert.equal(bridge.socket.sent.at(-1).requestId, 2);
  assert.equal(bridge.socket.sent.at(-1).uri, trackB);
  assert.equal(bridge.socket.sent.some((message) => message.rejected), false);
});
