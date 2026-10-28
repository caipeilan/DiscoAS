export const STARTUP_ENTRY_MS = 900;
export const STARTUP_STAGGER_MS = 140;
export const STARTUP_EXIT_DELAY_MS = 1520;
export const STARTUP_EXIT_MS = 720;
export const STARTUP_DURATION_MS = STARTUP_EXIT_DELAY_MS + STARTUP_EXIT_MS;

/** Normalized tanh keeps both ends still while accelerating through the middle. */
export function startupProgress(t: number): number {
  const edge = Math.tanh(2.5);
  return (Math.tanh(5 * (Math.max(0, Math.min(1, t)) - .5)) + edge) / (2 * edge);
}

export function startupEntrance(x: number, y: number): Keyframe[] {
  return Array.from({ length: 61 }, (_, i) => {
    const offset = i / 60, progress = startupProgress(offset);
    const bend = i === 0 || i === 60 ? 0 : Math.sin(Math.PI * progress) * .18;
    return { offset, opacity: progress, transform: `translate(${x * (1 - progress) - y * bend}px, ${y * (1 - progress) + x * bend}px)` };
  });
}

export function startupExit(motion = true): Keyframe[] {
  return Array.from({ length: 61 }, (_, i) => {
    const offset = i / 60, progress = startupProgress(offset);
    const expansion = Math.sin(Math.PI * offset / 2);
    return { offset, opacity: 1 - progress, transform: `scale(${motion ? 1 + .62 * expansion : 1})` };
  });
}
