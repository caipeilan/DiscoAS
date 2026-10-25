import type { HandCard, HandSettings } from "../../types";

/** Use committed ordinary-hand order, independently of Mini Discovery and drag previews. */
export function autoHandNextId(cards: readonly Pick<HandCard, "id" | "discovery" | "placeholder">[], settings: Pick<HandSettings, "enabled" | "auto_play" | "auto_play_position">, preview = false): string | null {
  if (!settings.enabled || !settings.auto_play || preview) return null;
  if (settings.auto_play_position === "last") {
    for (let i = cards.length - 1; i >= 0; i--) {
      if (!cards[i].discovery && !cards[i].placeholder) return cards[i].id;
    }
  } else {
    for (const card of cards) {
      if (!card.discovery && !card.placeholder) return card.id;
    }
  }
  return null;
}
