import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import type { AppState, DiscoveryState, GuiSettings, Song } from "../../types";
import type { OverlayPhase } from "../../overlayMotion";
import { Icon } from "../../Icon";
import { Cover } from "../../components/Cover";
import { PlatformTag } from "../../components/PlatformTag";
import { t } from "../../i18n";
import { defaultDiscoveryKeybindings } from "../settings/discoveryKeybindings";
import { useKeyboardSelection } from "./useKeyboardSelection";
import type { PreviewKeyEvent } from "./useKeyboardSelection";
import { setPreviewCloseRect } from "../../services/desktop";
import "./discovery.css";

export function DiscoveryView({ state, status, floating, loading, songError, songs, playing, replacing, slotRevision, previewGui, previewPointer, previewKey, cancelling, batchRevision, overlayPhase, busy = "", cancel, discover, play, replace, openOverlay, manageLibrary }: {
  state: AppState;
  status: DiscoveryState;
  floating: boolean;
  loading: boolean;
  songError: string;
  songs: Song[];
  playing: string;
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
  openOverlay: () => void;
  manageLibrary: () => void;
}) {
  const enabled = state.playlists.find((p) => p.enabled);
  const grid = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const [barTop, setBarTop] = useState<number | null>(null);
  const [columns, setColumns] = useState(Math.min(5, Math.max(1, songs.length)));
  const gui = previewGui || state.guiSettings;
  const preview = status.preview;
  const active = !loading && !cancelling && !playing && !songError && !busy && (!floating || overlayPhase === "open");
  const { selectedIndex, pointedIndex, inputMode, pointerMove, pointerDown, pointerLeave, focusIndex } = useKeyboardSelection({
    songs, grid, active, batchRevision, play, replace, preview, externalPointer: previewPointer, previewKey, preserveSlots: true,
    keybindings: state.settings.discovery_keybindings || defaultDiscoveryKeybindings,
  });
  const normalCount = state.settings.number_of_discovered_songs;
  const mysteryCount = state.settings.have_mystery_song ? state.settings.num_of_mystery_song : 0;
  const showBar = status.exclusionEnabled || status.replacementEnabled;
  useEffect(() => {
    const update = () => {
      if (grid.current) {
        const width = grid.current.clientWidth;
        const cardWidth = floating ? 200 * gui.card_size : 140;
        const gap = floating ? 45 * gui.card_size : 15;
        setColumns(Math.min(5, Math.max(1, songs.length), Math.max(1, Math.floor((width + gap) / (cardWidth + gap)))));
      }
      if (!floating || !body.current || !grid.current) { setBarTop(null); return; }
      const bodyRect = body.current.getBoundingClientRect();
      const slots = Array.from(grid.current.querySelectorAll<HTMLDivElement>(".song-card-slot"));
      if (slots.length) setBarTop(Math.max(...slots.map((slot) => slot.getBoundingClientRect().bottom)) - bodyRect.top + 16);
      if (preview && closeButton.current) {
        const rect = closeButton.current.getBoundingClientRect();
        void setPreviewCloseRect({ left: rect.left, top: rect.top, width: rect.width, height: rect.height }).catch(() => {});
      }
    };
    update();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    if (body.current) observer?.observe(body.current);
    if (grid.current) observer?.observe(grid.current);
    if (closeButton.current) observer?.observe(closeButton.current);
    grid.current?.querySelectorAll(".song-card-slot").forEach((slot) => observer?.observe(slot));
    window.addEventListener("resize", update);
    const element = grid.current;
    // Entering transforms move the exit control without resizing it.
    const shell = body.current?.closest(".floating-shell");
    const settled = (event: Event) => { if (event.target === shell) update(); };
    shell?.addEventListener("transitionend", settled);
    element?.addEventListener("scroll", update);
    return () => {
      observer?.disconnect(); window.removeEventListener("resize", update);
      element?.removeEventListener("scroll", update); shell?.removeEventListener("transitionend", settled);
    };
  }, [floating, gui, loading, overlayPhase, preview, songs, showBar]);
  return (
    <div className="discovery-body" ref={body} data-input-mode={inputMode} data-preview={preview || undefined}
      onPointerMove={pointerMove} onPointerDown={pointerDown} onPointerLeave={pointerLeave}
      style={{ "--replacement-scale": gui.replacement_button_size || 1, "--discovery-bar-scale": gui.discovery_bar_size || 1,
        "--discovery-columns": columns } as CSSProperties}>
      <div
        className="discovery-heading"
        data-tauri-drag-region={floating || undefined}
      >
        <div>
          <h1>{t("下一首，听什么？")}</h1>
        </div>
        {floating ? (
          <button
            ref={closeButton}
            className="icon-button"
            aria-label={preview ? t("退出预览") : t("收起发现窗口")}
            onClick={cancel}
          >
            <Icon name="close" />
          </button>
        ) : (
          <button
            className="button secondary"
            onClick={() =>
              openOverlay()
            }
          >
            <Icon name="discover" />
            {t("打开浮窗")}
          </button>
        )}
      </div>
      <div className="discovery-meta">
        <span className="small-chip">
          {normalCount}
          {t("首普通歌曲")}
          {mysteryCount ? t(" + {p0} 首神秘歌曲", { p0: mysteryCount }) : ""}
        </span>
        {enabled && <PlatformTag id={enabled.platform} />}
      </div>
      {loading && floating ? (
        <div className="overlay-message" role="status">
          <Icon name="refresh" className="spin" />
          {t("正在发现歌曲…")}
        </div>
      ) : loading ? (
        <div className="song-grid" aria-busy="true">
          {Array.from(
            { length: Math.min(normalCount + mysteryCount, 5) },
            (_, i) => (
              <div className="song-skeleton" key={i}>
                <div />
                <span />
                <span />
              </div>
            ),
          )}
        </div>
      ) : songError ? (
        <div className="discovery-empty">
          <Icon name="info" size={34} />
          <h3>{t("这次还没发现歌曲")}</h3>
          <p>{songError}</p>
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
              data-replacing={replacing === i || undefined}>
              <button
                className={`song-card ${song.mysteryMode ? "mystery-card" : ""} ${selectedIndex === i ? "keyboard-selected" : ""}`}
                disabled={!active || replacing === i}
                key={slotRevision[i] || 0}
                data-replaced={Boolean(slotRevision[i]) || undefined}
                aria-disabled={preview || undefined}
                data-keyboard-selected={selectedIndex === i || undefined}
                onFocus={() => focusIndex(i)}
                onClick={() => { if (!preview) void play(song); }}
                aria-label={
                  song.mysteryMode
                    ? t("播放神秘歌曲")
                    : t("播放 {p0}", { p0: song.name })
                }
              >
                <div className="song-art">
                  <Cover
                    url={song.albumPicUrl}
                    data={song.coverDataUri}
                    prepared
                    warning={song.coverError}
                    mystery={song.mysteryMode}
                    label={song.mysteryMode ? t("神秘歌曲") : song.name}
                  />
                </div>
                <span className="song-kind">
                  {song.mysteryMode
                    ? t("神秘歌曲")
                    : t("发现 {p0}", { p0: String(i + 1).padStart(2, "0") })}
                </span>
                <strong>
                  {song.mysteryMode
                    ? floating
                      ? "???"
                      : t("把答案交给播放键")
                    : song.name}
                </strong>
                <span className="song-artist">
                  {song.mysteryMode
                    ? floating
                      ? "???"
                      : t("点击，揭晓这一首")
                    : song.artistNames.join(" / ") || t("歌手信息暂不可用")}
                </span>
                {song.detailError && !song.mysteryMode && (
                  <span className="detail-warning">
                    {t("详情暂不可用，仍可尝试唤起")}
                  </span>
                )}
              </button>
              {status.replacementEnabled && !preview && (
                <button className="song-replace-button" type="button" tabIndex={-1}
                  aria-label={song.mysteryMode ? t("替换神秘歌曲") : t("替换 {p0}", { p0: song.name })}
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
          style={floating && barTop !== null ? { top: barTop } : undefined}>
          {status.exclusionEnabled && <span>{t("当前卡池剩{p0}首", { p0: status.remainingSongs })}</span>}
          {status.replacementEnabled && <span>{t("还可替换{p0}次", { p0: status.replacementsRemaining })}</span>}
        </div>
      )}
      <div className="discovery-footer">
        <span>
          {floating
            ? t("Esc 收起 · 点击歌曲唤起客户端")
            : t("歌曲将在对应的音乐客户端中打开")}
        </span>
        <button
          className="button secondary"
          disabled={!active || !enabled}
          onClick={() => discover(true)}
        >
          <Icon name="refresh" className={loading ? "spin" : ""} />
          {t("换一批")}
        </button>
      </div>
    </div>
  );
}
