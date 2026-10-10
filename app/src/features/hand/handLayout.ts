import type { HandCard, HandRect, HandSettings } from "../../types";

export interface CardPose { x: number; y: number; angle: number; width: number; height: number; scale: number }
const clamp = (v: number, low: number, high: number) => Math.max(low, Math.min(high, v));

export function handCardSize(fontSize = 14) {
  // Reserve two title lines and one artist line as the text grows.
  return { width: 156, height: 220 + Math.max(0, fontSize / 14 - 1) * (12 * 1.35 * 2 + 10 * 1.5) };
}

export function mergeHandCards(previous: HandCard[], incoming: HandCard[], order?: string[]): HandCard[] | null {
  if (!order) return incoming;
  const known = new Map(previous.map((card) => [card.id, card]));
  for (const card of incoming) known.set(card.id, card);
  if (order.some((id) => !known.has(id))) return null;
  return order.map((id) => known.get(id)!);
}

/** Coordinates share the native hand surface's logical DPI space. */
export function handLayout(count: number, settings: HandSettings, area: HandRect, active = -1, dock?: HandRect, fontSize = 14): CardPose[] {
  if (!count) return [];
  const vertical = settings.side !== "bottom";
  const margin = Math.min(Math.max(0, settings.edge_distance) + 12, Math.min(area.width, area.height) / 4);
  const edgeMargin = Math.min(settings.edge_distance + 12, Math.min(area.width, area.height) / 4);
  const card = handCardSize(fontSize);
  const size = Math.min(settings.scale, (area.width - 2 * margin) / 300, (area.height - 2 * margin) / (card.height + 120));
  const width = card.width * size, height = card.height * size;
  const radians = settings.tilt * Math.PI / 180;
  const widthAngle = Math.min(radians, Math.atan2(height, width));
  const heightAngle = Math.min(radians, Math.atan2(width, height));
  const halfWidth = (width * Math.cos(widthAngle) + height * Math.sin(widthAngle)) / 2;
  const halfHeight = (height * Math.cos(heightAngle) + width * Math.sin(heightAngle)) / 2;
  const length = vertical ? Math.max(height * 1.12, halfHeight * 2) : Math.max(width * 1.12, halfWidth * 2);
  const axisMargin = margin + 16 * size;
  const available = (vertical ? area.height : area.width) - 2 * axisMargin;
  const step = count < 2 ? 0 : Math.min((vertical ? height : width) * (1 - settings.overlap / 100), Math.max(0, (available - length) / (count - 1)));
  const span = length + step * (count - 1);
  const start = vertical ? area.top : area.left;
  const center = clamp(start + axisMargin + available * settings.position / 100, start + axisMargin + span / 2, start + axisMargin + available - span / 2);
  const bottomY = area.top + area.height - edgeMargin - halfHeight - 28 * size;
  return Array.from({ length: count }, (_, i) => {
    const offset = i - (count - 1) / 2;
    const relative = count < 2 ? 0 : offset / ((count - 1) / 2);
    const shift = active >= 0 && i !== active ? Math.sign(i - active) * 16 * size : 0;
    let x: number, y: number, angle: number;
    if (vertical) {
      x = settings.side === "left" ? area.left + edgeMargin + halfWidth + 20 * size : area.left + area.width - edgeMargin - halfWidth - 20 * size;
      y = center + offset * step + shift;
      angle = (settings.side === "left" ? 1 : -1) * settings.tilt;
      if (i === active) { x += (settings.side === "left" ? 1 : -1) * 44 * size; angle = 0; }
    } else {
      x = center + offset * step + shift;
      y = bottomY + relative * relative * 28 * size;
      angle = relative * settings.tilt;
      if (i === active) { y -= 44 * size; angle = 0; }
    }
    const scale = i === active ? 1.12 : 1;
    if (i === active) {
      const dx = width * scale / 2, dy = height * scale / 2;
      x = clamp(x, area.left + dx + 12, area.left + area.width - dx - 12);
      y = clamp(y, area.top + dy + 12, area.top + area.height - dy - 12);
      if (dock && x+dx > dock.left && x-dx < dock.left+dock.width && y+dy > dock.top && y-dy < dock.top+dock.height) {
        if (settings.side === "bottom") y = Math.max(area.top + dy + 12, dock.top - dy - 12);
        else {
          const inward = settings.side === "left" ? dock.left+dock.width+dx+12 : dock.left-dx-12;
          if (inward >= area.left+dx+12 && inward <= area.left+area.width-dx-12) x = inward;
          else y = clamp(dock.top+dock.height+dy+12, area.top+dy+12, area.top+area.height-dy-12);
        }
      }
    }
    return { x, y, angle, width, height, scale };
  });
}
export function handDock(poses: CardPose[], side: HandSettings["side"], area: HandRect, size: { width: number; height: number }) {
  if (side === "bottom") {
    const center = poses.length ? poses.reduce((sum, p) => sum + p.x, 0) / poses.length : area.left + area.width / 2;
    return { left: clamp(center, area.left + size.width / 2, area.left + area.width - size.width / 2),
      top: area.top + area.height, transform: "translate(-50%, -100%)" };
  }
  const top = poses.length ? Math.min(...poses.map((p) => p.y - p.height / 2)) - 56 : area.top + area.height / 2;
  return { left: side === "left" ? area.left : area.left + area.width,
    top: clamp(top, area.top, area.top + area.height - size.height),
    transform: side === "right" ? "translateX(-100%)" : undefined };
}
export function playDistance(side: HandSettings["side"], dx: number, dy: number): number {
  return side === "bottom" ? -dy : side === "left" ? dx : -dx;
}
export function canPlayDrag(side: HandSettings["side"], dx: number, dy: number, size: number): boolean {
  return playDistance(side, dx, dy) >= 90 * size;
}
export function reorderIndex(poses: CardPose[], side: HandSettings["side"], x: number, y: number): number {
  let nearest = 0, distance = Infinity;
  poses.forEach((p, i) => { const d = Math.abs(side === "bottom" ? x - p.x : y - p.y); if (d < distance) { nearest = i; distance = d; } });
  return nearest;
}
export function reorderCards<T extends { id: string }>(cards: T[], id: string, index: number): T[] {
  const from = cards.findIndex((card) => card.id === id);
  if (from < 0 || from === index) return cards;
  const reordered = cards.slice();
  reordered.splice(index, 0, reordered.splice(from, 1)[0]);
  return reordered;
}

export function hoverHandIndex(poses: CardPose[], side: HandSettings["side"], x: number, y: number, raised: CardPose | null = null, raisedIndex = -1): number {
  const contains = (p: CardPose) => {
    const angle = -p.angle * Math.PI / 180;
    const dx = x-p.x, dy = y-p.y;
    const localX = dx*Math.cos(angle)-dy*Math.sin(angle);
    const localY = dx*Math.sin(angle)+dy*Math.cos(angle);
    return Math.abs(localX) <= p.width*p.scale/2 && Math.abs(localY) <= p.height*p.scale/2;
  };
  const candidates = poses.map((p, i) => ({ p, i })).filter(({ p }) => contains(p));
  if (candidates.length) return candidates.reduce((nearest, candidate) => {
    const axis = side === "bottom" ? "x" : "y";
    const point = side === "bottom" ? x : y;
    return Math.abs(candidate.p[axis]-point) < Math.abs(nearest.p[axis]-point) ? candidate : nearest;
  }).i;
  return raised && contains(raised) ? raisedIndex : -1;
}
