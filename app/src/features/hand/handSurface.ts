export type ArrivalReady = { generation: number; id: string } | null;

/**
 * A card's border/hover highlight must wait for the real surface lifecycle.
 * `arrivalReady` is written only after native presentation, cover decode and
 * the arrival/flip animations of the exact arrival generation have finished.
 * A stale ready value (interrupted/replaced generation) never unlocks the new
 * card. The explicit drag-return path is outside this gate.
 */
export function handSurfaceWaiting(cardId: string, input: {
  generation: number;
  arrivalId?: string | null;
  arrivalReady: ArrivalReady;
  entering: ReadonlySet<string>;
  refreshing: boolean;
}): boolean {
  if (input.entering.has(cardId) || input.refreshing) return true;
  if (input.arrivalId !== cardId) return false;
  return !(input.arrivalReady?.id === cardId && input.arrivalReady.generation === input.generation);
}
