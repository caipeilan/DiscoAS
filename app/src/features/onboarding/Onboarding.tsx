import { useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { Icon } from "../../Icon";
import { t } from "../../i18n";
import type { OnboardingState } from "./onboardingState";
import { placeOnboarding } from "./onboardingPlacement";
import "./onboarding.css";

const copy = () => [
  ["", ""],
  [t("欢迎使用DiscoAS！"), t("如同这个程序的名字一样，Discover A Song！（发现一首歌！）现在就试试吧！")],
  [t("先用作者的网易云歌单试一遍吧！"), t("如果你没有安装网易云音乐，也可以先**发现**看看")],
  [t("现在，心里先选一首看得顺眼的歌吧！"), t("你可以选择正常显示信息的歌曲，也可以选择没有透露信息的「神秘歌曲」")],
  [t("再现在，选一首看不惯的歌，把ta替换掉！"), t("你可以点击对应歌曲的刷新按钮，用新的歌曲替换这首歌")],
  [t("最后，选择一首歌吧！"), t("点击卡片，以在对应播放器中打开这首歌！当然，你也可以通过右上角退出当前「发现」界面")],
  [t("要添加自己的歌单吗？"), t("如果不想要使用作者的歌单，那就自己导入吧！")],
  [t("粘贴你的歌单链接"), t("从受支持的平台获取歌单、专辑、合集或什么别的玩意的链接，粘贴到这里，就可以导入了！当然，你也可以先退出")],
  [t("启动用你刚刚导入的歌单！"), t("点击「启用」，之后「发现」就会改为从你的歌单抽取曲目！")],
  [t("最后，按你的习惯进行调整"), t("点击左侧的设置，可以调整「发现」数量、界面大小等内容")],
  [t("接下来，享受音乐吧！"), t("这里可以进行各种偏好设置，你也可以通过「通用」→「使用引导」重新进入教程界面！")],
];
export function Onboarding({ state, surface, handMode = false, failure = false, alreadyEnabled = false, busy = false, advance, back, retry }: {
  state: OnboardingState; surface: "main" | "discovery"; handMode?: boolean; failure?: boolean; alreadyEnabled?: boolean;
  busy?: boolean; advance: (step: number) => void; back?: () => void; retry?: () => void;
}) {
  const step = state.step;
  const visible = surface === "main" ? [1, 2, 6, 7, 8, 9, 10].includes(step) : [3, 4, 5].includes(step) || step === 2 && failure;
  const layer = useRef<HTMLDivElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const [placement, setPlacement] = useState<{ card: CSSProperties; target: CSSProperties | null } | null>(null);
  useLayoutEffect(() => {
    if (!visible || step === 7) { setPlacement(null); return; }
    const targets: Record<number, string> = {
      2: ".discover-launch", 3: ".song-card", 4: ".song-replace-button:not(:disabled)", 5: ".discovery-heading .icon-button",
      6: '[data-onboarding="import"]', 7: ".import-dialog input:not(:disabled)",
      8: `[data-onboarding-source="${CSS.escape(JSON.stringify([state.targetPlatform, state.targetKind, state.targetId]))}"] [data-onboarding="enable"]`,
      9: '[aria-controls="settings-subnavigation"]', 10: ".settings-topbar",
    };
    const findTarget = () => targets[step] ? document.querySelector<HTMLElement>(targets[step]) : null;
    let target = findTarget();
    let lastRect: DOMRect | undefined;
    let frame = 0;
    const update = () => {
      if (!card.current || !layer.current) return;
      const latest = findTarget();
      if (latest !== target) { target = latest; target?.scrollIntoView({ block: "nearest" }); }
      let scale = 1;
      for (let node: HTMLElement | null = layer.current; node; node = node.parentElement) {
        const zoom = getComputedStyle(node).zoom;
        const value = zoom.endsWith("%") ? parseFloat(zoom) / 100 : parseFloat(zoom);
        if (Number.isFinite(value) && value > 0) scale *= value;
      }
      const viewport = { width: innerWidth, height: innerHeight,
        top: document.querySelector(".window-titlebar")?.getBoundingClientRect().bottom ?? 0 };
      layer.current.style.setProperty("--onboarding-viewport-width", `${viewport.width / scale}px`);
      layer.current.style.setProperty("--onboarding-viewport-height", `${(viewport.height - viewport.top) / scale}px`);
      // Keep the prior anchor while a replacement temporarily disables its button.
      const rect = target?.getBoundingClientRect() ?? lastRect;
      lastRect = rect;
      const bounds = card.current.getBoundingClientRect();
      setPlacement({ card: placeOnboarding(rect ?? null, { width: bounds.width / scale, height: bounds.height / scale }, viewport, scale, step === 1),
        target: rect ? { left: rect.left / scale - 5, top: rect.top / scale - 5, width: rect.width / scale + 10, height: rect.height / scale + 10 } : null });
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; update(); }); };
    target?.scrollIntoView({ block: "nearest" }); update();
    const observer = new ResizeObserver(schedule); if (target) observer.observe(target); if (card.current) observer.observe(card.current);
    const mutations = new MutationObserver(schedule); mutations.observe(document.body, { childList: true, subtree: true });
    mutations.observe(document.documentElement, { attributes: true, attributeFilter: ["style"] });
    window.addEventListener("resize", schedule); document.addEventListener("scroll", schedule, true);
    document.addEventListener("transitionend", schedule, true);
    return () => { observer.disconnect(); mutations.disconnect(); cancelAnimationFrame(frame); window.removeEventListener("resize", schedule); document.removeEventListener("scroll", schedule, true); document.removeEventListener("transitionend", schedule, true); };
  }, [visible, step, state.targetPlatform, state.targetId, state.targetKind, failure, alreadyEnabled]);
  if (!visible) return null;
  const [title, body] = copy()[step];
  const text = failure ? t("没加载成功的说……要不先添加你的歌单？") : step === 8 && alreadyEnabled ? t("这份歌单已经启用。之后的发现会从这里抽取歌曲。") : body;
  const parts = text.split("**");
  return <div ref={layer} className="onboarding-layer" data-step={step} data-surface={surface} data-inline={step === 7 || undefined}>
    {placement?.target && !failure && <div className="onboarding-target" style={placement.target} />}
    <div key={step} ref={card} className={`onboarding-card ${step === 1 ? "onboarding-welcome" : ""}`}
      style={placement?.card} data-anchored={!!placement || undefined} role="region" aria-label={t("使用引导")}>
      <div className="onboarding-meta"><span>{t("使用引导")} · {step}/10</span>
        <button type="button" className="icon-button" aria-label={t("退出引导")} disabled={busy} onClick={() => advance(0)}><Icon name="close" size={16} /></button></div>
      <h2>{title}</h2><p>{parts.map((part, index) => index % 2 ? <strong key={index}>{part}</strong> : part)}</p>
      {handMode && [3, 5].includes(step) && <p className="onboarding-note">{t("你现在开启了手牌模式，卡牌会先加入手牌，之后可以从手牌打出以播放歌曲")}</p>}
      <div className="onboarding-actions">
        {step > 1 && back && <button type="button" className="text-button" disabled={busy} onClick={back}>{t("上一步")}</button>}
        {step === 1 && <><button type="button" className="text-button" onClick={() => advance(0)}>{t("跳过引导")}</button><button type="button" className="button primary" onClick={() => advance(2)}>{t("开始体验")}</button></>}
        {failure && <><button type="button" className="button secondary" onClick={() => advance(6)}>{t("先添加歌单")}</button><button type="button" className="button primary" onClick={retry}>{t("重试")}</button></>}
        {step === 3 && <button type="button" className="button primary" onClick={() => advance(4)}>{t("下一步")}</button>}
        {step === 4 && <button type="button" className="text-button" onClick={() => advance(5)}>{t("跳过这步")}</button>}
        {step === 8 && alreadyEnabled && <button type="button" className="button primary" onClick={() => advance(9)}>{t("继续")}</button>}
        {step === 10 && <button type="button" className="button primary" onClick={() => advance(0)}>{t("完成引导")}</button>}
      </div>
    </div>
  </div>;
}
