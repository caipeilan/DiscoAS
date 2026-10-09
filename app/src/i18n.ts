import { translations } from "./translations.js";
export type Language = "zh_CN" | "zh_TW" | "en_US";
let language: Language = "zh_CN";
export function setLanguage(value: string) {
  language = value === "en_US" || value === "zh_TW" ? value : "zh_CN";
}
export function currentLocale(): string {
  return { zh_CN: "zh-CN", zh_TW: "zh-TW", en_US: "en-US" }[language];
}
export function sourceKind(kind: string): string {
  if (kind === "video") return t("视频");
  if (kind === "favorites") return t("收藏夹");
  if (kind === "collection") return t("合集");
  if (kind === "series") return t("系列");
  if (kind === "album") return t("专辑");
  return language === "en_US"
    ? "Playlist"
    : language === "zh_TW"
      ? "歌單"
      : "歌单";
}
export function itemCountUnit(platform: string, detailed = false): string {
  return ["YouTube", "Bilibili"].includes(platform) ? t("个视频") : t(detailed ? "首歌曲" : "首");
}
export function t(
  source: string,
  values: Record<string, string | number> = {},
): string {
  const translated =
    language === "zh_CN"
      ? source
      : translations[source]?.[language === "zh_TW" ? 0 : 1] || source;
  return translated.replace(/\{(\w+)\}/g, (all, key: string) =>
    key in values ? String(values[key]) : all,
  );
}
export function errorText(error: unknown): string {
  const raw = String(error).replace(/^Error:\s*/, "");
  const concise: [RegExp, string][] = [
    [/切歌失败|playback failed|track switch failed/i, "错误：切歌失败"],
    [/timed?\s*out|timeout|连接超时|请求超时/i, "错误：网络连接超时"],
    [/无网络连接|network is unreachable|network unavailable|offline/i, "错误：无网络连接"],
    [/无法连接服务器|connection refused|connection reset|dns|error sending request|failed to fetch|connect error/i, "错误：无法连接服务器"],
    [/HTTP\s*429|请求过于频繁|rate.?limit/i, "错误：请求过于频繁"],
    [/HTTP\s*(401|403)|访问受限|无权访问/i, "错误：访问受限"],
  ];
  for (const [pattern, key] of concise) if (pattern.test(raw)) {
    if (/网络|连接/.test(key) && typeof navigator !== "undefined" && navigator.onLine === false)
      return t("错误：无网络连接");
    return t(key);
  }
  if (language === "zh_CN") return raw;
  if (translations[raw]) return t(raw);
  const cases: [RegExp, string][] = [
    [/快捷键.*(注册|占用)/, "快捷键已被占用，请选择其他组合键。"],
    [/快捷键格式/, "快捷键格式无效，例如 Alt+D 或 Ctrl+Shift+D"],
    [
      /无法唤起|客户端.*(安装|识别|最小化)|最小化.*(失败|完成)/,
      "无法完成客户端操作，请检查音乐客户端或手动最小化。",
    ],
    [
      /缓存.*(缺失|损坏|不可用|没有歌曲)|没有可用缓存/,
      "本地缓存不可用，请更新歌单或专辑。",
    ],
    [
      /类型.*不一致|链接.*(不支持|无效)|ID.*(无效|非法)/,
      "来源链接或类型不匹配，请检查平台和歌单类型。",
    ],
    [
      /网络|请求|HTTP|timeout|error sending request|平台.*(错误|失败|限制)/i,
      "网络请求未完成，请稍后重试；本地缓存仍保留。",
    ],
  ];
  for (const [pattern, key] of cases) if (pattern.test(raw)) return t(key);
  return /[\u3400-\u9fff]/.test(raw)
    ? t("操作未完成，请检查设置或稍后重试。")
    : raw;
}
export function isCancelledError(error: unknown): boolean {
  return /操作已取消|操作取消|operation cancelled|operation canceled/i.test(String(error));
}
