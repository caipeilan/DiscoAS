/** Only transitions that change a hit target's box require a native region scan. */
const REGION_ANIMATIONS = new Set(["hand-refill", "hand-entering", "hand-leave", "hand-play", "hand-discard"]);
type RegionEvent = { target: EventTarget | null; currentTarget: EventTarget | null };
type MatchableTarget = EventTarget & { matches?: (selector: string) => boolean };
const matches = (target: EventTarget | null, selector: string) =>
  !!target && typeof (target as MatchableTarget).matches === "function" && !!(target as MatchableTarget).matches?.(selector);
const isChildEvent = (event: RegionEvent) => !!event.target && event.target !== event.currentTarget;

export function isHandRegionTransition(event: RegionEvent & { propertyName: string }) {
  if (!isChildEvent(event)) return false;
  if (event.propertyName === "transform") return matches(event.target, "[data-hand-id]");
  if (event.propertyName === "width" || event.propertyName === "translate") return matches(event.target, ".hand-dock");
  return false;
}

export function isHandRegionAnimation(event: RegionEvent & { animationName: string }) {
  return isChildEvent(event) && matches(event.target, "[data-hand-id]") && REGION_ANIMATIONS.has(event.animationName);
}
