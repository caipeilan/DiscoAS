import { useLayoutEffect, useState } from "react";
import type { RefObject } from "react";

export function decodedHandBytes(stage: Pick<HTMLElement, "querySelectorAll"> | null) {
  const images = new Map<string, number>();
  for (const image of stage?.querySelectorAll<HTMLImageElement>("img") ?? []) {
    images.set(image.currentSrc || image.src, image.naturalWidth * image.naturalHeight * 4);
  }
  return [...images.values()].reduce((sum, bytes) => sum + bytes, 0);
}

/** Keep a recently folded DOM warm; native visibility and hit regions still follow cardsMounted. */
export function useRetainedHandCards(mounted: boolean, hidden: boolean, stage: RefObject<HTMLDivElement | null>, generation: number) {
  const [retained, setRetained] = useState(false);
  useLayoutEffect(() => {
    if (hidden) { setRetained(false); return; }
    if (mounted) { setRetained(true); return; }
    if (decodedHandBytes(stage.current) > 32 * 1024 * 1024) { setRetained(false); return; }
    const timer = setTimeout(() => setRetained(false), 2000);
    return () => clearTimeout(timer);
  }, [mounted, hidden, generation]);
  return mounted || (!hidden && retained);
}
