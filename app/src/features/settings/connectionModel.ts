import type { BrowserBridgeStatus, SpotifySetupStatus } from "../../types";

export type ExtensionBrowser = "chromium" | "firefox";

export function browserConnection(status: BrowserBridgeStatus | null, browser: ExtensionBrowser) {
  const path = status?.extensionPaths?.[browser]
    || (browser === "chromium" ? status?.extensionPath : null) || "";
  const connected = Boolean(status?.connected && (status.connectedBrowser ?? "chromium") === browser);
  const prepared = Boolean(path && (status?.preparedBrowsers
    ? status.preparedBrowsers.includes(browser) : true));
  return { path, connected, prepared };
}

export function connectionError(message: string) {
  return message.replace(/^(?:错误|Error)\s*[:：]\s*/i, "");
}

export function spotifySetupMessage(setup: SpotifySetupStatus | null, error: string) {
  if (error) return connectionError(error);
  return setup && ["error", "repair_required", "pending"].includes(setup.phase)
    ? connectionError(setup.message) : "";
}

export function spotifyNeedsAttention(setup: SpotifySetupStatus | null) {
  return Boolean(setup && ["error", "repair_required"].includes(setup.phase));
}
