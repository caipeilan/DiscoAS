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
    const system = window.matchMedia("(prefers-color-scheme: dark)");
    const changed = () => { if (gui.follow_system_theme) applyAppearance(gui); };
    system.addEventListener("change", changed);
    return () => system.removeEventListener("change", changed);
  }, [gui]);
}
