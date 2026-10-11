import { useEffect, useState, type ReactNode } from "react";
import { Icon } from "../Icon";
import { t } from "../i18n";
import { mainWindowAction, watchMainWindowState } from "../services/desktop";
import "./mainWindow.css";

export function MainWindowFrame({ children, onError }: { children: ReactNode; onError: (error: unknown) => void }) {
  const [windowState, setWindowState] = useState({ maximized: false, focused: true });
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void watchMainWindowState((next) => { if (!disposed) setWindowState(next); }, onError)
      .then((unsubscribe) => { if (disposed) unsubscribe(); else stop = unsubscribe; }).catch(onError);
    return () => { disposed = true; stop?.(); };
  }, []);
  const act = (action: "minimize" | "maximize" | "close" | "drag") => {
    void mainWindowAction(action).catch(onError);
  };
  return <div className="main-window" data-maximized={windowState.maximized || undefined} data-inactive={!windowState.focused || undefined}>
    <header className="window-titlebar">
      <div className="window-drag-area" onMouseDown={(event) => {
        if (event.button === 0) { event.preventDefault(); act(event.detail === 2 ? "maximize" : "drag"); }
      }}><span>DiscoAS</span></div>
      <div className="window-controls">
        <button aria-label={t("最小化")} title={t("最小化")} onClick={() => act("minimize")}><Icon name="minimize" size={15} /></button>
        <button aria-label={t(windowState.maximized ? "还原窗口" : "最大化")} title={t(windowState.maximized ? "还原窗口" : "最大化")}
          onClick={() => act("maximize")}><Icon name={windowState.maximized ? "restore" : "maximize"} size={15} /></button>
        <button className="window-close" aria-label={t("关闭")} title={t("关闭")} onClick={() => act("close")}><Icon name="close" size={15} /></button>
      </div>
    </header>
    {children}
  </div>;
}
