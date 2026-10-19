import { platformInfo } from "../platforms";
import { t, currentLocale } from "../i18n";

const dateFormatters = new Map<string, Intl.DateTimeFormat>();
function dateFormatter(locale: string) {
  let formatter = dateFormatters.get(locale);
  if (!formatter) {
    formatter = new Intl.DateTimeFormat(locale, { month: "numeric", day: "numeric" });
    dateFormatters.set(locale, formatter);
  }
  return formatter;
}

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
    : t("{p0} 更新", { p0: dateFormatter(currentLocale()).format(d) });
}
