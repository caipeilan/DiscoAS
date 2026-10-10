import { useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from "react";
import type { HandRect } from "../../types";
import { appLogo } from "../../assets";
import { Icon } from "../../Icon";
import { t } from "../../i18n";
import { floatingDockRect, settleDockPoint, type DockPoint, type DockDirection } from "./floatingDock";

interface Props {
  element: RefObject<HTMLDivElement | null>;
  point: DockPoint; areas: HandRect[]; expanded: boolean; preview: boolean;
  diameter: number; count: number; capacity: number; busy: boolean; confirm: boolean;
  toCanvas: (x: number, y: number) => DockPoint;
  beginDrag: () => Promise<unknown>;
  dragging: (value: boolean) => void;
  move: (point: DockPoint) => void;
  toggle: () => void; clear: () => void;
}

export function HandDock({ element, point, areas, expanded, preview, diameter, count, capacity, busy, confirm,
  toCanvas, beginDrag, dragging, move, toggle, clear }: Props) {
  const [moving, setMoving] = useState(false);
  const gesture = useRef<{ pointerId: number; x: number; y: number; dx: number; dy: number;
    origin: DockPoint; started: boolean } | null>(null);
  const animationFrame = useRef(0);
  const wantedDirection = floatingDockRect(point, expanded || preview, diameter, areas).direction;
  const [appearance, setAppearance] = useState({ direction: wantedDirection, folding: false });
  const appearanceRef = useRef(appearance);
  const desiredDirection = useRef(wantedDirection);
  const held = gesture.current;
  const displayPoint = held?.started ? { x: held.origin.x + held.dx, y: held.origin.y + held.dy } : point;
  const pillExpanded = (expanded || preview) && !appearance.folding;
  const geometry = floatingDockRect(displayPoint, pillExpanded, diameter, areas, appearance.direction);
  const latest = useRef({ point, areas, expanded: expanded || preview, diameter, toCanvas });
  latest.current = { point, areas, expanded: expanded || preview, diameter, toCanvas };
  const show = (next: typeof appearance) => { appearanceRef.current = next; setAppearance(next); };
  const finishFold = () => {
    if (appearanceRef.current.folding && element.current && element.current.offsetWidth <= latest.current.diameter + 1) {
      show({ direction: desiredDirection.current, folding: false });
    }
  };
  const requestDirection = (direction: DockDirection) => {
    desiredDirection.current = direction;
    const view = appearanceRef.current;
    // While shrinking, keep following the pointer and use its latest side at the circle.
    if (view.folding || view.direction === direction) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches ||
      !element.current || element.current.offsetWidth <= latest.current.diameter + 1) {
      show({ direction, folding: false });
    } else show({ ...view, folding: true });
  };
  const paint = () => {
    animationFrame.current = 0;
    const held = gesture.current, node = element.current;
    if (!held?.started || !node) return;
    const state = latest.current;
    const position = { x: held.origin.x + held.dx, y: held.origin.y + held.dy };
    requestDirection(floatingDockRect(position, state.expanded, state.diameter, state.areas).direction);
    const view = appearanceRef.current;
    const rect = floatingDockRect(position, state.expanded && !view.folding, state.diameter, state.areas, view.direction);
    node.style.setProperty("--dock-x", `${position.x - state.diameter / 2}px`);
    node.style.setProperty("--dock-y", `${rect.top}px`);
    for (const [edge, touching] of Object.entries(rect.edges)) {
      node.toggleAttribute(`data-edge-${edge}`, touching);
    }
  };
  useLayoutEffect(() => {
    if (!gesture.current?.started) requestDirection(wantedDirection);
  }, [wantedDirection, expanded, preview, diameter]);
  useLayoutEffect(() => {
    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const changed = () => { if (motion.matches) finishFold(); };
    motion.addEventListener("change", changed);
    return () => { cancelAnimationFrame(animationFrame.current); motion.removeEventListener("change", changed); };
  }, []);
  const finish = (cancelled = false) => {
    const held = gesture.current;
    if (!held) return;
    gesture.current = null;
    cancelAnimationFrame(animationFrame.current); animationFrame.current = 0;
    if (held.started) {
      const next = cancelled ? latest.current.point : settleDockPoint({ x: held.origin.x + held.dx, y: held.origin.y + held.dy }, latest.current.diameter, latest.current.areas);
      requestDirection(floatingDockRect(next, latest.current.expanded, latest.current.diameter, latest.current.areas).direction);
      move(next); dragging(false);
    } else if (!cancelled) toggle();
    setMoving(false);
  };
  return <div ref={element} data-hand-hit data-direction={geometry.direction} data-dock-expanded={expanded || preview}
    data-folding={appearance.folding || undefined}
    data-edge-left={geometry.edges.left || undefined} data-edge-right={geometry.edges.right || undefined}
    data-edge-top={geometry.edges.top || undefined} data-edge-bottom={geometry.edges.bottom || undefined}
    className={`hand-toolbar hand-dock${moving ? " hand-dock-dragging" : ""}`}
    style={{ "--dock-x": `${displayPoint.x - diameter / 2}px`, "--dock-y": `${geometry.top}px`, "--dock-width": `${geometry.width}px`,
      "--dock-diameter": `${diameter}px` } as CSSProperties}
    onTransitionEnd={(event) => { if (event.target === event.currentTarget && event.propertyName === "width") finishFold(); }}>
    <button className="hand-dock-logo" data-hand-close={preview || undefined} data-hand-toggle
      aria-expanded={preview ? undefined : expanded} aria-label={preview ? t("退出预览") : expanded ? t("收纳手牌") : t("展开手牌")}
      onClick={(event) => { if (preview || event.detail === 0) toggle(); }}
      onPointerDown={(event) => {
        if (event.button !== 0 || preview) return;
        event.preventDefault();
        const bounds = event.currentTarget.getBoundingClientRect();
        const origin = toCanvas(bounds.left + bounds.width / 2, bounds.top + bounds.height / 2);
        const cursor = toCanvas(event.clientX, event.clientY);
        gesture.current = { pointerId: event.pointerId, x: cursor.x, y: cursor.y, dx: 0, dy: 0,
          origin, started: false };
        event.currentTarget.setPointerCapture(event.pointerId);
      }} onPointerMove={(event) => {
        const held = gesture.current;
        if (!held || held.pointerId !== event.pointerId) return;
        const cursor = latest.current.toCanvas(event.clientX, event.clientY);
        held.dx = cursor.x - held.x; held.dy = cursor.y - held.y;
        if (!held.started && Math.hypot(held.dx, held.dy) > 5) {
          held.started = true; setMoving(true); dragging(true);
          void beginDrag().catch(() => { if (gesture.current === held) finish(true); });
        }
        if (held.started && !animationFrame.current) animationFrame.current = requestAnimationFrame(paint);
      }} onPointerUp={(event) => {
        if (gesture.current?.pointerId !== event.pointerId) return;
        finish();
        if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
      }} onPointerCancel={() => finish(true)} onLostPointerCapture={() => finish(true)}>
      <img src={appLogo} draggable={false} alt="" />
    </button>
    <div className="hand-dock-actions" aria-hidden={!pillExpanded} inert={!pillExpanded}>
      <span className="hand-dock-count">{t("手牌 {p0}／{p1}", { p0: count, p1: capacity })}</span>
      {preview ? <button className="hand-clear" aria-label={t("退出预览")} data-hand-close onClick={toggle}><Icon name="close" size={16} /></button>
        : <button className={`hand-clear${confirm ? " hand-clear-confirm" : ""}`} disabled={!count || busy}
          aria-label={confirm ? t("确认清空") : t("清空手牌")} onClick={clear}><Icon name={confirm ? "check" : "remove"} size={16} /></button>}
    </div>
  </div>;
}
