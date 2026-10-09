import type { DiscoveryWeighting } from "../../types";

export function parseSongBasis(text: string): number | null {
  if (!/^\d+$/.test(text.trim())) return null;
  const count = Number(text);
  return Number.isSafeInteger(count) && count > 0 ? count : null;
}

/** A nominal N/K cycle sets the cadence; it does not promise complete random coverage. */
export function calculateDiscoveryWeighting(songBasis: number, drawCount: number, enabled = true): DiscoveryWeighting | null {
  if (!Number.isSafeInteger(songBasis) || songBasis < 1 || !Number.isInteger(drawCount) || drawCount < 1 || drawCount > 15) return null;
  const cycle = Math.min(10000, Math.ceil(songBasis / Math.min(songBasis, drawCount)));
  return {
    enabled, base_weight: 100, discovered_penalty: 25, selected_penalty: 50,
    recovery_batches: Math.max(1, Math.ceil(cycle / 4)), boost_after_batches: cycle,
    boost_per_batch: Number((100 / cycle).toFixed(4)), max_weight: 300,
  };
}
