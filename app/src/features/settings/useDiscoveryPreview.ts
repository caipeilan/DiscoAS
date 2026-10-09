import { useCallback, useEffect, useRef, useState } from "react";
import type { GuiSettings, Preferences } from "../../types";
import { endDiscoveryPreview, listenPreviewClosed, startDiscoveryPreview, updateDiscoveryPreview } from "../../services/desktop";
import { errorText } from "../../i18n";

/** Keep preview commands ordered without saving or rediscovering on each draft change. */
export function useDiscoveryPreview(settings: Preferences, gui: GuiSettings) {
  const [active, setActive] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const mounted = useRef(true);
  const desired = useRef(false);
  const session = useRef(0);
  const current = useRef({ settings, gui });
  current.current = { settings, gui };
  const chain = useRef<Promise<void>>(Promise.resolve());
  const enqueue = useCallback((operation: () => Promise<void>) => {
    chain.current = chain.current.catch(() => {}).then(operation);
    return chain.current;
  }, []);
  const change = useCallback((value: boolean) => {
    desired.current = value;
    const stamp = ++session.current;
    setActive(value);
    setPending(true);
    setError("");
    enqueue(async () => {
      if (value && (!mounted.current || !desired.current || stamp !== session.current)) return;
      if (value) await startDiscoveryPreview(current.current.settings, current.current.gui);
      else await endDiscoveryPreview();
    }).catch((failure) => {
      if (mounted.current && stamp === session.current) {
        desired.current = false;
        setActive(false);
        setError(errorText(failure));
      }
    }).finally(() => {
      if (mounted.current && stamp === session.current) setPending(false);
    });
  }, [enqueue]);
  useEffect(() => {
    mounted.current = true;
    let stop: (() => void) | undefined;
    listenPreviewClosed(() => {
      if (!mounted.current) return;
      session.current++;
      desired.current = false;
      setActive(false);
      setPending(false);
    }).then((cleanup) => {
      if (mounted.current) stop = cleanup;
      else cleanup();
    }).catch((failure) => {
      if (mounted.current) setError(errorText(failure));
    });
    return () => {
      mounted.current = false;
      desired.current = false;
      session.current++;
      stop?.();
      enqueue(() => endDiscoveryPreview()).catch(() => {});
    };
  }, [enqueue]);
  useEffect(() => {
    if (!active || pending) return;
    const stamp = session.current;
    const timer = window.setTimeout(() => {
      enqueue(async () => {
        if (!mounted.current || !desired.current || session.current !== stamp) return;
        await updateDiscoveryPreview(current.current.settings, current.current.gui);
      }).catch((failure) => {
        if (mounted.current && session.current === stamp) setError(errorText(failure));
      });
    }, 100);
    return () => window.clearTimeout(timer);
  }, [settings, gui, active, pending, enqueue]);
  return { active, pending, error, change };
}
