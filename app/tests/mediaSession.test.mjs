import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/mediaSession.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { emptyMediaSession, mergeMediaSession, mediaToggleAction, canControlMedia } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
test("an old reply cannot restore a restarted player's old track or artwork", () => {
  const latest = { ...emptyMediaSession, revision: 8, sessionId: "2:cloudmusic.exe", title: "new", coverDataUri: "new-cover" };
  assert.strictEqual(mergeMediaSession(latest, { ...latest, revision: 7, sessionId: "1:cloudmusic.exe", coverDataUri: "old-cover" }), latest);
  assert.equal(mergeMediaSession(latest, { ...emptyMediaSession, revision: 9 }).sessionId, "");
});
test("one transport button uses the actual playback state and supported action", () => {
  const paused = { ...emptyMediaSession, sessionId: "player", canPlay: true, canPause: false, canNext: true };
  assert.equal(mediaToggleAction(paused), "play");
  assert.equal(canControlMedia(paused, mediaToggleAction(paused)), true);
  const playing = { ...paused, playing: true };
  assert.equal(mediaToggleAction(playing), "pause");
  assert.equal(canControlMedia(playing, mediaToggleAction(playing)), false);
  assert.equal(canControlMedia(emptyMediaSession, "next"), false);
});

test("lightweight playback updates retain artwork only within the same session", () => {
  const current = { ...emptyMediaSession, revision: 4, sessionId: "player", coverDataUri: "cover", playing: true };
  const { coverDataUri, ...transport } = current;
  const paused = mergeMediaSession(current, { ...transport, revision: 5, playing: false });
  assert.equal(paused.coverDataUri, "cover"); assert.equal(paused.playing, false);
  assert.equal(mergeMediaSession(paused, { ...transport, revision: 6, coverDataUri: "" }).coverDataUri, "");
  assert.equal(mergeMediaSession(paused, { ...transport, revision: 6, sessionId: "new-player" }).coverDataUri, "");
});

async function hookHarness() {
  const values = [], effects = [], events = new Map(), refreshes = [], watches = [];
  let cursor = 0, lock = { token: 0, busy: false }, visible = true;
  const effect = (run, deps) => {
    const index = cursor++, old = values[index];
    if (!old || deps.some((value, i) => value !== old.deps[i])) {
      old?.cleanup?.();
      const entry = { deps }; values[index] = entry;
      effects.push(() => { entry.cleanup = run(); });
    }
  };
  const react = {
    useState(initial) { const index = cursor++; if (!(index in values)) values[index] = initial;
      return [values[index], next => { values[index] = typeof next === "function" ? next(values[index]) : next; }]; },
    useRef(initial) { const index = cursor++; return values[index] ??= { current: initial }; },
    useEffect: effect, useLayoutEffect: effect,
    useCallback: callback => { cursor++; return callback; }, useMemo: factory => { cursor++; return factory(); },
  };
  const bridge = { desktop: true,
    onDesktopEvent: async (name, callback) => { events.set(name, callback); return () => events.delete(name); },
    watchMediaSession: async enabled => watches.push(enabled), getMediaSession: async () => emptyMediaSession,
    refreshMediaSession: () => new Promise((resolve, reject) => refreshes.push({ resolve, reject })),
    controlMediaSession: async () => ({ ...emptyMediaSession, revision: 10, sessionId: "player", playing: false }),
  };
  globalThis.__mediaHookTest = { react, bridge, i18n: { errorText: String },
    model: { emptyMediaSession, mergeMediaSession }, lock: { idleAutoHandState: { token: 0, busy: false } } };
  const compiled = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/useMediaSession.ts", import.meta.url), "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
  }).outputText.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => {
    const key = { react: "react", "../../services/desktop": "bridge", "../../i18n": "i18n", "./mediaSession": "model", "./autoHandState": "lock" }[path];
    assert.ok(key, path); return `const {${names}} = globalThis.__mediaHookTest.${key};`;
  });
  const { useMediaSession } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}#${Math.random()}`);
  return { refreshes, watches, events,
    render(nextLock = lock, nextVisible = visible) { lock = nextLock; visible = nextVisible; cursor = 0;
      const controller = useMediaSession(visible, lock); effects.splice(0).forEach(run => run()); return controller; },
    cleanup() { values.forEach(value => value?.cleanup?.()); delete globalThis.__mediaHookTest; },
  };
}
const flush = () => new Promise(resolve => setImmediate(resolve));

test("lock hides intermediate metadata until a fresh unlock sample; late replies cannot reveal a new lock", async () => {
  const h = await hookHarness();
  try {
    assert.equal(h.render().masked, true); assert.equal(h.refreshes.length, 0);
    await flush(); h.render();
    h.refreshes[0].resolve({ ...emptyMediaSession, revision: 1, sessionId: "player", title: "initial" }); await flush();
    assert.equal(h.render().masked, false);
    assert.equal(h.render({ token: 1, busy: true }).masked, true);
    h.events.get("media-session-changed")({ ...emptyMediaSession, revision: 2, sessionId: "player", title: "intermediate" });
    assert.equal(h.render().masked, true);
    h.render({ token: 1, busy: false }); assert.equal(h.render().masked, true);
    h.render({ token: 2, busy: true });
    h.refreshes[1].resolve({ ...emptyMediaSession, revision: 3, sessionId: "player", title: "late unlock" }); await flush();
    assert.equal(h.render().masked, true);
    h.render({ token: 2, busy: false });
    h.refreshes[2].resolve({ ...emptyMediaSession, revision: 4, sessionId: "player", title: "final", coverDataUri: "final-cover" }); await flush();
    const shown = h.render(); assert.equal(shown.masked, false); assert.equal(shown.state.title, "final");
    h.events.get("media-session-changed")({ ...emptyMediaSession, revision: 3, sessionId: "player", coverDataUri: "late-cover" });
    assert.equal(h.render().state.coverDataUri, "final-cover");
    assert.deepEqual(h.watches, [true]);
  } finally { h.cleanup(); await flush(); }
});

