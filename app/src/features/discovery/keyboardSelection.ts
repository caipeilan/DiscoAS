import type { DiscoveryKeybindings, Song } from "../../types";
import { keybindingMatches } from "../settings/discoveryKeybindings";

export type SelectionDirection = "up" | "left" | "down" | "right";
export type SelectionInputMode = "pointer" | "keyboard";

export function moveCardSelection(index: number, direction: SelectionDirection, count: number, columns: number): number {
  if (count < 1) return -1;
  if (index < 0 || index >= count) return 0;
  const width = Math.max(1, Math.min(count, Math.floor(columns) || 1));
  if (direction === "left") return index % width === 0 ? index : index - 1;
  if (direction === "right") return index % width === width - 1 ? index : Math.min(count - 1, index + 1);
  if (direction === "up") return index < width ? index : index - width;
  return index + width < count ? index + width : (Math.floor(index / width) < Math.floor((count - 1) / width) ? count - 1 : index);
}

export function discoveryBatchKey(songs: Song[], revision: number): string {
  return JSON.stringify([revision, songs.map((song) => [song.platform, song.typename, song.playlistId, song.songId])]);
}

export function keyboardAction(event: KeyboardEvent, bindings: DiscoveryKeybindings): SelectionDirection | "select" | "replace" | null {
  for (const action of ["up", "left", "down", "right", "select", "replace"] as const) {
    if (keybindingMatches(event, bindings[action], true)) return action;
  }
  return null;
}

/** Keeps a selection attached to the exact displayed batch, not just an index. */
export class CardSelection {
  private batch = "";
  private count = 0;
  private keyboardStarted = false;
  private pointerPosition: { x: number; y: number } | null = null;
  inputMode: SelectionInputMode = "pointer";
  index = -1;

  synchronize(batch: string, count: number, active: boolean): boolean {
    const changed = batch !== this.batch || !active;
    if (changed) {
      this.index = -1;
      this.keyboardStarted = false;
      this.inputMode = "pointer";
    }
    this.batch = batch;
    this.count = count;
    return changed;
  }

  point(index: number) {
    this.index = index >= 0 && index < this.count ? index : -1;
    return this.index;
  }

  keyboard() {
    this.inputMode = "keyboard";
  }

  /** Layout changes beneath a stationary pointer must not cancel keyboard feedback. */
  pointer(x: number, y: number, movementX = 0, movementY = 0, pressed = false): boolean {
    const previous = this.pointerPosition;
    this.pointerPosition = { x, y };
    const moved = previous ? x !== previous.x || y !== previous.y : movementX !== 0 || movementY !== 0;
    if (!moved && !pressed) return false;
    this.inputMode = "pointer";
    return true;
  }

  move(direction: SelectionDirection, columns: number) {
    this.keyboard();
    this.index = moveCardSelection(this.keyboardStarted ? this.index : -1, direction, this.count, columns);
    this.keyboardStarted = this.index >= 0;
    return this.index;
  }

  selected(batch: string): number {
    return batch === this.batch && this.index >= 0 && this.index < this.count ? this.index : -1;
  }

  highlighted(batch: string): number {
    return this.inputMode === "keyboard" ? this.selected(batch) : -1;
  }
}
