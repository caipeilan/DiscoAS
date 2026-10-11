import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import type { HandCard, HandSnapshot, HandKey, HandRect, HandSurface } from "../../types";
import { defaultGuiSettings, defaultHandSettings, emptyState } from "../../types";
import { call, desktop, onDesktopEvent, notifyHandBusy, notifyHandClear } from "../../services/desktop";
import { applyAppearance } from "../../appearance";
import { currentLocale, errorText, setLanguage, t } from "../../i18n";
import { keybindingFromEvent } from "../settings/discoveryKeybindings";
import { Cover } from "../../components/Cover";
import { Icon } from "../../Icon";
import { handCardSize, mergeHandCards, handLayout, canPlayDrag, reorderIndex, reorderCards, hoverHandIndex, type CardPose } from "./handLayout";
import { HandDock } from "./HandDock";
import { defaultDockPoint, floatingDockRect, type DockPoint } from "./floatingDock";
import { arrivalDrawingBounds, combineHandBounds, handPoseBounds } from "./handDrawingBounds";
import "../../App.css";
import "./hand.css";

const fallback: HandSnapshot = {
  generation: 0, cards: [], settings: defaultHandSettings, gui: defaultGuiSettings,
  keys: emptyState.settings.discovery_keybindings, workArea: { left: 0, top: 0, width: window.innerWidth, height: window.innerHeight },
  preview: false, focus: false, expanded: false, arrival: null,
};
interface Drag { id: string; index: number; playable: boolean; pointerId: number; startX: number; startY: number; dx: number; dy: number; pose: CardPose; element: HTMLElement }
type Flying = { card: HandCard; pose: CardPose; complete: () => void };
type Discarding = { card: HandCard; pose: CardPose; delay: number; complete: () => void };
const restTransform = "translate(-50%, -50%) translate(var(--hand-x, 0px), var(--hand-y, 0px)) translate(var(--card-x, 0px), var(--card-y, 0px)) rotate(var(--drag-angle, var(--hand-angle, 0deg))) scale(var(--hand-card-scale, 1))";
const sameRect = (a: HandRect | null | undefined, b: HandRect | null | undefined) => a === b || !!a && !!b &&
  a.left === b.left && a.top === b.top && a.width === b.width && a.height === b.height;
const sameSurface = (a: HandSurface | undefined, b: HandSurface | undefined) => sameRect(a, b) && a?.scale === b?.scale;
const readPose = (element: HTMLElement): CardPose => {
  const style = getComputedStyle(element);
  const matrix = new DOMMatrixReadOnly(style.transform);
  const width = parseFloat(style.width), height = parseFloat(style.height);
  return { x: parseFloat(style.left) + width / 2 + matrix.e, y: parseFloat(style.top) + height / 2 + matrix.f,
    width, height, angle: Math.atan2(matrix.b, matrix.a) * 180 / Math.PI, scale: Math.hypot(matrix.a, matrix.b) };
};
const HandCardContent = memo(function HandCardContent({ card }: { card: HandCard }) {
  return <>
    <div className="hand-art"><Cover url={card.song.albumPicUrl} data={card.song.coverDataUri} prepared mystery={card.song.mysteryMode} label={card.song.name} /></div>
    <strong>{card.song.mysteryMode ? "???" : card.song.name}</strong>
    <span>{card.song.mysteryMode ? "???" : card.song.artistNames.join(" / ")}</span>
  </>;
});

