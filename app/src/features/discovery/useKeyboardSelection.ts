import { useCallback, useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent, RefObject } from "react";
import type { DiscoveryKeybindings, Song } from "../../types";
import { CardSelection, discoveryBatchKey, keyboardAction } from "./keyboardSelection";

function gridColumns(grid: HTMLDivElement | null): number {
  const slots = grid ? Array.from(grid.querySelectorAll<HTMLDivElement>(".song-card-slot")) : [];
  if (!slots.length) return 1;
  const top = slots[0].offsetTop;
  const nextRow = slots.findIndex((slot) => Math.abs(slot.offsetTop - top) > 1);
  return nextRow === -1 ? slots.length : Math.max(1, nextRow);
}

function typingOrDialog(target: EventTarget | null): boolean {
  if (document.querySelector('[role="dialog"][aria-modal="true"], dialog[open]')) return true;
  if (!(target instanceof Element)) return false;
  return Boolean(target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"], [role="dialog"], [role="listbox"]'));
}

/** Navigates only the current visible batch and never selects a song implicitly. */
export type PreviewKeyEvent = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey" | "repeat" | "isComposing"> & {
  source: "native";
  action?: "up" | "left" | "down" | "right" | "select" | "replace";
};

export function useKeyboardSelection({ songs, keybindings, grid, active, batchRevision, play, replace, preview = false, externalPointer, previewKey, preserveSlots = false }: {
  songs: Song[];
  keybindings: DiscoveryKeybindings;
  grid: RefObject<HTMLDivElement | null>;
  active: boolean;
  batchRevision: number;
  play: (song: Song) => Promise<void>;
  replace?: (song: Song) => Promise<void>;
  preview?: boolean;
  externalPointer?: { x: number; y: number } | null;
  previewKey?: { event: PreviewKeyEvent; sequence: number } | null;
  preserveSlots?: boolean;
}) {
  const selection = useRef(new CardSelection());
  const [, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((revision) => revision + 1), []);
  const batch = preserveSlots
    ? JSON.stringify([batchRevision, songs.length, songs[0]?.platform, songs[0]?.typename, songs[0]?.playlistId])
    : discoveryBatchKey(songs, batchRevision);
  selection.current.synchronize(batch, songs.length, active);
  const current = useRef({ songs, keybindings, active, batch, play, replace, preview });
  current.current = { songs, keybindings, active, batch, play, replace, preview };
  useEffect(refresh, [batch, active, refresh]);
  const pointer = useCallback((event: ReactPointerEvent<HTMLDivElement>, pressed: boolean) => {
    if (!current.current.active || (!pressed && event.pointerType !== "mouse" && event.pointerType !== "pen")) return;
    const model = selection.current;
    const previousMode = model.inputMode;
    const previousIndex = model.index;
    if (!model.pointer(event.clientX, event.clientY, event.movementX, event.movementY, pressed)) return;
    const cards = grid.current ? Array.from(grid.current.querySelectorAll<HTMLButtonElement>(".song-card")) : [];
    const target = event.target instanceof Element ? event.target : null;
    const card = target?.closest<HTMLButtonElement>(".song-card")
      ?? target?.closest(".song-card-slot")?.querySelector<HTMLButtonElement>(".song-card");
    model.point(card ? cards.indexOf(card) : -1);
    if (model.inputMode !== previousMode || model.index !== previousIndex) refresh();
  }, [grid, refresh]);
  const pointerMove = useCallback((event: ReactPointerEvent<HTMLDivElement>) => pointer(event, false), [pointer]);
  const pointerDown = useCallback((event: ReactPointerEvent<HTMLDivElement>) => pointer(event, true), [pointer]);
  const pointerLeave = useCallback(() => {
    if (selection.current.inputMode !== "pointer" || selection.current.index < 0) return;
    selection.current.point(-1);
    refresh();
  }, [refresh]);
  useEffect(() => {
    if (!preview || !active || !externalPointer) return;
    const model = selection.current;
    const previousIndex = model.index;
    const previousMode = model.inputMode;
    const moved = model.pointer(externalPointer.x, externalPointer.y);
    if (moved || model.inputMode === "pointer") {
      const slots = grid.current ? Array.from(grid.current.querySelectorAll<HTMLDivElement>(".song-card-slot")) : [];
      model.point(slots.findIndex((slot) => {
        const rect = slot.getBoundingClientRect();
        return externalPointer.x >= rect.left && externalPointer.x <= rect.right
          && externalPointer.y >= rect.top && externalPointer.y <= rect.bottom;
      }));
    }
    if (previousIndex !== model.index || previousMode !== model.inputMode) refresh();
  }, [active, batch, externalPointer, grid, preview, refresh]);
  const focusIndex = useCallback((index: number) => {
    // Keyboard navigation sets the mode before calling focus; mouse/programmatic focus does not.
    if (!current.current.active || selection.current.inputMode !== "keyboard") return;
    if (selection.current.index === index) return;
    selection.current.point(index);
    refresh();
  }, [refresh]);
  const handleKey = useCallback((event: KeyboardEvent, native = false) => {
      const latest = current.current;
      if (!latest.active || event.defaultPrevented || event.isComposing
        || (!native && (latest.preview || document.visibilityState === "hidden" || typingOrDialog(event.target)))) return;
      if (event.key === "Tab") {
        selection.current.keyboard();
        refresh();
        return;
      }
      const action = native
        ? (event as unknown as PreviewKeyEvent).action || keyboardAction(event, latest.keybindings)
        : keyboardAction(event, latest.keybindings);
      if (!action) {
        // Native button activation must respect a customized confirmation key.
        if ((event.key === "Enter" || event.key === " ") && event.target instanceof Element && event.target.closest(".song-card")) event.preventDefault();
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      if (action === "select") {
        const index = selection.current.highlighted(latest.batch);
        if (!latest.preview && !event.repeat && index >= 0) void latest.play(latest.songs[index]);
        return;
      }
      if (action === "replace") {
        const index = selection.current.selected(latest.batch);
        if (!latest.preview && !event.repeat && index >= 0) void latest.replace?.(latest.songs[index]);
        return;
      }
      const index = selection.current.move(action, gridColumns(grid.current));
      refresh();
      const card = grid.current?.querySelectorAll<HTMLButtonElement>(".song-card")[index];
      if (!card || !grid.current) return;
      if (!latest.preview) card.focus({ preventScroll: true });
      const rect = card.getBoundingClientRect();
      const frame = grid.current.getBoundingClientRect();
      if (rect.top < Math.max(0, frame.top) || rect.bottom > Math.min(window.innerHeight, frame.bottom)
        || rect.left < Math.max(0, frame.left) || rect.right > Math.min(window.innerWidth, frame.right)) {
        card.scrollIntoView({
          block: "nearest", inline: "nearest",
          behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth",
        });
      }
    }, [grid, refresh]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => handleKey(event);
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [handleKey]);
  useEffect(() => {
    if (!preview || !previewKey) return;
    handleKey({ ...previewKey.event, target: null, defaultPrevented: false,
      preventDefault() {}, stopPropagation() {} } as unknown as KeyboardEvent, true);
  }, [handleKey, preview, previewKey]);
  return { selectedIndex: active ? selection.current.highlighted(batch) : -1,
    pointedIndex: active && selection.current.inputMode === "pointer" ? selection.current.selected(batch) : -1,
    inputMode: selection.current.inputMode, pointerMove, pointerDown, pointerLeave, focusIndex };
}
