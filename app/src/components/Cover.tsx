import { useEffect, useState } from "react";
import { Icon } from "../Icon";
import { call, desktop } from "../services/desktop";
import { defaultMysteryCover } from "../assets";
import { t, errorText } from "../i18n";

export function Cover({
  url,
  mystery = false,
  label = "",
  data = null,
  prepared = false,
  warning = null,
}: {
  url: string;
  mystery?: boolean;
  label?: string;
  data?: string | null;
  prepared?: boolean;
  warning?: string | null;
}) {
  const [src, setSrc] = useState(data || "");
  const [failedData, setFailedData] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    setSrc(data || "");
    setFailedData(null);
    if (!prepared && url && desktop)
      call<string | null>("get_image", { url })
        .then((data) => {
          if (active && data) setSrc(data);
        })
        .catch(() => { });
    return () => {
      active = false;
    };
  }, [url, data, prepared]);
  const visibleSrc = prepared ? (data && data !== failedData ? data : "") : src;
  return (
    <div
      className={`cover ${mystery ? "mystery-cover" : ""}`}
      title={warning ? errorText(warning) : undefined}
    >
      {visibleSrc ? (
        <img
          src={visibleSrc}
          draggable={false}
          alt={label}
          onError={() => {
            setFailedData(data);
            setSrc("");
          }}
        />
      ) : mystery ? (
        <img src={defaultMysteryCover} alt={t("神秘歌曲")} draggable={false} />
      ) : (
        <Icon name="music" size={28} />
      )}
    </div>
  );
}
