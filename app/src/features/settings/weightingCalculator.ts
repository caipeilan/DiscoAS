import type { DiscoveryWeighting } from "../../types";

export function parseSongBasis(text: string): number | null {
  if (!/^\d+$/.test(text.trim())) return null;
  const count = Number(text);
  return Number.isSafeInteger(count) && count > 0 ? count : null;
}

/** Default ranked soft band: 10% of the enabled source, or 20 without one. */
export function recommendedSoftBand(songCount: number): number {
  return Number.isSafeInteger(songCount) && songCount > 0 ? Math.max(1, Math.round(songCount * 0.1)) : 20;
}

/** A nominal N/K cycle sets the cadence; it does not promise complete random coverage. */
export function calculateDiscoveryWeighting(songBasis: number, drawCount: number, enabled = true): DiscoveryWeighting | null {
  if (!Number.isSafeInteger(songBasis) || songBasis < 1 || !Number.isInteger(drawCount) || drawCount < 1 || drawCount > 15) return null;
  const cycle = Math.min(10000, Math.ceil(songBasis / Math.min(songBasis, drawCount)));
  return {
    enabled, baseWeight: 100, discoveredPenalty: 25, selectedPenalty: 50,
    recoveryBatches: Math.max(1, Math.ceil(cycle / 4)), boostAfterBatches: cycle,
    boostPerBatch: Number((100 / cycle).toFixed(4)), maxWeight: 300,
  };
}
