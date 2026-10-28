import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
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
import { CreateHandDialog } from "./features/hand/CreateHandDialog";
import { Onboarding } from "./features/onboarding/Onboarding";
import { useOnboarding } from "./features/onboarding/useOnboarding";
import type { SettingsCategory } from "./features/settings/settingsNavigation";
import { isSettingsCategory } from "./features/settings/settingsNavigation";
import { useConsole } from "./features/console/useConsole";
import "./App.css";

const ConsoleView = lazy(() => import("./features/console/ConsoleView"));

/** Composes feature controllers and page views; desktop/feature logic lives in their modules. */
export default function App() {
  const { toast, notice, dismiss } = useNotice();
  const app = useAppState(false, notice);
  const { state, guiDraft, busy } = app;
  const [page, setPage] = useState<Page>("library");
  const consoleController = useConsole(page === "console");
  const previousPage = useRef<Page>("library");
  useEffect(() => { if (page !== "console") previousPage.current = page; }, [page]);
  const [settingsCategory, setSettingsCategory] = useState<SettingsCategory>(() => {
    const remembered = localStorage.getItem("discoas-settings-category");
    return isSettingsCategory(remembered) ? remembered : "general";
  });
  const [saveError, setSaveError] = useState("");
  const [creatingHand, setCreatingHand] = useState(false);
  const createHandler = useRef(() => {});
  useEffect(() => {
    const subscription = onDesktopEvent("open-create-hand", () => createHandler.current());
    return () => { void subscription.then(stop => stop()).catch(() => {}); };
  }, []);
  const content = useRef<HTMLElement>(null);
  const library = useLibrary(app, notice);
  createHandler.current = () => {
    if (busy) { notice(t("请等待当前操作完成后再创建手牌。"), true); return; }
    library.closeImport(); library.closeRemove(); library.closeRemark(); setCreatingHand(true);
  };
  useAppearance(guiDraft);
  const settings = useSettings(app, notice, setSaveError);
  const reportGuideError = useCallback((error: unknown) => notice(errorText(error), true), [notice]);
  const guide = useOnboarding(reportGuideError);
  const guideStep = guide.state.step;
  const advanceGuide = (next: number) => { void guide.advance(guideStep, next).catch(reportGuideError); };
  const importWasOpen = useRef(false);
  useEffect(() => {
    if ([1, 2, 6, 8, 9].includes(guideStep)) setPage("library");
    if (guideStep === 10) { setSettingsCategory("discovery"); setPage("settings"); }
    if ([2, 6, 8].includes(guideStep)) { library.setQuery(""); library.setFilter(""); }
  }, [guideStep]);
  useEffect(() => {
    if (guideStep === 6 && library.importing) void guide.advance(6, 7).catch(reportGuideError);
    else if (guideStep === 7 && importWasOpen.current && !library.importing) void guide.advance(7, 9).catch(reportGuideError);
    importWasOpen.current = library.importing;
  }, [guideStep, library.importing, guide.advance, reportGuideError]);
  const restartOnboarding = async () => {
    if (busy) return;
    app.setBusy("onboarding");
    try { await call("start_onboarding"); }
    catch (error) { reportGuideError(error); }
    finally { app.setBusy(""); }
  };
  const backGuide = async () => {
    const next = guideStep === 6 ? 5 : guideStep - 1;
    await guide.advance(guideStep, next);
    if (guideStep === 7) library.closeImport();
    if (next === 7) library.openImport();
    if (next >= 3 && next <= 5) await call("show_discover");
    if (next === 9) setPage("library");
  };
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
    if (next === "settings" && guideStep === 9) {
      setSettingsCategory("discovery");
      await guide.advance(9, 10).catch(reportGuideError);
    }
  };
  const consoleAction = useRef<(action: string, params: Record<string, unknown>) => Promise<unknown>>(async () => null);
  consoleAction.current = async (action, params) => {
    const requireIdle = () => { if (busy) throw new Error(t("请等待当前操作完成。")); };
    switch (action) {
      case "clear": consoleController.clear(); return { cleared: true };
      case "console.close": setPage(previousPage.current); return { closed: true };
      case "console.output": consoleController.setFormat(params.format as "text" | "json"); return { format: params.format };
      case "window.navigate": {
        if (typeof params.category === "string" && isSettingsCategory(params.category)) setSettingsCategory(params.category);
        await navigate(params.page as Page); return { page: params.page, category: params.category };
      }
      case "source.import-dialog": requireIdle(); library.openImport(); return { opened: true };
      case "settings.draft.get": return { music: app.draft, gui: guiDraft, desktop: app.desktopDraft };
      case "settings.draft.apply": {
        requireIdle();
        if (!await settings.saveSettings(app.draft, guiDraft, app.desktopDraft)) throw new Error(t("设置未保存，请查看设置页的错误提示。"));
        return { saved: true };
      }
      case "settings.draft.discard": requireIdle(); settings.discardSettings(); return { discarded: true };
      case "onboarding.exit":
        if (params.expectedStep !== undefined && params.expectedStep !== guideStep) throw new Error(t("引导步骤已改变，请重新读取状态。"));
        await guide.advance(guideStep, 0); return { step: 0 };
      case "onboarding.back":
      case "onboarding.next": {
        requireIdle();
        if (params.expectedStep !== undefined && params.expectedStep !== guideStep) throw new Error(t("引导步骤已改变，请重新读取状态。"));
        if (action === "onboarding.back") { if (guideStep <= 1) throw new Error(t("当前没有上一步。")); await backGuide(); return { changed: true }; }
        let next: number;
        if (guideStep === 1 || guideStep === 3 || guideStep === 4) next = guideStep + 1;
        else if (guideStep === 10) next = 0;
        else if (guideStep === 8 && state.sources.some(source => source.enabled && source.platform === guide.state.targetPlatform && source.sourceId === guide.state.targetId && source.sourceKind === guide.state.targetKind)) next = 9;
        else throw new Error(t("这一步需要完成对应操作：发现、选歌或取消、导入、启用歌单、打开设置。"));
        await guide.advance(guideStep, next); return { step: next };
      }
      default: throw new Error(`未知界面操作：${action}`);
    }
  };
  useEffect(() => {
    let alive = true, stop: (() => void) | undefined;
    void onDesktopEvent("console-ui-request", async ({ id, action, params }) => {
      try {
        const data = await consoleAction.current(action, params);
        await call("console_ui_reply", { id, data });
      } catch (error) {
        await call("console_ui_reply", { id, error: { code: "UI_OPERATION_FAILED", message: errorText(error) } });
      }
    }).then(cleanup => { if (alive) stop = cleanup; else cleanup(); });
    return () => { alive = false; stop?.(); };
  }, []);
  const openDiscover = () => {
    call("show_discover").catch((e) => notice(errorText(e), true, { label: t("查看发现设置"), run: () => { setSettingsCategory("discovery"); setPage("settings"); } }));
  };
  const guideView = !app.startup && <Onboarding state={guide.state} surface="main" busy={!!busy}
    alreadyEnabled={state.sources.some(entry => entry.enabled && entry.platform === guide.state.targetPlatform && entry.sourceKind === guide.state.targetKind && entry.sourceId === guide.state.targetId)}
    advance={advanceGuide} back={() => { void backGuide().catch(reportGuideError); }} />;
  const body = (
    <div className="app-shell">
      <Sidebar page={page} playlistCount={state.sources.length} navigate={navigate} version={state.version}
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
            migrate={settings.migrate} enablePlaylist={async entry => {
              const success = await library.enablePlaylist(entry);
              if (success && guideStep === 8 && entry.platform === guide.state.targetPlatform && entry.sourceId === guide.state.targetId && entry.sourceKind === guide.state.targetKind) await guide.advance(8, 9).catch(reportGuideError);
            }}
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
            openDataFolder={settings.openDataFolder} openLogFolder={settings.openLogFolder} restartOnboarding={() => { void restartOnboarding(); }} />
        )}
        {page === "history" && <HistoryView />}
        {page === "console" && <Suspense fallback={<p>{t("正在打开控制台…")}</p>}><ConsoleView controller={consoleController} /></Suspense>}
        {page === "about" && <About version={state.version} />}
        </div>
      </main>
      <Presence show={!!toast}>{toast && <Toast toast={toast} dismiss={dismiss} />}</Presence>
      <Presence show={library.importing} modal>
        <ImportDialog busy={busy === "import"} initialPaths={library.droppedPaths} close={library.closeImport} submit={library.importPlaylist}
          progress={library.progress} cancelling={library.cancelling} cancel={library.cancelOperation} guide={guideStep === 7 ? guideView : undefined} />
      </Presence>
      <Presence show={creatingHand} modal><CreateHandDialog platform={state.sources.find(source => source.enabled)?.platform ?? ""} close={() => setCreatingHand(false)} /></Presence>
      <Presence show={!!library.remove} modal>{library.remove && (
        <RemoveDialog remove={library.remove} busy={busy} close={library.closeRemove} confirm={library.removePlaylist} />
      )}</Presence>
      <Presence show={!!library.editing} modal>{library.editing && <RemarkDialog key={`${library.editing.platform}-${library.editing.sourceKind}-${library.editing.sourceId}`} entry={library.editing} busy={busy === "remark"} close={library.closeRemark} save={library.saveRemark} />}</Presence>
      {!library.importing && guideView}
    </div>
  );
  return <MainWindowFrame onError={(error) => notice(errorText(error), true)}>{body}</MainWindowFrame>;
}
