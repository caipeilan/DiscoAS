import { type CSSProperties, type RefObject } from "react";
import { defaultMysteryCover } from "../../assets";
import { Cover } from "../../components/Cover";
import { Icon } from "../../Icon";
import { t } from "../../i18n";
import { canControlMedia, mediaToggleAction } from "./mediaSession";
import type { MediaController } from "./useMediaSession";

export function HandPlayback({ element, media, showNext, open, side, folding, style, finishFold, mysteryCover = "" }: {
  element: RefObject<HTMLDivElement | null>; media: MediaController; showNext: boolean;
  mysteryCover?: string;
  open: boolean; side: "above" | "below"; folding: boolean; style: CSSProperties; finishFold: () => void;
}) {
  const { state, pending, error, control, masked } = media;
  const action = mediaToggleAction(state), label = t(state.playing ? "暂停播放" : "继续播放");
  return <div ref={element} className="hand-playback" data-hand-hit={open || undefined}
    data-open={open || undefined} data-side={side} data-folding={folding || undefined}
    aria-hidden={!open} inert={!open} style={style}
    onTransitionEnd={event => { if (event.target === event.currentTarget && event.propertyName === "opacity") finishFold(); }}>
    {masked ? <div className="hand-playback-cover"><Cover url={mysteryCover} mystery label={t("神秘歌曲")} /></div>
      : <img className="hand-playback-cover" src={state.coverDataUri || defaultMysteryCover} alt="" draggable={false} />}
    <div className="hand-playback-text" title={masked ? t("神秘歌曲") : error || [state.title, state.artist].filter(Boolean).join(" — ")}>
      <strong>{masked ? "???" : state.title || t("暂无播放")}</strong>
      <span role={!masked && error ? "alert" : undefined}>{masked ? "???" : error || state.artist || t("等待播放器媒体信息")}</span>
    </div>
    <button type="button" className="hand-playback-toggle" aria-label={label} title={label}
      disabled={pending || !canControlMedia(state, action)} onClick={() => { void control(action); }}>
      <Icon name={state.playing ? "pause" : "play"} size={18} />
    </button>
    {showNext && <button type="button" aria-label={t("下一首")} title={t("下一首")}
      disabled={pending || !canControlMedia(state, "next")} onClick={() => { void control("next"); }}><Icon name="next" size={18} /></button>}
  </div>;
}
