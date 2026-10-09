export type KeyStroke = Pick<
  KeyboardEvent,
  | "key"
  | "code"
  | "ctrlKey"
  | "altKey"
  | "shiftKey"
  | "metaKey"
  | "repeat"
  | "isComposing"
>;
export function shortcutFromEvent(event: KeyStroke): string | null {
  if (
    event.repeat ||
    event.isComposing ||
    ["Control", "Alt", "Shift", "Meta", "AltGraph", "Escape"].includes(
      event.key,
    )
  )
    return null;
  const modifiers = [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Super",
  ].filter(Boolean);
  // A plain character would prevent typing everywhere. Function keys are safe standalone.
  if (!modifiers.length && !/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(event.key))
    return null;
  const aliases: Record<string, string> = {
    " ": "Space",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    "+": "Equal",
    "-": "Minus",
  };
  const key = /^Key[A-Z]$/.test(event.code)
    ? event.code.slice(3)
    : /^Digit[0-9]$/.test(event.code)
      ? event.code.slice(5)
      : aliases[event.key] || event.key;
  if (
    !/^(?:[A-Z0-9]|F(?:[1-9]|1[0-9]|2[0-4])|Space|Tab|Enter|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Up|Down|Left|Right|Equal|Minus)$/.test(
      key,
    )
  )
    return null;
  return [...modifiers, key].join("+");
}
