import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { controlMediaSession, desktop, getMediaSession, refreshMediaSession, onDesktopEvent, watchMediaSession } from "../../services/desktop";
import { errorText } from "../../i18n";
import { emptyMediaSession, mergeMediaSession, type MediaAction, type MediaSessionUpdate } from "./mediaSession";
import { idleAutoHandState, type AutoHandState } from "./autoHandState";

export function useMediaSession(visible: boolean, lock: AutoHandState = idleAutoHandState) {
  const [state, setState] = useState(emptyMediaSession);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [watching, setWatching] = useState(false);
  const inFlight = useRef(false);
  const [revealedToken, setRevealedToken] = useState<number | null>(null);
  const currentLock = useRef(lock); currentLock.current = lock;
  useEffect(() => {
    if (!desktop || !visible) return;
    let cancelled = false;
    const accept = (next: MediaSessionUpdate) => { if (!cancelled) setState(old => mergeMediaSession(old, next)); };
    const subscription = onDesktopEvent("media-session-changed", accept);
    const errors = onDesktopEvent("media-session-error", error => { if (!cancelled) setError(errorText(error)); });
    void subscription.then(async () => {
      if (cancelled) return;
      await watchMediaSession(true);
      if (cancelled) return;
      setWatching(true);
      accept(await getMediaSession());
    }).catch(error => { if (!cancelled) setError(errorText(error)); });
    return () => {
      cancelled = true;
      setWatching(false);
      void subscription.then(stop => stop());
      void errors.then(stop => stop());
      void watchMediaSession(false).catch(() => {});
    };
  }, [visible]);
  useLayoutEffect(() => {
    setRevealedToken(null);
    if (!desktop || !visible || !watching || lock.busy) return;
    let cancelled = false, retry: ReturnType<typeof setTimeout> | undefined;
    const refresh = async () => {
      try {
        const next = await refreshMediaSession();
        if (cancelled || currentLock.current.busy || currentLock.current.token !== lock.token) return;
        setState(old => mergeMediaSession(old, next));
        setRevealedToken(lock.token); setError("");
      } catch (error) {
        if (!cancelled) { setError(errorText(error)); retry = setTimeout(refresh, 1000); }
      }
    };
    void refresh();
    return () => { cancelled = true; clearTimeout(retry); };
  }, [visible, watching, lock.busy, lock.token]);
  const control = useCallback(async (action: MediaAction) => {
    if (inFlight.current) return;
    inFlight.current = true; setPending(true); setError("");
    try { await controlMediaSession(state.sessionId, action); }
    catch (error) { setError(errorText(error)); }
    finally { inFlight.current = false; setPending(false); }
  }, [state.sessionId]);
  const masked = lock.busy || revealedToken !== lock.token;
  return useMemo(() => ({ state, pending, error, control, masked }), [state, pending, error, control, masked]);
}
export type MediaController = ReturnType<typeof useMediaSession>;
