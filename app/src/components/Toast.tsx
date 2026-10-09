import { Icon } from "../Icon";
import { t, errorText } from "../i18n";

export function Toast({ toast, dismiss }: {
  toast: { text: string; error: boolean };
  dismiss: () => void;
}) {
  return (
    <div className={`toast ${toast.error ? "error-toast" : ""}`} role={toast.error ? "alert" : "status"}>
      <Icon name={toast.error ? "info" : "check"} />
      <span>{toast.error ? errorText(toast.text) : t(toast.text)}</span>
      <button className="icon-button" aria-label={t("关闭提示")} onClick={dismiss}>
        <Icon name="close" size={16} />
      </button>
    </div>
  );
}
