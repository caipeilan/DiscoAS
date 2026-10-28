import { useEffect, useMemo, useRef, useState } from "react";
import type { HandCard, HandSnapshot, HandRect } from "../../types";
import { defaultGuiSettings, defaultHandSettings, emptyState } from "../../types";
import { call, desktop, onDesktopEvent, notifyHandBusy, notifyHandClear } from "../../services/desktop";
import { applyAppearance } from "../../appearance";
import { currentLocale, errorText, setLanguage } from "../../i18n";
import { handCardSize, mergeHandCards, handLayout, groupedHandLayout, groupSettings, raisedHandPose, reorderHandGroups, handReplacementFocusId, type CardPose } from "./handLayout";
import { defaultDockPoint, floatingDockRect, type DockPoint } from "./floatingDock";
import { combineHandBounds, handPoseBounds, handLayoutTransitionBounds } from "./handDrawingBounds";
import { releaseHandMotion } from "./handMotion";
import { useHandInput } from "./useHandInput";
import { useHandSurface } from "./useHandSurface";
import { useHandMotion } from "./useHandMotion";
import { readPose, type Drag, type Flying, type Discarding, type ArrivalMotion } from "./handControllerTypes";

import { idleAutoHandState, mergeAutoHandState, type AutoHandState } from "./autoHandState";
import { autoHandNextId } from "./autoHandNext";
import type { ArrivalReady } from "./handSurface";

