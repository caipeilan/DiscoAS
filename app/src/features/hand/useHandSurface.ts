import { useLayoutEffect, useRef } from "react";
import type { HandRect, HandSnapshot, HandSurface } from "../../types";
import { call } from "../../services/desktop";
import { errorText } from "../../i18n";
import { combineHandBounds } from "./handDrawingBounds";
import { LatestRegionUpdate, HandBoundsCache } from "./regionUpdates";
import type { DockPoint } from "./floatingDock";
import type { Ref, Setter, Drag, Flying, Discarding, ArrivalDrawing } from "./handControllerTypes";
const sameRect = (a: HandRect | null | undefined, b: HandRect | null | undefined) => a === b || !!a && !!b &&
  a.left === b.left && a.top === b.top && a.width === b.width && a.height === b.height;
const sameSurface = (a: HandSurface | undefined, b: HandSurface | undefined) => sameRect(a, b) && a?.scale === b?.scale;
interface SurfaceContext {
  snapshot: HandSnapshot; current: Ref<HandSnapshot>; setSnapshot: Setter<HandSnapshot>; mounted: Ref<boolean>;
  stage: Ref<HTMLDivElement | null>; toolbarElement: Ref<HTMLDivElement | null>; toolbarBounds: Ref<HandRect | null>;
  dragRef: Ref<Drag | null>; drawingBounds: Ref<HandRect | null>; arrivalDrawing: Ref<ArrivalDrawing | null>; dragDrawing: Ref<HandRect | null>;
  dockMoving: boolean; phase: string; active: string | null; drag: Drag | null; flying: Flying | null; discarding: Discarding[];
  busy: string; error: string; clearConfirm: boolean; hidden: boolean; cardsMounted: boolean; dockPoint: {x:number;y:number} | null;
  setError: Setter<string>;
  layoutKey: string;
}
/** Serializes native regions and acknowledges the committed coordinate layout before presentation. */
export function useHandSurface(context: SurfaceContext) {
  const { snapshot, current, setSnapshot, mounted, stage, toolbarElement, toolbarBounds, dragRef, drawingBounds,
    arrivalDrawing, dragDrawing, dockMoving, phase, active, drag, flying, discarding, busy, error, clearConfirm,
    hidden, cardsMounted, dockPoint, setError, layoutKey } = context;
  const renderedSurface = useRef(snapshot.surface); renderedSurface.current = snapshot.surface;
  const surfaceLayout = useRef<{ surface: HandSurface; promise: Promise<void>; complete: () => void } | null>(null);
  const reportedRegions = useRef<{ generation: number; dragging: boolean; rects: HandRect[]; drawingBounds: HandRect | null } | null>(null);
  const scheduleRegions = useRef<() => void>(() => {});
  const commitRegions = useRef<() => Promise<unknown>>(() => Promise.resolve());
  const regionWork = useRef<() => Promise<unknown>>(() => Promise.resolve());
  const regionQueue = useRef<LatestRegionUpdate | null>(null);
  if (!regionQueue.current) regionQueue.current = new LatestRegionUpdate(() => regionWork.current());
  const boundsCache = useRef(new HandBoundsCache());
  const toCanvas = (x: number, y: number): DockPoint => {
    const surface = renderedSurface.current;
    return { x: (surface?.left ?? 0) + x / (surface?.scale ?? 1), y: (surface?.top ?? 0) + y / (surface?.scale ?? 1) };
  };
  const applySurface = (surface: HandSurface): Promise<void> => {
    if (sameSurface(current.current.surface, surface)) return surfaceLayout.current?.promise ?? Promise.resolve();
    surfaceLayout.current?.complete();
    let complete = () => {};
    const promise = new Promise<void>((resolve) => { complete = resolve; });
    surfaceLayout.current = { surface, promise, complete };
    current.current = { ...current.current, surface };
    setSnapshot(current.current);
    return promise;
  };
  commitRegions.current = () => {
    const generation = current.current.generation;
    const task = async () => {
      if (surfaceLayout.current) await surfaceLayout.current.promise;
      if (!mounted.current || !generation || current.current.generation !== generation) return;
      const state = current.current;
      if (toolbarElement.current) {
        const rect = toolbarElement.current.getBoundingClientRect(), point = toCanvas(rect.left, rect.top);
        toolbarBounds.current = { left: point.x, top: point.y, width: rect.width / (renderedSurface.current?.scale ?? 1), height: rect.height / (renderedSurface.current?.scale ?? 1) };
      }
      const elements = stage.current?.querySelectorAll<HTMLElement>(state.preview ? "[data-hand-close]" : "[data-hand-hit]") || [];
      const dpi = window.devicePixelRatio, scale = renderedSurface.current?.scale ?? 1;
      const dragging = !!dragRef.current || dockMoving;
      const rects = Array.from(elements).filter((element) => !element.classList.contains("hand-pending"))
        .map((element) => boundsCache.current.read(element, element === dragRef.current?.element)).map((rect) => {
          const left = Math.floor(rect.left * dpi) / dpi, top = Math.floor(rect.top * dpi) / dpi;
          const point = toCanvas(left, top);
          return { left: point.x, top: point.y, width: (Math.ceil(rect.right * dpi) / dpi - left) / scale,
            height: (Math.ceil(rect.bottom * dpi) / dpi - top) / scale };
        });
      if (dragging) dragDrawing.current = combineHandBounds([dragDrawing.current, ...rects]);
      const bounds = state.preview ? null : combineHandBounds([drawingBounds.current,
        arrivalDrawing.current?.generation === generation ? arrivalDrawing.current.bounds : null,
        dragging ? dragDrawing.current : null, ...rects]);
      const next = { generation, rects, dragging, drawingBounds: bounds };
      const old = reportedRegions.current;
      if (old?.generation === generation && old.dragging === dragging && sameRect(old.drawingBounds, bounds) &&
        old.rects.length === rects.length && rects.every((rect, index) => sameRect(rect, old.rects[index]))) return;
      const surface = await call<HandSurface | null>("set_hand_hit_regions", next);
      if (!mounted.current || current.current.generation !== generation) return;
      if (surface) await applySurface(surface);
      reportedRegions.current = next;
    };
    regionWork.current = task;
    return regionQueue.current!.run();
  };
  useLayoutEffect(() => {
    const pending = surfaceLayout.current;
    if (pending && sameSurface(snapshot.surface, pending.surface)) {
      surfaceLayout.current = null;
      pending.complete();
    }
  }, [snapshot.surface?.left, snapshot.surface?.top, snapshot.surface?.width, snapshot.surface?.height, snapshot.surface?.scale]);

  useLayoutEffect(() => {
    if (!snapshot.generation || hidden) return;
    boundsCache.current.reset();
    let cancelled = false, frame = 0, measuring = false, dirty = false;
    const schedule = () => {
      dirty = true;
      if (!cancelled && !frame && !measuring) frame = requestAnimationFrame(measure);
    };
    const measure = async () => {
      frame = 0;
      if (cancelled) return;
      measuring = true; dirty = false;
      await commitRegions.current().catch((e) => { if (!cancelled) setError(errorText(e)); });
      measuring = false;
      // Animated geometry is committed by the state change that starts it and
      // by the transition/animation-end hooks. Polling every animation frame
      // forced a DOM read plus native region IPC exactly while the game was
      // competing for the same frame budget.
      if (!cancelled && dirty) schedule();
    };
    scheduleRegions.current = schedule;
    const observer = new ResizeObserver(schedule);
    if (toolbarElement.current) observer.observe(toolbarElement.current);
    void measure();
    return () => { cancelled = true; cancelAnimationFrame(frame); observer.disconnect(); if (scheduleRegions.current === schedule) scheduleRegions.current = () => {}; };
  }, [snapshot.generation, snapshot.preview, snapshot.expanded, snapshot.cards, snapshot.settings, snapshot.gui.font_size,
    snapshot.workArea, snapshot.surface?.left, snapshot.surface?.top, snapshot.surface?.width, snapshot.surface?.height, snapshot.surface?.scale,
    phase, active, drag?.id, drag?.index, drag?.intake, flying, discarding, busy, error, clearConfirm, hidden, cardsMounted, dockPoint, dockMoving, layoutKey]);

  useLayoutEffect(() => () => {
    surfaceLayout.current?.complete(); surfaceLayout.current = null;
  }, []);
  return { toCanvas, commitRegions, scheduleRegions };
}
