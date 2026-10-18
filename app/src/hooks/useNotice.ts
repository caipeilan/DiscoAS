import { useCallback, useEffect, useState } from "react";

export interface NoticeAction { label: string; run: () => void }
export type Notice = (text: string, error?: boolean, action?: NoticeAction) => void;

export function useNotice() {
  const [toast, setToast] = useState<{ text: string; error: boolean; action?: NoticeAction } | null>(null);
  const notice: Notice = useCallback(
    (text, error = false, action) => setToast({ text, error, action }),
    [],
  );
  useEffect(() => {
    if (!toast || toast.error) return;
    const timer = window.setTimeout(() => setToast(null), 4500);
    return () => window.clearTimeout(timer);
  }, [toast]);
  return { toast, notice, dismiss: () => setToast(null) };
}
