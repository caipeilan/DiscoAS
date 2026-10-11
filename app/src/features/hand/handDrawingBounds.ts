import type { HandRect } from "../../types";
import type { CardPose } from "./handLayout";

export function combineHandBounds(rects: Array<HandRect | null | undefined>): HandRect | null {
  const visible = rects.filter((rect): rect is HandRect => !!rect && rect.width > 0 && rect.height > 0);
  if (!visible.length) return null;
  const left = Math.min(...visible.map((rect) => rect.left));
  const top = Math.min(...visible.map((rect) => rect.top));
  return { left, top, width: Math.max(...visible.map((rect) => rect.left + rect.width)) - left,
    height: Math.max(...visible.map((rect) => rect.top + rect.height)) - top };
}

export function handPoseBounds(pose: CardPose, padding = 20): HandRect {
  const radians = pose.angle * Math.PI / 180;
  const width = (Math.abs(Math.cos(radians)) * pose.width + Math.abs(Math.sin(radians)) * pose.height) * pose.scale;
  const height = (Math.abs(Math.sin(radians)) * pose.width + Math.abs(Math.cos(radians)) * pose.height) * pose.scale;
  return { left: pose.x - width / 2 - padding, top: pose.y - height / 2 - padding,
    width: width + padding * 2, height: height + padding * 2 };
}

export function arrivalDrawingBounds(origin: HandRect, pose: CardPose): HandRect {
  const x = origin.left + origin.width / 2, y = origin.top + origin.height / 2;
  const source = { ...pose, x, y, angle: 0, scale: origin.width / pose.width };
  const middle = { ...pose, x: x * .45 + pose.x * .55, y: y * .7 + pose.y * .3 - 80,
    angle: pose.angle - 8, scale: .8 };
  // The whole path stays drawable while the card rotates between its three keyframes.
  const radius = Math.hypot(pose.width, pose.height) * Math.max(source.scale, .8, pose.scale) / 2 + 20;
  return combineHandBounds([source, middle, pose].map((point) => ({
    left: point.x - radius, top: point.y - radius, width: radius * 2, height: radius * 2,
  })))!;
}
