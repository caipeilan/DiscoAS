import { platformInfo } from "../platforms";
import { t, currentLocale } from "../i18n";

export function PlatformTag({ id }: { id: string }) {
  const p = platformInfo(id);
  return (
    <span className="platform-tag">
      <i style={{ background: p.color }} />
      {t(p.label)}
    </span>
  );
}
export function formatDate(raw: string) {
  const n = Number(raw);
  const d = n > 0 && n < 9999999999 ? new Date(n * 1000) : new Date(raw);
  return Number.isNaN(d.getTime())
    ? ""
    : t("{p0} 更新", {
      p0: new Intl.DateTimeFormat(currentLocale(), {
        month: "numeric",
        day: "numeric",
      }).format(d),
    });
}
