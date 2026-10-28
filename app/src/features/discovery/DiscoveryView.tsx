import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import type { AppState, DiscoveryState, GuiSettings, Song } from "../../types";
import type { OverlayPhase } from "../../overlayMotion";
import { Icon } from "../../Icon";
import { Cover } from "../../components/Cover";
import { t } from "../../i18n";
import { defaultDiscoveryKeybindings } from "../settings/discoveryKeybindings";
import { useKeyboardSelection } from "./useKeyboardSelection";
import type { PreviewKeyEvent } from "./useKeyboardSelection";
import { call, setPreviewCloseRect } from "../../services/desktop";
import { Onboarding } from "../onboarding/Onboarding";
import { useOnboarding } from "../onboarding/useOnboarding";
import "./discovery.css";

export function DiscoveryView({ state, status, loading, songError, songs, playing, collecting, replacing, slotRevision, previewGui, previewPointer, previewKey, cancelling, batchRevision, overlayPhase, busy = "", cancel, discover, play, replace, manageLibrary }: {
  state: Pick<AppState, "settings" | "guiSettings"> & { sources: Array<{ platform: string; enabled: boolean }> };
  status: DiscoveryState;
  loading: boolean;
  songError: string;
  songs: Song[];
  playing: string;
  collecting: string;
  replacing: number | null;
  slotRevision: number[];
  previewGui: GuiSettings | null;
  previewPointer: { x: number; y: number } | null;
  previewKey: { event: PreviewKeyEvent; sequence: number } | null;
  cancelling: boolean;
  batchRevision: number;
  overlayPhase: OverlayPhase;
  busy?: string;
  cancel: () => Promise<void>;
  discover: (force?: boolean) => Promise<void>;
  play: (song: Song) => Promise<void>;
  replace: (song: Song) => Promise<void>;
  manageLibrary: () => void;
}) {
  const enabled = state.sources.find((p) => p.enabled);
  const grid = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const [barTop, setBarTop] = useState<number | null>(null);
  const [columns, setColumns] = useState(Math.min(5, Math.max(1, songs.length)));
  const gui = previewGui || state.guiSettings;
  const preview = status.preview;
  const active = !loading && !cancelling && !playing && !songError && !busy && overlayPhase === "open";
  const { selectedIndex, pointedIndex, inputMode, pointerMove, pointerDown, pointerLeave, focusIndex } = useKeyboardSelection({
    songs, grid, active, batchRevision, play, replace, preview, externalPointer: previewPointer, previewKey, preserveSlots: true,
    layoutKey: `${gui.discoveryCardScale}:${gui.fontSize ?? 0}`,
    keybindings: state.settings.discoveryKeybindings || defaultDiscoveryKeybindings,
  });
  const showBar = status.exclusionEnabled || status.replacementEnabled;
  const guide = useOnboarding();
  const guideStep = guide.state.step;
  const guideOpened = useRef(false);
  const replacementCheckpoint = useRef<{ step: number; signature: string }>({ step: 0, signature: "" });
  useEffect(() => {
    if (preview) return;
    if (overlayPhase === "open") guideOpened.current = true;
    if (guideStep === 2 && active && songs.length) void guide.advance(2, 3);
    if (guideOpened.current && overlayPhase === "closed" && guideStep >= 3 && guideStep <= 5) {
      guideOpened.current = false;
      void guide.advance(guideStep, 6);
    }
    const signature = slotRevision.join(":");
    if (guideStep === 4 && replacementCheckpoint.current.step === 4 && replacementCheckpoint.current.signature !== signature) void guide.advance(4, 5);
    replacementCheckpoint.current = { step: guideStep, signature };
  }, [guideStep, active, songs.length, overlayPhase, preview, slotRevision, guide.advance]);
  useEffect(() => {
    let frame = 0;
    const update = () => {
      if (grid.current) {
        const width = grid.current.clientWidth;
        const cardWidth = 200 * gui.discoveryCardScale;
        const gap = 45 * gui.discoveryCardScale;
        setColumns(Math.min(5, Math.max(1, songs.length), Math.max(1, Math.floor((width + gap) / (cardWidth + gap)))));
      }
      if (!body.current || !grid.current) { setBarTop(null); return; }
      const bodyRect = body.current.getBoundingClientRect();
      const slots = Array.from(grid.current.querySelectorAll<HTMLDivElement>(".song-card-slot"));
      if (slots.length) setBarTop(Math.max(...slots.map((slot) => slot.getBoundingClientRect().bottom)) - bodyRect.top + 16);
      if (preview && closeButton.current) {
        const rect = closeButton.current.getBoundingClientRect();
        void setPreviewCloseRect({ left: rect.left, top: rect.top, width: rect.width, height: rect.height }).catch(() => {});
      }
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; update(); }); };
    update();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    if (body.current) observer?.observe(body.current);
    if (grid.current) observer?.observe(grid.current);
    if (closeButton.current) observer?.observe(closeButton.current);
    grid.current?.querySelectorAll(".song-card-slot").forEach((slot) => observer?.observe(slot));
    window.addEventListener("resize", schedule);
    const element = grid.current;
    // Entering transforms move the exit control without resizing it.
    const shell = body.current?.closest(".floating-shell");
    const settled = (event: Event) => { if (event.target === shell) schedule(); };
    shell?.addEventListener("transitionend", settled);
    element?.addEventListener("scroll", schedule);
    return () => {
      observer?.disconnect(); cancelAnimationFrame(frame); window.removeEventListener("resize", schedule);
      element?.removeEventListener("scroll", schedule); shell?.removeEventListener("transitionend", settled);
    };
  }, [gui, loading, overlayPhase, preview, songs, showBar]);
  return (
    <div className="discovery-body" ref={body} data-input-mode={inputMode} data-preview={preview || undefined} data-onboarding-step={preview ? undefined : guideStep || undefined}
      data-hand-mode={state.settings.hand?.enabled || undefined} data-collecting={collecting || undefined}
      onPointerMove={pointerMove} onPointerDown={pointerDown} onPointerLeave={pointerLeave}
      style={{ "--replacement-scale": gui.replacementButtonScale || 1, "--discovery-bar-scale": gui.discoveryBarScale || 1,
        "--discovery-columns": columns } as CSSProperties}>
      <div
        className="discovery-heading"
        data-tauri-drag-region
      >
        <button
          ref={closeButton}
          className="icon-button"
          aria-label={preview ? t("退出预览") : t("收起发现窗口")}
          onClick={cancel}
        >
          <Icon name="close" />
        </button>
      </div>
      {loading ? (
        <div className="overlay-message" role="status">
          <Icon name="refresh" className="spin" />
          {t("正在发现歌曲…")}
        </div>
      ) : songError ? (
        <div className="discovery-empty">
          <Icon name="info" size={34} />
          <h3>{t("这次还没发现歌曲")}</h3>
          <p>{songError}</p>
          {enabled && <button className="button primary" disabled={loading || cancelling || !!busy} onClick={() => { void discover(); }}><Icon name="refresh" />{t("重试")}</button>}
          <button
            className="button secondary"
            onClick={() =>
              manageLibrary()
            }
          >
            {t("管理歌单")}
            <Icon name="arrow" />
          </button>
        </div>
      ) : songs.length ? (
        <div className="song-grid" ref={grid}>
          {songs.map((song, i) => (
            <div className="song-card-slot" key={i} data-preview-hover={preview && (selectedIndex === i || pointedIndex === i) || undefined}
              data-replacing={replacing === i || undefined} data-collected={collecting === song.songId || undefined}>
              <button
                className={`song-card ${song.mysteryMode ? "mystery-card" : ""} ${selectedIndex === i ? "keyboard-selected" : ""}`}
                data-song-id={song.songId}
                disabled={!active || replacing === i}
                key={slotRevision[i] || 0}
                data-replaced={Boolean(slotRevision[i]) || undefined}
                aria-disabled={preview || undefined}
                data-keyboard-selected={selectedIndex === i || undefined}
                onFocus={() => focusIndex(i)}
                onClick={() => { if (!preview) void play(song); }}
                aria-label={
                  state.settings.hand?.enabled ? t("加入手牌") : song.mysteryMode
                    ? t("播放神秘歌曲")
                    : t("播放 {p0}", { p0: song.title })
                }
              >
                <div className="song-art">
                  <Cover
                    url={song.albumPicUrl}
                    data={song.coverDataUri}
                    prepared
                    warning={song.coverError}
                    mystery={song.mysteryMode}
                    label={song.mysteryMode ? t("神秘歌曲") : song.title}
                  />
                </div>
                <strong>{song.mysteryMode ? "???" : song.title}</strong>
                <span className="song-artist">
                  {song.mysteryMode ? "???" : song.artistNames.join(" / ") || t("歌手信息暂不可用")}
                </span>
              </button>
              {status.replacementEnabled && !preview && (
                <button className="song-replace-button" type="button" tabIndex={-1}
                  aria-label={song.mysteryMode ? t("替换神秘歌曲") : t("替换 {p0}", { p0: song.title })}
                  disabled={!active || replacing !== null || status.replacementsRemaining < 1}
                  onClick={(event) => { event.stopPropagation(); void replace(song); }}>
                  <Icon name="refresh" className={replacing === i ? "spin" : ""} />
                </button>
              )}
            </div>
          ))}
        </div>
      ) : (
        <div className="discovery-empty">
          <Icon name="discover" size={40} />
          <h3>{enabled ? t("准备好听下一首了吗？") : t("先选择一个歌单")}</h3>
          <p>
            {enabled
              ? t("随机发现几首歌，也给神秘歌曲一个机会。")
              : t("导入公开歌单后，即可开始发现。")}
          </p>
          {!enabled && (
            <button
              className="button secondary"
              onClick={() =>
                manageLibrary()
              }
            >
              {t("去导入歌单")}
              <Icon name="arrow" />
            </button>
          )}
        </div>
      )}
      {showBar && !loading && !songError && songs.length > 0 && (
        <div className="discovery-status-bar" role="status" aria-live="polite"
          style={barTop !== null ? { top: barTop } : undefined}>
          {status.exclusionEnabled && <span>{t("当前卡池剩{p0}首", { p0: status.remainingSongs })}</span>}
          {status.replacementEnabled && <span>{t("还可替换{p0}次", { p0: status.replacementsRemaining })}</span>}
        </div>
      )}
      {!preview && <Onboarding state={guide.state} surface="discovery" handMode={state.settings.hand?.enabled}
        failure={!!songError} busy={loading || cancelling || !!playing || replacing !== null}
        retry={() => { void discover(true); }}
        advance={next => { void guide.advance(guideStep, next).then(async () => { if (next === 6) { await cancel(); await call("show_main"); } }); }}
        back={() => { void guide.advance(guideStep, guideStep - 1).then(async () => { if (guideStep === 3) { await cancel(); await call("show_main"); } }); }} />}
    </div>
  );
}
