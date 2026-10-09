import { useEffect } from "react";
import type { GuiSettings } from "../../types";
import { applyAppearance } from "../../appearance";
import { currentLocale, setLanguage } from "../../i18n";

export function useAppearance(gui: GuiSettings) {
  setLanguage(gui.language);
  useEffect(() => {
    document.documentElement.lang = currentLocale();
  }, [gui.language]);
  useEffect(() => {
    applyAppearance(gui);
  }, [gui]);
}
