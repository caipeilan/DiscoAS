import type { ReactNode } from "react";
import { t } from "../../i18n";
import { openExternalUrl } from "../../services/desktop";
import "./about.css";

const authorUrl = "https://space.bilibili.com/29285623";
const licenseUrl = "https://github.com/caipeilan/DiscoAS/blob/main/LICENSE";

function ExternalLink({ url, children }: { url: string; children: ReactNode }) {
  return <a href={url} onClick={(event) => {
    event.preventDefault();
    void openExternalUrl(url).catch(() => {});
  }}>{children}</a>;
}

export function AboutContent() {
  return <section className="settings-section about-links">
    <div className="about-link-row">
      <span>{t("作者")}</span>
      <ExternalLink url={authorUrl}>bilibili@{t("蔡佩兰")}</ExternalLink>
    </div>
    <div className="about-link-row">
      <span>{t("许可协议")}</span>
      <ExternalLink url={licenseUrl}>{t("GPLv3 协议")}</ExternalLink>
    </div>
    <div className="about-link-row">
      <span>{t("平台图标")}</span>
      <span><ExternalLink url="https://github.com/Arcticons-Team/Arcticons">Arcticons</ExternalLink> (<ExternalLink url="https://creativecommons.org/licenses/by-sa/4.0/">CC BY-SA 4.0</ExternalLink>) · <ExternalLink url="https://www.iconfont.cn/">iconfont</ExternalLink></span>
    </div>
  </section>;
}
