import React from "react";
import { createRoot } from "react-dom/client";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { defaultGuiSettings, defaultHandSettings, emptyState, type HandCard, type HandSnapshot } from "../../src/types";

// Exercise the real HandWindow/controller with a deterministic desktop boundary.
Object.assign(globalThis, { isTauri: true });
mockWindows("hand");
const covers = ["#7594a4", "#9e927c", "#879989"];
const initial: HandCard[] = covers.map((color, index) => ({
  id: `play-${index}`, collectedAt: 0, mysteryRevealed: false,
  song: { songId: `song-${index}`, title: ["出牌位置", "保持锁定", "等待下一首"][index], artistNames: ["DiscoAS"],
    coverDataUri: `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="240" height="240"><rect width="240" height="240" fill="${color}"/><circle cx="120" cy="120" r="70" fill="#ffffff30"/><circle cx="120" cy="120" r="24" fill="#ffffff80"/></svg>`)}`,
    albumPicUrl: "", coverError: null, mysteryMode: false, platform: "NeteaseCloudMusic", sourceId: "preview", sourceKind: "playlist", filename: "", detailError: null },
}));
let cards = [...initial], generation = 1, token = 0, busy = false, playingCardId: string | undefined;
let dark = false, requests = 0;
const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
const snapshot = (): HandSnapshot => ({ generation, cards,
  settings: { ...defaultHandSettings, enabled: true, side: "bottom", autoPlayEnabled: true },
  gui: { ...defaultGuiSettings, followSystemTheme: false, nightMode: dark }, keys: emptyState.settings.discoveryKeybindings,
  workArea: { left: 0, top: 0, width: innerWidth, height: innerHeight }, preview: false, focus: false, expanded: true, arrival: null,
  surface: { left: 0, top: 0, width: innerWidth, height: innerHeight, scale: 1 },
});
const state = () => ({ token, busy, configured: true, enabled: true, revision: 0, ...(playingCardId ? { playingCardId } : {}) });
async function begin(id: string) {
  token++; busy = true; playingCardId = id;
  await emit("auto-hand-state-changed", state());
  await sleep(700);
  cards = cards.filter(card => card.id !== id); generation++;
  await emit("hand-state-changed", snapshot());
  setTimeout(async () => { busy = false; playingCardId = undefined; await emit("auto-hand-state-changed", state()); renderControls(); }, 1500);
}
mockIPC(async (command, args) => {
  if (command === "hand_ready") return snapshot();
  if (command === "get_auto_hand_state") return state();
  if (command === "play_hand_card") { requests++; renderControls(); await begin(String(args?.id)); }
  if (command === "set_hand_hit_regions") return snapshot().surface;
  if (command === "get_client_hide_enabled") return false;
  return null;
}, { shouldMockEvents: true });
const { HandWindow } = await import("../../src/features/hand/HandWindow");
createRoot(document.getElementById("root")!).render(<HandWindow />);
const controls = createRoot(document.getElementById("controls")!);
async function autoPlay() {
  if (busy || !cards.length) return;
  const id = cards[0].id;
  const element = document.querySelector<HTMLElement>(`[data-hand-id="${id}"]`);
  const rect = element!.getBoundingClientRect();
  await emit("hand-pointer", [rect.x + rect.width / 2, rect.y + 30, false]);
  await sleep(220);
  token++; busy = true; playingCardId = undefined;
  await emit("auto-hand-state-changed", state());
  await sleep(180);
  // begin() emits the selected identity as a separate same-token state.
  token--;
  await begin(id);
  renderControls();
}
async function keyboardPlay() {
  if (busy || !cards.length) return;
  await emit("hand-key", { key: "ArrowLeft", repeat: false });
  await sleep(180);
  await emit("hand-key", { key: "Enter", repeat: false });
}
async function reset() {
  busy = false; playingCardId = undefined; token++; cards = [...initial]; generation++;
  await emit("auto-hand-state-changed", state()); await emit("hand-state-changed", snapshot()); renderControls();
}
function renderControls() {
  controls.render(<div style={{ position: "fixed", left: 40, top: 36, zIndex: 2000 }}>
    <h1 style={{ fontSize: 24, margin: "0 0 12px" }}>出牌动画</h1>
    <div style={{ display: "flex", gap: 12 }}>
      <button className="button secondary" onClick={autoPlay}>悬停后自动出牌</button>
      <button className="button secondary" onClick={keyboardPlay}>键盘出牌</button>
      <button className="button secondary" onClick={reset}>重置</button>
      <button className="button secondary" onClick={async () => { dark = !dark; generation++; await emit("hand-state-changed", snapshot()); renderControls(); }}>{dark ? "浅色" : "深色"}</button>
    </div>
    <p role="status" style={{ color: "var(--text-secondary)" }}>真实手牌界面 · 手动播放请求 {requests} 次</p>
  </div>);
}
renderControls();
