import { useState } from "react";
import { createRoot } from "react-dom/client";
import { HandAutoPlayLock } from "../../src/features/hand/HandAutoPlayLock";
import { autoHandNextId } from "../../src/features/hand/autoHandNext";
import { Icon } from "../../src/Icon";
import "../../src/App.css";
import "../../src/features/hand/hand.css";

function Preview() {
  const initial = [
    { id: "mini", discovery: true, name: "迷你发现", color: "#b48c74" },
    { id: "first", discovery: false, name: "第一张手牌", color: "#6e8f98" },
    { id: "middle", discovery: false, name: "第二张手牌", color: "#8186a0" },
    { id: "last", discovery: false, name: "第三张手牌", color: "#6f8e78" },
  ];
  const [cards, setCards] = useState(initial);
  const [locked, setLocked] = useState(false);
  const [dark, setDark] = useState(false);
  const [enabled, setEnabled] = useState(true);
  const [position, setPosition] = useState<"first" | "last">("first");
  const [hover, setHover] = useState("");
  const next = autoHandNextId(cards, { enabled: true, auto_play: enabled, auto_play_position: position });
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  return <main style={{ padding: "36px 44px", maxWidth: 1100, margin: "0 auto" }}>
    <h1 style={{ fontSize: 24, margin: "0 0 12px" }}>下一张自动打出</h1>
    <p style={{ color: "var(--text-secondary)", marginBottom: 28 }}>橙色边框标记下一张普通手牌。手动与自动出牌期间，全部卡片锁定；关闭自动打出后，手动出牌仍会显示锁。</p>
    <div style={{ display: "flex", gap: 12, marginBottom: 36, flexWrap: "wrap" }}>
      <button className="button secondary" onClick={() => setEnabled(!enabled)}>{enabled ? "关闭自动打出" : "开启自动打出"}</button>
      <button className="button secondary" onClick={() => setPosition(position === "first" ? "last" : "first")}>{position === "first" ? "改用最后一张" : "改用第一张"}</button>
      <button className="button secondary" onClick={() => setLocked(!locked)}>{locked ? "预览解锁过渡" : "预览锁定过渡"}</button>
      <button className="button secondary" onClick={() => setDark(!dark)}>{dark ? "浅色" : "深色"}</button>
      <button className="button secondary" onClick={() => setCards([cards[0], ...cards.slice(1).reverse()])}>反转普通手牌</button>
      <button className="button secondary" disabled={!next || locked} onClick={() => setCards(cards.filter(card => card.id !== next))}>打出标记牌</button>
      <button className="button secondary" onClick={() => setCards(initial)}>重置手牌</button>
    </div>
    <div style={{ display: "flex", gap: 28, flexWrap: "wrap" }}>
      {cards.map(card => <div key={card.id}>
        <div className={`hand-card${card.discovery ? " hand-discovery" : ""}${hover === card.id && !locked ? " hand-active" : ""}`} data-auto-next={card.id === next || undefined} data-auto-locked={locked || undefined}
          title={card.id === next ? "下一张自动打出" : undefined} onMouseEnter={() => setHover(card.id)} onMouseLeave={() => setHover("")}
          style={{ position: "relative", left: 0, top: 0, width: 156, height: 220, transform: "none", opacity: 1 }}>
          <div className="hand-card-face"><div className="hand-art" style={{ background: card.color, display: "grid", placeItems: "center", color: "#fff" }}><Icon name="music" size={60} /></div><strong>{card.name}</strong><span>{card.discovery ? "DiscoAS" : "手牌模式"}</span></div>
          <HandAutoPlayLock locked={locked} />
        </div>
        <p style={{ marginTop: 14, textAlign: "center", color: "var(--text-secondary)", fontSize: 13 }}>{card.id === next ? "下一张自动打出" : card.discovery ? "迷你发现" : "普通手牌"}</p>
      </div>)}
    </div>
  </main>;
}
createRoot(document.getElementById("root")!).render(<Preview />);
