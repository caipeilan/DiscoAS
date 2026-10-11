import { useCallback, useEffect, useRef, useState } from "react";
import type { DiscoveryPreferences, PreferencePatch } from "../../types";
import { emptyState } from "../../types";
import { call, desktop, onDesktopEvent } from "../../services/desktop";
import type { Notice } from "../../hooks/useNotice";
import { errorText } from "../../i18n";

/** The discovery window needs saved music/appearance preferences, not library summaries. */
export function useOverlayPreferences(notice: Notice) {
  const [preferences, setPreferences] = useState<DiscoveryPreferences>({
    settings: emptyState.settings, guiSettings: emptyState.guiSettings,
  });
  const latest = useRef(preferences); latest.current = preferences;
  const revision = useRef(0);
  const apply = useCallback((patch: PreferencePatch) => {
    const next = { settings: patch.settings ?? latest.current.settings,
      guiSettings: patch.guiSettings ?? latest.current.guiSettings };
    latest.current = next; setPreferences(next);
  }, []);
  const reload = useCallback(async () => {
    if (!desktop) return;
    const stamp = ++revision.current, before = latest.current;
    const next = await call<DiscoveryPreferences>("get_discovery_preferences");
    if (stamp !== revision.current) return;
    apply({
      settings: latest.current.settings === before.settings ? next.settings : latest.current.settings,
      guiSettings: latest.current.guiSettings === before.guiSettings ? next.guiSettings : latest.current.guiSettings,
    });
  }, [apply]);
  useEffect(() => {
    if (!desktop) return;
    let active = true; const stops: Array<() => void> = [];
    void Promise.all([
      onDesktopEvent("preferences-changed", (patch) => { if (active) apply(patch); }),
      onDesktopEvent("gui-changed", (patch) => {
        if (active) { if (patch) apply(patch); else void reload().catch((e) => notice(errorText(e), true)); }
      }),
    ]).then((removers) => {
      if (!active) { removers.forEach((stop) => stop()); return; }
      stops.push(...removers); return reload();
    }).catch((e) => { if (active) notice(errorText(e), true); });
    return () => { active = false; revision.current++; stops.forEach((stop) => stop()); };
  }, [apply, reload, notice]);
  return { preferences, reload };
}
