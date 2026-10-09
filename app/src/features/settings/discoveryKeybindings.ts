import type { DiscoveryKeybindings } from "../../types";
import type { KeyStroke } from "../../shortcut";

export const defaultDiscoveryKeybindings: DiscoveryKeybindings = {
  up: "W", left: "A", down: "S", right: "D", select: "Enter", replace: "R",
};
const asciiUpper = (value: string) => value.replace(/[a-z]/g, (letter) => letter.toUpperCase());

/** Names correspond to physical keys, so changing the typing language does not change controls. */
export function normalizeKeybinding(binding: string): string | null {
  if (typeof binding !== "string") return null;
  const parts = binding.split("+").map((part) => part.trim());
  const base = asciiUpper(parts.pop() || "");
  if (!base) return null;
  const aliases: Record<string, string> = {
    UP: "ArrowUp", ARROWUP: "ArrowUp", DOWN: "ArrowDown", ARROWDOWN: "ArrowDown",
    LEFT: "ArrowLeft", ARROWLEFT: "ArrowLeft", RIGHT: "ArrowRight", ARROWRIGHT: "ArrowRight",
    ENTER: "Enter", SPACE: "Space",
  };
  const key = /^[A-Z0-9]$/.test(base) || /^F(?:[1-9]|1[0-2])$/.test(base) ? base : aliases[base];
  if (!key) return null;
  const modifiers = new Set<string>();
  const modifierNames: Record<string, string> = { CTRL: "Ctrl", ALT: "Alt", SHIFT: "Shift" };
  for (const part of parts) {
    const name = modifierNames[asciiUpper(part)];
    if (!name || modifiers.has(name)) return null;
    modifiers.add(name);
  }
  return [...["Ctrl", "Alt", "Shift"].filter((name) => modifiers.has(name)), key].join("+");
}

export function keybindingFromEvent(event: KeyStroke, allowRepeat = false): string | null {
  if ((!allowRepeat && event.repeat) || event.isComposing || event.metaKey || ["Escape", "Process", "AltGraph"].includes(event.key)) return null;
  const base = /^Key[A-Z]$/.test(event.code) ? event.code.slice(3)
    : /^(?:Digit|Numpad)[0-9]$/.test(event.code) ? event.code.slice(event.code.startsWith("Digit") ? 5 : 6)
      : event.code === "NumpadEnter" ? "Enter" : event.code;
  return normalizeKeybinding([
    event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift", base,
  ].filter(Boolean).join("+"));
}

export function keybindingMatches(event: KeyStroke, binding: string, allowRepeat = false): boolean {
  const accepted = normalizeKeybinding(binding);
  return accepted !== null && accepted === keybindingFromEvent(event, allowRepeat);
}

function normalizeGlobalShortcut(binding: string): string | null {
  const parts = binding.split("+").map((part) => part.trim());
  const base = asciiUpper(parts.pop() || "");
  const key = /^KEY[A-Z]$/.test(base) ? base.slice(3)
    : /^DIGIT[0-9]$/.test(base) ? base.slice(5) : base;
  const modifiers = parts.map((part) => {
    const name = asciiUpper(part);
    if (["CONTROL", "CTRL", "COMMANDORCONTROL", "COMMANDORCTRL", "CMDORCONTROL", "CMDORCTRL"].includes(name)) return "Ctrl";
    if (["OPTION", "ALT"].includes(name)) return "Alt";
    return part;
  });
  return normalizeKeybinding([...modifiers, key].join("+"));
}

export function validateDiscoveryKeybindings(value: DiscoveryKeybindings, globalShortcut = ""): string | null {
  const unique = new Set<string>();
  for (const action of ["up", "left", "down", "right", "select", "replace"] as const) {
    const binding = normalizeKeybinding(value[action]);
    if (!binding) return "错误：选歌按键无效";
    if (unique.has(binding)) return "错误：选歌按键不能重复";
    unique.add(binding);
  }
  const global = normalizeGlobalShortcut(globalShortcut);
  if (global && unique.has(global)) return "错误：选歌按键不能与全局快捷键重复";
  return null;
}
