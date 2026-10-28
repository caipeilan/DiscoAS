import { useEffect, useState } from "react";
import { call, desktop, onDesktopEvent } from "../../services/desktop";
import { idleAutoHandState, mergeAutoHandState, type AutoHandState } from "./autoHandState";

export function useAutoHandControl() {
  const [state, setState] = useState(idleAutoHandState);
  const [pending, setPending] = useState(false);
  useEffect(() => {
    if (!desktop) return;
    let cancelled = false;
    const accept = (next: AutoHandState) => { if (!cancelled) setState(old => mergeAutoHandState(old, next)); };
    const subscription = onDesktopEvent("auto-hand-state-changed", accept);
    void subscription.then(() => call<AutoHandState>("get_auto_hand_state")).then(accept).catch(() => {});
    return () => { cancelled = true; void subscription.then(stop => stop()); };
  }, []);
  const toggle = async () => {
    if (pending) return;
    setPending(true);
    try { const next = await call<AutoHandState>("set_auto_hand_enabled", { enabled: !state.enabled }); setState(old => mergeAutoHandState(old, next)); }
    finally { setPending(false); }
  };
  return { state, pending, toggle };
}
