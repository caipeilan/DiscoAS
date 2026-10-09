import type { SystemFont } from "../types";
export type { SystemFont } from "../types";

export interface FontOption {
  value: string;
  label: string;
  searchTerms: string[];
}

function validName(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0
    && value.length <= 1024 && !/[\u0000-\u001f\u007f]/u.test(value);
}

/** Keep all languages and aliases; callers never need a guessed list of fonts. */
export function normalizeSystemFonts(value: unknown): SystemFont[] {
  if (!Array.isArray(value)) return [];
  const families = new Map<string, SystemFont>();
  for (const item of value) {
    if (!item || typeof item !== "object" || !validName(item.family)) continue;
    const family = item.family.trim();
    const label = validName(item.label) ? item.label.trim() : family;
    const aliases = Array.isArray(item.aliases)
      ? item.aliases.filter(validName).map((alias: string) => alias.trim()) : [];
    const names = [family, label, ...aliases];
    const existing = families.get(family.toLocaleLowerCase());
    if (existing) {
      existing.aliases = [...new Set([...existing.aliases, ...names])];
    } else {
      families.set(family.toLocaleLowerCase(), { family, label, aliases: [...new Set(names)] });
    }
  }
  return [...families.values()];
}

/** Existing preferences, including localized aliases, are preserved verbatim. */
export function fontOptions(
  fonts: SystemFont[],
  current: string,
  systemLabel: string,
  locale: string,
): FontOption[] {
  const collator = new Intl.Collator(locale, { sensitivity: "base", numeric: true });
  const currentKey = current.toLocaleLowerCase();
  let knownCurrent = current === "";
  const options = fonts.map((font) => {
    const selected = current !== "" && [font.family, ...font.aliases]
      .some((name) => name.toLocaleLowerCase() === currentKey);
    if (selected) knownCurrent = true;
    return {
      value: selected ? current : font.family,
      label: font.label,
      searchTerms: [font.family, ...font.aliases],
    };
  }).sort((a, b) => collator.compare(a.label, b.label));
  if (!knownCurrent) options.unshift({ value: current, label: current, searchTerms: [current] });
  return [{ value: "", label: systemLabel, searchTerms: [systemLabel] }, ...options];
}
