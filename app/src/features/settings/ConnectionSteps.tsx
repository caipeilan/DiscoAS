import { Icon } from "../../Icon";
import { t } from "../../i18n";

export function ConnectionSteps({ steps, current, complete = false }: { steps: string[]; current: number; complete?: boolean }) {
  return <ol className="connection-steps" aria-label={t("连接步骤")}>
    {steps.map((label, index) => <li key={label} aria-current={!complete && index === current ? "step" : undefined} data-complete={complete || index < current || undefined}>
      <span>{complete || index < current ? <Icon name="check" size={13} /> : index + 1}</span>{t(label)}
    </li>)}
  </ol>;
}
