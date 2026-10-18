import { useEffect, useRef, useState } from "react";
import { About } from "./About";
import { Sidebar } from "./components/Sidebar";
import type { Page } from "./components/Sidebar";
import { Toast } from "./components/Toast";
import { Presence } from "./components/Motion";
import { MainWindowFrame } from "./components/MainWindowFrame";
import { LibraryView } from "./features/library/LibraryView";
import { ImportDialog } from "./features/library/ImportDialog";
import { RemoveDialog } from "./features/library/RemoveDialog";
import { RemarkDialog } from "./features/library/RemarkDialog";
import { useLibrary } from "./features/library/useLibrary";
import { Settings } from "./features/settings/Settings";
import { HistoryView } from "./features/history/HistoryView";
import { useSettings } from "./features/settings/useSettings";
import { useAppearance } from "./features/settings/useAppearance";
import { useAppState } from "./hooks/useAppState";
import { useNotice } from "./hooks/useNotice";
import { call, onDesktopEvent } from "./services/desktop";
import { t, errorText } from "./i18n";
import type { SettingsCategory } from "./features/settings/settingsNavigation";
import { isSettingsCategory } from "./features/settings/settingsNavigation";
import "./App.css";

/** Composes feature controllers and page views; desktop/feature logic lives in their modules. */
export default function App() {
  const { toast, notice, dismiss } = useNotice();
  const app = useAppState(false, notice);
  const { state, guiDraft, busy } = app;
  const [page, setPage] = useState<Page>("library");
  const [settingsCategory, setSettingsCategory] = useState<SettingsCategory>(() => {
    const remembered = localStorage.getItem("discoas-settings-category");
    return isSettingsCategory(remembered) ? remembered : "general";
  });
  const [saveError, setSaveError] = useState("");
  const content = useRef<HTMLElement>(null);
  const library = useLibrary(app, notice);
  useAppearance(guiDraft);
  const settings = useSettings(app, notice, setSaveError);
  useEffect(() => {
    let active = true, stop: (() => void) | undefined;
    void onDesktopEvent("open-settings-requested", ({ category }) => {
      setSettingsCategory(category); setPage("settings");
    }).then(cleanup => { if (active) stop = cleanup; else cleanup(); })
      .catch(error => notice(errorText(error), true));
    return () => { active = false; stop?.(); };
  }, [notice]);
  useEffect(() => {
    content.current?.scrollTo({ top: 0 });
  }, [page, settingsCategory]);
  useEffect(() => { localStorage.setItem("discoas-settings-category", settingsCategory); }, [settingsCategory]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (library.importing && !busy) library.closeImport();
      else if (library.remove && !busy) library.closeRemove();
      else if (library.editing && !busy) library.closeRemark();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [library.importing, library.remove, library.editing, busy]);
  const navigate = async (next: Page) => {
    setPage(next);
  };
  const openDiscover = () => {
    call("show_discover").catch((e) => notice(errorText(e), true, { label: t("查看发现设置"), run: () => { setSettingsCategory("discovery"); setPage("settings"); } }));
  };
  const body = (
    <div className="app-shell">
      <Sidebar page={page} playlistCount={state.playlists.length} navigate={navigate} version={state.version}
        settingsCategory={settingsCategory} selectSettingsCategory={setSettingsCategory} />
      <main
        ref={content}
        className="main-content"
      >
        <div key={page} className="page-body">
        {page === "library" && (
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
            discover={() => { openDiscover(); }}
          />
        )}
        {page === "settings" && (
          <Settings state={state} draft={app.draft} setDraft={app.setDraft}
            guiDraft={guiDraft} setGuiDraft={app.setGuiDraft}
            desktopDraft={app.desktopDraft} setDesktopDraft={app.setDesktopDraft}
            busy={busy} save={settings.saveSettings} migrate={settings.migrate}
            discard={settings.discardSettings} saveError={saveError} clearSaveError={() => setSaveError("")} category={settingsCategory} selectCategory={setSettingsCategory}
            chooseMysteryCover={settings.chooseMysteryCover}
            openDataFolder={settings.openDataFolder} openLogFolder={settings.openLogFolder} />
        )}
        {page === "history" && <HistoryView />}
        {page === "about" && <About version={state.version} />}
        </div>
      </main>
      <Presence show={!!toast}>{toast && <Toast toast={toast} dismiss={dismiss} />}</Presence>
      <Presence show={library.importing} modal>
        <ImportDialog busy={busy === "import"} initialPaths={library.droppedPaths} close={library.closeImport} submit={library.importPlaylist}
          progress={library.progress} cancelling={library.cancelling} cancel={library.cancelOperation} />
      </Presence>
      <Presence show={!!library.remove} modal>{library.remove && (
        <RemoveDialog remove={library.remove} busy={busy} close={library.closeRemove} confirm={library.removePlaylist} />
      )}</Presence>
      <Presence show={!!library.editing} modal>{library.editing && <RemarkDialog key={`${library.editing.platform}-${library.editing.kind}-${library.editing.id}`} entry={library.editing} busy={busy === "remark"} close={library.closeRemark} save={library.saveRemark} />}</Presence>
    </div>
  );
  return <MainWindowFrame onError={(error) => notice(errorText(error), true)}>{body}</MainWindowFrame>;
}
