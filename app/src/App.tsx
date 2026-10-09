import { useEffect, useRef, useState } from "react";
import { About } from "./About";
import { Sidebar } from "./components/Sidebar";
import type { Page } from "./components/Sidebar";
import { Toast } from "./components/Toast";
import { LibraryView } from "./features/library/LibraryView";
import { ImportDialog } from "./features/library/ImportDialog";
import { RemoveDialog } from "./features/library/RemoveDialog";
import { RemarkDialog } from "./features/library/RemarkDialog";
import { useLibrary } from "./features/library/useLibrary";
import { DiscoveryView } from "./features/discovery/DiscoveryView";
import { useDiscovery } from "./features/discovery/useDiscovery";
import { Settings } from "./features/settings/Settings";
import { HistoryView } from "./features/history/HistoryView";
import { useSettings } from "./features/settings/useSettings";
import { useAppearance } from "./features/settings/useAppearance";
import { useAppState } from "./hooks/useAppState";
import { useNotice } from "./hooks/useNotice";
import { call } from "./services/desktop";
import { errorText } from "./i18n";
import "./App.css";

const floating = new URLSearchParams(location.search).get("view") === "discover";

/** Composes feature controllers and page views; desktop/feature logic lives in their modules. */
export default function App() {
  const { toast, notice, dismiss } = useNotice();
  const app = useAppState(floating, notice);
  const { state, guiDraft, busy } = app;
  const [page, setPage] = useState<Page>(floating ? "discover" : "library");
  const content = useRef<HTMLElement>(null);
  const pageRef = useRef(page);
  pageRef.current = page;
  const library = useLibrary(app, notice);
  const discovery = useDiscovery({ floating, page: pageRef, reload: app.reload, notice });
  useAppearance(floating ? discovery.previewGui ?? state.guiSettings : guiDraft);
  const settings = useSettings(app, notice);
  const { songs, overlayPhase, viewportWidth, cancel, discover } = discovery;
  useEffect(() => {
    content.current?.scrollTo({ top: 0 });
  }, [page]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (library.importing && !busy) library.closeImport();
      else if (library.remove && !busy) library.closeRemove();
      else if (library.editing && !busy) library.closeRemark();
      else if (floating && !discovery.status.preview && !discovery.previewGui) cancel();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [library.importing, library.remove, library.editing, busy, cancel, discovery.status.preview, discovery.previewGui]);
  const navigate = async (next: Page) => {
    if (page === "discover" && next !== "discover" && songs.length) await cancel();
    setPage(next);
    if (next === "discover") discover();
  };
  const openOverlay = () => {
    call("show_discover").catch((e) => notice(errorText(e), true));
  };
  const manageLibrary = () => {
    if (floating) call("show_main").catch((e) => notice(errorText(e), true));
    else setPage("library");
  };
  const normalCount = state.settings.number_of_discovered_songs;
  const mysteryCount = state.settings.have_mystery_song ? state.settings.num_of_mystery_song : 0;
  return (
    <div className={floating ? `floating-shell overlay-${overlayPhase}` : "app-shell"}>
      {!floating && <Sidebar page={page} playlistCount={state.playlists.length} navigate={navigate} version={state.version} />}
      <main
        ref={content}
        className={floating ? "floating-content" : "main-content"}
        style={floating ? ({
          "--overlay-columns": Math.min(
            5,
            Math.max(1, songs.length || normalCount + mysteryCount),
            Math.max(1, Math.floor((viewportWidth - 55) / (245 * (discovery.previewGui ?? state.guiSettings).card_size))),
          ),
        } as React.CSSProperties) : undefined}
      >
        {!floating && page === "library" && (
          <LibraryView
            state={state} busy={busy} startup={app.startup}
            query={library.query} setQuery={library.setQuery}
            filter={library.filter} setFilter={library.setFilter} filtered={library.filtered}
            sort={library.sort} setSort={library.setSort} selected={library.selected}
            toggleSelected={library.toggleSelected} selectVisible={library.selectVisible}
            updateSelected={library.updateSelected} editRemark={library.editRemark}
            removeSelected={library.removeSelected}
            progress={library.progress} cancelling={library.cancelling}
            cancelOperation={library.cancelOperation} operationSummary={library.operationSummary}
            importDialogOpen={library.importing}
            openImport={library.openImport} confirmRemove={library.confirmRemove}
            migrate={settings.migrate} enablePlaylist={library.enablePlaylist}
            updatePlaylist={library.updatePlaylist} importSample={library.importSample}
          />
        )}
        {page === "discover" && (
          <DiscoveryView state={state} floating={floating} busy={busy} {...discovery}
            openOverlay={openOverlay} manageLibrary={manageLibrary} />
        )}
        {!floating && page === "settings" && (
          <Settings state={state} draft={app.draft} setDraft={app.setDraft}
            guiDraft={guiDraft} setGuiDraft={app.setGuiDraft}
            desktopDraft={app.desktopDraft} setDesktopDraft={app.setDesktopDraft}
            busy={busy} save={settings.saveSettings} migrate={settings.migrate}
            chooseMysteryCover={settings.chooseMysteryCover}
            openDataFolder={settings.openDataFolder} openLogFolder={settings.openLogFolder} />
        )}
        {!floating && page === "history" && <HistoryView />}
        {!floating && page === "about" && <About version={state.version} />}
      </main>
      {toast && <Toast toast={toast} dismiss={dismiss} />}
      {library.importing && (
        <ImportDialog busy={busy === "import"} close={library.closeImport} submit={library.importPlaylist}
          progress={library.progress} cancelling={library.cancelling} cancel={library.cancelOperation} />
      )}
      {library.remove && (
        <RemoveDialog remove={library.remove} busy={busy} close={library.closeRemove} confirm={library.removePlaylist} />
      )}
      {library.editing && <RemarkDialog key={`${library.editing.platform}-${library.editing.kind}-${library.editing.id}`} entry={library.editing} busy={busy === "remark"} close={library.closeRemark} save={library.saveRemark} />}
    </div>
  );
}
