import { Icon } from "../Icon";
import { appLogo } from "../assets";
import { t } from "../i18n";
import { Reveal } from "./Motion";
import { settingsCategories, type SettingsCategory } from "../features/settings/settingsNavigation";

export type Page = "library" | "history" | "settings" | "console" | "about";

export function Sidebar({ page, playlistCount, navigate, version, settingsCategory, selectSettingsCategory }: {
  page: Page;
  playlistCount: number;
  navigate: (page: Page) => Promise<void>;
  version: string;
  settingsCategory: SettingsCategory;
  selectSettingsCategory: (category: SettingsCategory) => void;
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
            ["console", t("控制台")],
            ["about", t("关于")],
          ] as const
        ).map(([id, label]) => (
          <div key={id} className="nav-group">
          <button
            className={page === id ? id === "settings" ? "nav-item section-open" : "nav-item selected" : "nav-item"}
            aria-current={page === id && id !== "settings" ? "page" : undefined}
            aria-expanded={id === "settings" ? page === "settings" : undefined}
            aria-controls={id === "settings" ? "settings-subnavigation" : undefined}
            aria-label={label}
            title={label}
            onClick={() => navigate(id)}
          >
            <Icon name={id === "about" ? "info" : id} />
            {label}
            {id === "library" && (
              <span className="nav-count">{playlistCount}</span>
            )}
            {id === "settings" && <Icon name="chevron" size={13} className="nav-disclosure" />}
          </button>
          {id === "settings" && <Reveal open={page === "settings"}>
            <div className="settings-subnavigation" id="settings-subnavigation" role="group" aria-label={t("设置分类")}>
              {settingsCategories.map(category => <button type="button" key={category.id}
                className={settingsCategory === category.id ? "settings-subitem selected" : "settings-subitem"}
                aria-current={page === "settings" && settingsCategory === category.id ? "page" : undefined}
                onClick={() => selectSettingsCategory(category.id)}>{t(category.label)}</button>)}
            </div>
          </Reveal>}
          </div>
        ))}
      </nav>
      <div className="sidebar-bottom">
        <small>{version}</small>
      </div>
    </aside>

  );
}
