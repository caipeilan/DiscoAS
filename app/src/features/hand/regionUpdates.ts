/** One in-flight update, followed by the latest requested state. Callers await the flush. */
export class LatestRegionUpdate {
  private pending: Promise<void> | null = null;
  private dirty = false;
  constructor(private readonly update: () => Promise<unknown>) {}
  run(): Promise<void> {
    this.dirty = true;
    if (!this.pending) {
      this.pending = Promise.resolve().then(async () => {
        try {
          while (this.dirty) { this.dirty = false; await this.update(); }
        } finally { this.pending = null; }
      });
    }
    return this.pending;
  }
}

type MeasuredRect = Pick<DOMRect, "left" | "top" | "right" | "bottom" | "width" | "height">;
type MeasurableCard = { getBoundingClientRect(): MeasuredRect; getAnimations(): Array<{ playState: string; pending: boolean }> };
/** Static card geometry survives animation frames; a moving card is read through its final frame. */
export class HandBoundsCache {
  private cache = new WeakMap<MeasurableCard, MeasuredRect>();
  reset() { this.cache = new WeakMap(); }
  read(element: MeasurableCard, dragging = false): MeasuredRect {
    const moving = dragging || element.getAnimations().some((a) => a.playState === "running" || a.pending);
    if (!moving) { const cached = this.cache.get(element); if (cached) return cached; }
    const rect = element.getBoundingClientRect();
    if (moving) this.cache.delete(element); else this.cache.set(element, rect);
    return rect;
  }
}
