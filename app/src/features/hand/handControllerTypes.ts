import type { Dispatch, SetStateAction } from "react";
import type { HandCard, HandRect, HandSnapshot } from "../../types";
import type { CardPose } from "./handLayout";
export type Ref<T> = { current: T };
export type Setter<T> = Dispatch<SetStateAction<T>>;
export interface Drag {
  id: string; index: number; playable: boolean; pointerId: number;
  startX: number; startY: number; dx: number; dy: number; pose: CardPose; element: HTMLElement;
  intake: number | null; restPoses: CardPose[]; ready: boolean; ending: boolean; prepared: Promise<unknown>;
  card: HandCard; groupPoses: CardPose[]; groupOffset: number; positionDirty: boolean; nativeStarted: boolean;
}
export type Flying = { card: HandCard; pose: CardPose; complete: () => void; collecting?: boolean };
export type Discarding = { card: HandCard; pose: CardPose; delay: number; complete: () => void };
export type ArrivalMotion = { generation: number; element: HTMLElement; animation: Animation | null };
export type ArrivalDrawing = { generation: number; bounds: HandRect };
export interface HandMotionRefs {
  accepted: Ref<number>; unfolding: Ref<boolean>;
  arrivalPlayed: Ref<number>; arrivalMotion: Ref<ArrivalMotion | null>;
  arrivalDrawing: Ref<ArrivalDrawing | null>; returning: Ref<string[]>;
}
export interface HandInputContext {
  snapshot: HandSnapshot; current: Ref<HandSnapshot>; mounted: Ref<boolean>; dockMoving: boolean;
  stage: Ref<HTMLDivElement | null>; busyRef: Ref<string>;
  selectedRef: Ref<string | null>; hoverRef: Ref<string | null>; inputRef: Ref<"pointer" | "keyboard">;
  pointer: Ref<{ x: number; y: number } | null>;
  toolbarBounds: Ref<HandRect | null>; posesRef: Ref<CardPose[]>; restPosesRef: Ref<CardPose[]>;
  dragRef: Ref<Drag | null>; dragFrame: Ref<number>; dragDrawing: Ref<HandRect | null>;
  setDrag: Setter<Drag | null>; setInputMode: Setter<"pointer" | "keyboard">;
  setHover: Setter<string | null>; setSelected: Setter<string | null>;
  setSnapshot: Setter<HandSnapshot>; setError: Setter<string>;
  toCanvas: (x: number, y: number) => { x: number; y: number };
  scheduleRegions: Ref<() => void>;
  commitRegions: Ref<() => Promise<unknown>>;
  operation: (name: "play_hand_card" | "discard_hand_card" | "collect_hand_discovery_card", card: HandCard, pose?: CardPose, insert?: number) => Promise<void>;
}
export function readPose(element: HTMLElement): CardPose {
  const style = getComputedStyle(element), matrix = new DOMMatrixReadOnly(style.transform);
  const width = parseFloat(style.width), height = parseFloat(style.height);
  return { x: parseFloat(style.left) + width / 2 + matrix.e, y: parseFloat(style.top) + height / 2 + matrix.f,
    width, height, angle: Math.atan2(matrix.b, matrix.a) * 180 / Math.PI, scale: Math.hypot(matrix.a, matrix.b) };
}
