export const sourceKinds = (platform: string): string[] =>
  platform === "YouTube" ? ["playlist", "video"] :
    platform === "Bilibili" ? ["video", "favorites", "collection", "series"] : ["playlist", "album"];

export function identifySource(value: string): { platform: string; kind: string } | null {
  const source = value.trim();
  if (/^BV[A-Za-z0-9]{10}(?:_p[1-9]\d*)?$/.test(source)) return { platform: "Bilibili", kind: "video" };
  if (source.startsWith("spotify:")) return { platform: "Spotify", kind: source.includes(":album:") ? "album" : "playlist" };
  let url: URL;
  try { url = new URL(source.match(/https?:\/\/[^\s<>"，。]+/)?.[0] || source); } catch { return null; }
  const host = url.hostname.toLowerCase();
  if (host === "kuwo.cn" || host.endsWith(".kuwo.cn"))
    return { platform: "KuwoMusic", kind: /album/i.test(url.pathname + url.hash) || url.searchParams.has("albumId") || url.searchParams.has("albumid") ? "album" : "playlist" };
  if (["www.qishui.com", "qishui.com", "music.douyin.com", "www.douyin.com", "douyin.com"].includes(host) && /\/(playlist|album)(\/|$)/.test(url.pathname))
    return { platform: "QishuiMusic", kind: /\/album(\/|$)/.test(url.pathname) ? "album" : "playlist" };
  if (["youtube.com", "www.youtube.com", "m.youtube.com", "music.youtube.com", "youtu.be"].includes(host))
    return { platform: "YouTube", kind: url.searchParams.has("list") ? "playlist" : "video" };
  if (["www.bilibili.com", "bilibili.com", "space.bilibili.com", "m.bilibili.com", "b23.tv"].includes(host)) {
    const kind = url.searchParams.has("fid") || url.searchParams.has("media_id") || url.pathname.includes("/medialist/detail/") ? "favorites" :
      url.searchParams.get("type") === "series" || url.searchParams.has("series_id") || url.pathname.includes("seriesdetail") ? "series" :
        url.searchParams.get("type") === "season" || url.searchParams.has("season_id") || url.pathname.includes("collectiondetail") ? "collection" : "video";
    return { platform: "Bilibili", kind };
  }
  const platform = host === "open.spotify.com" ? "Spotify" : host === "music.163.com" ? "NeteaseCloudMusic" :
    host === "y.qq.com" ? "QQMusic" : host === "kugou.com" || host.endsWith(".kugou.com") ? "KugouMusic" : "";
  if (!platform) return null;
  return { platform, kind: /\/album(?:Detail)?[/?]|:album:|albumid=|albummid=/.test(source) ? "album" : "playlist" };
}