const fallback: HandSnapshot = {
  generation: 0, cards: [], settings: defaultHandSettings, gui: defaultGuiSettings,
  keys: emptyState.settings.discoveryKeybindings, workArea: { left: 0, top: 0, width: window.innerWidth, height: window.innerHeight },
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
  const [manualPlaying, setManualPlaying] = useState(false);
  const [autoState, setAutoState] = useState(idleAutoHandState);
  const autoStateRef = useRef(autoState); autoStateRef.current = autoState;
  const autoLocked = (autoState.busy || manualPlaying) && !snapshot.preview;
  const [discoveryPhase, setDiscoveryPhase] = useState<"" | "fold" | "fill">("");
  const [refill, setRefill] = useState<HandCard | null>(null);
  const [refillCount, setRefillCount] = useState(0);
  const [replacing, setReplacing] = useState(-1);
  const replacementSlot = useRef<{ id: string; index: number } | null>(null);
  const [error, setError] = useState("");
  const [clearConfirm, setClearConfirm] = useState(false);
  const [flying, setFlying] = useState<Flying | null>(null);
  const [discarding, setDiscarding] = useState<Discarding[]>([]);
  const [retiring, setRetiring] = useState<Array<{ card: HandCard; pose: CardPose; side: string }>>([]);
  const retiringRef = useRef(retiring);
  const [entering, setEntering] = useState<Set<string>>(new Set());
  const [arrivalReady, setArrivalReady] = useState<ArrivalReady>(null);
  const [layoutTransition, setLayoutTransition] = useState<{ generation: number; pending: boolean; poses: Map<string, CardPose>; bounds: HandRect | null } | null>(null);
  const layoutPrepared = useRef((generation: number) => setLayoutTransition(previous => previous?.generation === generation && previous.pending ? { ...previous, pending: false } : previous));
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
  const busyRef = useRef(busy); busyRef.current = autoLocked ? "auto" : busy;
  const inputRef = useRef(inputMode); inputRef.current = inputMode;
  const selectedRef = useRef(selected); selectedRef.current = selected;
  const hoverRef = useRef(hover); hoverRef.current = hover;
  const active = inputMode === "keyboard" ? selected : hover;
  const groups = useMemo(() => {
    const discovery: HandCard[] = [], ordinary: HandCard[] = [];
    for (const card of snapshot.cards) (card.discovery ? discovery : ordinary).push(card);
    return { discovery, ordinary };
  }, [snapshot.cards]);
  const autoNextId = useMemo(() => autoState.active === false ? null : autoHandNextId(snapshot.cards, snapshot.settings, snapshot.preview),
    [snapshot.cards, snapshot.settings.enabled, snapshot.settings.autoPlayEnabled, snapshot.settings.autoPlayPosition, snapshot.preview, autoState.active]);
  const displayed = useMemo(() => refill && snapshot.discoverySettings?.enabled && groups.discovery.length < refillCount
    ? groups.discovery.concat(refill, groups.ordinary) : snapshot.cards,
  [snapshot.cards, groups, snapshot.discoverySettings?.enabled, refill, refillCount]);
  const cards = useMemo(() => drag ? reorderHandGroups(displayed, drag.id, drag.index, drag.intake) : displayed, [displayed, drag?.id, drag?.index, drag?.intake]);
  const cardIndices = useMemo(() => new Map(cards.map((card, index) => [card.id, index])), [cards]);
  const activeIndex = active ? cardIndices.get(active) ?? -1 : -1;
  const layoutDiscoveryCount = useMemo(() => cards.reduce((total, card) => total + Number(!!card.discovery), 0), [cards]);
  const cardSize = handCardSize(snapshot.gui.fontSize);
  const restPoses = useMemo(() => groupedHandLayout(cards, snapshot.settings, snapshot.discoverySettings, snapshot.workArea, -1, snapshot.gui.fontSize), [cards, snapshot.settings, snapshot.discoverySettings, snapshot.workArea, snapshot.gui.fontSize]);
  const restPosesRef = useRef(restPoses); restPosesRef.current = restPoses;
  const diameter = 44 * snapshot.gui.fontSize / 14;
  const dockAreas = useMemo(() => snapshot.workAreas?.length ? snapshot.workAreas : [snapshot.workArea], [snapshot.workAreas, snapshot.workArea]);
  const dockSide = snapshot.settings.enabled ? snapshot.settings.side : snapshot.discoverySettings?.side ?? snapshot.settings.side;
  const dockPosition = dockPoint ?? defaultDockPoint(restPoses, dockSide, snapshot.workArea, diameter);
  if (desktop && !snapshot.preview) toolbarBounds.current = null;
  const poses = useMemo(() => groupedHandLayout(cards, snapshot.settings, snapshot.discoverySettings, snapshot.workArea, activeIndex, snapshot.gui.fontSize),
    [cards, snapshot.settings, snapshot.discoverySettings, snapshot.workArea, activeIndex, snapshot.gui.fontSize]);
  const posesRef = useRef(poses); posesRef.current = poses;
  const layoutDrawingBounds = useMemo(() => {
    const all = [...restPoses];
    restPoses.forEach((pose, index) => {
      const settings = groupSettings(cards[index], snapshot.settings, snapshot.discoverySettings);
      all.push(raisedHandPose(pose, settings.side, snapshot.workArea));
      const shift = 16 * pose.width / 156;
      for (const direction of [index > 0 ? 1 : 0, index < cards.length - 1 ? -1 : 0]) {
        if (direction) all.push({ ...pose,
          x: pose.x + (settings.side === "bottom" ? shift * direction : 0),
          y: pose.y + (settings.side !== "bottom" ? shift * direction : 0) });
      }
    });
    const folded = restPoses.map((pose, index) => {
      const side = groupSettings(cards[index], snapshot.settings, snapshot.discoverySettings).side;
      return { ...pose, angle: side === "left" ? -20 : side === "right" ? 20 : 0,
        scale: .75, x: pose.x + (side === "left" ? -pose.width : side === "right" ? pose.width : 0),
        y: pose.y + (side === "bottom" ? pose.height : 0) };
    });
    return combineHandBounds([...all, ...folded].map((pose) => handPoseBounds(pose)));
  }, [restPoses, cards, snapshot.settings, snapshot.discoverySettings, snapshot.workArea, snapshot.gui.fontSize]);
  const drawingBounds = useRef<HandRect | null>(null);
  drawingBounds.current = combineHandBounds([
    cardsMounted ? layoutDrawingBounds : null,
    layoutTransition?.bounds,
    ...discarding.map((item) => combineHandBounds([handPoseBounds(item.pose), handPoseBounds({ ...item.pose, y: item.pose.y + 14 })])),
    flying ? handPoseBounds(flying.pose) : null,
    ...retiring.map((item) => combineHandBounds([handPoseBounds(item.pose), handPoseBounds({ ...item.pose,
      x: item.pose.x + (item.side === "left" ? -42 : item.side === "right" ? 42 : 0),
      y: item.pose.y + (item.side === "bottom" ? 42 : 0) })])),
  ]);
  const accepted = useRef(0);
  const arrivalPlayed = useRef(0);
  const arrivalMotion = useRef<ArrivalMotion | null>(null);
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
    active, drag, flying, discarding, busy, error, clearConfirm, hidden, cardsMounted, dockPoint, setError,
    layoutKey: cards.map(c => c.id).join(",") + retiring.map(item => item.card.id).join(",") + discoveryPhase + replacing });
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
    setLayoutTransition(null);
    if (current.current.expanded && next.expanded && !unfolding.current && !busyRef.current && !next.arrival
      && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      const oldPoses = new Map<string, CardPose>();
      for (const card of current.current.cards) {
        const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${card.id}"]`);
        if (element) oldPoses.set(card.id, readPose(element));
      }
      const targets = groupedHandLayout(next.cards, next.settings, next.discoverySettings, next.workArea, -1, next.gui.fontSize);
      const changed = next.cards.length !== oldPoses.size || next.cards.some((card, index) => {
        const old = oldPoses.get(card.id), target = targets[index];
        return !old || ["x", "y", "width", "height", "angle", "scale"].some(key => Math.abs(old[key as keyof CardPose] - target[key as keyof CardPose]) > .01);
      });
      if (changed) setLayoutTransition({ generation: next.generation, pending: true, poses: oldPoses,
        bounds: handLayoutTransitionBounds([...oldPoses.values(), ...targets]) });
    }
    const ids = new Set(next.cards.map(card => card.id));
    const retiringCards = next.expanded ? retiringRef.current.filter(item => !ids.has(item.card.id)) : [];
    if (current.current.expanded && next.expanded && !unfolding.current && !busyRef.current && !next.arrival
      && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      for (const card of current.current.cards) {
        if (ids.has(card.id) || retiringCards.some(item => item.card.id === card.id)) continue;
        const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${card.id}"]`);
        if (element) retiringCards.push({ card, pose: readPose(element),
          side: groupSettings(card, current.current.settings, current.current.discoverySettings).side });
      }
      const previousIds = new Set(current.current.cards.map(card => card.id));
      setEntering(new Set(next.cards.filter(card => card.discovery && !previousIds.has(card.id)).map(card => card.id)));
    }
    retiringRef.current = retiringCards; setRetiring(retiringCards);
    if (current.current.expanded && next.expanded && !unfolding.current && accepted.current && !next.arrival) returning.current.push(
      ...next.cards.filter((card) => !card.discovery && !current.current.cards.some((old) => old.id === card.id)).map((c) => c.id));
    if (next.gui !== current.current.gui) {
      setLanguage(next.gui.language); document.documentElement.lang = currentLocale(); applyAppearance(next.gui);
    }
    if (next.arrival && (!current.current.arrival || current.current.arrival.id !== next.arrival.id || current.current.generation !== next.generation)) {
      setArrivalReady(null);
    }
    accepted.current = next.generation;
    current.current = next;
    setSnapshot(next); setHidden(false); setClearConfirm(false);
    if (!desktop || next.preview) setDockPoint((previous) => next.dockPoint ?? previous ?? defaultDockPoint(
      handLayout(next.cards.length, next.settings, next.workArea, -1, next.gui.fontSize),
      next.settings.side, next.workArea, 44 * next.gui.fontSize / 14));
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
    else {
      selectedRef.current = handReplacementFocusId(next.cards, selectedRef.current, replacementSlot.current);
      setSelected(selectedRef.current);
    }
    hoverRef.current = handReplacementFocusId(next.cards, hoverRef.current, replacementSlot.current);
    setHover(hoverRef.current);
  };

  const pause = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, window.matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : ms));
  const decoded = async () => {
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    await Promise.all(Array.from(stage.current?.querySelectorAll("img") ?? []).map(img => img.decode().catch(() => {})));
  };
  const refreshDiscovery = async () => {
    if (busyRef.current || current.current.preview || !current.current.discoverySettings?.enabled) return;
    busyRef.current = "refresh"; setBusy("refresh"); setError(""); setDiscoveryPhase("fold");
    try { await Promise.all([call("refresh_hand_discovery"), pause(340)]); await decoded(); }
    catch (e) { setError(errorText(e)); }
    finally { if (mounted.current) { setDiscoveryPhase(""); busyRef.current = ""; setBusy(""); } }
  };
  const operation = async (name: "play_hand_card" | "discard_hand_card" | "collect_hand_discovery_card", card: HandCard, pose?: CardPose, insert = 0) => {
    if (autoStateRef.current.busy || busyRef.current || current.current.preview) return;
    busyRef.current = card.id; setBusy(card.id); setError("");
    if (name === "play_hand_card") setManualPlaying(true);
    if (card.discovery) {
      const state = current.current, batchEpoch = state.discovery?.batchEpoch;
      const args = { platform: card.song.platform, songId: card.song.songId, sourceId: card.song.sourceId, sourceKind: card.song.sourceKind, filename: card.song.filename };
      const index = state.cards.filter(c => c.discovery).findIndex(c => c.id === card.id);
      try {
        if (name === "discard_hand_card") {
          replacementSlot.current = { id: card.id, index };
          setReplacing(index);
          await Promise.all([call("replace_hand_discovery_card", { args, batchEpoch }), pause(340)]);
          await decoded(); setReplacing(-1);
        } else {
          setDiscoveryPhase("fold");
          let flight: Promise<unknown> = Promise.resolve();
          if (pose) {
            flight = new Promise<void>(complete => { flyingRef.current = { card, pose, complete, collecting: name === "collect_hand_discovery_card" }; setFlying(flyingRef.current); });
          }
          if (name === "collect_hand_discovery_card") {
            const id = await call<string>("collect_hand_discovery_card", { args, batchEpoch, insert });
            const surface = current.current.surface;
            const origin = pose ? { left: (pose.x-pose.width*pose.scale/2-(surface?.left ?? 0))*(surface?.scale ?? 1),
              top: (pose.y-pose.height*pose.scale/2-(surface?.top ?? 0))*(surface?.scale ?? 1), width: pose.width*pose.scale*(surface?.scale ?? 1), height: pose.height*pose.scale*(surface?.scale ?? 1) } : undefined;
            await call("show_collected_hand_card", { id, origin, mysteryCover: card.song.mysteryMode ? card.song.coverDataUri : undefined });
            finishFlight(); await pause(480);
          } else { await call("play_hand_discovery_card", { args, batchEpoch }); await flight; await pause(40); }
          if (!mounted.current) return;
          setDiscoveryPhase("fill"); setRefillCount(state.cards.filter(c => c.discovery).length); setRefill({ ...card, id: "hand-discovery-refill", placeholder: true });
          await Promise.all([pause(420), call("refresh_hand_discovery")]);
          setRefill(null); await decoded(); await pause(180);
        }
      } catch (e) { finishFlight(); setError(errorText(e)); }
      finally {
        replacementSlot.current = null;
        if (mounted.current) {
          setRefill(null); setReplacing(-1); setDiscoveryPhase(""); setManualPlaying(false); busyRef.current = ""; setBusy("");
          if (name === "discard_hand_card") requestAnimationFrame(() => refreshPointerHover());
        }
      }
      return;
    }
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
      if (name !== "play_hand_card") returning.current.push(card.id);
      setError(errorText(e));
    }
    finally { finishDiscards(); setManualPlaying(false); busyRef.current = ""; setBusy(""); }
  };
  const clearHand = async () => {
    if (busyRef.current || current.current.preview) { if (desktop) void notifyHandClear(null); return; }
    const targets = current.current.cards.filter(c => !c.discovery);
    if (!targets.length) { if (desktop) void notifyHandClear(null); return; }
    busyRef.current = "clear"; setBusy("clear"); setError("");
    const flight = animateDiscard(targets);
    let message: string | null = null;
    try { await call("clear_hand"); await flight; }
    catch (e) { await flight; returning.current.push(...targets.map((card) => card.id)); message = errorText(e); setError(message); }
    finally { finishDiscards(); busyRef.current = ""; setBusy(""); if (desktop) void notifyHandClear(message); }
  };
  const clearHandRef = useRef(clearHand); clearHandRef.current = clearHand;
  useEffect(() => { if (desktop && !snapshot.preview) void notifyHandBusy(!!busy || autoLocked); }, [busy, autoLocked, snapshot.preview]);
  const { cancelDrag, endDrag, onCardPointerDown, onCardPointerMove, lostPointerCapture, refreshPointerHover } = useHandInput({ snapshot, current, mounted,
    dockMoving, stage, busyRef, selectedRef, hoverRef, inputRef, pointer, toolbarBounds, posesRef, restPosesRef,
    dragRef, dragFrame, dragDrawing, setDrag, setInputMode, setHover, setSelected, setSnapshot, setError,
    toCanvas, scheduleRegions, commitRegions, operation });
  useEffect(() => {
    if (!desktop) return;
    let cancelled = false, stop: (() => void) | undefined;
    const acceptAuto = (next: AutoHandState) => {
      if (cancelled) return;
      const accepted = mergeAutoHandState(autoStateRef.current, next);
      autoStateRef.current = accepted; setAutoState(accepted);
      if (accepted.busy) {
        busyRef.current = "auto"; cancelDrag();
        hoverRef.current = null; selectedRef.current = null; setHover(null); setSelected(null);
      }
    };
    void onDesktopEvent("auto-hand-state-changed", acceptAuto).then(cleanup => {
      if (cancelled) { cleanup(); return; }
      stop = cleanup;
      return call<AutoHandState>("get_auto_hand_state").then(acceptAuto);
    }).catch(e => { if (!cancelled) setError(errorText(e)); });
    return () => { cancelled = true; stop?.(); };
  }, []);
  const operationRef = useRef(operation); operationRef.current = operation;
  const refreshRef = useRef(refreshDiscovery); refreshRef.current = refreshDiscovery;
  useEffect(() => {
    if (!layoutTransition || layoutTransition.pending) return;
    let cancelled = false;
    const frame = requestAnimationFrame(() => {
      const animations = stage.current?.getAnimations({ subtree: true }) ?? [];
      void Promise.all(animations.map(animation => animation.finished.catch(() => {}))).then(() => {
        if (!cancelled) setLayoutTransition(previous => previous === layoutTransition ? null : previous);
      });
    });
    return () => { cancelled = true; cancelAnimationFrame(frame); };
  }, [layoutTransition]);
  useEffect(() => {
    let cancelled = false; const stops: Array<() => void> = [];
    void Promise.all([
      onDesktopEvent("hand-discovery-refresh-requested", () => { void refreshRef.current(); }),
      onDesktopEvent("hand-discovery-play-requested", id => {
        const card = current.current.cards.find(c => c.id === id);
        if (card) { const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${id}"]`); void operationRef.current("play_hand_card", card, element ? readPose(element) : undefined); }
      }),
    ]).then(values => { if (cancelled) values.forEach(stop => stop()); else stops.push(...values); });
    return () => { cancelled = true; stops.forEach(stop => stop()); };
  }, []);
  useEffect(() => {
    if (!snapshot.discovery || !snapshot.expanded || snapshot.preview || discoveryPhase || replacing >= 0 || !snapshot.cards.some(c => c.discovery)) return;
    let cancelled = false;
    void decoded().then(() => {
      if (cancelled || !mounted.current) return;
      return call("hand_discovery_displayed", { batchEpoch: snapshot.discovery!.batchEpoch,
        identities: snapshot.cards.filter(c => c.discovery).map(c => ({ platform: c.song.platform, songId: c.song.songId, sourceId: c.song.sourceId, sourceKind: c.song.sourceKind, filename: c.song.filename })) });
    }).catch(e => { if (!cancelled) setError(errorText(e)); });
    return () => { cancelled = true; };
  }, [snapshot.discovery?.batchEpoch, snapshot.expanded, snapshot.preview, discoveryPhase, replacing]);
  useEffect(() => {
    mounted.current = true;
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
    const settleTransitions = () => { if (reduced.matches) { retiringRef.current = []; setRetiring([]); setEntering(new Set()); setLayoutTransition(null); } };
    reduced.addEventListener("change", settleTransitions);
    let cancelled = false; const cleanups: Array<() => void> = [];
    const listen = async <E extends "hand-state-changed" | "hand-visibility-changed" | "hand-clear-requested" | "hand-hide" | "hand-error">(event: E, handler: Parameters<typeof onDesktopEvent<E>>[1]) => {
      const stop = await onDesktopEvent(event, handler); if (cancelled) stop(); else cleanups.push(stop);
    };
    void Promise.all([
      listen("hand-state-changed", accept),
      listen("hand-visibility-changed", (value) => accept({ ...current.current, ...value, arrival: null })),
      listen("hand-clear-requested", () => { void clearHandRef.current(); }),
      listen("hand-hide", (generation) => { if (generation >= accepted.current) { accepted.current = generation; setHidden(true); setPhase("closed"); unfolding.current = true; retiringRef.current = []; setRetiring([]); setEntering(new Set()); setLayoutTransition(null); finishFlight(); finishDiscards(); cancelDrag(); } }),
      listen("hand-error", (e) => setError(errorText(e))),
    ]).then(async () => { if (!cancelled) { const next = await call<HandSnapshot | null>("hand_ready"); if (next) accept(next); } }).catch((e) => setError(errorText(e)));
    return () => { mounted.current = false; cancelled = true; reduced.removeEventListener("change", settleTransitions); cancelAnimationFrame(dragFrame.current); flyingRef.current?.complete(); discardingRef.current.forEach((item) => item.complete()); cleanups.forEach((c) => c()); timers.current.forEach(clearTimeout); };
  }, []);

  const showCardWindow = snapshot.preview || (cardsMounted && (snapshot.cards.length > 0 || discarding.length > 0 || retiring.length > 0)) || !!flying;
  const markArrivalReady = (id: string, generation: number) => {
    if (generation === accepted.current) setArrivalReady({ id, generation });
  };
  useHandMotion({ snapshot, phase, hidden, cardsMounted, showCardWindow, busy, flying, poses, posesRef, stage,
    setCardsMounted, setPhase, setError, commitRegions, scheduleRegions, accepted, unfolding, arrivalPlayed,
    arrivalMotion, arrivalDrawing, returning, layoutPrepared, arrivalReady, markArrivalReady });
  const surface = snapshot.surface;
  const discardById = useMemo(() => new Map(discarding.map((item) => [item.card.id, item])), [discarding]);
  const retiringById = useMemo(() => new Map(retiring.map(item => [item.card.id, item])), [retiring]);
  const visibleCards = useMemo(() => {
    const visible = new Map(displayed.map((card) => [card.id, card]));
    discarding.forEach((item) => { if (!visible.has(item.card.id)) visible.set(item.card.id, item.card); });
    retiring.forEach(item => { if (!visible.has(item.card.id)) visible.set(item.card.id, item.card); });
    if (flying && !visible.has(flying.card.id)) visible.set(flying.card.id, flying.card);
    return [...visible.values()];
  }, [displayed, discarding, retiring, flying]);
  const finishCardTransition = (id: string) => {
    if (retiringRef.current.some(item => item.card.id === id)) {
      retiringRef.current = retiringRef.current.filter(item => item.card.id !== id); setRetiring(retiringRef.current);
    }
    setEntering(previous => { if (!previous.has(id)) return previous; const next = new Set(previous); next.delete(id); return next; });
  };
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
    scheduleRegions, busy: autoLocked ? "auto" : busy, autoLocked, autoNextId, error, clearConfirm, toCanvas, setDockMoving, moveDock, toggleDock, clearDock, visibleCards, discardById,
    flying, drag, poses, cardIndices, active, operation, finishFlight, onCardPointerDown, onCardPointerMove,
    endDrag, cancelDrag, lostPointerCapture, errorPosition, clearError, discoveryPhase, replacingId: groups.discovery[replacing]?.id,
    refreshDiscovery, restPoses, layoutDiscoveryCount, discoveryCount: groups.discovery.length, ordinaryCount: groups.ordinary.length,
    retiringById, entering, finishCardTransition, layoutTransition, arrivalReady };

}
