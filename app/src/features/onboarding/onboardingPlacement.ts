type Rect = { left: number; top: number; right: number; bottom: number };

/** DOM rectangles are in viewport pixels; fixed offsets inherit the interface zoom. */
export function placeOnboarding(target: Rect | null, size: { width: number; height: number },
  viewport: { width: number; height: number; top: number }, scale: number, centered = false) {
  const margin = 16 * scale;
  const width = size.width * scale, height = size.height * scale;
  const minTop = viewport.top + margin;
  let left = centered ? (viewport.width - width) / 2 : target?.left ?? margin;
  let top = centered ? (viewport.height + viewport.top - height) / 2 : target ? target.bottom + margin : viewport.height - height - margin;
  if (target && !centered) {
    if (top + height > viewport.height - margin) top = target.top - height - margin;
    if (top < minTop) { top = viewport.height - height - margin; left = target.right + margin; }
  }
  return {
    left: Math.max(margin, Math.min(left, viewport.width - width - margin)) / scale,
    top: Math.max(minTop, Math.min(top, viewport.height - height - margin)) / scale,
  };
}
