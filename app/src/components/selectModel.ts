export interface SelectOption { value: string; label: string; disabled?: boolean; searchTerms?: string[] }
export function filterOptions(options: SelectOption[], query: string): SelectOption[] {
  const needle = query.trim().toLocaleLowerCase();
  return !needle ? options : options.filter((option) => [option.label, option.value, ...(option.searchTerms || [])].some((term) => term.toLocaleLowerCase().includes(needle)));
}
export function moveOption(options: SelectOption[], current: number, direction: 1 | -1): number {
  if (!options.length) return -1;
  for (let step = 1; step <= options.length; step++) {
    const index = ((current + direction * step) % options.length + options.length) % options.length;
    if (!options[index].disabled) return index;
  }
  return -1;
}
