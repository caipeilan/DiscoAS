export function placePopover(rect: { left: number; top: number; right: number; bottom: number; width: number },
  viewport: { width: number; height: number }, width: number, scale = 1) {
  const margin = 8, gap = 5;
  const availableWidth = Math.max(1, viewport.width - margin * 2);
  const acceptedWidth = Math.min(width, availableWidth);
  const below = Math.max(0, viewport.height - rect.bottom - margin - gap);
  const above = Math.max(0, rect.top - margin - gap);
  const upwards = below < 160 * scale && above > below;
  return {
    left: Math.max(margin, Math.min(rect.left, viewport.width - margin - acceptedWidth)),
    width: acceptedWidth,
    maxHeight: Math.min(280 * scale, upwards ? above : below),
    ...(upwards ? { bottom: Math.max(margin, viewport.height - rect.top + gap) } : { top: Math.max(margin, rect.bottom + gap) }),
  };
}
