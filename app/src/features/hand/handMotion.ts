const motions = new WeakMap<HTMLElement, Animation>();
const finished = new Map<HTMLElement, Animation>();
let queued = false;

export const handRestTransform = "translate(-50%, -50%) translate(var(--hand-x, 0px), var(--hand-y, 0px)) translate(var(--card-x, 0px), var(--card-y, 0px)) rotate(var(--drag-angle, var(--hand-angle, 0deg))) scale(var(--hand-card-scale, 1))";

/** Hand the actual visible frame back to CSS before allowing the next transition. */
export function releaseHandMotion(element: HTMLElement) {
  handoff([element]);
}

function handoff(elements: HTMLElement[]) {
  const frames = elements.filter(element => motions.has(element)).map(element => {
    const style = getComputedStyle(element);
    return { element, animation: motions.get(element)!, transform: style.transform, opacity: style.opacity };
  });
  for (const { element, animation, transform, opacity } of frames) {
    element.style.transform = transform; element.style.opacity = opacity;
    motions.delete(element); finished.delete(element);
    animation.cancel(); element.removeAttribute("data-hand-motion");
  }
  // One style flush establishes every held frame before CSS resumes.
  if (frames.length) void getComputedStyle(frames[0].element).transform;
  for (const { element } of frames) { element.style.removeProperty("transform"); element.style.removeProperty("opacity"); }
}

export function animateHandMotion(element: HTMLElement, frames: Keyframe[], options: KeyframeAnimationOptions): Animation {
  releaseHandMotion(element);
  element.setAttribute("data-hand-motion", "");
  const animation = element.animate(frames, { ...options, fill: "both" });
  motions.set(element, animation);
  void animation.finished.then(() => {
    if (motions.get(element) !== animation) return;
    finished.set(element, animation);
    if (!queued) {
      queued = true;
      queueMicrotask(() => {
        queued = false;
        handoff([...finished].filter(([node, value]) => motions.get(node) === value).map(([node]) => node));
        finished.clear();
      });
    }
  }, () => {
    if (motions.get(element) === animation) {
      motions.delete(element);
      element.removeAttribute("data-hand-motion");
    }
  });
  return animation;
}
