import { useEffect, useState } from "react";
import { copyText } from "../../services/desktop";
import { errorText, t } from "../../i18n";
import { AnimatedDetails } from "../../components/Motion";

/** The instructions stay in the settings page so users can follow each step. */
export function InstallationGuide({
  title,
  open,
  onOpenChange,
  children,
}: {
  title: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
}) {
  return <AnimatedDetails className="installation-guide" open={open} onOpenChange={onOpenChange} summary={title}>
    <div className="installation-guide-content">{children}</div>
  </AnimatedDetails>;
}

export function CopyableInstruction({
  label,
  value,
  onError,
}: {
  label: string;
  value: string;
  onError: (message: string) => void;
}) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(timer);
  }, [copied]);
  useEffect(() => setCopied(false), [value]);
  return <div className="instruction-copy">
    <code>{value}</code>
    <button type="button" className="button secondary" onClick={async () => {
      setCopied(false);
      onError("");
      try {
        await copyText(value);
        setCopied(true);
      } catch (failure) {
        onError(errorText(failure));
      }
    }}>{copied ? t("已复制") : label}</button>
  </div>;
}
