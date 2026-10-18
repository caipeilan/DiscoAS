import { useLayoutEffect, useRef, type RefObject } from "react";

const controls = 'button, input, select, textarea, a[href], [tabindex]';

/** Body-portaled menus remain part of their owning dialog's keyboard order. */
export function useDialogFocus(open: boolean, root: RefObject<HTMLElement | null>) {
  const returnTo = useRef<HTMLElement | null>(null);
  const wasOpen = useRef(false);
  if (open && !wasOpen.current) returnTo.current = document.activeElement as HTMLElement | null;
  wasOpen.current = open;
  useLayoutEffect(() => {
    if (!open || !root.current) return;
    const dialog = root.current.querySelector<HTMLElement>('[role="dialog"]');
    if (!dialog) return;
    const previous = returnTo.current;
    const owner = dialog.getAttribute("aria-labelledby");
    const portals = () => Array.from(document.querySelectorAll<HTMLElement>('[data-dialog-owner]'))
      .filter(node => node.dataset.dialogOwner === owner && !node.inert);
    const inside = (node: Node | null) => !!node && (dialog.contains(node) || portals().some(portal => portal.contains(node)));
    const available = (node: HTMLElement) => node.tabIndex >= 0 && !node.matches(':disabled') && !node.closest('[inert], [hidden]') && node.getClientRects().length > 0;
    const targets = () => [dialog, ...portals()].flatMap(node => Array.from(node.querySelectorAll<HTMLElement>(controls))).filter(available);
    const initial = () => dialog.querySelector<HTMLElement>('[data-autofocus]') ?? targets()[0] ?? dialog;
    const siblings = Array.from(root.current.parentElement?.children ?? []).filter((node): node is HTMLElement => node instanceof HTMLElement && node !== root.current);
    const inert = siblings.map(node => node.inert);
    siblings.forEach(node => { node.inert = true; });
    const focus = (node: HTMLElement) => node.focus({ preventScroll: true });
    const guard = (event: FocusEvent) => { if (!inside(event.target as Node)) focus(initial()); };
    const tab = (event: KeyboardEvent) => {
      if (event.key !== 'Tab' || event.defaultPrevented) return;
      const active = document.activeElement as HTMLElement | null;
      if (active?.closest('[data-dialog-owner]')) return;
      const nodes = targets(), index = nodes.indexOf(active!);
      if (!nodes.length) { event.preventDefault(); focus(dialog); }
      else if (event.shiftKey ? index <= 0 : index === nodes.length - 1 || index < 0) {
        event.preventDefault(); focus(nodes[event.shiftKey ? nodes.length - 1 : 0]);
      }
    };
    document.addEventListener('focusin', guard);
    document.addEventListener('keydown', tab);
    if (!inside(document.activeElement)) focus(initial());
    return () => {
      document.removeEventListener('focusin', guard);
      document.removeEventListener('keydown', tab);
      siblings.forEach((node, index) => { node.inert = inert[index]; });
      const target = previous?.isConnected && previous !== document.body && !previous.matches(':disabled') && !previous.closest('[inert]') ? previous
        : document.querySelector<HTMLElement>('.main-content button:not(:disabled), .main-content input:not(:disabled)');
      target?.focus({ preventScroll: true });
    };
  }, [open, root]);
}
