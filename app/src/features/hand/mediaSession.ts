export interface MediaSessionState {
  revision: number;
  sessionId: string;
  title: string;
  artist: string;
  coverDataUri: string;
  playing: boolean;
  canPlay: boolean;
  canPause: boolean;
  canNext: boolean;
}
export type MediaAction = "play" | "pause" | "next";
export type MediaSessionUpdate = Omit<MediaSessionState, "coverDataUri"> & { coverDataUri?: string };
export const emptyMediaSession: MediaSessionState = {
  revision: 0, sessionId: "", title: "", artist: "", coverDataUri: "",
  playing: false, canPlay: false, canPause: false, canNext: false,
};
export const mergeMediaSession = (current: MediaSessionState, next: MediaSessionUpdate): MediaSessionState =>
  next.revision >= current.revision ? { ...(next.sessionId === current.sessionId ? current : emptyMediaSession), ...next } : current;
export const mediaToggleAction = (state: MediaSessionState): MediaAction => state.playing ? "pause" : "play";
export const canControlMedia = (state: MediaSessionState, action: MediaAction) =>
  !!state.sessionId && (action === "pause" ? state.canPause : action === "play" ? state.canPlay : state.canNext);
