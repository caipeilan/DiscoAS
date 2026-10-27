import type { HandCard, HandRect, HandSettings, HandDiscoverySettings } from "../../types";

export interface CardPose { x: number; y: number; angle: number; width: number; height: number; scale: number }
const clamp = (v: number, low: number, high: number) => Math.max(low, Math.min(high, v));

export function raisedHandPose(pose: CardPose, side: HandSettings["side"], area: HandRect): CardPose {
  const size = pose.width / 156;
  const dx = pose.width * 1.12 / 2, dy = pose.height * 1.12 / 2;
  return { ...pose, angle: 0, scale: 1.12,
    x: clamp(pose.x + (side === "left" ? 44 * size : side === "right" ? -44 * size : 0), area.left + dx + 12, area.left + area.width - dx - 12),
    y: clamp(pose.y - (side === "bottom" ? 44 * size : 0), area.top + dy + 12, area.top + area.height - dy - 12) };
}

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
export function handLayout(count: number, settings: HandSettings, area: HandRect, active = -1, fontSize = 14): CardPose[] {
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
    } else {
      x = center + offset * step + shift;
      y = bottomY + relative * relative * 28 * size;
      angle = relative * settings.tilt;
    }
    const pose = { x, y, angle, width, height, scale: 1 };
    return i === active ? raisedHandPose(pose, settings.side, area) : pose;
  });
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

export function groupSettings(card: HandCard, hand: HandSettings, discovery?: HandDiscoverySettings): HandSettings {
  return card.discovery && discovery && (!hand.enabled || !discovery.merged) ? { ...hand, ...discovery } : hand;
}

export function groupedHandLayout(cards: HandCard[], hand: HandSettings, discovery: HandDiscoverySettings | undefined, area: HandRect, active = -1, fontSize = 14): CardPose[] {
  const d = cards.filter(c => c.discovery), h = cards.filter(c => !c.discovery);
  if (!discovery || !d.length) return handLayout(cards.length, hand, area, active, fontSize);
  const own = { ...hand, ...discovery };
  if (!h.length && !hand.enabled) return handLayout(cards.length, own, area, active, fontSize);
  if (discovery.merged) return handLayout(cards.length, hand.enabled ? hand : own, area, active, fontSize);
  if (own.side === hand.side && d.length && h.length) {
    const all = handLayout(cards.length + 1, hand, area, active < d.length ? active : active < 0 ? -1 : active + 1, fontSize);
    return all.filter((_, index) => index !== d.length);
  }
  const a = handLayout(d.length, own, area, active < d.length ? active : -1, fontSize);
  const b = handLayout(h.length, hand, area, active >= d.length ? active - d.length : -1, fontSize);
  return a.concat(b);
}

export function reorderHandGroups(cards: HandCard[], id: string, index: number, intake: number | null = null): HandCard[] {
  const chosen = cards.find(c => c.id === id); if (!chosen) return cards;
  const d = cards.filter(c => c.discovery), h = cards.filter(c => !c.discovery);
  if (chosen.discovery && intake !== null) {
    h.splice(intake, 0, { ...chosen, discovery: false }); return d.filter(c => c.id !== id).concat(h);
  }
  return chosen.discovery ? reorderCards(d, id, Math.min(d.length-1, index)).concat(h)
    : d.concat(reorderCards(h, id, Math.max(0, index-d.length)));
}

export function handIntakeIndex(cards: HandCard[], poses: CardPose[], hand: HandSettings, discovery: HandDiscoverySettings | undefined, area: HandRect, x: number, y: number, fontSize = 14): number | null {
  if (!hand.enabled || !discovery?.enabled) return null;
  const count = cards.filter(c => c.discovery).length, held = poses.slice(count);
  const along = hand.side === "bottom" ? x : y;
  const insertion = () => { const index = held.findIndex(p => along < (hand.side === "bottom" ? p.x : p.y)); return index < 0 ? held.length : index; };
  if (discovery.merged) {
    const last = poses[count-1]; if (!last) return null;
    const boundary = hand.side === "bottom" ? last.x : last.y;
    const close = hand.side === "bottom" ? y > area.top+area.height-last.height*1.5
      : hand.side === "left" ? x < area.left+last.width*1.5 : x > area.left+area.width-last.width*1.5;
    return along > boundary+18*last.width/156 && close ? insertion() : null;
  }
  const targets = held.length ? held : handLayout(1, hand, area, -1, fontSize);
  return targets.some(p => Math.abs(x-p.x) < p.width/2+24 && Math.abs(y-p.y) < p.height/2+24) ? insertion() : null;
}

export function hoverHandIndex(poses: CardPose[], side: HandSettings["side"], x: number, y: number, raised: CardPose | null = null, raisedIndex = -1): number {
  const contains = (p: CardPose) => {
    const angle = -p.angle * Math.PI / 180;
    const dx = x-p.x, dy = y-p.y;
    const localX = dx*Math.cos(angle)-dy*Math.sin(angle);
    const localY = dx*Math.sin(angle)+dy*Math.cos(angle);
    return Math.abs(localX) <= p.width*p.scale/2 && Math.abs(localY) <= p.height*p.scale/2;
  };
  const axis = side === "bottom" ? "x" : "y", point = side === "bottom" ? x : y;
  let nearest = -1, distance = Infinity;
  poses.forEach((pose, index) => {
    if (!contains(pose)) return;
    const next = Math.abs(pose[axis] - point);
    if (next < distance) { nearest = index; distance = next; }
  });
  if (nearest >= 0) return nearest;
  return raised && contains(raised) ? raisedIndex : -1;
}

export function hoverGroupedHandIndex(poses: CardPose[], x: number, y: number, raised: CardPose | null, raisedIndex: number): number {
  const contains = (p: CardPose) => {
    const angle = -p.angle*Math.PI/180, dx = x-p.x, dy = y-p.y;
    return Math.abs(dx*Math.cos(angle)-dy*Math.sin(angle)) <= p.width*p.scale/2 && Math.abs(dx*Math.sin(angle)+dy*Math.cos(angle)) <= p.height*p.scale/2;
  };
  // HandWindow paints the active card above all others, then by card order.
  if (raised && raisedIndex >= 0 && contains(raised)) return raisedIndex;
  for (let i = poses.length - 1; i >= 0; i--) {
    if (i !== raisedIndex && contains(poses[i])) return i;
  }
  return -1;
}

export function handReplacementFocusId(cards: HandCard[], focused: string | null,
  replacement: { id: string; index: number } | null): string | null {
  if (cards.some(card => card.id === focused)) return focused;
  if (replacement && focused === replacement.id) {
    return cards.filter(card => card.discovery)[replacement.index]?.id ?? null;
  }
  return null;
}
