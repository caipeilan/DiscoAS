import { useEffect, useMemo, useRef, useState } from "react";
import type { HandCard, HandSnapshot, HandRect } from "../../types";
import { defaultGuiSettings, defaultHandSettings, emptyState } from "../../types";
import { call, desktop, onDesktopEvent, notifyHandBusy, notifyHandClear } from "../../services/desktop";
import { applyAppearance } from "../../appearance";
import { currentLocale, errorText, setLanguage } from "../../i18n";
import { handCardSize, mergeHandCards, handLayout, raisedHandPose, reorderCards, type CardPose } from "./handLayout";
import { defaultDockPoint, floatingDockRect, type DockPoint } from "./floatingDock";
import { combineHandBounds, handPoseBounds } from "./handDrawingBounds";
import { releaseHandMotion } from "./handMotion";
import { useHandInput } from "./useHandInput";
import { useHandSurface } from "./useHandSurface";
import { useHandMotion } from "./useHandMotion";
import { readPose, type Drag, type Flying, type Discarding } from "./handControllerTypes";

const fallback: HandSnapshot = {
  generation: 0, cards: [], settings: defaultHandSettings, gui: defaultGuiSettings,
  keys: emptyState.settings.discovery_keybindings, workArea: { left: 0, top: 0, width: window.innerWidth, height: window.innerHeight },
  preview: false, focus: false, expanded: false, arrival: null,
};

