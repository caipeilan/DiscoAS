import { useEffect, useRef, useState } from "react";
import { checkForUpdates } from "../../services/desktop";
import type { UpdateInfo } from "../../services/desktop";
import { errorText } from "../../i18n";

/** A button starts one public request; nothing runs on startup or in the background. */
export function useUpdateCheck() {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const active = useRef(true);
  useEffect(() => {
    active.current = true;
    return () => { active.current = false; };
  }, []);
  const check = async () => {
    if (pending.current || !active.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    setInfo(null);
    try {
      const next = await checkForUpdates();
      if (active.current) setInfo(next);
    } catch (error) {
      if (active.current) setError(errorText(error));
    } finally {
      pending.current = false;
      if (active.current) setBusy(false);
    }
  };
  return { info, error, busy, check };
}