test("a failed refresh keeps the placeholder despite regular media events", async () => {
  const h = await hookHarness();
  try {
    h.render(); await flush(); h.render();
    h.refreshes[0].reject(Error("media unavailable")); await flush();
    h.events.get("media-session-changed")({ ...emptyMediaSession, revision: 3, sessionId: "player", title: "unconfirmed" });
    assert.equal(h.render().masked, true); assert.match(h.render().error, /media unavailable/);
    h.render({ token: 0, busy: false }, false);
  } finally { h.cleanup(); await flush(); }
});

test("transport completion cannot overtake an artwork event or reveal locked information", async () => {
  const h = await hookHarness();
  try {
    h.render(); await flush(); h.render();
    h.refreshes[0].resolve({ ...emptyMediaSession, revision: 1, sessionId: "player", coverDataUri: "current-cover", playing: true }); await flush();
    const player = h.render({ token: 1, busy: true });
    await player.control("pause");
    h.events.get("media-session-changed")({ ...emptyMediaSession, revision: 2, sessionId: "player", coverDataUri: "new-cover", playing: false });
    assert.equal(h.render().state.coverDataUri, "new-cover"); assert.equal(h.render().masked, true);
  } finally { h.cleanup(); await flush(); }
});

test("the locked playback row uses the configured mystery art and keeps the transport controls independent of AUTO", async () => {
  const h = (type, props, ...children) => ({ type, props: { ...props, children: children.flat() } });
  const stubs = { "../../assets": { defaultMysteryCover: "default-cover" }, "../../components/Cover": { Cover: "cover" },
    "../../Icon": { Icon: "icon" }, "../../i18n": { t: value => value }, "./mediaSession": { canControlMedia, mediaToggleAction } };
  globalThis.__mediaViewTest = { h, stubs };
  try {
    const compiled = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/HandPlayback.tsx", import.meta.url), "utf8"), {
      compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.React, jsxFactory: "h" },
    }).outputText.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => {
      assert.ok(stubs[path], path); return `const {${names}} = globalThis.__mediaViewTest.stubs[${JSON.stringify(path)}];`;
    });
    const { HandPlayback } = await import(`data:text/javascript;base64,${Buffer.from('const {h}=globalThis.__mediaViewTest;\n'+compiled).toString("base64")}`);
    const render = showNext => HandPlayback({ element: { current: null }, open: true, side: "below", folding: false, style: {}, finishFold() {}, showNext, mysteryCover: "custom-cover",
      media: { state: { ...emptyMediaSession, sessionId: "player", title: "unexpected-track", artist: "unexpected-artist", canPlay: true, canNext: true }, pending: false, error: "", masked: true, control: async () => {} } });
    const row = render(false), walk = node => typeof node === "object" && node ? [node, ...(node.props?.children ?? []).flatMap(walk)] : [];
    assert.equal(JSON.stringify(row).includes("unexpected"), false);
    assert.equal(walk(row).find(node => node.type === "cover").props.url, "custom-cover");
    const buttons = walk(row).filter(node => node.type === "button");
    assert.equal(buttons.length, 1); assert.equal(buttons[0].props.disabled, false); assert.equal(buttons[0].props["aria-label"], "继续播放");
    assert.equal(walk(render(true)).filter(node => node.type === "button").length, 2);
  } finally { delete globalThis.__mediaViewTest; }
});

test("the initial busy-lock read becomes ready only after a state reply or event", async () => {
  const compile = file => ts.transpileModule(fs.readFileSync(new URL(file, import.meta.url), "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
  }).outputText;
  const model = await import(`data:text/javascript;base64,${Buffer.from(compile("../src/features/hand/autoHandState.ts")).toString("base64")}`);
  const values = [], effects = []; let cursor = 0, installed = false, accept, reply;
  const stubs = { react: { useState(initial) { const i = cursor++; if (!(i in values)) values[i] = initial;
    return [values[i], next => { values[i] = typeof next === "function" ? next(values[i]) : next; }]; },
    useEffect(run) { if (!installed) { installed = true; effects.push(run); } } },
    bridge: { desktop: true, onDesktopEvent: async (_, listener) => { accept = listener; return () => {}; },
      call: () => new Promise(resolve => { reply = resolve; }) }, model };
  globalThis.__initialAutoTest = stubs;
  let cleanup;
  try {
    const source = compile("../src/features/hand/useAutoHandControl.ts").replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => {
      const key = { react: "react", "../../services/desktop": "bridge", "./autoHandState": "model" }[path];
      assert.ok(key, path); return `const {${names}}=globalThis.__initialAutoTest.${key};`;
    });
    const { useAutoHandControl } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
    const render = () => { cursor = 0; return useAutoHandControl(); };
    assert.equal(render().ready, false); cleanup = effects[0](); await flush();
    assert.equal(render().ready, false);
    accept({ token: 6, busy: true }); assert.equal(render().ready, true); assert.equal(render().state.busy, true);
    reply({ token: 5, busy: false }); await flush();
    assert.equal(render().state.busy, true);
  } finally { cleanup?.(); await flush(); delete globalThis.__initialAutoTest; }
});
