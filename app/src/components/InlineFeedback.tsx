import { Icon } from "../Icon";
import { t } from "../i18n";

export function InlineFeedback({ message, retry, busy = false, dismiss }: {
  message: string; retry?: () => void; busy?: boolean; dismiss?: () => void;
}) {
  if (!message) return null;
  return <div className="inline-feedback" role="alert">
    <Icon name="info" size={18} />
    <p>{message}</p>
    <div className="feedback-actions">
      {retry && <button type="button" className="button secondary compact" disabled={busy} onClick={retry}>{t("重试")}</button>}
      {dismiss && <button type="button" className="icon-button" aria-label={t("关闭提示")} onClick={dismiss}><Icon name="close" size={16} /></button>}
    </div>
  </div>;
}
