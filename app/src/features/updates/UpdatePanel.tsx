import { Icon } from "../../Icon";
import { t } from "../../i18n";
import { openExternalUrl } from "../../services/desktop";
import { useUpdateCheck } from "./useUpdateCheck";
import "./updates.css";

const repository = "https://github.com/caipeilan/DiscoAS";
export function UpdatePanel({ version }: { version: string }) {
  const { info, error, busy, check } = useUpdateCheck();
  const link = (url: string) => { void openExternalUrl(url).catch(() => {}); };
  return (
    <section className="settings-section update-section" aria-label={t("版本与更新")}>
      <div className="update-heading">
        <h2>DiscoAS {version}</h2>
        <button className="button secondary" disabled={busy} onClick={() => void check()}>
          <Icon name="refresh" />{t(busy ? "正在检查更新" : "检查更新")}
        </button>
      </div>
      <div className="update-links">
        <button className="button secondary" onClick={() => link(repository)}><Icon name="link" />{t("源码仓库")}</button>
        <button className="button secondary" onClick={() => link(`${repository}/releases`)}>{t("发布记录")}</button>
        <button className="button secondary" onClick={() => link(`${repository}/issues`)}>{t("问题反馈")}</button>
      </div>
      <div role="status" aria-live="polite">
        {error && <p className="update-error">{error}</p>}
        {info?.status === "no_release" && <p>{t("暂时没有正式发布版本")}</p>}
        {info?.status === "up_to_date" && <p>{t("当前已是最新版本")}</p>}
        {info?.status === "update_available" && <>
          <p>{t("发现新版本 {p0}", { p0: info.latestVersion || "" })}</p>
          {info.releaseNotes && <details className="release-notes" open><summary>{t("更新说明")}</summary><p>{info.releaseNotes}</p></details>}
          <div className="update-links">
            <button className="button" onClick={() => link(info.downloadUrl || info.releaseUrl)}>{t("下载安装包")}</button>
            {info.fullDownloadUrl && <button className="button secondary" onClick={() => link(info.fullDownloadUrl!)}>{t("下载完整安装包")}</button>}
          </div>
        </>}
      </div>
    </section>
  );
}
