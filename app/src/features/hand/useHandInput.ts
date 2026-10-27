import { useEffect, useRef } from "react";
import type { HandCard, HandKey } from "../../types";
import { call, desktop, onDesktopEvent } from "../../services/desktop";
import { errorText } from "../../i18n";
import { keybindingFromEvent } from "../settings/discoveryKeybindings";
import { canPlayDrag, reorderHandGroups, reorderIndex, hoverGroupedHandIndex, groupSettings, handIntakeIndex } from "./handLayout";
import { releaseHandMotion } from "./handMotion";
import { readPose, type Drag, type HandInputContext } from "./handControllerTypes";

const instantHolds = new WeakMap<HTMLElement, number>();
/** Runs the next committed style change without fading, then restores transitions. */
function holdInstant(element: HTMLElement) {
  const token = (instantHolds.get(element) ?? 0) + 1;
  instantHolds.set(element, token);
  element.dataset.handInstant = "";
  setTimeout(() => {
    if (instantHolds.get(element) !== token) return;
    instantHolds.delete(element);
    // Flush the committed state while transitions are still suspended, then restore.
    void element.offsetWidth;
    delete element.dataset.handInstant;
  }, 0);
}

/** Owns pointer gestures, hover mode and keyboard selection; card rendering stays declarative. */
export function useHandInput(context: HandInputContext) {
  const { snapshot, current, mounted, dockMoving, stage, busyRef, selectedRef, hoverRef, inputRef, pointer,
    toolbarBounds, posesRef, restPosesRef, dragRef, dragFrame, setDrag, setInputMode, setHover,
    setSelected, setSnapshot, setError, toCanvas, operation } = context;
  const dragIntentAt = useRef(0);
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
    if (!action) return;
    event instanceof KeyboardEvent && event.preventDefault();
    const index = state.cards.findIndex((c) => c.id === selectedRef.current);
    setInputMode("keyboard"); inputRef.current = "keyboard";
    if (action === "select" || action === "discard" || action === "replace") {
      if (index >= 0 && !event.repeat) {
        if (action === "replace" && !state.cards[index].discovery) return;
        const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${state.cards[index].id}"]`);
        void operation(action === "select" ? "play_hand_card" : "discard_hand_card", state.cards[index], element ? readPose(element) : posesRef.current[index]);
      }
      else if (index < 0) setSelected(state.cards[0].id);
    } else {
      const next = index < 0 ? 0 : Math.max(0, Math.min(state.cards.length - 1, index + (["left", "up"].includes(action) ? -1 : 1)));
      selectedRef.current = state.cards[next].id; setSelected(state.cards[next].id);
    }
  };
  const finishNativeDrag = (held: Drag, reveal = true) => {
    cancelAnimationFrame(dragFrame.current); dragFrame.current = 0;
    if (held.element.hasPointerCapture(held.pointerId)) held.element.releasePointerCapture(held.pointerId);
    if (reveal && held.presented) {
      // The hidden source card sits at its pickup pose. Re-anchor it to the release
      // point so clearing the drag state transitions it back instead of snapping.
      const element = held.element;
      element.style.setProperty("--hand-x", `${held.pose.x}px`);
      element.style.setProperty("--hand-y", `${held.pose.y}px`);
      element.style.setProperty("--card-x", `${held.dx}px`);
      element.style.setProperty("--card-y", `${held.dy}px`);
      element.style.setProperty("--hand-card-scale", "1.12");
      element.style.setProperty("--hand-angle", "0deg");
      void element.offsetWidth;
    } else {
      // Played or collected cards must not flash back at the source position.
      holdInstant(held.element);
    }
    dragRef.current = null; dragIntentAt.current = 0; setDrag(null);
  };
  const cancelDrag = () => {
    const held = dragRef.current; if (!held) return;
    if (held.nativeStarted) {
      const generation = current.current.generation;
      void held.prepared.catch(() => {}).then(() => call("end_hand_drag", { generation, cancelled: true })).catch(() => {});
    }
    finishNativeDrag(held);
  };
  const beginNativeDrag = (held: Drag) => {
    const state = current.current;
    held.nativeStarted = true;
    setDrag({ ...held });
    held.prepared = call("begin_hand_drag", {
      generation: state.generation, card: held.card, settings: state.settings,
      discoverySettings: state.discoverySettings!, gui: state.gui,
      pointer: { x: held.startX, y: held.startY }, pose: held.pose,
    }).then(() => {
      // Do not wait forever for the drag surface acknowledgement.
      setTimeout(() => {
        if (dragRef.current === held && held.nativeStarted && !held.presented) {
          held.presented = true; setDrag({ ...held });
        }
      }, 300);
    }).catch((error) => {
      held.nativeStarted = false;
      void call("end_hand_drag", { generation: state.generation, cancelled: true }).catch(() => {});
      if (!held.ending && dragRef.current === held) cancelDrag();
      setError(errorText(error));
    });
  };
  const paintDrag = () => {
    dragFrame.current = 0;
    const held = dragRef.current; if (!held?.ready) return;
    if (held.positionDirty) {
      held.positionDirty = false;
      const state = current.current, card = held.card;
      const settings = groupSettings(card, state.settings, state.discoverySettings);
      const x = held.pose.x + held.dx, y = held.pose.y + held.dy;
      const index = reorderIndex(held.groupPoses, settings.side, x, y) + held.groupOffset;
      const intake = card.discovery ? handIntakeIndex(state.cards, held.restPoses, state.settings, state.discoverySettings, state.workArea, x, y, state.gui.font_size) : null;
      const playable = intake === null && canPlayDrag(settings.side, held.dx, held.dy, settings.scale);
      const changed = held.index !== index || held.playable !== playable || held.intake !== intake;
      held.index = index; held.intake = intake; held.playable = playable;
      if (changed && !held.ending) setDrag({ ...held });
      if (held.nativeStarted) {
        const now = performance.now();
        if (changed || now - dragIntentAt.current > 33) {
          dragIntentAt.current = now;
          void call("set_hand_drag_intent", {
            generation: state.generation, playable: held.playable,
            collecting: held.intake !== null, dx: held.dx, dy: held.dy,
          }).catch(() => {});
        }
      }
    }
  };
  const endDrag = async (event: React.PointerEvent) => {
    const held = dragRef.current; if (!held || held.ending || event.pointerId !== held.pointerId) return;
    if (Number.isFinite(event.clientX) && Number.isFinite(event.clientY)) updateDragPosition(held, event.clientX, event.clientY);
    held.ending = true;
    // The begin command may still be creating/showing the drag surface; never
    // let an end race ahead of it and leave a stale visible window.
    await held.prepared.catch(() => {});
    if (dragRef.current !== held) return;
    const state = current.current;
    const card = state.cards.find((c) => c.id === held.id);
    const moved = Math.hypot(held.dx, held.dy) > 8;
    const settings = card ? groupSettings(card, state.settings, state.discoverySettings) : state.settings;
    const valid = canPlayDrag(settings.side, held.dx, held.dy, settings.scale);
    if (!card || state.preview) { cancelDrag(); return; }
    if (!moved) { cancelDrag(); if (card.discovery) void operation("play_hand_card", card, readPose(held.element)); return; }
    if (!held.nativeStarted) { cancelDrag(); return; }
    paintDrag();
    const finishNative = () => {
      const generation = state.generation;
      void call("end_hand_drag", { generation, cancelled: false }).catch(() => {});
    };
    if (card.discovery && held.intake !== null) {
      finishNative();
      await operation("collect_hand_discovery_card", card, undefined, held.intake);
      if (dragRef.current === held) finishNativeDrag(held, false);
      return;
    }
    if (valid) {
      finishNative();
      await operation("play_hand_card", card);
      if (dragRef.current === held) finishNativeDrag(held, false);
      return;
    }
    const from = state.cards.findIndex((c) => c.id === held.id);
    const reordered = reorderHandGroups(state.cards, held.id, held.index);
    finishNative();
    if (from !== held.index) {
      current.current = { ...state, cards: reordered }; setSnapshot(current.current);
      try {
        await call(card.discovery ? "reorder_hand_discovery" : "reorder_hand", {
          ids: reordered.filter(c => !!c.discovery === !!card.discovery).map((c) => c.id), batchEpoch: state.discovery?.batchEpoch,
        });
      } catch (e) {
        current.current = state; setSnapshot(state); setError(errorText(e));
      }
    }
    if (dragRef.current === held) finishNativeDrag(held);
  };
  const onCardPointerDown = (card: HandCard, e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || snapshot.preview || busyRef.current || dragRef.current || card.placeholder || (e.target as HTMLElement).closest("button")) return;
    e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId);
    const visualPose = readPose(e.currentTarget);
    e.currentTarget.focus({ preventScroll: true }); releaseHandMotion(e.currentTarget); e.currentTarget.getAnimations().forEach((animation) => animation.cancel());
    const point = toCanvas(e.clientX, e.clientY);
    const discoveryCount = snapshot.cards.filter(c => c.discovery).length;
    const held: Drag = { id: card.id, index: snapshot.cards.findIndex((c) => c.id === card.id), playable: false, pointerId: e.pointerId, startX: point.x, startY: point.y, dx: 0, dy: 0, pose: visualPose, element: e.currentTarget,
      intake: null, restPoses: [...restPosesRef.current], ready: true, ending: false, prepared: Promise.resolve(), nativeStarted: false, presented: false,
      card, groupPoses: card.discovery ? restPosesRef.current.slice(0, discoveryCount) : restPosesRef.current.slice(discoveryCount), groupOffset: card.discovery ? 0 : discoveryCount, positionDirty: false };
    dragRef.current = held; setDrag(held); setInputMode("pointer"); setHover(card.id); setSelected(card.id);
  };
  const updateDragPosition = (held: Drag, clientX: number, clientY: number) => {
    const point = toCanvas(clientX, clientY);
    const dx = point.x-held.startX, dy = point.y-held.startY;
    if (held.dx !== dx || held.dy !== dy) held.positionDirty = true;
    held.dx = dx; held.dy = dy;
  };
  const onCardPointerMove = (card: HandCard, e: React.PointerEvent) => {
    const held = dragRef.current; if (!held || held.ending || held.id !== card.id || e.pointerId !== held.pointerId) return;
    updateDragPosition(held, e.clientX, e.clientY);
    if (!held.nativeStarted && Math.hypot(held.dx, held.dy) > 5) beginNativeDrag(held);
    if (held.ready && !dragFrame.current) dragFrame.current = requestAnimationFrame(paintDrag);
  };
  const keyboardRef = useRef(keyboard); keyboardRef.current = keyboard;
  const updateHover = (x: number, y: number) => {
    const state = current.current;
    const oldIndex = state.cards.findIndex((c) => c.id === hoverRef.current);
    const bar = toolbarBounds.current;
    const overToolbar = bar && x >= bar.left && x <= bar.left+bar.width && y >= bar.top && y <= bar.top+bar.height;
    const element = oldIndex >= 0 ? stage.current?.querySelector<HTMLElement>(`[data-hand-id="${hoverRef.current}"]`) : null;
    // Read only the raised card so an in-progress lift keeps its visible hit area.
    const raised = element ? readPose(element) : posesRef.current[oldIndex] ?? null;
    const index = overToolbar ? -1 : hoverGroupedHandIndex(posesRef.current, x, y, raised, oldIndex);
    const id = state.cards[index]?.id ?? null;
    if (hoverRef.current !== id) { hoverRef.current = id; setHover(id); }
  };
  const pointerHandler = ([x, y]: [number, number, boolean]) => {
    const previous = pointer.current; pointer.current = { x, y };
    if (busyRef.current || dragRef.current || !mounted.current || !current.current.expanded) return;
    if (previous && Math.abs(previous.x-x) + Math.abs(previous.y-y) < 1) return;
    if (previous && inputRef.current === "keyboard") { setInputMode("pointer"); inputRef.current = "pointer"; }
    updateHover(x, y);
  };
  const refreshPointerHover = () => {
    if (!pointer.current || inputRef.current !== "pointer" || busyRef.current || dragRef.current
      || !mounted.current || !current.current.expanded) return;
    updateHover(pointer.current.x, pointer.current.y);
  };
  const pointerHandlerRef = useRef(pointerHandler); pointerHandlerRef.current = pointerHandler;
  const escapeRef = useRef(() => {}); escapeRef.current = () => {
    if (dragRef.current) cancelDrag();
    else if (current.current.expanded) void call(current.current.preview ? "end_hand_preview" : "toggle_hand_expanded");
  };
  useEffect(() => {
    let active = true; const stops: Array<() => void> = [];
    void Promise.all([
      onDesktopEvent("hand-pointer", (value) => pointerHandlerRef.current(value)),
      onDesktopEvent("hand-key", (value) => keyboardRef.current(value)),
      onDesktopEvent("hand-escape", () => escapeRef.current()),
      onDesktopEvent("hand-drag-presented", (generation) => {
        const held = dragRef.current;
        if (!held || !held.nativeStarted || held.presented || generation !== current.current.generation) return;
        // Let the drag surface paint one frame before the source disappears.
        requestAnimationFrame(() => {
          if (dragRef.current !== held || held.presented) return;
          held.presented = true; setDrag({ ...held });
        });
      }),
    ]).then((removers) => { if (active) stops.push(...removers); else removers.forEach((stop) => stop()); })
      .catch((e) => { if (active) setError(errorText(e)); });
    const key = (e: KeyboardEvent) => keyboardRef.current(e);
    window.addEventListener("keydown", key);
    return () => { active = false; stops.forEach((stop) => stop()); window.removeEventListener("keydown", key); };
  }, []);
  const lostPointerCapture = (id: string) => { if (dragRef.current?.id === id && !dragRef.current.ending) cancelDrag(); };
  return { cancelDrag, endDrag, onCardPointerDown, onCardPointerMove, lostPointerCapture, refreshPointerHover };
}
