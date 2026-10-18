import type { CSSProperties } from "react";
import { Icon } from "../Icon";
import netease from "../assets/platforms/neteasecloudmusic.svg";
import qq from "../assets/platforms/qq-music.svg";
import spotify from "../assets/platforms/spotify.svg";
import youtube from "../assets/platforms/youtube.svg";
import bilibili from "../assets/platforms/bilibili.svg";
import kugou from "../assets/platforms/kugou.svg";
import kuwo from "../assets/platforms/kuwo.svg";
import qishui from "../assets/platforms/qishui.svg";

const logos: Record<string, string> = { NeteaseCloudMusic: netease, QQMusic: qq, Spotify: spotify, YouTube: youtube, Bilibili: bilibili,
  KugouMusic: kugou, KuwoMusic: kuwo, QishuiMusic: qishui };
export function PlatformIcon({ id, size = 24 }: { id: string; size?: number }) {
  if (id === "LocalMusic") return <Icon name="folder" size={size} />;
  const logo = logos[id];
  if (!logo) return <span aria-hidden="true" style={{ display: "inline-block", flexShrink: 0, width: size, height: size }} />;
  return <span aria-hidden="true" className="platform-logo" style={{ "--platform-logo": `url("${logo}")`, width: size, height: size } as CSSProperties} />;
}
