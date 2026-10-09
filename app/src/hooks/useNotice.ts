import { useCallback, useEffect, useState } from "react";

export type Notice = (text: string, error?: boolean) => void;

export function useNotice() {
  const [toast, setToast] = useState<{ text: string; error: boolean } | null>(null);
  const notice: Notice = useCallback(
    (text, error = false) => setToast({ text, error }),
    [],
  );
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), toast.error ? 10000 : 4500);
    return () => window.clearTimeout(timer);
  }, [toast]);
  return { toast, notice, dismiss: () => setToast(null) };
}
