import type { HandSettings } from "../../types";
import { defaultHandSettings } from "../../types";
import { AnimatedDetails } from "../../components/Motion";
import { HelpHint, SettingRow } from "../../components/SettingsControls";
import { ShortcutRecorder } from "../../ShortcutRecorder";
import { t } from "../../i18n";

/** Hand visibility and positional playback share the shortcut settings category. */
export function HandPlayShortcuts({ value, discoveryEnabled, onChange }: {
  value: HandSettings;
  discoveryEnabled: boolean;
  onChange: (value: HandSettings) => void;
}) {
  const shortcuts = Array.from({ length: 9 }, (_, index) =>
    value.playShortcuts?.[index] ?? defaultHandSettings.playShortcuts[index]);
  return <section className="settings-section">
    <h2>{t("手牌与迷你发现快捷键")}</h2>
    <SettingRow title={t("手牌快捷键")} description={t("展开或收纳手牌；手牌模式或迷你发现开启时生效。")}
      hint={!value.enabled && !discoveryEnabled ? t("手牌模式或迷你发现开启后生效") : undefined}>
      <ShortcutRecorder label={t("手牌快捷键")} value={value.shortcut} onChange={shortcut => onChange({ ...value, shortcut })} />
    </SettingRow>
    <AnimatedDetails className="hand-shortcut-settings" summary={<>{t("快捷打出（前九张）")}<span onClick={(event) => event.stopPropagation()}><HelpHint label={t("快捷打出（前九张）")}>{t("优先作用于普通手牌；普通手牌未开启时作用于迷你发现。调整顺序后立即生效。")}</HelpHint></span></>}>
      {shortcuts.map((key, index) => <SettingRow key={index} title={t("第 {p0} 张", { p0: index + 1 })}>
        <ShortcutRecorder label={t("打出第 {p0} 张手牌", { p0: index + 1 })} value={key} onChange={(next) => {
          const keys = [...shortcuts]; keys[index] = next;
          onChange({ ...value, playShortcuts: keys });
        }} />
      </SettingRow>)}
      <div className="appearance-actions"><button type="button" className="button secondary" onClick={() => onChange({ ...value, playShortcuts: [...defaultHandSettings.playShortcuts] })}>{t("恢复打出快捷键")}</button></div>
    </AnimatedDetails>
  </section>;
}
