import { useEffect, useRef } from "react";
import { DiscoveryView } from "./DiscoveryView";
import { useDiscovery } from "./useDiscovery";
import { useOverlayPreferences } from "./useOverlayPreferences";
import { useAppearance } from "../settings/useAppearance";
import { useNotice } from "../../hooks/useNotice";
import { Toast } from "../../components/Toast";
import { Presence } from "../../components/Motion";
import { call } from "../../services/desktop";
import { errorText } from "../../i18n";
import "../../App.css";

/** A dedicated entry keeps library editors and settings drafts in the main window. */
export default function DiscoverWindow() {
  const { toast, notice, dismiss } = useNotice();
  const { preferences, reload } = useOverlayPreferences(notice);
  const page = useRef("discover");
  const discovery = useDiscovery({ floating: true, page, reload, notice, hand: preferences.settings.hand });
  const gui = discovery.previewGui ?? preferences.guiSettings;
  useAppearance(gui);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !discovery.status.preview && !discovery.previewGui) void discovery.cancel();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [discovery.cancel, discovery.status.preview, discovery.previewGui]);
  const state = { ...preferences, playlists: preferences.settings.playlist_albums.map((source) => ({
    platform: source.name, enabled: source.enabled,
  })) };
  const show = (name: "show_main" | "show_discover") => {
    void call(name).catch((e) => notice(errorText(e), true));
  };
  return <div className={`floating-shell overlay-${discovery.overlayPhase}`}>
    <main className="floating-content">
      <DiscoveryView state={state} {...discovery}
        manageLibrary={() => show("show_main")} />
    </main>
    <Presence show={!!toast}>{toast && <Toast toast={toast} dismiss={dismiss} />}</Presence>
  </div>;
}
