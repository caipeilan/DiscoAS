import { Icon } from "../Icon";
import { appLogo } from "../assets";
import { t } from "../i18n";

export type Page = "library" | "discover" | "history" | "settings" | "about";

export function Sidebar({ page, playlistCount, navigate, version }: {
  page: Page;
  playlistCount: number;
  navigate: (page: Page) => Promise<void>;
  version: string;
}) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <img className="brand-mark" src={appLogo} alt="" />
        <div>
          <strong>DiscoAS</strong>
          <small>Discover A Song!</small>
        </div>
      </div>
      <nav aria-label={t("主导航")}>
        {(
          [
            ["library", t("歌单")],
            ["history", t("发现记录")],
            ["settings", t("设置")],
            ["about", t("关于")],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            className={page === id ? "nav-item selected" : "nav-item"}
            aria-current={page === id ? "page" : undefined}
            aria-label={label}
            title={label}
            onClick={() => navigate(id)}
          >
            <Icon name={id === "about" ? "info" : id} />
            {label}
            {id === "library" && (
              <span className="nav-count">{playlistCount}</span>
            )}
          </button>
        ))}
      </nav>
      <div className="sidebar-bottom">
        <small>{version}</small>
      </div>
    </aside>

  );
}
