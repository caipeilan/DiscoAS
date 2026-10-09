import { useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { placePopover } from "./popoverPlacement";

/** Body portal avoids clipped controls at large interface scales and scroll edges. */
export function Popover({ anchor, children, onClose, matchWidth = true, className = "", id, role }: {
  anchor: HTMLElement;
  children: React.ReactNode;
  onClose: () => void;
  matchWidth?: boolean;
  className?: string;
  id?: string;
  role?: React.AriaRole;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const [style, setStyle] = useState<React.CSSProperties>({ visibility: "hidden" });
  useLayoutEffect(() => {
    const position = () => {
      const rect = anchor.getBoundingClientRect();
      if (!anchor.isConnected || rect.bottom < 0 || rect.top > window.innerHeight || anchor.matches(":disabled")) { onClose(); return; }
      const computed = getComputedStyle(anchor);
      let scale = 1;
      for (let node: HTMLElement | null = anchor; node; node = node.parentElement) {
        const zoom = getComputedStyle(node).zoom;
        const value = zoom.endsWith("%") ? parseFloat(zoom) / 100 : parseFloat(zoom);
        if (Number.isFinite(value) && value > 0) scale *= value;
      }
      const colors = Object.fromEntries(["--accent", "--accent-hover", "--accent-text", "--accent-soft", "--accent-selected", "--accent-border", "--accent-focus", "--accent-muted", "--accent-surface", "--accent-selected-surface", "--accent-shadow"].map((key) => [key, computed.getPropertyValue(key)]));
      setStyle({ ...colors, ...placePopover(rect, { width: window.innerWidth, height: window.innerHeight }, matchWidth ? Math.max(rect.width, 130 * scale) : 270 * scale, scale), fontSize: `${parseFloat(computed.fontSize) * scale}px`, fontFamily: computed.fontFamily });
    };
    const outside = (event: Event) => {
      const target = event.target as Node | null;
      if (target && !anchor.contains(target) && !panel.current?.contains(target)) onClose();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.isComposing) return;
      event.preventDefault(); event.stopPropagation(); onClose(); anchor.focus();
    };
    const scroll = (event: Event) => { if (!panel.current?.contains(event.target as Node)) position(); };
    position();
    const observer = new ResizeObserver(position);
    observer.observe(anchor);
    const fieldset = anchor.closest("fieldset");
    const changes = new MutationObserver(position);
    if (fieldset) changes.observe(fieldset, { attributes: true, attributeFilter: ["disabled"] });
    window.addEventListener("resize", position);
    window.addEventListener("scroll", scroll, true);
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("focusin", outside, true);
    window.addEventListener("keydown", escape, true);
    return () => {
      observer.disconnect(); changes.disconnect();
      window.removeEventListener("resize", position);
      window.removeEventListener("scroll", scroll, true);
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("focusin", outside, true);
      window.removeEventListener("keydown", escape, true);
    };
  }, [anchor, matchWidth, onClose]);
  return createPortal(<div ref={panel} className={`control-popover ${className}`} style={style} id={id} role={role}>{children}</div>, document.body);
}
