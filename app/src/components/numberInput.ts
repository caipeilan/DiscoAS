/** Keep integer-only counts separate from continuous preferences. */
export function parseNumberInput(text: string, decimal = false): number | null {
  const normalized = text.trim().replace(",", ".");
  if (!/^-?(?:\d+(?:\.\d*)?|\.\d+)$/.test(normalized)) return null;
  const value = Number(normalized);
  return Number.isFinite(value) && (decimal || Number.isInteger(value)) ? value : null;
}

export function commitNumberInput(text: string, previous: number, min: number, max: number, decimal = false): number {
  const value = parseNumberInput(text, true);
  return value === null ? previous : Math.min(max, Math.max(min, decimal ? value : Math.round(value)));
}

/** Stepping changes the units, rather than snapping fractional input to a native step grid. */
export function stepNumberInput(value: number, direction: 1 | -1, min: number, max: number): number {
  const [mantissa, exponent = "0"] = value.toString().toLowerCase().split("e");
  const digits = Math.max(0, (mantissa.split(".")[1]?.length || 0) - Number(exponent));
  const stepped = Number((value + direction).toFixed(Math.min(15, digits)));
  return Math.min(max, Math.max(min, stepped));
}
