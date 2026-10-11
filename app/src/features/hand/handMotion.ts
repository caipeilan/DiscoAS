const motions = new WeakMap<HTMLElement, Animation>();

export const handRestTransform = "translate(-50%, -50%) translate(var(--hand-x, 0px), var(--hand-y, 0px)) translate(var(--card-x, 0px), var(--card-y, 0px)) rotate(var(--drag-angle, var(--hand-angle, 0deg))) scale(var(--hand-card-scale, 1))";

/** Hand the actual visible frame back to CSS before allowing the next transition. */
export function releaseHandMotion(element: HTMLElement) {
  const animation = motions.get(element);
  if (!animation) return;
  const style = getComputedStyle(element);
  element.style.transform = style.transform;
  element.style.opacity = style.opacity;
  motions.delete(element);
  animation.cancel();
  element.removeAttribute("data-hand-motion");
  // Establish the held frame as the transition's starting value, once per handoff.
  void getComputedStyle(element).transform;
  element.style.removeProperty("transform");
  element.style.removeProperty("opacity");
}

export function animateHandMotion(element: HTMLElement, frames: Keyframe[], options: KeyframeAnimationOptions): Animation {
  releaseHandMotion(element);
  element.setAttribute("data-hand-motion", "");
  const animation = element.animate(frames, { ...options, fill: "both" });
  motions.set(element, animation);
  void animation.finished.then(() => {
    if (motions.get(element) === animation) releaseHandMotion(element);
  }, () => {
    if (motions.get(element) === animation) {
      motions.delete(element);
      element.removeAttribute("data-hand-motion");
    }
  });
  return animation;
}
