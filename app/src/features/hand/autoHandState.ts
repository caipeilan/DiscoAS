export interface AutoHandState { token: number; busy: boolean }
export const idleAutoHandState: AutoHandState = { token: 0, busy: false };
/** Completion wins over a late initial-state reply for the same request. */
export function mergeAutoHandState(current: AutoHandState, next: AutoHandState): AutoHandState {
  if (next.token < current.token || next.token === current.token && !current.busy && next.busy) return current;
  return next.token === current.token && next.busy === current.busy ? current : next;
}
