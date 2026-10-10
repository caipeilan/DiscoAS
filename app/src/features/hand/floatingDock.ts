import type { HandRect, HandSettings } from "../../types";
import type { CardPose } from "./handLayout";

export interface DockPoint { x: number; y: number }
export type DockDirection = "left" | "right";
const clamp = (value: number, low: number, high: number) => Math.max(low, Math.min(high, value));

export function dockArea(point: DockPoint, areas: HandRect[]): HandRect {
  return areas.reduce((nearest, area) => {
    const distance = (rect: HandRect) => Math.hypot(
      point.x - clamp(point.x, rect.left, rect.left + rect.width),
      point.y - clamp(point.y, rect.top, rect.top + rect.height),
    );
    return distance(area) < distance(nearest) ? area : nearest;
  });
}

export function defaultDockPoint(poses: CardPose[], side: HandSettings["side"], area: HandRect, diameter: number): DockPoint {
  const radius = diameter / 2;
  if (side === "bottom") return { x: area.left + area.width / 2, y: area.top + area.height - radius - 10 };
  const y = poses.length ? Math.min(...poses.map((p) => p.y - p.height / 2)) - radius - 18 : area.top + area.height / 2;
  return { x: side === "left" ? area.left + radius + 10 : area.left + area.width - radius - 10,
    y: clamp(y, area.top + radius + 10, area.top + area.height - radius - 10) };
}

export function floatingDockRect(point: DockPoint, expanded: boolean, diameter: number, areas: HandRect[], direction?: DockDirection) {
  const area = dockArea(point, areas);
  direction ??= point.x < area.left + area.width / 2 ? "right" : "left";
  const width = Math.min(expanded ? diameter * 4.5 : diameter, area.width - 16);
  const left = direction === "right" ? point.x - diameter / 2 : point.x + diameter / 2 - width;
  const top = point.y - diameter / 2;
  return { direction, left, top, width, height: diameter,
    edges: {
      left: left <= area.left,
      right: left + width >= area.left + area.width,
      top: top <= area.top,
      bottom: top + diameter >= area.top + area.height,
    } };
}

export function settleDockPoint(point: DockPoint, diameter: number, areas: HandRect[]): DockPoint {
  const area = dockArea(point, areas), radius = diameter / 2;
  const settle = (value: number, start: number, length: number) => {
    if (value < start + radius + 12) return start + 8;
    if (value > start + length - radius - 12) return start + length - 8;
    return clamp(value, start + radius, start + length - radius);
  };
  return { x: settle(point.x, area.left, area.width), y: settle(point.y, area.top, area.height) };
}
