import { useEffect, useRef, useState, type ReactNode } from "react";
import { useDialogFocus } from "./dialogFocus";

/** Keep the closing frame mounted; reopening cancels its pending removal. */
export function usePresence(open: boolean, duration = 180) {
  const [mounted, setMounted] = useState(open);
  const [entered, setEntered] = useState(false);
  useEffect(() => {
    let frame = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    if (open) {
      setMounted(true);
      frame = requestAnimationFrame(() => { frame = requestAnimationFrame(() => setEntered(true)); });
    } else {
      setEntered(false);
      timer = setTimeout(() => setMounted(false), window.matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : duration);
    }
    return () => { cancelAnimationFrame(frame); clearTimeout(timer); };
  }, [open, duration]);
  return { mounted: open || mounted, visible: open && entered };
}

export function Presence({ show, children, modal = false }: { show: boolean; children: ReactNode; modal?: boolean }) {
  const presence = usePresence(show);
  const root = useRef<HTMLDivElement>(null);
  useDialogFocus(show && modal, root);
  const retained = useRef(children);
  if (show) retained.current = children;
  return presence.mounted ? <div ref={root} className="presence-shell" data-open={presence.visible} inert={!show} aria-hidden={!show || undefined}>
    {retained.current}
  </div> : null;
}

export function Reveal({ open, children }: { open: boolean; children: ReactNode }) {
  const presence = usePresence(open, 220);
  const retained = useRef(children);
  if (open) retained.current = children;
  return presence.mounted ? <div className="motion-reveal" data-open={presence.visible} inert={!open} aria-hidden={!open || undefined}>
    <div className="motion-reveal-inner">{retained.current}</div>
  </div> : null;
}

export function AnimatedDetails({ summary, children, className, open, onOpenChange }: {
  summary: ReactNode; children: ReactNode; className?: string; open?: boolean; onOpenChange?: (open: boolean) => void;
}) {
  const [localOpen, setLocalOpen] = useState(false);
  const expanded = open ?? localOpen;
  const presence = usePresence(expanded, 220);
  return <details className={className} open={expanded || presence.mounted}>
    <summary aria-expanded={expanded} onClick={(event) => {
      if ((event.target as HTMLElement).closest("button, a, input")) { event.preventDefault(); return; }
      event.preventDefault(); setLocalOpen(!expanded); onOpenChange?.(!expanded);
    }}>{summary}</summary>
    <Reveal open={expanded}>{children}</Reveal>
  </details>;
}
