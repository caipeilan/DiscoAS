import type { HandSettings } from "../../types";
import { defaultHandSettings } from "../../types";
import { AnimatedDetails } from "../../components/Motion";
import { HelpHint, SettingRow } from "../../components/SettingsControls";
import { ShortcutRecorder } from "../../ShortcutRecorder";
import { t } from "../../i18n";

/** Positional playback keys live with the other shortcut settings, not in the hand layout card. */
export function HandPlayShortcuts({ value, discoveryEnabled, onChange }: {
  value: HandSettings;
  discoveryEnabled: boolean;
  onChange: (value: HandSettings) => void;
}) {
  if (!value.enabled && !discoveryEnabled) return null;
  const shortcuts = Array.from({ length: 9 }, (_, index) =>
    value.play_shortcuts?.[index] ?? defaultHandSettings.play_shortcuts[index]);
  return <section className="settings-section">
    <h2>{t("快捷打出")}</h2>
    <AnimatedDetails className="hand-shortcut-settings" summary={<>{t("快捷打出（前九张）")}<span onClick={(event) => event.stopPropagation()}><HelpHint label={t("快捷打出（前九张）")}>{t("优先作用于普通手牌；普通手牌未开启时作用于迷你发现。调整顺序后立即生效。")}</HelpHint></span></>}>
      {shortcuts.map((key, index) => <SettingRow key={index} title={t("第 {p0} 张", { p0: index + 1 })}>
        <ShortcutRecorder label={t("打出第 {p0} 张手牌", { p0: index + 1 })} value={key} onChange={(next) => {
          const keys = [...shortcuts]; keys[index] = next;
          onChange({ ...value, play_shortcuts: keys });
        }} />
      </SettingRow>)}
      <div className="appearance-actions"><button type="button" className="button secondary" onClick={() => onChange({ ...value, play_shortcuts: [...defaultHandSettings.play_shortcuts] })}>{t("恢复打出快捷键")}</button></div>
    </AnimatedDetails>
  </section>;
}
