import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import type { HandDockSnapshot, HandPoint, HandSurface } from "../../types";
import { call, onDesktopEvent, requestHandClear, requestHandDiscoveryRefresh } from "../../services/desktop";
import { applyAppearance } from "../../appearance";
import { currentLocale, errorText, setLanguage } from "../../i18n";
import { defaultDockPoint, settleDockPoint } from "./floatingDock";
import { handLayout } from "./handLayout";
import { HandDock } from "./HandDock";
import "../../App.css";
import "./hand.css";
import { useAppearance } from "../settings/useAppearance";
import { defaultGuiSettings } from "../../types";
import { useMediaSession } from "./useMediaSession";
import { useAutoHandControl } from "./useAutoHandControl";
import { changesDockBounds } from "./handRegionEvents";

export function HandDockWindow() {
  const [snapshot, setSnapshot] = useState<HandDockSnapshot | null>(null);
  useAppearance(snapshot?.gui ?? defaultGuiSettings);
  const [surface, setSurface] = useState<HandSurface | null>(null);
  const [hidden, setHidden] = useState(true);
  const auto = useAutoHandControl();
  const media = useMediaSession(!hidden && auto.ready, auto.state);
  const [moving, setMoving] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState("");
  const [expandedWidth, setExpandedWidth] = useState(198);
  const movingRef = useRef(false), accepted = useRef(0);
  const current = useRef(snapshot), currentSurface = useRef(surface);
  if (!movingRef.current) { current.current = snapshot; currentSurface.current = surface; }
  const stage = useRef<HTMLDivElement>(null), dock = useRef<HTMLDivElement>(null);
  const motion = useRef<((point: HandPoint) => void) | null>(null);
  const previousRegions = useRef("");
  const confirmTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pointOf = (next: HandDockSnapshot) => settleDockPoint(next.dockPoint ?? current.current?.dockPoint ?? defaultDockPoint(
    handLayout(next.count, next.settings, next.workArea, -1, next.gui.fontSize),
    next.settings.enabled ? next.settings.side : next.discoverySettings?.side ?? next.settings.side, next.workArea, 44 * next.gui.fontSize / 14), 44 * next.gui.fontSize / 14, next.workAreas.length ? next.workAreas : [next.workArea]);

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
    const subscribe = async <E extends "hand-dock-state-changed" | "hand-dock-moved" | "hand-dock-position-changed" | "hand-dock-hide" | "hand-busy-changed" | "hand-clear-finished" | "hand-error">(
      event: E, callback: Parameters<typeof onDesktopEvent<E>>[1]) => {
      const stop = await onDesktopEvent(event, callback);
      if (cancelled) stop(); else stops.push(stop);
    };
    void Promise.all([
      subscribe("hand-dock-state-changed", accept),
      subscribe("hand-dock-moved", (value) => {
        const old = current.current;
        if (!old || !movingRef.current || value.generation !== accepted.current) return;
        currentSurface.current = value.surface;
        current.current = { ...old, dockPoint: value.point };
        if (stage.current) stage.current.style.transform = `scale(${value.surface.scale}) translate(${-value.surface.left}px, ${-value.surface.top}px)`;
        motion.current?.(value.point);
      }),
      subscribe("hand-dock-position-changed", (value) => {
        if (current.current && value.generation === accepted.current && !movingRef.current) {
          current.current = { ...current.current, dockPoint: value.point }; setSnapshot(current.current);
        }
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
    void call<HandSurface>("position_hand_dock", { generation: snapshot.generation, point: snapshot.dockPoint, width: expandedWidth }).then((value) => {
      if (!cancelled) { currentSurface.current = value; setSurface(value); }
    }).catch((e) => { if (!cancelled) setError(errorText(e)); });
    return () => { cancelled = true; };
  }, [snapshot?.generation, snapshot?.gui.fontSize, snapshot?.gui.playbackCoverScale, snapshot?.dockPoint?.x, snapshot?.dockPoint?.y, hidden, moving, expandedWidth]);

  useLayoutEffect(() => {
    if (moving) previousRegions.current = "";
    if (!snapshot || !surface || hidden || moving) return;
    let cancelled = false, frame = 0, measuring = false, dirty = false;
    const schedule = () => {
      dirty = true;
      if (!cancelled && !frame && !measuring) frame = requestAnimationFrame(measure);
    };
    const measure = async () => {
      frame = 0;
      if (cancelled || !dock.current) return;
      measuring = true; dirty = false;
      const dpi = window.devicePixelRatio;
      const regions = [], drawingRects = [];
      let animating = false;
      for (const node of stage.current!.querySelectorAll<HTMLElement>(".hand-dock, .hand-playback, [data-hand-hit]")) {
        const rect = node.getBoundingClientRect();
        const left = Math.floor(rect.left * dpi), top = Math.floor(rect.top * dpi);
        const bounds = { left: left / dpi, top: top / dpi, width: (Math.ceil(rect.right * dpi) - left) / dpi, height: (Math.ceil(rect.bottom * dpi) - top) / dpi };
        if (node.matches("[data-hand-hit]:not([data-folding])")) regions.push(bounds);
        if (node.matches(".hand-dock, .hand-playback")) {
          drawingRects.push(bounds);
          animating ||= node.getAnimations().some(changesDockBounds);
        }
      }
      const key = JSON.stringify([snapshot.generation, surface.scale, dpi, regions, drawingRects]);
      if (key !== previousRegions.current) {
        await call("set_hand_dock_hit_regions", { generation: snapshot.generation,
          rects: regions, drawingRects })
          .then(() => { if (!cancelled) previousRegions.current = key; })
          .catch((e) => { if (!cancelled) setError(errorText(e)); });
      }
      measuring = false;
      if (dirty || animating) schedule();
    };
    const observer = new ResizeObserver(schedule);
    for (const node of stage.current!.querySelectorAll<HTMLElement>(".hand-dock, .hand-playback")) observer.observe(node);
    const mutations = new MutationObserver(schedule);
    mutations.observe(stage.current!, { subtree: true, attributes: true, attributeFilter: ["data-open", "data-folding", "data-side", "data-dock-expanded"] });
    schedule();
    return () => { cancelled = true; cancelAnimationFrame(frame); observer.disconnect(); mutations.disconnect(); };
  }, [snapshot?.generation, surface, hidden, moving]);

  if (!snapshot || !snapshot.dockPoint) return null;
  const point = moving ? current.current?.dockPoint ?? snapshot.dockPoint : snapshot.dockPoint;
  const activeSurface = moving ? currentSurface.current : surface;
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
    style={{ width: 1, height: 1, transform: activeSurface ? `scale(${activeSurface.scale}) translate(${-activeSurface.left}px, ${-activeSurface.top}px)` : undefined } as CSSProperties}>
    <HandDock element={dock} motion={motion} media={media} autoControl={auto} mysteryCover={snapshot.mysteryCover} coverScale={snapshot.gui.playbackCoverScale} point={point} areas={snapshot.workAreas.length ? snapshot.workAreas : [snapshot.workArea]}
      expanded={snapshot.expanded} preview={false} diameter={44 * snapshot.gui.fontSize / 14}
      count={snapshot.count} capacity={snapshot.settings.capacity} busy={busy} confirm={confirm} message={error}
      handEnabled={snapshot.settings.enabled} createCards={snapshot.settings.createCards} discovery={snapshot.discovery} refresh={() => { setError(""); void requestHandDiscoveryRefresh().catch(e => setError(errorText(e))); }}
      onWidthChange={setExpandedWidth}
      toCanvas={(x, y) => ({ x: (currentSurface.current?.left ?? 0) + x / (currentSurface.current?.scale ?? 1),
        y: (currentSurface.current?.top ?? 0) + y / (currentSurface.current?.scale ?? 1) })}
      beginDrag={() => Promise.resolve()}
      nativeDrag={(cursor) => call<HandPoint>("begin_hand_dock_drag", { generation: current.current!.generation, cursor })}
      dragging={(value) => {
        if (!value) setSurface(currentSurface.current);
        movingRef.current = value; setMoving(value);
      }}
      move={(next) => {
        current.current = { ...current.current!, dockPoint: next }; setSnapshot(current.current);
        void call("save_hand_dock", { generation: current.current.generation, point: next }).catch((e) => setError(errorText(e)));
      }}
      toggle={() => { setError(""); void call("toggle_hand_expanded").catch((e) => setError(errorText(e))); }} clear={clear} />
  </div>;
}
