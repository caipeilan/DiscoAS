import { useCallback, useEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import type { DiscoveryState, GuiSettings, Song, HandSettings, CollectedHandCard } from "../../types";
import { call, desktop, discoverBatch, getDiscoveryState, replaceDiscoverySong, endDiscoveryPreview, listenDiscoveryState, listenPreviewPointer, listenPreviewKey, listenPreviewAppearance, listenPreviewClosed, onDesktopEvent, hideCurrentWindow, showCurrentWindow, isCurrentWindowVisible } from "../../services/desktop";
import type { DesktopEvents } from "../../services/desktop";
import { OverlayMotion } from "../../overlayMotion";
import type { OverlayPhase } from "../../overlayMotion";
import { prepareCovers } from "../../prepareCovers";
import { defaultMysteryCover } from "../../assets";
import { platformInfo } from "../../platforms";
import type { Notice } from "../../hooks/useNotice";
import { t, errorText } from "../../i18n";
import type { PreviewKeyEvent } from "./useKeyboardSelection";
import { sameSong } from "./songData";

const emptyDiscovery: DiscoveryState = {
  songs: [], batchEpoch: 0, remainingSongs: 0, replacementsRemaining: 0,
  exclusionEnabled: false, replacementEnabled: false, preview: false,
};
const songArgs = (song: Song) => ({
  platform: song.platform, songId: song.songId, playlistId: song.playlistId,
  typename: song.typename, filename: song.filename,
});

/** Owns discovery requests, cover readiness, playback, cancellation and overlay lifecycle. */
export function useDiscovery({ floating, page, reload, notice, hand }: {
  floating: boolean;
  page: RefObject<string>;
  reload: () => Promise<void>;
  notice: Notice;
  hand?: HandSettings;
}) {
  const [songs, setSongs] = useState<Song[]>([]);
  const songsRef = useRef<Song[]>([]);
  const [status, setStatus] = useState<DiscoveryState>(emptyDiscovery);
  const statusRef = useRef<DiscoveryState>(emptyDiscovery);
  const [slotRevision, setSlotRevision] = useState<number[]>([]);
  const [replacing, setReplacing] = useState<number | null>(null);
  const replacingRef = useRef<number | null>(null);
  const [previewGui, setPreviewGui] = useState<GuiSettings | null>(null);
  const [previewPointer, setPreviewPointer] = useState<{ x: number; y: number } | null>(null);
  const [previewKey, setPreviewKey] = useState<{ event: PreviewKeyEvent; sequence: number } | null>(null);
  const previewKeySequence = useRef(0);
  const commitState = useCallback((next: DiscoveryState, replacement = false) => {
    const previous = songsRef.current;
    const batch = next.songs.map((song, i) => sameSong(song, previous[i]) ? previous[i] : song);
    songsRef.current = batch;
    const prepared = { ...next, songs: batch };
    statusRef.current = prepared;
    setStatus(prepared);
    if (!next.preview) {
      setPreviewGui(null);
      setPreviewPointer(null);
      setPreviewKey(null);
    }
    setSongs(batch);
    setSlotRevision((revisions) => batch.map((song, i) =>
      replacement ? (revisions[i] || 0) + (song !== previous[i] ? 1 : 0)
        : song === previous[i] ? (revisions[i] || 0) : 0));
  }, []);
  const commitSongs = useCallback((batch: Song[]) => {
    commitState({ ...statusRef.current, songs: batch });
  }, [commitState]);
  const prepareBatch = useCallback(async (batch: Song[]) => {
    const previous = songsRef.current;
    const changed = batch.filter((song, index) => !sameSong(song, previous[index]));
    if (!changed.length) return batch.map((_, index) => previous[index]);
    const prepared = await prepareCovers(changed, defaultMysteryCover);
    let index = 0;
    // A history/status event carries covers again, but unchanged cards are already decoded.
    return batch.map((song, slot) => sameSong(song, previous[slot]) ? previous[slot] : prepared[index++]);
  }, []);
  const [loading, setLoading] = useState(false);
  const [songError, setSongError] = useState("");
  const [playing, setPlaying] = useState("");
  const [collecting, setCollecting] = useState("");
  const playingRef = useRef("");
  const [cancelling, setCancelling] = useState(false);
  const cancellingRef = useRef(false);
  const cancelSequence = useRef(0);
  const [batchRevision, setBatchRevision] = useState(0);
  const [viewportWidth, setViewportWidth] = useState(window.innerWidth);
  const [overlayPhase, setOverlayPhase] = useState<OverlayPhase>("closed");
  const motion = useRef<OverlayMotion | null>(null);
  if (!motion.current)
    motion.current = new OverlayMotion(
      setOverlayPhase,
      hideCurrentWindow,
      showCurrentWindow,
      () =>
        new Promise((resolve) => window.requestAnimationFrame(() => {
          const transitions = document.querySelector(".floating-shell")?.getAnimations({ subtree: true })
            .filter((animation) => animation instanceof CSSTransition
              && ["opacity", "transform"].includes(animation.transitionProperty)) || [];
          void Promise.all(transitions.map((animation) => animation.finished.catch(() => {}))).then(() => resolve());
        })),
    );
  const loadingRef = useRef(false);
  const discoveryRequest = useRef(0);
  const stateDelivery = useRef(0);
  const invalidateBatch = useCallback((clear = true) => {
    const revision = ++discoveryRequest.current;
    ++stateDelivery.current;
    setBatchRevision(revision);
    setCollecting("");
    songsRef.current = [];
    if (clear) setSongs([]);
    return revision;
  }, []);
  const discoveryPending = useRef<Promise<void>>(Promise.resolve());
  const cancelPromise = useRef<Promise<void>>(Promise.resolve());
  const lastRecordedDisplay = useRef("");
  useEffect(() => {
    if (!songs.length || status.preview || (floating ? !motion.current!.visible : page.current !== "discover")) return;
    const signature = `${discoveryRequest.current}:${JSON.stringify(songs.map((song) => [song.platform, song.songId, song.playlistId, song.typename]))}`;
    if (lastRecordedDisplay.current === signature) return;
    lastRecordedDisplay.current = signature;
    // Runs after React commits the cover-ready cards; preloaded and cancelled batches stay out of history.
    call<boolean>("record_discovery_displayed", { args: songs.map(songArgs) })
      .catch((error) => notice(errorText(error), true));
  }, [songs, status.preview, floating, notice, page]);
  const discover = useCallback((force = false) => {
    if (loadingRef.current || cancellingRef.current) return discoveryPending.current;
    const request = invalidateBatch();
    loadingRef.current = true;
    setLoading(true);
    setSongError("");
    const task = (async () => {
      try {
        const next = await discoverBatch(force);
        const batch = await prepareBatch(next.songs);
        if (request === discoveryRequest.current) commitState({ ...next, songs: batch });
      } catch (e) {
        if (request === discoveryRequest.current) setSongError(errorText(e));
      } finally {
        loadingRef.current = false;
        setLoading(false);
      }
    })();
    discoveryPending.current = task;
    return task;
  }, [invalidateBatch, commitState, prepareBatch]);
  const cancel = useCallback(() => {
    if (cancellingRef.current) return cancelPromise.current;
    if (floating && !motion.current!.visible) return Promise.resolve();
    const request = invalidateBatch(false);
    const cancelId = ++cancelSequence.current;
    cancellingRef.current = true;
    setCancelling(true);
    const pending = discoveryPending.current;
    const task = (async () => {
      const closing = floating
        ? motion.current!.close()
        : Promise.resolve(false);
      try {
        if (statusRef.current.preview) await endDiscoveryPreview();
        else await call("report_cancelled");
        await pending;
      } catch (e) {
        notice(errorText(e), true);
      }
      await closing.catch((e) => notice(errorText(e), true));
      if (request === discoveryRequest.current) {
        commitSongs([]);
        statusRef.current = emptyDiscovery;
        setStatus(emptyDiscovery);
        setPreviewGui(null);
        setPreviewPointer(null);
        setPreviewKey(null);
        setSongError("");
      }
      if (cancelId === cancelSequence.current) {
        cancellingRef.current = false;
        setCancelling(false);
      }
    })();
    cancelPromise.current = task;
    return task;
  }, [notice, invalidateBatch, commitSongs, floating]);
  useEffect(() => {
    const resize = () => setViewportWidth(window.innerWidth);
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    const removers: Array<() => void> = [];
    const attach = <E extends "library-changed" | "show-overlay" | "cancel-overlay">(event: E, handler: (payload: DesktopEvents[E]) => void) =>
      onDesktopEvent(event, handler).then((fn) => {
        if (active) removers.push(fn);
        else fn();
      });
    attach("library-changed", (change) => {
      if (change?.discoveryInvalidated === false) {
        // Unrelated sources, remarks and shortcuts do not reset the visible batch or keyboard focus.
        reload().catch((e) => notice(errorText(e), true));
        return;
      }
      const request = invalidateBatch();
      const pending = discoveryPending.current;
      Promise.all([reload(), pending])
        .then(() => {
          if (
            request === discoveryRequest.current &&
            (floating
              ? motion.current!.visible
              : page.current === "discover")
          )
            discover();
        })
        .catch((e) => notice(errorText(e), true));
    });
    listenDiscoveryState(async (payload) => {
      if (!active || loadingRef.current || replacingRef.current !== null || playingRef.current || cancellingRef.current) return;
      if (payload.preview && floating && !motion.current!.visible) {
        const intent = motion.current!.open();
        window.setTimeout(() => motion.current!.finishOpen(intent), 380);
      }
      if (
        floating &&
        (!motion.current!.visible ||
          (playingRef.current && !payload.songs.length))
      )
        return;
      const request = discoveryRequest.current;
      const delivery = ++stateDelivery.current;
      const prepared = await prepareBatch(payload.songs);
      if (
        active && request === discoveryRequest.current && delivery === stateDelivery.current &&
        (!floating || motion.current!.visible)
      ) {
        const replacingSlots = payload.songs.length === songsRef.current.length
          && payload.songs.some((song, i) => song.songId !== songsRef.current[i]?.songId);
        commitState({ ...payload, songs: prepared }, replacingSlots);
      }
    }).then((fn) => {
      if (active) removers.push(fn);
      else fn();
    });
    if (floating) {
      for (const attachment of [
        listenPreviewPointer((point) => setPreviewPointer(point)),
        listenPreviewKey((event) => setPreviewKey({ event: { ...event, isComposing: false }, sequence: ++previewKeySequence.current })),
        listenPreviewAppearance((gui) => setPreviewGui(gui)),
        listenPreviewClosed(() => {
          const request = invalidateBatch(false);
          statusRef.current = emptyDiscovery;
          setPreviewPointer(null);
          setPreviewKey(null);
          void motion.current!.close().then(() => {
            // Preserve draft dimensions and transparency until the exit animation is hidden.
            if (request === discoveryRequest.current) commitSongs([]);
          });
        }),
      ]) attachment.then((fn) => { if (active) removers.push(fn); else fn(); });
    }
    if (floating) {
      const open = () => {
        const intent = motion.current!.open();
        const request = invalidateBatch();
        window.setTimeout(() => motion.current!.finishOpen(intent), 380);
        cancelPromise.current.then(() => {
          if (
            active &&
            request === discoveryRequest.current &&
            motion.current!.isCurrent(intent)
          ) {
            reload().catch(() => { });
            void getDiscoveryState().then(async (current) => {
              if (!active || request !== discoveryRequest.current || !motion.current!.isCurrent(intent)) return;
              if (current.preview) {
                const batch = await prepareBatch(current.songs);
                if (active && request === discoveryRequest.current && motion.current!.isCurrent(intent))
                  commitState({ ...current, songs: batch });
              } else await discover();
            }).catch((error) => notice(errorText(error), true));
          }
        });
      };
      attach("show-overlay", open);
      attach("cancel-overlay", () => {
        if (motion.current!.visible) cancel();
      });
      const initialRequest = discoveryRequest.current;
      isCurrentWindowVisible()
        .then((visible) => {
          if (visible && active)
            if (initialRequest === discoveryRequest.current) open();
        })
        .catch(() => { });
    }
    return () => {
      active = false;
      removers.forEach((fn) => fn());
    };
  }, [reload, discover, cancel, notice, invalidateBatch, commitSongs, commitState, prepareBatch, floating, page]);
  const replace = async (song: Song) => {
    if (statusRef.current.preview || !statusRef.current.replacementEnabled || statusRef.current.replacementsRemaining < 1
      || replacingRef.current !== null || playingRef.current || loadingRef.current || cancellingRef.current
      || !songsRef.current.includes(song) || (floating ? !motion.current!.visible : page.current !== "discover")) return;
    const index = songsRef.current.indexOf(song);
    const request = discoveryRequest.current;
    const epoch = statusRef.current.batchEpoch;
    ++stateDelivery.current;
    replacingRef.current = index;
    setReplacing(index);
    try {
      const next = await replaceDiscoverySong(songArgs(song), epoch);
      const batch = await prepareBatch(next.songs);
      if (request === discoveryRequest.current && !cancellingRef.current)
        commitState({ ...next, songs: batch }, true);
    } catch (error) {
      if (request === discoveryRequest.current) notice(errorText(error), true);
    } finally {
      replacingRef.current = null;
      setReplacing(null);
    }
  };
  const play = async (song: Song) => {
    if (statusRef.current.preview || replacingRef.current !== null || playingRef.current || loadingRef.current || cancellingRef.current || !songsRef.current.includes(song)
      || (floating ? !motion.current!.visible : page.current !== "discover")) return;
    const request = discoveryRequest.current;
    ++stateDelivery.current;
    setPlaying(song.songId);
    playingRef.current = song.songId;
    let collected: DiscoveryState | null = null;
    try {
      if (hand?.enabled) {
        const element = Array.from(document.querySelectorAll<HTMLElement>(".song-card")).find((e) => e.dataset.songId === song.songId);
        const rect = element?.getBoundingClientRect();
        const next = await call<CollectedHandCard>("collect_hand_card", { args: songArgs(song), batchEpoch: statusRef.current.batchEpoch });
        if (request !== discoveryRequest.current) return;
        collected = next.discovery;
        const keepOpen = hand.keep_discovery_open && next.discovery.songs.length > 0;
        const receive = () => call("show_collected_hand_card", { id: next.id,
          mysteryCover: song.mysteryMode ? song.coverDataUri : null,
          origin: rect ? { left: rect.left, top: rect.top, width: rect.width, height: rect.height } : null });
        if (floating && !keepOpen) {
          setCollecting(song.songId);
          await motion.current!.close(receive);
          if (request !== discoveryRequest.current) return;
        } else await receive();
        if (request !== discoveryRequest.current) return;
        if (keepOpen) { commitState(next.discovery); return; }
      } else {
        await call("play_song", { args: songArgs(song) });
        if (request === discoveryRequest.current && floating) await motion.current!.close();
      }
      if (request === discoveryRequest.current) commitSongs([]);
      if (!floating)
        notice(
          hand?.enabled ? t("已加入手牌") : t("已唤起 {p0}。", { p0: t(platformInfo(song.platform).label) }),
        );
    } catch (e) {
      if (collected && request === discoveryRequest.current) commitState(collected);
      notice(errorText(e), true);
    } finally {
      setCollecting("");
      playingRef.current = "";
      setPlaying("");
    }
  };
  return { songs, status, replacing, slotRevision, previewGui, previewPointer, previewKey, loading, songError, playing, collecting, cancelling,
    batchRevision, viewportWidth, overlayPhase, discover, cancel, play, replace };
}
