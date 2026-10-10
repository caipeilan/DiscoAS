import type { SystemFont } from "../types";
export type { SystemFont } from "../types";

export interface FontOption {
  value: string;
  label: string;
  searchTerms: string[];
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
