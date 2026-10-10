import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import type { HandDockSnapshot, HandPoint, HandSurface } from "../../types";
import { call, onDesktopEvent, requestHandClear } from "../../services/desktop";
import { applyAppearance } from "../../appearance";
import { currentLocale, errorText, setLanguage } from "../../i18n";
import { defaultDockPoint } from "./floatingDock";
import { handLayout } from "./handLayout";
import { HandDock } from "./HandDock";
import "../../App.css";
import "./hand.css";

export function HandDockWindow() {
  const [snapshot, setSnapshot] = useState<HandDockSnapshot | null>(null);
  const [surface, setSurface] = useState<HandSurface | null>(null);
  const [hidden, setHidden] = useState(true);
  const [moving, setMoving] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState("");
  const current = useRef(snapshot), currentSurface = useRef(surface);
  current.current = snapshot; currentSurface.current = surface;
  const stage = useRef<HTMLDivElement>(null), dock = useRef<HTMLDivElement>(null);
  const movingRef = useRef(false), accepted = useRef(0);
  const confirmTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pointOf = (next: HandDockSnapshot) => next.dockPoint ?? current.current?.dockPoint ?? defaultDockPoint(
    handLayout(next.count, next.settings, next.workArea, -1, undefined, next.gui.font_size),
    next.settings.side, next.workArea, 44 * next.gui.font_size / 14);

  useEffect(() => {
    let cancelled = false;
    const stops: Array<() => void> = [];
    const accept = (next: HandDockSnapshot) => {
      if (cancelled || next.generation < accepted.current) return;
      accepted.current = next.generation;
      next = { ...next, dockPoint: pointOf(next) };
      setLanguage(next.gui.language); document.documentElement.lang = currentLocale(); applyAppearance(next.gui);
      current.current = next; setSnapshot(next); setHidden(false); setConfirm(false);
    };
    const subscribe = async <E extends "hand-dock-state-changed" | "hand-dock-moved" | "hand-dock-hide" | "hand-busy-changed" | "hand-clear-finished" | "hand-error">(
      event: E, callback: Parameters<typeof onDesktopEvent<E>>[1]) => {
      const stop = await onDesktopEvent(event, callback);
      if (cancelled) stop(); else stops.push(stop);
    };
    void Promise.all([
      subscribe("hand-dock-state-changed", accept),
      subscribe("hand-dock-moved", (value) => {
        const old = current.current;
        if (!old || value.generation !== accepted.current) return;
        currentSurface.current = value.surface; setSurface(value.surface);
        current.current = { ...old, dockPoint: value.point }; setSnapshot(current.current);
      }),
      subscribe("hand-dock-hide", (generation) => {
        if (generation >= accepted.current) { accepted.current = generation; setHidden(true); }
      }),
      subscribe("hand-busy-changed", setBusy),
      subscribe("hand-clear-finished", (message) => { setBusy(false); setError(message ? errorText(message) : ""); }),
      subscribe("hand-error", (message) => setError(errorText(message))),
    ]).then(async () => {
      if (!cancelled) { const next = await call<HandDockSnapshot | null>("hand_dock_ready"); if (next) accept(next); }
    }).catch((e) => setError(errorText(e)));
    return () => {
      cancelled = true; stops.forEach((stop) => stop());
      if (confirmTimer.current) clearTimeout(confirmTimer.current);
    };
  }, []);

  useLayoutEffect(() => {
    if (!snapshot || hidden || movingRef.current) return;
    let cancelled = false;
    void call<HandSurface>("position_hand_dock", { generation: snapshot.generation, point: snapshot.dockPoint }).then((value) => {
      if (!cancelled) { currentSurface.current = value; setSurface(value); }
    }).catch((e) => { if (!cancelled) setError(errorText(e)); });
    return () => { cancelled = true; };
  }, [snapshot?.generation, snapshot?.gui.font_size, snapshot?.dockPoint?.x, snapshot?.dockPoint?.y, hidden, moving]);

  useLayoutEffect(() => {
    if (!snapshot || !surface || hidden || moving) return;
    let cancelled = false, frame = 0, measuring = false, dirty = false;
    let previous = "";
    const schedule = () => {
      dirty = true;
      if (!cancelled && !frame && !measuring) frame = requestAnimationFrame(measure);
    };
    const measure = async () => {
      frame = 0;
      if (cancelled || !dock.current) return;
      measuring = true; dirty = false;
      const rect = dock.current.getBoundingClientRect();
      const dpi = window.devicePixelRatio;
      const left = Math.floor(rect.left * dpi), top = Math.floor(rect.top * dpi);
      const right = Math.ceil(rect.right * dpi), bottom = Math.ceil(rect.bottom * dpi);
      const key = `${left},${top},${right},${bottom}`;
      if (key !== previous) {
        previous = key;
        await call("set_hand_dock_hit_regions", { generation: snapshot.generation,
          rects: [{ left: left / dpi, top: top / dpi, width: (right - left) / dpi, height: (bottom - top) / dpi }] })
          .catch((e) => { if (!cancelled) setError(errorText(e)); });
      }
      measuring = false;
      if (dirty || stage.current?.getAnimations({ subtree: true }).some((a) => a.playState === "running" || a.pending)) schedule();
    };
    const observer = new ResizeObserver(schedule);
    if (dock.current) observer.observe(dock.current);
    schedule();
    return () => { cancelled = true; cancelAnimationFrame(frame); observer.disconnect(); };
  }, [snapshot?.generation, surface, hidden, moving]);

  if (!snapshot || !snapshot.dockPoint) return null;
  const point = snapshot.dockPoint;
  const clear = () => {
    if (!confirm) {
      setConfirm(true);
      if (confirmTimer.current) clearTimeout(confirmTimer.current);
      confirmTimer.current = setTimeout(() => setConfirm(false), 3000);
    } else {
      setConfirm(false); setBusy(true); setError("");
      void requestHandClear().catch((e) => { setBusy(false); setError(errorText(e)); });
    }
  };
  return <div ref={stage} className="hand-stage hand-dock-stage" data-hidden={hidden || undefined}
    style={{ width: 1, height: 1, transform: surface ? `scale(${surface.scale}) translate(${-surface.left}px, ${-surface.top}px)` : undefined } as CSSProperties}>
    <HandDock element={dock} point={point} areas={snapshot.workAreas.length ? snapshot.workAreas : [snapshot.workArea]}
      expanded={snapshot.expanded} preview={false} diameter={44 * snapshot.gui.font_size / 14}
      count={snapshot.count} capacity={snapshot.settings.capacity} busy={busy} confirm={confirm} message={error}
      toCanvas={(x, y) => ({ x: (currentSurface.current?.left ?? 0) + x / (currentSurface.current?.scale ?? 1),
        y: (currentSurface.current?.top ?? 0) + y / (currentSurface.current?.scale ?? 1) })}
      beginDrag={() => Promise.resolve()}
      nativeDrag={() => call<HandPoint>("begin_hand_dock_drag", { generation: current.current!.generation })}
      dragging={(value) => { movingRef.current = value; setMoving(value); }}
      move={(next) => {
        current.current = { ...current.current!, dockPoint: next }; setSnapshot(current.current);
        void call("save_hand_dock", { generation: current.current.generation, point: next }).catch((e) => setError(errorText(e)));
      }}
      toggle={() => { setError(""); void call("toggle_hand_expanded").catch((e) => setError(errorText(e))); }} clear={clear} />
  </div>;
}
