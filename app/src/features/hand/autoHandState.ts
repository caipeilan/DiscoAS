export interface AutoHandState { token: number; busy: boolean; configured?: boolean; enabled?: boolean; revision?: number }
export const idleAutoHandState: AutoHandState = { token: 0, busy: false };
/** Completion wins over a late initial-state reply for the same request. */
export function mergeAutoHandState(current: AutoHandState, next: AutoHandState): AutoHandState {
  if ((next.revision ?? 0) < (current.revision ?? 0)) return current;
  if ((next.revision ?? 0) > (current.revision ?? 0)) return next;
  if (next.token < current.token || next.token === current.token && !current.busy && next.busy) return current;
  return next.token === current.token && next.busy === current.busy && next.enabled === current.enabled && next.configured === current.configured ? current : next;
}
