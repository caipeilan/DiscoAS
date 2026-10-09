import { useCallback, useEffect, useRef } from "react";
import { call, desktop } from "./services/desktop";
import startupLogo from "../../assets/DiscoAS.png";

export function StartupScreen({ onComplete }: { onComplete: () => void }) {
  const finished = useRef(false);
  const complete = useCallback(() => {
    if (finished.current) return;
    finished.current = true;
    if (desktop)
      call("finish_startup").catch(() => {
        finished.current = false;
      });
    else onComplete();
  }, [onComplete]);
  useEffect(() => {
    const duration = window.matchMedia("(prefers-reduced-motion: reduce)")
      .matches
      ? 0
      : 2100;
    const fallback = window.setTimeout(complete, duration);
    return () => window.clearTimeout(fallback);
  }, [complete]);
  return (
    <div
      className="startup-screen"
      onClick={complete}
      onAnimationEnd={complete}
    >
      <img className="startup-logo" src={startupLogo} alt="DiscoAS" />
    </div>
  );
}
