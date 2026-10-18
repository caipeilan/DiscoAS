import { Icon } from "../Icon";
import { t, errorText } from "../i18n";
import type { NoticeAction } from "../hooks/useNotice";

export function Toast({ toast, dismiss }: {
  toast: { text: string; error: boolean; action?: NoticeAction };
  dismiss: () => void;
}) {
  return (
    <div className={`toast ${toast.error ? "error-toast" : ""}`} role={toast.error ? "alert" : "status"}>
      <Icon name={toast.error ? "info" : "check"} />
      <span>{toast.error ? errorText(toast.text) : t(toast.text)}</span>
      {toast.action && <button className="text-button toast-action" onClick={() => { dismiss(); toast.action?.run(); }}>{toast.action.label}</button>}
      <button className="icon-button" aria-label={t("关闭提示")} onClick={dismiss}>
        <Icon name="close" size={16} />
      </button>
    </div>
  );
}
