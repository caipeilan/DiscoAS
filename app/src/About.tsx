import { t } from "./i18n";
import { AboutContent } from "./features/about/AboutContent";
import { UpdatePanel } from "./features/updates/UpdatePanel";

export function About({ version }: { version: string }) {
  return <>
    <header className="page-header"><h1>{t("关于")}</h1></header>
    <UpdatePanel version={version} />
    <AboutContent />
  </>;
}
