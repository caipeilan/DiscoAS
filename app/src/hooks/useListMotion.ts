import { useLayoutEffect, useRef, type RefObject } from "react";

/** Animate actual row changes; filtering and pagination establish a fresh layout. */
export function useListMotion(root: RefObject<HTMLElement | null>, revision: unknown, resetKey: string) {
  const previous = useRef(new Map<string, { node: HTMLElement; top: number; width: number }>());
  const reset = useRef(resetKey);
  const height = useRef(0);
  const initialized = useRef(false);
  useLayoutEffect(() => {
    const parent = root.current;
    if (!parent) { previous.current.clear(); height.current = 0; initialized.current = false; return; }
    const readRows = () => new Map(Array.from(parent.querySelectorAll<HTMLElement>(":scope > [data-motion-key]"))
      .map(node => [node.dataset.motionKey!, { node, top: node.offsetTop, width: node.offsetWidth }]));
    const next = readRows();
    const nextHeight = parent.offsetHeight;
    const animations: Animation[] = [], exits: HTMLElement[] = [];
    let resizing: Animation | undefined;
    const motion = initialized.current && !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (motion && (!!next.size !== !!previous.current.size)) {
      resizing = parent.animate([{ height: `${height.current}px`, overflow: "clip" }, { height: `${nextHeight}px`, overflow: "clip" }], { duration: 180, easing: "ease-out" });
      animations.push(resizing);
    }
    if (motion && reset.current !== resetKey && next.size) {
      animations.push(parent.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 140, easing: "ease-out" }));
    }
    if (motion && (reset.current === resetKey || !next.size)) {
      for (const [key, row] of next) {
        const old = previous.current.get(key);
        if (!old) animations.push(row.node.animate([{ opacity: 0, transform: "translateY(6px)" }, { opacity: 1, transform: "none" }], { duration: 180, easing: "ease-out" }));
        else if (old.top !== row.top) animations.push(row.node.animate([{ transform: `translateY(${old.top - row.top}px)` }, { transform: "none" }], { duration: 220, easing: "ease-out" }));
      }
      for (const [key, row] of previous.current) {
        if (next.has(key)) continue;
        const exit = row.node.cloneNode(true) as HTMLElement;
        exit.removeAttribute("data-motion-key"); exit.inert = true; exit.setAttribute("aria-hidden", "true");
        Object.assign(exit.style, { position: "absolute", top: `${row.top}px`, left: "0", width: `${row.width}px`, pointerEvents: "none", zIndex: "1" });
        parent.append(exit); exits.push(exit);
        const animation = exit.animate([{ opacity: 1 }, { opacity: 0, transform: "translateY(6px)" }], { duration: 180, fill: "forwards" });
        animations.push(animation); void animation.finished.then(() => exit.remove(), () => exit.remove());
      }
    }
    previous.current = next; reset.current = resetKey; height.current = nextHeight; initialized.current = true;
    const observer = new ResizeObserver(() => {
      // Height-only empty-state transitions do not move the rows' own layout.
      if (!animations.some(animation => animation.playState === "running")) {
        previous.current = readRows(); height.current = parent.offsetHeight;
      }
    });
    observer.observe(parent);
    return () => {
      observer.disconnect();
      if (resizing?.playState === "running") height.current = parent.offsetHeight;
      for (const row of previous.current.values()) {
        if (!row.node.isConnected) continue;
        const transform = getComputedStyle(row.node).transform;
        if (transform && transform !== "none") row.top += new DOMMatrixReadOnly(transform).m42;
      }
      animations.forEach(animation => animation.cancel()); exits.forEach(exit => exit.remove());
    };
  }, [root, revision, resetKey]);
}
