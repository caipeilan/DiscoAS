import { useCallback, useEffect, useId, useRef } from "react";
import { call, desktop } from "./services/desktop";
import { STARTUP_DURATION_MS, STARTUP_ENTRY_MS, STARTUP_EXIT_DELAY_MS, STARTUP_EXIT_MS, STARTUP_STAGGER_MS, startupEntrance, startupExit } from "./startupMotion";
import "./startup.css";

export function StartupScreen({ onComplete }: { onComplete: () => void }) {
  const finished = useRef(false);
  const logo = useRef<SVGSVGElement>(null);
  const cards = useRef<Array<SVGGElement | null>>([]);
  const shape = useId();
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
    let cancelled = false, fallback = 0;
    const animations: Animation[] = [];
    const start = async () => {
      if (desktop) await call("startup_ready");
      if (desktop) await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
      if (cancelled || !logo.current) return;
      const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      const size = logo.current.getBoundingClientRect().width;
      const distance = Math.min(window.innerWidth, window.innerHeight) * 1024 / size;
      const directions = [[-.85, .32], [.35, .85], [.35, -.85]];
      cards.current.forEach((card, index) => {
        if (card) animations.push(card.animate(startupEntrance(
          reduced ? 0 : directions[index][0] * distance,
          reduced ? 0 : directions[index][1] * distance,
        ), { duration: reduced ? 200 : STARTUP_ENTRY_MS, delay: reduced ? 0 : index * STARTUP_STAGGER_MS, fill: "both" }));
      });
      const exit = logo.current.animate(startupExit(!reduced), {
        duration: reduced ? 200 : STARTUP_EXIT_MS, delay: reduced ? 500 : STARTUP_EXIT_DELAY_MS, fill: "both",
      });
      animations.push(exit);
      void exit.finished.then(() => { if (!cancelled) complete(); }).catch(() => {});
      fallback = window.setTimeout(complete, (reduced ? 700 : STARTUP_DURATION_MS) + 150);
    };
    void start().catch(() => { if (!cancelled) complete(); });
    return () => { cancelled = true; window.clearTimeout(fallback); animations.forEach(animation => animation.cancel()); };
  }, [complete]);
  return (
    <div className="startup-screen">
      <svg ref={logo} className="startup-logo" viewBox="0 0 1024 1024" role="img" aria-label="DiscoAS">
        <defs><path id={shape} d="M96 0H224C266 0 284 0 302 18C320 36 320 54 320 96V576C320 618 320 636 302 654C284 672 266 672 224 672H96C54 672 36 672 18 654C0 636 0 618 0 576V96C0 54 0 36 18 18C36 0 54 0 96 0Z" /></defs>
        <g ref={node => { cards.current[2] = node; }} className="startup-card" data-startup-card="red"><use href={`#${shape}`} x="592" y="128" fill="#FF8793" /></g>
        <g ref={node => { cards.current[1] = node; }} className="startup-card" data-startup-card="blue"><use href={`#${shape}`} x="352" y="176" fill="#5ACFF0" /></g>
        <g ref={node => { cards.current[0] = node; }} className="startup-card" data-startup-card="yellow"><use href={`#${shape}`} x="112" y="224" fill="#FFDE61" /></g>
      </svg>
    </div>
  );
}
