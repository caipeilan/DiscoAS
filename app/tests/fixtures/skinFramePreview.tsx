import { useLayoutEffect, useState, type CSSProperties } from "react";
import { createRoot } from "react-dom/client";
import { applyAppearance } from "../../src/appearance";
import { defaultGuiSettings } from "../../src/types";
import { skins, type SkinId } from "../../src/skins";
import { Icon } from "../../src/Icon";
import { appLogo } from "../../src/assets";
import "../../src/App.css";
import "../../src/features/hand/hand.css";
import "../../src/features/hand/handControls.css";
import "../../src/components/mainWindow.css";
import "../../src/styles/skins.css";

document.documentElement.classList.add("floating-window");

function Preview() {
  const [skin, setSkin] = useState<SkinId>("indigestion"), [dark, setDark] = useState(false);
  const [mask, setMask] = useState(true), [scale, setScale] = useState(1);
  const [flipped, setFlipped] = useState(false);
  useLayoutEffect(() => {
    applyAppearance({ ...defaultGuiSettings, skin, nightMode: dark, followSystemTheme: false });
    document.documentElement.dataset.previewMask = mask ? "on" : "off";
  }, [skin, dark, mask]);
  return <div className="skin-frame-preview">
    <style>{`
      html,body,#root { overflow:auto; height:auto; min-height:100%; }
      body { background:#252627; }
      .skin-frame-preview { padding:26px; color:var(--text); }
      .preview-options { display:flex; flex-wrap:wrap; align-items:center; gap:16px; padding:18px; background:var(--canvas); margin-bottom:24px; }
      .preview-options h1 { font-size:18px; margin-right:auto; }
      .preview-options label { display:flex; align-items:center; gap:6px; }
      .preview-grid { display:grid; grid-template-columns:minmax(320px,1fr) minmax(340px,1fr); gap:24px; }
      .preview-board { position:relative; min-height:340px; padding:32px; background-color:#202224; background-image:conic-gradient(#333638 25%,transparent 0 50%,#333638 0 75%,transparent 0); background-size:24px 24px; }
      .preview-board.light { background-color:#ddd; background-image:conic-gradient(#f4f4f4 25%,transparent 0 50%,#f4f4f4 0 75%,transparent 0); }
      .preview-card { left:130px; top:165px; --hand-x:0px; --hand-y:0px; --hand-angle:8deg; --hand-card-scale:1; }
      .preview-card + .preview-card { left:300px; top:180px; --hand-angle:-8deg; }
      .preview-card .hand-art { display:grid; place-items:center; background:var(--surface-subtle); }
      .preview-card .hand-art img { width:65%; height:65%; object-fit:contain; }
      .preview-board .hand-playback { left:30px; top:45px; width:calc(100% - 60px); max-width:440px; --player-x:0px; --player-y:0px; }
      .preview-board .hand-dock { left:30px; top:145px; width:calc(100% - 60px); max-width:440px; height:42px; --dock-x:0px; --dock-y:0px; --dock-diameter:42px; display:flex; align-items:center; gap:12px; padding:5px 12px; }
      .preview-board .hand-dock > img { width:26px; }
      .preview-form { display:flex; gap:16px; position:absolute; left:30px; right:30px; top:230px; align-items:center; flex-wrap:wrap; }
      .preview-board .settings-section { margin:0; }
      .preview-board .settings-section > h2 { padding:10px 20px; }
      .preview-board .setting-row { min-height:65px; }
      html[data-preview-mask="off"] :is(.hand-card-face,.hand-playback,.hand-dock,.button,.settings-section)::before { display:none; }
      .skin-frame-preview .preview-window.main-window { height:240px; --skin-frame-width:var(--skin-window-width); --skin-frame-outset:var(--skin-window-outset); }
      .preview-window .app-shell { padding:18px; }
      @media(max-width:850px) { .preview-grid { grid-template-columns:1fr; } }
    `}</style>
    <header className="preview-options"><h1>皮肤边框对照</h1>
      <select aria-label="皮肤" value={skin} onChange={e => setSkin(e.target.value as SkinId)}>{skins.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}</select>
      <label><input type="checkbox" checked={dark} onChange={e => setDark(e.target.checked)} />夜间</label>
      <label><input type="checkbox" checked={mask} onChange={e => setMask(e.target.checked)} />纸底承托</label>
      <label><input type="checkbox" checked={flipped} onChange={e => setFlipped(e.target.checked)} />翻面</label>
      <label>缩放<select aria-label="卡牌缩放" value={scale} onChange={e => setScale(Number(e.target.value))}>{[.75,1,1.5].map(value => <option key={value} value={value}>{value * 100}%</option>)}</select></label>
    </header>
    <div className="preview-grid">
      {[false, true].map(light => <div key={String(light)} className={`preview-board${light ? " light" : ""}`}>
        {[false, true].map(active => <div key={String(active)} className={`hand-card preview-card${active ? " hand-active" : ""}`} style={{ width:156 * scale, height:220 * scale, "--hand-base-scale":scale } as CSSProperties}>
          <div className="hand-flip" data-flippable style={{ transform:flipped ? "rotateY(180deg)" : undefined }}><div className="hand-card-face"><div className="hand-art"><img src={appLogo} alt="" /></div><strong>{active ? "静止倾斜卡牌" : "纸底连续性"}</strong><span>DiscoAS · 边缘对照</span></div><div className="hand-card-back"><div className="hand-card-face"><div className="hand-art"><img src={appLogo} alt="" /></div><strong>背面纸底</strong><span>DiscoAS · 边缘对照</span></div></div></div>
        </div>)}
      </div>)}
      {[false, true].map(light => <div key={String(light)} className={`preview-board${light ? " light" : ""}`}>
        <div className="hand-playback" data-open><img className="hand-playback-cover" src={appLogo} alt="" /><div className="hand-playback-text"><strong>夜航 · Night Flight</strong><span>DiscoAS · 播放栏</span></div><button aria-label="暂停"><Icon name="pause" /></button><button aria-label="下一首"><Icon name="next" /></button></div>
        <div className="hand-dock"><img src={appLogo} alt="" /><span>手牌 8／10</span><button className="hand-show-client">显示客户端</button><button className="hand-auto-toggle" aria-pressed="true">AUTO</button><button className="hand-refresh"><Icon name="refresh" /></button></div>
        <div className="preview-form"><button className="button primary">保存设置</button><button className="button secondary">取消</button><input className="input" aria-label="示例输入框" placeholder="搜索歌曲" /></div>
      </div>)}
      <div className="preview-board light" style={{ minHeight:200 }}><section className="settings-section"><h2>设置分组标题</h2><div className="setting-row"><strong>内容与描边的间距</strong><button className="button secondary">更改</button></div></section></div>
      <div className="preview-board"><div className="main-window preview-window"><header className="window-titlebar"><div className="window-drag-area">DiscoAS</div><div className="window-controls"><button aria-label="关闭"><Icon name="close" /></button></div></header><div className="app-shell"><section><h2>纸面边缘</h2><p>主窗口边框留白与向外渐隐</p></section></div></div></div>
    </div>
  </div>;
}
createRoot(document.getElementById("root")!).render(<Preview />);
