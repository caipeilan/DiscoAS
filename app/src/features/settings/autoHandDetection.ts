import type { HandSettings } from "../../types";

export type DetectionMode = HandSettings["auto_play_detection"];
export function supportsDetection(platform: string | undefined, mode: DetectionMode): boolean {
  if (!platform) return true;
  if (platform === "KuwoMusic") return mode === "window";
  if (["NeteaseCloudMusic", "QQMusic", "KugouMusic", "Spotify"].includes(platform)) return true;
  return mode === "system";
}
export function detectionForPlatform(platform: string | undefined, mode: DetectionMode = "mixed"): DetectionMode {
  return supportsDetection(platform, mode) ? mode : supportsDetection(platform, "window") ? "window" : "system";
}
