import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Icon } from "../../Icon";
import { applyAppearance } from "../../appearance";
import { currentLocale, setLanguage } from "../../i18n";
import { call, desktop, onDesktopEvent } from "../../services/desktop";
import { defaultGuiSettings } from "../../types";
import { nextMenuIndex, trayActions, type TrayAction, type TrayMenuSnapshot } from "./trayMenuModel";
import "./trayMenu.css";

const fallback: TrayMenuSnapshot = {
  generation: 0,
  gui: defaultGuiSettings,
  labels: ["发现一首歌", "显示手牌", "歌单与设置", "暂停全局快捷键", "重启 DiscoAS", "退出 DiscoAS"],
  paused: false,
  handEnabled: false,
};

function TrayGlyph({ action }: { action: TrayAction }) {
  if (action === "pause") return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" aria-hidden="true"><path d="M9 5v14M15 5v14" /></svg>;
  if (action === "quit") return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" aria-hidden="true"><path d="M12 3v9M6.35 6.35a8 8 0 1 0 11.3 0" /></svg>;
  return <Icon name={{ discover: "discover", hand: "hand", main: "library", restart: "refresh" }[action]} />;
}

export function TrayMenu() {
  const [snapshot, setSnapshot] = useState(fallback);
  const [busy, setBusy] = useState(false);
  const panel = useRef<HTMLDivElement>(null);
  const items = useRef<Array<HTMLButtonElement | null>>([]);
  const active = useRef<TrayMenuSnapshot>(fallback);
  const inFlight = useRef(false);
  const mounted = useRef(false);
  const accept = (next: TrayMenuSnapshot) => {
    if (!mounted.current || next.generation <= active.current.generation) return;
    active.current = next;
    inFlight.current = false;
    setBusy(false);
    setSnapshot(next);
  };

  useEffect(() => {
    mounted.current = true;
    let cancelled = false;
    let unlisten = () => {};
    if (desktop) {
      void onDesktopEvent("tray-menu-open", accept).then(async (stop) => {
        if (cancelled) { stop(); return; }
        unlisten = stop;
        const pending = await call<TrayMenuSnapshot | null>("tray_menu_ready");
        if (!cancelled && pending) accept(pending);
      }).catch(() => {});
    }
    return () => { cancelled = true; mounted.current = false; unlisten(); };
  }, []);

  useLayoutEffect(() => {
    setLanguage(snapshot.gui.language);
    document.documentElement.lang = currentLocale();
    applyAppearance(snapshot.gui);
    if (!snapshot.generation || !desktop) return;
    if (active.current.generation !== snapshot.generation || !panel.current) return;
    // Hidden WebViews may suspend animation frames. Forced layout is sufficient for this handshake.
    const bounds = panel.current.getBoundingClientRect();
    void call<boolean>("present_tray_menu", {
      generation: snapshot.generation,
      width: Math.ceil(bounds.width + 28),
      height: Math.ceil(bounds.height + 28),
    }).then((presented) => {
      if (presented && mounted.current && active.current.generation === snapshot.generation)
        items.current[0]?.focus({ preventScroll: true });
    }).catch(() => {
      if (active.current.generation === snapshot.generation)
        void call("dismiss_tray_menu", { generation: snapshot.generation }).catch(() => {});
    });
  }, [snapshot]);

  const dismiss = () => {
    if (desktop) void call("dismiss_tray_menu", { generation: active.current.generation }).catch(() => {});
  };
  const activate = async (action: TrayAction) => {
    if (inFlight.current || !desktop) return;
    inFlight.current = true;
    setBusy(true);
    try {
      await call("tray_menu_action", { generation: snapshot.generation, action });
    } finally {
      if (mounted.current && active.current.generation === snapshot.generation) {
        inFlight.current = false;
        setBusy(false);
      }
    }
  };

  return <div className="tray-menu-stage" onContextMenu={(event) => event.preventDefault()} onPointerDown={(event) => { if (event.target === event.currentTarget) dismiss(); }}>
    <div ref={panel} className="tray-menu-panel" role="menu" aria-label="DiscoAS" aria-busy={busy}
      onKeyDown={(event) => {
        if (event.key === "Escape") { event.preventDefault(); dismiss(); return; }
        const index = items.current.findIndex((item) => item === document.activeElement);
        const next = nextMenuIndex(event.key, index);
        if (next !== null) { event.preventDefault(); items.current[next]?.focus(); }
        if (event.key === "Tab") {
          event.preventDefault();
          items.current[nextMenuIndex(event.shiftKey ? "ArrowUp" : "ArrowDown", index)!]?.focus();
        }
      }}>
      {trayActions.map((action, index) => <div key={action} className={index === 4 ? "tray-menu-group" : undefined}>
        <button ref={(element) => { items.current[index] = element; }}
          type="button" role={action === "pause" ? "menuitemcheckbox" : "menuitem"}
          aria-checked={action === "pause" ? snapshot.paused : undefined}
          className="tray-menu-item" disabled={busy || (action === "hand" && !snapshot.handEnabled)} tabIndex={-1}
          onPointerMove={(event) => { if (event.movementX || event.movementY) event.currentTarget.focus({ preventScroll: true }); }}
          onClick={() => { void activate(action).catch(() => {}); }}>
          <span className="tray-menu-icon"><TrayGlyph action={action} /></span>
          <span className="tray-menu-label">{snapshot.labels[index]}</span>
          <span className="tray-menu-check">{action === "pause" && snapshot.paused ? <Icon name="check" /> : null}</span>
        </button>
      </div>)}
    </div>
  </div>;
}
