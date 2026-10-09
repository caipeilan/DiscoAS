import { useEffect, useState } from "react";
import { currentLocale, errorText, t } from "../i18n";
import { call } from "../services/desktop";
import { Select } from "./Select";
import { fontOptions, normalizeSystemFonts, type SystemFont } from "./fontOptions";

let fontRequest: Promise<SystemFont[]> | undefined;
function installedFonts(): Promise<SystemFont[]> {
  fontRequest ??= call<unknown>("get_system_fonts")
    .then(normalizeSystemFonts)
    .catch((error) => {
      fontRequest = undefined;
      throw error;
    });
  return fontRequest;
}

export function FontPicker({
  value,
  onChange,
  label = t("字体"),
}: {
  value: string;
  onChange: (value: string) => void;
  label?: string;
}) {
  const [fonts, setFonts] = useState<SystemFont[]>([]);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let active = true;
    setError("");
    installedFonts().then((result) => {
      if (active) setFonts(result);
    }).catch((reason) => {
      if (active) setError(errorText(reason));
    });
    return () => { active = false; };
  }, [retry]);

  return (
    <div className="font-picker">
      <Select
        label={label}
        className="font-select"
        value={value}
        onChange={onChange}
        options={fontOptions(fonts, value, t("系统字体"), currentLocale())}
        searchable
        searchLabel={t("搜索字体")}
        emptyMessage={t("没有匹配的字体")}
      />
      {error && (
        <button type="button" className="font-retry" title={error}
          aria-label={t("重新读取系统字体")} onClick={() => setRetry((n) => n + 1)}>
          {t("重试")}
        </button>
      )}
    </div>
  );
}
