import { useEffect, useState, type CSSProperties } from "react";
import type { HandDragIntent, HandDragSnapshot } from "../../types";
import { defaultGuiSettings } from "../../types";
import { call, onDesktopEvent } from "../../services/desktop";
import { t } from "../../i18n";
import { Icon } from "../../Icon";
import { HandCardContent } from "./HandCardContent";
import { groupSettings, handCardSize } from "./handLayout";
import { useAppearance } from "../settings/useAppearance";
import "../../App.css";
import "./hand.css";

/** A small, click-through surface that owns only the card being dragged. */
export function HandDragWindow() {
  const [snapshot, setSnapshot] = useState<HandDragSnapshot | null>(null);
  const [intent, setIntent] = useState<HandDragIntent>({ playable: false, collecting: false, dx: 0, dy: 0 });
  const [dropping, setDropping] = useState(false);
  useAppearance(snapshot?.gui ?? defaultGuiSettings);

  useEffect(() => {
    let cancelled = false;
    const stops: Array<() => void> = [];
    const subscribe = async <E extends "hand-drag-state" | "hand-drag-intent" | "hand-drag-end">(
      event: E, handler: Parameters<typeof onDesktopEvent<E>>[1]) => {
      const stop = await onDesktopEvent(event, handler);
      if (cancelled) stop(); else stops.push(stop);
    };
    void Promise.all([
      subscribe("hand-drag-state", (value) => {
        setSnapshot(value);
        setIntent({ playable: false, collecting: false, dx: 0, dy: 0 });
        setDropping(false);
      }),
      subscribe("hand-drag-intent", setIntent),
      subscribe("hand-drag-end", (value) => { if (!value.cancelled) setDropping(true); }),
    ]).then(() => { if (!cancelled) void call("hand_drag_ready").catch(() => {}); });
    return () => { cancelled = true; stops.forEach(stop => stop()); };
  }, []);

  if (!snapshot) return null;
  const { card, settings, discoverySettings, gui, pose } = snapshot;
  const liquid = discoverySettings.liquid_strength ?? 100;
  const liquidMode = discoverySettings.liquid_mode ?? "static";
  const glassFilter = liquidMode === "blur" && liquid ? `blur(${.22 * liquid}px) saturate(${100 + .6 * liquid}%)` : "none";
  const group = groupSettings(card, settings, discoverySettings);
  const cardHeight = handCardSize(gui.font_size).height;
  const style = {
    width: pose.width, height: pose.height,
    "--hand-x": "0px", "--hand-y": "0px", "--hand-base-scale": pose.width / 156,
    "--hand-card-height": `${cardHeight}px`, "--hand-card-scale": 1.12, "--hand-angle": "0deg",
    "--card-x": "0px", "--card-y": "0px", "--drag-angle": `${Math.max(-8, Math.min(8, intent.dx * .05))}deg`,
    "--hand-glass-outline": `${discoverySettings.text_outline_width ?? 1}px`,
    "--hand-glass-filter": glassFilter,
    "--hand-glass-opacity": `${100 - .54 * liquid}%`, "--hand-glass-highlight": `${.7 * liquid}%`,
    zIndex: 1000,
  } as CSSProperties;
  return <div className="hand-stage hand-drag-stage" data-liquid-mode={liquidMode}>
    <div data-side={group.side} style={style}
      className={`hand-card hand-dragging hand-active${card.discovery ? " hand-discovery" : ""}${intent.playable || intent.collecting ? " hand-playable" : ""}${dropping ? " hand-drag-dropping" : ""}`}>
      <div className="hand-card-face">
        <HandCardContent card={card} />
        {(intent.playable || intent.collecting) && <div className="hand-release-label">
          <Icon name={intent.collecting ? "hand" : "play"} size={10} />
          {t(intent.collecting ? "松开以加入手牌" : "松开以播放")}
        </div>}
      </div>
    </div>
  </div>;
}