export function useHandController() {
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
  const dragRef = useRef<Drag | null>(null);
  const dragFrame = useRef(0);
  const busyRef = useRef(busy); busyRef.current = busy;
  const inputRef = useRef(inputMode); inputRef.current = inputMode;
  const selectedRef = useRef(selected); selectedRef.current = selected;
  const hoverRef = useRef(hover); hoverRef.current = hover;
  const active = inputMode === "keyboard" ? selected : hover;
  const cards = useMemo(() => drag ? reorderCards(snapshot.cards, drag.id, drag.index) : snapshot.cards, [snapshot.cards, drag?.id, drag?.index]);
  const cardIndices = useMemo(() => new Map(cards.map((card, index) => [card.id, index])), [cards]);
  const activeIndex = cards.findIndex((c) => c.id === active);
  const cardSize = handCardSize(snapshot.gui.font_size);
  const restPoses = useMemo(() => handLayout(cards.length, snapshot.settings, snapshot.workArea, -1, snapshot.gui.font_size), [cards.length, snapshot.settings, snapshot.workArea, snapshot.gui.font_size]);
  const restPosesRef = useRef(restPoses); restPosesRef.current = restPoses;
  const diameter = 44 * snapshot.gui.font_size / 14;
  const dockAreas = useMemo(() => snapshot.workAreas?.length ? snapshot.workAreas : [snapshot.workArea], [snapshot.workAreas, snapshot.workArea]);
  const dockPosition = dockPoint ?? defaultDockPoint(restPoses, snapshot.settings.side, snapshot.workArea, diameter);
  if (desktop && !snapshot.preview) toolbarBounds.current = null;
  const poses = useMemo(() => handLayout(cards.length, snapshot.settings, snapshot.workArea, activeIndex, snapshot.gui.font_size),
    [cards.length, snapshot.settings, snapshot.workArea, activeIndex, snapshot.gui.font_size]);
  const posesRef = useRef(poses); posesRef.current = poses;
  const layoutDrawingBounds = useMemo(() => {
    const all = [...restPoses];
    restPoses.forEach((pose, index) => {
      all.push(raisedHandPose(pose, snapshot.settings.side, snapshot.workArea));
      const shift = 16 * pose.width / 156;
      for (const direction of [index > 0 ? 1 : 0, index < cards.length - 1 ? -1 : 0]) {
        if (direction) all.push({ ...pose,
          x: pose.x + (snapshot.settings.side === "bottom" ? shift * direction : 0),
          y: pose.y + (snapshot.settings.side !== "bottom" ? shift * direction : 0) });
      }
    });
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
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());
  const later = (f: () => void, ms: number) => {
    const timer = setTimeout(() => { timers.current.delete(timer); f(); }, ms);
    timers.current.add(timer);
  };
  const finishFlight = () => {
    flyingRef.current?.complete(); flyingRef.current = null; setFlying(null);
  };
  const { toCanvas, commitRegions, scheduleRegions } = useHandSurface({ snapshot, current, setSnapshot, mounted, stage,
    toolbarElement, toolbarBounds, dragRef, drawingBounds, arrivalDrawing, dragDrawing, dockMoving, phase,
    active, drag, flying, discarding, busy, error, clearConfirm, hidden, cardsMounted, dockPoint, setError });
  const animateDiscard = (targets: HandCard[]) => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return Promise.resolve();
    const flights = targets.map((card, index) => new Promise<void>((complete) => {
      const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${card.id}"]`);
      if (!element) { complete(); return; }
      const pose = readPose(element);
      releaseHandMotion(element);
      discardingRef.current.push({ card, pose, delay: Math.min(index * 15, 75), complete });
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
    if (!desktop || next.preview) setDockPoint((previous) => next.dockPoint ?? previous ?? defaultDockPoint(
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
      const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${card.id}"]`);
      if (element) releaseHandMotion(element);
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
  const { cancelDrag, endDrag, onCardPointerDown, onCardPointerMove, lostPointerCapture } = useHandInput({ snapshot, current, mounted,
    dockMoving, stage, busyRef, selectedRef, hoverRef, inputRef, pointer, toolbarBounds, posesRef, restPosesRef,
    dragRef, dragFrame, dragDrawing, setDrag, setInputMode, setHover, setSelected, setSnapshot, setError,
    toCanvas, scheduleRegions, operation });
  useEffect(() => {
    mounted.current = true;
    let cancelled = false; const cleanups: Array<() => void> = [];
    const listen = async <E extends "hand-state-changed" | "hand-visibility-changed" | "hand-clear-requested" | "hand-hide" | "hand-error">(event: E, handler: Parameters<typeof onDesktopEvent<E>>[1]) => {
      const stop = await onDesktopEvent(event, handler); if (cancelled) stop(); else cleanups.push(stop);
    };
    void Promise.all([
      listen("hand-state-changed", accept),
      listen("hand-visibility-changed", (value) => accept({ ...current.current, ...value, arrival: null })),
      listen("hand-clear-requested", () => { void clearHandRef.current(); }),
      listen("hand-hide", (generation) => { if (generation >= accepted.current) { accepted.current = generation; setHidden(true); setPhase("closed"); unfolding.current = true; finishFlight(); finishDiscards(); cancelDrag(); } }),
      listen("hand-error", (e) => setError(errorText(e))),
    ]).then(async () => { if (!cancelled) { const next = await call<HandSnapshot | null>("hand_ready"); if (next) accept(next); } }).catch((e) => setError(errorText(e)));
    return () => { mounted.current = false; cancelled = true; cancelAnimationFrame(dragFrame.current); flyingRef.current?.complete(); discardingRef.current.forEach((item) => item.complete()); cleanups.forEach((c) => c()); timers.current.forEach(clearTimeout); };
  }, []);

  const showCardWindow = snapshot.preview || (cardsMounted && (snapshot.cards.length > 0 || discarding.length > 0)) || !!flying;
  useHandMotion({ snapshot, phase, hidden, cardsMounted, showCardWindow, busy, flying, poses, posesRef, stage,
    setCardsMounted, setPhase, setError, commitRegions, scheduleRegions, accepted, unfolding, arrivalPlayed,
    arrivalMotion, arrivalDrawing, returning });
  const surface = snapshot.surface;
  const discardById = useMemo(() => new Map(discarding.map((item) => [item.card.id, item])), [discarding]);
  const visibleCards = useMemo(() => {
    const visible = new Map(snapshot.cards.map((card) => [card.id, card]));
    discarding.forEach((item) => { if (!visible.has(item.card.id)) visible.set(item.card.id, item.card); });
    if (flying && !visible.has(flying.card.id)) visible.set(flying.card.id, flying.card);
    return [...visible.values()];
  }, [snapshot.cards, discarding, flying]);
  const moveDock = (point: DockPoint) => {
    current.current = { ...current.current, dockPoint: point }; setSnapshot(current.current); setDockPoint(point);
    if (!snapshot.preview) void call("save_hand_dock", { generation: current.current.generation, point }).catch((e) => setError(errorText(e)));
  };
  const toggleDock = () => { void call(snapshot.preview ? "end_hand_preview" : "toggle_hand_expanded").catch((e) => setError(errorText(e))); };
  const clearDock = () => {
    if (!clearConfirm) { setClearConfirm(true); later(() => setClearConfirm(false), 3000); return; }
    setClearConfirm(false); void clearHand();
  };
  const errorDock = error ? floatingDockRect(dockPosition, snapshot.expanded || snapshot.preview, diameter, dockAreas) : null;
  const errorPosition = errorDock ? { left: errorDock.left, top: Math.max(snapshot.workArea.top + 8, errorDock.top - 48) } : undefined;
  const clearError = () => setError("");
  return { snapshot, phase, hidden, inputMode, cardsMounted, stage, surface, cardSize, toolbarElement, dockPosition, dockAreas, diameter,
    busy, error, clearConfirm, toCanvas, setDockMoving, moveDock, toggleDock, clearDock, visibleCards, discardById,
    flying, drag, poses, cardIndices, active, operation, finishFlight, onCardPointerDown, onCardPointerMove,
    endDrag, cancelDrag, lostPointerCapture, errorPosition, clearError };

}