export function HandWindow() {
  const [snapshot, setSnapshot] = useState(fallback);
  const [phase, setPhase] = useState("closed");
  const [hidden, setHidden] = useState(true);
  const [cardsMounted, setCardsMounted] = useState(false);
  const [hover, setHover] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [inputMode, setInputMode] = useState<"pointer" | "keyboard">("pointer");
  const [drag, setDrag] = useState<Drag | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [clearConfirm, setClearConfirm] = useState(false);
  const [flying, setFlying] = useState<Flying | null>(null);
  const [discarding, setDiscarding] = useState<Discarding[]>([]);
  const discardingRef = useRef<Discarding[]>([]);
  const [dockPoint, setDockPoint] = useState<DockPoint | null>(null);
  const [dockMoving, setDockMoving] = useState(false);
  const flyingRef = useRef<Flying | null>(null);
  const stage = useRef<HTMLDivElement>(null);
  const toolbarElement = useRef<HTMLDivElement>(null);
  const toolbarBounds = useRef<HandSnapshot["workArea"] | null>(null);
  const current = useRef(snapshot); current.current = snapshot;
  const renderedSurface = useRef(snapshot.surface); renderedSurface.current = snapshot.surface;
  const surfaceLayout = useRef<{ surface: HandSurface; promise: Promise<void>; complete: () => void } | null>(null);
  const dragRef = useRef<Drag | null>(null);
  const dragFrame = useRef(0);
  const busyRef = useRef(busy); busyRef.current = busy;
  const inputRef = useRef(inputMode); inputRef.current = inputMode;
  const selectedRef = useRef(selected); selectedRef.current = selected;
  const hoverRef = useRef(hover); hoverRef.current = hover;
  const active = inputMode === "keyboard" ? selected : hover;
  const cards = useMemo(() => drag ? reorderCards(snapshot.cards, drag.id, drag.index) : snapshot.cards, [snapshot.cards, drag?.id, drag?.index]);
  const activeIndex = cards.findIndex((c) => c.id === active);
  const cardSize = handCardSize(snapshot.gui.font_size);
  const restPoses = useMemo(() => handLayout(cards.length, snapshot.settings, snapshot.workArea, -1, snapshot.gui.font_size), [cards.length, snapshot.settings, snapshot.workArea, snapshot.gui.font_size]);
  const restPosesRef = useRef(restPoses); restPosesRef.current = restPoses;
  const diameter = 44 * snapshot.gui.font_size / 14;
  const dockAreas = useMemo(() => snapshot.workAreas?.length ? snapshot.workAreas : [snapshot.workArea], [snapshot.workAreas, snapshot.workArea]);
  const dockPosition = dockPoint ?? defaultDockPoint(restPoses, snapshot.settings.side, snapshot.workArea, diameter);
  const toolbar = floatingDockRect(dockPosition, snapshot.expanded || snapshot.preview, diameter, dockAreas);
  if (desktop && !snapshot.preview) toolbarBounds.current = toolbar;
  const poses = useMemo(() => handLayout(cards.length, snapshot.settings, snapshot.workArea, activeIndex, snapshot.gui.font_size),
    [cards.length, snapshot.settings, snapshot.workArea, activeIndex, snapshot.gui.font_size]);
  const posesRef = useRef(poses); posesRef.current = poses;
  const layoutDrawingBounds = useMemo(() => {
    const all = [...restPoses];
    for (let index = 0; index < cards.length; index++)
      all.push(...handLayout(cards.length, snapshot.settings, snapshot.workArea, index, snapshot.gui.font_size));
    const folded = restPoses.map((pose) => ({ ...pose, angle: snapshot.settings.side === "left" ? -20 : snapshot.settings.side === "right" ? 20 : 0,
      scale: .75, x: pose.x + (snapshot.settings.side === "left" ? -pose.width : snapshot.settings.side === "right" ? pose.width : 0),
      y: pose.y + (snapshot.settings.side === "bottom" ? pose.height : 0) }));
    return combineHandBounds([...all, ...folded].map((pose) => handPoseBounds(pose)));
  }, [restPoses, cards.length, snapshot.settings, snapshot.workArea, snapshot.gui.font_size]);
  const drawingBounds = useRef<HandRect | null>(null);
  drawingBounds.current = combineHandBounds([
    cardsMounted ? layoutDrawingBounds : null,
    ...discarding.map((item) => combineHandBounds([handPoseBounds(item.pose), handPoseBounds({ ...item.pose, y: item.pose.y + 14 })])),
    flying ? handPoseBounds(flying.pose) : null,
  ]);
  const accepted = useRef(0);
  const arrivalPlayed = useRef(0);
  const arrivalMotion = useRef<{ generation: number; element: HTMLElement; animation: Animation | null } | null>(null);
  const arrivalDrawing = useRef<{ generation: number; bounds: HandRect } | null>(null);
  const dragDrawing = useRef<HandRect | null>(null);
  const returning = useRef<string[]>([]);
  const pointer = useRef<{ x: number; y: number } | null>(null);
  const mounted = useRef(false);
  const unfolding = useRef(true);
  const regionTask = useRef<Promise<unknown>>(Promise.resolve());
  const reportedRegions = useRef<{ generation: number; dragging: boolean; rects: HandRect[]; drawingBounds: HandRect | null } | null>(null);
  const scheduleRegions = useRef<() => void>(() => {});
  const commitRegions = useRef<() => Promise<unknown>>(() => Promise.resolve());
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());
  const later = (f: () => void, ms: number) => {
    const timer = setTimeout(() => { timers.current.delete(timer); f(); }, ms);
    timers.current.add(timer);
  };
  const finishFlight = () => {
    flyingRef.current?.complete(); flyingRef.current = null; setFlying(null);
  };
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
    const task = regionTask.current.catch(() => {}).then(async () => {
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
        .map((element) => element.getBoundingClientRect()).map((rect) => {
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
    });
    regionTask.current = task;
    return task;
  };
  const animateDiscard = (targets: HandCard[]) => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return Promise.resolve();
    const flights = targets.map((card, index) => new Promise<void>((complete) => {
      const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${card.id}"]`);
      if (!element) { complete(); return; }
      discardingRef.current.push({ card, pose: readPose(element), delay: Math.min(index * 15, 75), complete });
    }));
    setDiscarding([...discardingRef.current]);
    return Promise.all(flights);
  };
  const finishDiscards = () => {
    discardingRef.current.forEach((item) => item.complete()); discardingRef.current = []; setDiscarding([]);
  };

  const accept = (next: HandSnapshot) => {
    if (!mounted.current || next.generation < accepted.current) return;
    const merged = mergeHandCards(current.current.cards, next.cards, next.cardOrder);
    if (!merged) {
      // An update can reach a newly mounted WebView before its initial snapshot reply.
      void call<HandSnapshot | null>("hand_ready").then((full) => { if (full) accept(full); }).catch((e) => setError(errorText(e)));
      return;
    }
    next = { ...next, cards: merged, cardOrder: undefined };
    if (current.current.expanded && next.expanded && !unfolding.current && accepted.current && !next.arrival) returning.current.push(
      ...next.cards.filter((card) => !current.current.cards.some((old) => old.id === card.id)).map((c) => c.id));
    if (next.gui !== current.current.gui) {
      setLanguage(next.gui.language); document.documentElement.lang = currentLocale(); applyAppearance(next.gui);
    }
    accepted.current = next.generation;
    current.current = next;
    setSnapshot(next); setHidden(false); setClearConfirm(false);
    setDockPoint((previous) => next.dockPoint ?? previous ?? defaultDockPoint(
      handLayout(next.cards.length, next.settings, next.workArea, -1, next.gui.font_size),
      next.settings.side, next.workArea, 44 * next.gui.font_size / 14));
    if (!next.expanded) {
      cancelDrag(); setPhase("closed"); unfolding.current = true;
      hoverRef.current = null; selectedRef.current = null; pointer.current = null;
      setHover(null); setSelected(null); setInputMode("pointer"); inputRef.current = "pointer";
    } else {
      setCardsMounted(true);
      if (document.activeElement?.closest(".hand-toolbar")) stage.current?.focus({ preventScroll: true });
    }
    if (next.focus && next.expanded) {
      inputRef.current = "keyboard"; setInputMode("keyboard");
      selectedRef.current = next.cards[0]?.id ?? null; setSelected(selectedRef.current);
    }
    else if (!next.cards.some((c) => c.id === selectedRef.current)) setSelected(null);
    if (!next.cards.some((c) => c.id === hoverRef.current)) setHover(null);
  };

  const operation = async (name: "play_hand_card" | "discard_hand_card", card: HandCard, pose?: CardPose) => {
    if (busyRef.current || current.current.preview) return;
    busyRef.current = card.id; setBusy(card.id); setError("");
    let flight: Promise<unknown> = name === "discard_hand_card" ? animateDiscard([card]) : Promise.resolve();
    if (name === "play_hand_card" && pose && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      flight = new Promise<void>((complete) => {
        flyingRef.current = { card, pose, complete }; setFlying(flyingRef.current);
      });
    }
    try {
      await call(name, { id: card.id });
      await flight;
    } catch (e) {
      await flight;
      returning.current.push(card.id); setError(errorText(e));
    }
    finally { finishDiscards(); busyRef.current = ""; setBusy(""); }
  };
  const clearHand = async () => {
    if (busyRef.current || current.current.preview) { if (desktop) void notifyHandClear(null); return; }
    const targets = current.current.cards;
    if (!targets.length) { if (desktop) void notifyHandClear(null); return; }
    busyRef.current = "clear"; setBusy("clear"); setError("");
    const flight = animateDiscard(targets);
    let message: string | null = null;
    try { await call("clear_hand"); await flight; }
    catch (e) { await flight; returning.current.push(...targets.map((card) => card.id)); message = errorText(e); setError(message); }
    finally { finishDiscards(); busyRef.current = ""; setBusy(""); if (desktop) void notifyHandClear(message); }
  };
  const clearHandRef = useRef(clearHand); clearHandRef.current = clearHand;
  useEffect(() => { if (desktop && !snapshot.preview) void notifyHandBusy(!!busy); }, [busy, snapshot.preview]);
  const keyboard = (event: KeyboardEvent | HandKey) => {
    const state = current.current;
    if (!state.expanded || dockMoving) return;
    if (event instanceof KeyboardEvent && desktop) return;
    if (!state.preview && document.activeElement?.closest(".hand-toolbar button")) return;
    if (event.key === "Escape") {
      event instanceof KeyboardEvent && event.preventDefault();
      if (dragRef.current) { cancelDrag(); return; }
      void call(state.preview ? "end_hand_preview" : "toggle_hand_expanded"); return;
    }
    if (busyRef.current || dragRef.current || !state.cards.length || ("isComposing" in event && event.isComposing)) return;
    let action: string | undefined;
    if ("action" in event) action = event.action;
    else {
      const binding = keybindingFromEvent(event);
      action = Object.entries(state.keys).find(([, value]) => value === binding)?.[0];
      if (!action) action = ({ ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down", Enter: "select", Delete: "discard" } as Record<string,string>)[event.key];
    }
    if (!action || action === "replace") return;
    event instanceof KeyboardEvent && event.preventDefault();
    const index = state.cards.findIndex((c) => c.id === selectedRef.current);
    setInputMode("keyboard"); inputRef.current = "keyboard";
    if (action === "select" || action === "discard") {
      if (index >= 0 && !event.repeat) {
        const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${state.cards[index].id}"]`);
        void operation(action === "select" ? "play_hand_card" : "discard_hand_card", state.cards[index], element ? readPose(element) : posesRef.current[index]);
      }
      else if (index < 0) setSelected(state.cards[0].id);
    } else {
      const next = index < 0 ? 0 : Math.max(0, Math.min(state.cards.length - 1, index + (["left", "up"].includes(action) ? -1 : 1)));
      selectedRef.current = state.cards[next].id; setSelected(state.cards[next].id);
    }
  };
  const keyboardRef = useRef(keyboard); keyboardRef.current = keyboard;

  useEffect(() => {
    mounted.current = true;
    let cancelled = false; const cleanups: Array<() => void> = [];
    const listen = async <E extends "hand-state-changed" | "hand-visibility-changed" | "hand-surface-changed" | "hand-dock-position-changed" | "hand-clear-requested" | "hand-hide" | "hand-pointer" | "hand-key" | "hand-error" | "hand-escape">(event: E, handler: Parameters<typeof onDesktopEvent<E>>[1]) => {
      const stop = await onDesktopEvent(event, handler); if (cancelled) stop(); else cleanups.push(stop);
    };
    void Promise.all([
      listen("hand-state-changed", accept),
      listen("hand-visibility-changed", (value) => accept({ ...current.current, ...value, arrival: null })),
      listen("hand-surface-changed", ({ generation, surface }) => {
        if (generation === accepted.current) void applySurface(surface);
      }),
      listen("hand-dock-position-changed", (value) => {
        if (value.generation !== accepted.current) return;
        current.current = { ...current.current, dockPoint: value.point }; setSnapshot(current.current); setDockPoint(value.point);
      }),
      listen("hand-clear-requested", () => { void clearHandRef.current(); }),
      listen("hand-hide", (generation) => { if (generation >= accepted.current) { accepted.current = generation; setHidden(true); setPhase("closed"); unfolding.current = true; finishFlight(); finishDiscards(); cancelDrag(); } }),
      listen("hand-error", (e) => setError(errorText(e))),
      listen("hand-key", (e) => keyboardRef.current(e)),
      listen("hand-escape", () => {
        if (dragRef.current) cancelDrag();
        else if (current.current.expanded) void call(current.current.preview ? "end_hand_preview" : "toggle_hand_expanded");
      }),
      listen("hand-pointer", ([x, y]) => {
        if (dragRef.current || !mounted.current || !current.current.expanded) return;
        const previous = pointer.current; pointer.current = { x, y };
        if (previous && Math.abs(previous.x-x) + Math.abs(previous.y-y) < 1) return;
        if (previous && inputRef.current === "keyboard") { setInputMode("pointer"); inputRef.current = "pointer"; }
        const state = current.current;
        const oldIndex = state.cards.findIndex((c) => c.id === hoverRef.current);
        const bar = toolbarBounds.current;
        const overToolbar = bar && x >= bar.left && x <= bar.left+bar.width && y >= bar.top && y <= bar.top+bar.height;
        const index = overToolbar ? -1 : hoverHandIndex(restPosesRef.current, state.settings.side, x, y, posesRef.current[oldIndex] ?? null, oldIndex);
        const id = state.cards[index]?.id ?? null;
        if (hoverRef.current !== id) { hoverRef.current = id; setHover(id); }
      }),
    ]).then(async () => { if (!cancelled) { const next = await call<HandSnapshot | null>("hand_ready"); if (next) accept(next); } }).catch((e) => setError(errorText(e)));
    const key = (e: KeyboardEvent) => keyboardRef.current(e);
    window.addEventListener("keydown", key);
    return () => { mounted.current = false; cancelled = true; surfaceLayout.current?.complete(); surfaceLayout.current = null; cancelAnimationFrame(dragFrame.current); flyingRef.current?.complete(); discardingRef.current.forEach((item) => item.complete()); cleanups.forEach((c) => c()); timers.current.forEach(clearTimeout); window.removeEventListener("keydown", key); };
  }, []);

  useLayoutEffect(() => {
    const pending = surfaceLayout.current;
    if (pending && sameSurface(snapshot.surface, pending.surface)) {
      surfaceLayout.current = null;
      pending.complete();
    }
  }, [snapshot.surface?.left, snapshot.surface?.top, snapshot.surface?.width, snapshot.surface?.height, snapshot.surface?.scale]);

  useLayoutEffect(() => {
    if (phase !== "closed" || !cardsMounted || (!hidden && snapshot.expanded)) return;
    let cancelled = false;
    const frame = requestAnimationFrame(() => {
      const animations = stage.current?.getAnimations({ subtree: true }) ?? [];
      void Promise.all(animations.map((a) => a.finished.catch(() => {}))).then(() => {
        if (!cancelled) setCardsMounted(false);
      });
    });
    return () => { cancelled = true; cancelAnimationFrame(frame); };
  }, [phase, snapshot.expanded, hidden, cardsMounted]);

  useLayoutEffect(() => {
    if (!snapshot.generation || hidden) return;
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
      if (!cancelled && (dirty || stage.current?.getAnimations({ subtree: true }).some((a) => a.playState === "running" || a.pending))) {
        schedule();
      }
    };
    scheduleRegions.current = schedule;
    const observer = new ResizeObserver(schedule);
    if (toolbarElement.current) observer.observe(toolbarElement.current);
    void measure();
    return () => { cancelled = true; cancelAnimationFrame(frame); observer.disconnect(); if (scheduleRegions.current === schedule) scheduleRegions.current = () => {}; };
  }, [snapshot.generation, snapshot.preview, snapshot.expanded, snapshot.cards, snapshot.settings, snapshot.gui.font_size,
    snapshot.workArea, snapshot.surface?.left, snapshot.surface?.top, snapshot.surface?.width, snapshot.surface?.height, snapshot.surface?.scale,
    phase, active, drag?.id, drag?.index, flying, discarding, busy, error, clearConfirm, hidden, cardsMounted, dockPoint, dockMoving]);

  const showCardWindow = snapshot.preview || (cardsMounted && (snapshot.cards.length > 0 || discarding.length > 0)) || !!flying;
  useEffect(() => {
    if (!snapshot.generation || hidden) return;
    let cancelled = false, frame = 0;
    const present = (async () => {
      if (!desktop) return;
      if (showCardWindow) await commitRegions.current();
      if (cancelled || snapshot.generation !== accepted.current) return;
      await call("present_hand", { generation: snapshot.generation, showCards: showCardWindow });
    })();
    void present.then(async () => {
      if (cancelled || snapshot.generation !== accepted.current) return;
      if (!snapshot.expanded || !showCardWindow) return;
      const arrival = arrivalMotion.current;
      if (arrival?.generation === snapshot.generation) {
        await Promise.all(Array.from(arrival.element.querySelectorAll("img")).map((image) => image.decode().catch(() => {})));
        if (cancelled || snapshot.generation !== accepted.current) return;
        frame = requestAnimationFrame(() => {
          if (cancelled) return;
          unfolding.current = false; setPhase("open");
          // Paint the receiving card at its origin before the discovery window hides.
          frame = requestAnimationFrame(() => {
            if (cancelled) return;
            void (desktop ? call("hand_arrival_ready", { generation: snapshot.generation }) : Promise.resolve()).then(() => {
              if (cancelled || snapshot.generation !== accepted.current) return;
              arrival.animation?.play();
              scheduleRegions.current();
              if (snapshot.cards.find((c) => c.id === snapshot.arrival?.id)?.mysteryRevealed && arrival.animation)
                arrival.element.querySelector(".hand-flip")?.animate([
                  { transform: "rotateY(180deg)" }, { transform: "rotateY(90deg)", offset: .5 }, { transform: "rotateY(0deg)" },
                ], { duration: 420, delay: 120, fill: "backwards" });
            }).catch((e) => setError(errorText(e)));
          });
        });
        return;
      }
      if (!unfolding.current) return;
      // Commit the closed pose before transitioning to the live CSS target.
      frame = requestAnimationFrame(() => { if (!cancelled) { unfolding.current = false; setPhase("open"); } });
    }).catch((e) => {
      setError(errorText(e));
      if (snapshot.arrival && desktop) void call("hand_arrival_ready", { generation: snapshot.generation, error: errorText(e) });
    });
    return () => { cancelled = true; cancelAnimationFrame(frame); };
  }, [snapshot.generation, snapshot.expanded, hidden, showCardWindow]);

  useLayoutEffect(() => {
    const arrival = snapshot.arrival;
    if (hidden || !cardsMounted || !arrival || snapshot.generation <= arrivalPlayed.current) return;
    const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${arrival.id}"]`);
    const pose = posesRef.current[snapshot.cards.findIndex((c) => c.id === arrival.id)];
    if (!element || !pose) return;
    arrivalPlayed.current = snapshot.generation;
    const x = arrival.rect.left + arrival.rect.width / 2;
    const y = arrival.rect.top + arrival.rect.height / 2;
    const animation = window.matchMedia("(prefers-reduced-motion: reduce)").matches ? null : element.animate([
      { transform: `translate(-50%, -50%) translate(${x}px, ${y}px) rotate(0deg) scale(${arrival.rect.width / pose.width})`, opacity: 1 },
      { transform: `translate(-50%, -50%) translate(${x*.45+pose.x*.55}px, ${y*.7+pose.y*.3-80}px) rotate(${pose.angle-8}deg) scale(.8)`, offset: .55 },
      { transform: restTransform, opacity: 1 },
    ], { duration: 480, easing: "cubic-bezier(.22,.75,.24,1)", fill: "none" });
    animation?.pause();
    if (animation) animation.currentTime = 0;
    const motion = { generation: snapshot.generation, element, animation };
    arrivalMotion.current = motion;
    arrivalDrawing.current = animation ? { generation: snapshot.generation, bounds: arrivalDrawingBounds(arrival.rect, pose) } : null;
    const finishArrival = () => {
      if (arrivalMotion.current !== motion) return;
      arrivalDrawing.current = null;
      scheduleRegions.current();
    };
    if (animation) void animation.finished.then(finishArrival, finishArrival);
    return () => {
      animation?.cancel();
      if (arrivalMotion.current === motion) {
        arrivalMotion.current = null;
        arrivalDrawing.current = null;
      }
    };
  }, [snapshot.generation, cardsMounted, hidden]);

  useLayoutEffect(() => {
    if (phase !== "open" || !returning.current.length) return;
    const ready = returning.current.filter((id) => id !== flying?.card.id && id !== busy);
    returning.current = returning.current.filter((id) => !ready.includes(id));
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    ready.forEach((id) => {
      const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${id}"]`);
      if (!element) return;
      const pose = poses[snapshot.cards.findIndex((c) => c.id === id)];
      const dx = snapshot.settings.side === "left" ? 150 : snapshot.settings.side === "right" ? -150 : 0;
      const dy = snapshot.settings.side === "bottom" ? -150 : 0;
      element.animate([{ transform: `translate(-50%,-50%) translate(${pose.x+dx}px,${pose.y+dy}px) scale(.85)`, opacity: .4 },
        { transform: restTransform, opacity: 1 }], { duration: 360, easing: "cubic-bezier(.18,.9,.28,1.15)" });
    });
  }, [snapshot, busy, flying, phase]);

  const paintDrag = () => {
    dragFrame.current = 0;
    const held = dragRef.current; if (!held) return;
    held.element.style.setProperty("--card-x", `${held.dx}px`);
    held.element.style.setProperty("--card-y", `${held.dy}px`);
    held.element.style.setProperty("--drag-angle", `${Math.max(-8, Math.min(8, held.dx*.05))}deg`);
    scheduleRegions.current();
  };
  const cancelDrag = () => {
    const held = dragRef.current; if (!held) return;
    cancelAnimationFrame(dragFrame.current); dragFrame.current = 0;
    held.element.style.setProperty("--card-x", "0px"); held.element.style.setProperty("--card-y", "0px");
    held.element.style.removeProperty("--drag-angle");
    dragRef.current = null; dragDrawing.current = null; setDrag(null);
    scheduleRegions.current();
    if (held.element.hasPointerCapture(held.pointerId)) held.element.releasePointerCapture(held.pointerId);
  };
  const endDrag = (event: React.PointerEvent) => {
    const held = dragRef.current; if (!held || event.pointerId !== held.pointerId) return;
    const state = current.current;
    const card = state.cards.find((c) => c.id === held.id);
    const moved = Math.hypot(held.dx, held.dy) > 8;
    const valid = canPlayDrag(state.settings.side, held.dx, held.dy, state.settings.scale);
    if (!card || !moved || state.preview) { cancelDrag(); return; }
    if (valid) { cancelAnimationFrame(dragFrame.current); paintDrag(); void operation("play_hand_card", card, readPose(held.element)); cancelDrag(); return; }
    const from = state.cards.findIndex((c) => c.id === held.id);
    const reordered = reorderCards(state.cards, held.id, held.index);
    if (from !== held.index) {
      current.current = { ...state, cards: reordered }; setSnapshot(current.current);
      void call("reorder_hand", { ids: reordered.map((c) => c.id) }).catch((e) => { setSnapshot(state); setError(errorText(e)); });
    }
    cancelDrag();
  };
  const surface = snapshot.surface;
  const visibleCards = [...snapshot.cards, ...discarding.filter((item) => !snapshot.cards.some((card) => card.id === item.card.id)).map((item) => item.card)];
  return <div ref={stage} tabIndex={-1} className={`hand-stage hand-${phase}`} data-side={snapshot.settings.side} data-preview={snapshot.preview || undefined}
    data-hidden={hidden || undefined} data-expanded={snapshot.expanded} data-input-mode={inputMode}
    style={{ "--hand-card-height": `${cardSize.height}px`, width: surface?.width, height: surface?.height,
      transform: surface ? `scale(${surface.scale}) translate(${-surface.left}px, ${-surface.top}px)` : undefined } as CSSProperties} onContextMenu={(e) => e.preventDefault()}>
    {(!desktop || snapshot.preview) && <HandDock element={toolbarElement} point={dockPosition} areas={dockAreas} expanded={snapshot.expanded} preview={snapshot.preview}
      diameter={diameter} count={snapshot.cards.length} capacity={snapshot.settings.capacity} busy={!!busy} confirm={clearConfirm}
      toCanvas={toCanvas} dragging={setDockMoving}
      beginDrag={() => Promise.resolve()}
      move={(point) => {
        current.current = { ...current.current, dockPoint: point }; setSnapshot(current.current); setDockPoint(point);
        void call("save_hand_dock", { generation: current.current.generation, point }).catch((e) => setError(errorText(e)));
      }}
      toggle={() => { void call(snapshot.preview ? "end_hand_preview" : "toggle_hand_expanded").catch((e) => setError(errorText(e))); }}
      clear={() => {
        if (!clearConfirm) { setClearConfirm(true); later(() => setClearConfirm(false), 3000); return; }
        setClearConfirm(false); void clearHand();
      }} />}
    {error && <div className="hand-error" role="alert" data-hand-hit style={{ left: toolbar.left,
      top: Math.max(snapshot.workArea.top+8, toolbar.top - 48) }} onClick={() => setError("")}>{error}</div>}
    {cardsMounted && visibleCards.map((card) => {
      const removing = discarding.find((item) => item.card.id === card.id);
      const i = cards.findIndex((candidate) => candidate.id === card.id);
      const pose = removing?.pose ?? poses[i]; const held = !removing && drag?.id === card.id ? drag : null;
      const playing = !!held && held.playable;
      const style = { "--hand-x": `${held ? held.pose.x : pose.x}px`, "--hand-y": `${held ? held.pose.y : pose.y}px`, width: pose.width, height: pose.height,
        "--hand-base-scale": pose.width / 156,
        zIndex: held ? 1000 : active === card.id ? 500 : i+1,
        "--card-x": `${held ? held.dx : 0}px`, "--card-y": `${held ? held.dy : 0}px`,
        "--hand-angle": `${held ? 0 : pose.angle}deg`,
        "--hand-card-scale": held ? 1.12 : pose.scale, "--hand-order": i, "--discard-delay": `${removing?.delay ?? 0}ms`,
      } as CSSProperties;
      return <div key={card.id} data-hand-id={card.id} data-hand-hit className={`hand-card${active === card.id ? " hand-active" : ""}${held ? " hand-dragging" : ""}${playing ? " hand-playable" : ""}${removing ? " hand-discarding" : ""}${!removing && (busy === card.id || flying?.card.id === card.id) ? " hand-pending" : ""}`}
        role="button" tabIndex={-1} aria-label={card.song.mysteryMode ? t("神秘歌曲") : card.song.name} aria-disabled={snapshot.preview || !!busy}
        style={style} onAnimationEnd={(event) => { if (event.target === event.currentTarget) removing?.complete(); }} onPointerDown={(e) => {
          if (e.button !== 0 || snapshot.preview || busyRef.current || (e.target as HTMLElement).closest("button")) return;
          e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId);
          const visualPose = readPose(e.currentTarget);
          e.currentTarget.focus({ preventScroll: true }); e.currentTarget.getAnimations().forEach((animation) => animation.cancel());
          const point = toCanvas(e.clientX, e.clientY);
          const held: Drag = { id: card.id, index: snapshot.cards.findIndex((c) => c.id === card.id), playable: false, pointerId: e.pointerId, startX: point.x, startY: point.y, dx: 0, dy: 0, pose: visualPose, element: e.currentTarget };
          dragDrawing.current = handPoseBounds(visualPose);
          dragRef.current = held; setDrag(held); setInputMode("pointer"); setHover(card.id); setSelected(card.id);
        }} onPointerMove={(e) => {
          const held = dragRef.current; if (!held || held.id !== card.id) return;
          const point = toCanvas(e.clientX, e.clientY);
          const dx = point.x-held.startX, dy = point.y-held.startY;
          const index = reorderIndex(restPosesRef.current, snapshot.settings.side, held.pose.x+dx, held.pose.y+dy);
          const playable = canPlayDrag(snapshot.settings.side, dx, dy, snapshot.settings.scale);
          const changed = held.index !== index || held.playable !== playable;
          held.dx = dx; held.dy = dy; held.index = index; held.playable = playable;
          if (changed) setDrag({ ...held });
          if (!dragFrame.current) dragFrame.current = requestAnimationFrame(paintDrag);
        }} onPointerUp={endDrag} onPointerCancel={cancelDrag} onLostPointerCapture={() => { if (dragRef.current?.id === card.id) cancelDrag(); }}>
        <div className="hand-flip"><div className="hand-card-face">
          <HandCardContent card={card} />
          {!snapshot.preview && <button className="hand-discard" aria-label={t("弃掉此牌")} disabled={!!busy}
            onPointerDown={(e) => e.stopPropagation()} onClick={(e) => { e.stopPropagation(); void operation("discard_hand_card", card); }}><Icon name="close" size={14} /></button>}
          {playing && <div className="hand-release-label"><Icon name="play" size={10} />{t("松开以播放")}</div>}
        </div>{card.mysteryRevealed && snapshot.arrival?.id === card.id && <div className="hand-card-back"><div className="hand-card-face"><div className="hand-art"><Cover url="" data={snapshot.arrival.mysteryCover} prepared mystery label={t("神秘歌曲")} /></div><strong>???</strong><span>???</span></div></div>}</div>
      </div>;
    })}
    {flying && <div className="hand-card hand-flying" data-hand-hit
      onAnimationEnd={(event) => { if (event.target === event.currentTarget) finishFlight(); }}
      style={{ left: flying.pose.x, top: flying.pose.y, width: flying.pose.width, height: flying.pose.height, "--hand-base-scale": flying.pose.width / 156,
        "--hand-angle": `${flying.pose.angle}deg`, "--hand-card-scale": flying.pose.scale } as CSSProperties}>
      <div className="hand-card-face"><HandCardContent card={flying.card} /></div>
    </div>}
  </div>;
}
