import { useLayoutEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { HandDock } from "../../src/features/hand/HandDock";
import { TrayMenu } from "../../src/features/tray/TrayMenu";
import { defaultGuiSettings } from "../../src/types";
import { applyAppearance } from "../../src/appearance";
import { appLogo } from "../../src/assets";
import { emptyMediaSession } from "../../src/features/hand/mediaSession";
import "../../src/App.css";
import "../../src/features/hand/hand.css";
import "./handDockPreview.css";

function Preview() {
  const desktop = useRef<HTMLDivElement>(null), dock = useRef<HTMLDivElement>(null);
  const [area, setArea] = useState({ left: 0, top: 0, width: 1000, height: 500 });
  const [point, setPoint] = useState({ x: 100, y: 90 });
  const [expanded, setExpanded] = useState(true), [enabled, setEnabled] = useState(true);
  const [playing, setPlaying] = useState(true), [dark, setDark] = useState(false), [tray, setTray] = useState(false);
  const [empty, setEmpty] = useState(false), [long, setLong] = useState(false), [track, setTrack] = useState(1);
  const [coverScale, setCoverScale] = useState(1);
  useLayoutEffect(() => {
    const observer = new ResizeObserver(() => setArea({ left: 0, top: 0, width: desktop.current!.clientWidth, height: desktop.current!.clientHeight }));
    observer.observe(desktop.current!); return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => { applyAppearance({ ...defaultGuiSettings, followSystemTheme: false, nightMode: dark }); }, [dark]);
  const state = empty ? emptyMediaSession : { ...emptyMediaSession, sessionId: "demo", title: long ? "夜航 · Night Flight — 一首用于检查长标题省略、文字缩放和两行布局的示例歌曲" : `夜航 · Night Flight ${track}`,
    artist: "DiscoAS Demo", coverDataUri: appLogo, playing, canPlay: true, canPause: true, canNext: true };
  return <main className="dock-preview">
    <header><h1>DiscoAS 2.1.1 · 浮窗</h1><p>拖动 Logo，观察播放栏在屏幕上下半区换位。点击 Logo 收起或展开。</p></header>
    <nav aria-label="预览选项">
      {(["左上", "右上", "左下", "右下"] as const).map((label, index) => <button className="button secondary" key={label} onClick={() => setPoint({ x: index % 2 ? area.width - 85 : 85, y: index < 2 ? 65 : area.height - 65 })}>{label}</button>)}
      <button className="button secondary" onClick={() => setEnabled(!enabled)}>AUTO {enabled ? "开" : "关"}</button>
      <button className="button secondary" onClick={() => setDark(!dark)}>{dark ? "浅色" : "深色"}</button>
      <button className="button secondary" onClick={() => setEmpty(!empty)}>{empty ? "恢复播放" : "无媒体会话"}</button>
      <button className="button secondary" onClick={() => setLong(!long)}>长标题</button>
      <button className="button secondary" onClick={() => setTray(!tray)}>托盘菜单</button>
      <label className="preview-cover-scale">封面大小 <input type="range" aria-label="封面大小" min={50} max={200} value={Math.round(coverScale * 100)} onChange={event => setCoverScale(Number(event.target.value) / 100)} /><output>{Math.round(coverScale * 100)}%</output></label>
    </nav>
    <div ref={desktop} className="preview-desktop">
      <span className="preview-half top">上半区 · 播放栏向下展开</span><span className="preview-half bottom">下半区 · 播放栏向上展开</span>
      <div className="hand-stage" style={{ position: "absolute", width: "100%", height: "100%" }}>
        <HandDock element={dock} coverScale={coverScale} point={point} areas={[area]} expanded={expanded} preview={false} diameter={44} count={4} capacity={10} busy={false} confirm={false} createCards
          toCanvas={(x, y) => ({ x: x - desktop.current!.getBoundingClientRect().left, y: y - desktop.current!.getBoundingClientRect().top })}
          beginDrag={async () => {}} dragging={() => {}} move={setPoint} toggle={() => setExpanded(!expanded)} clear={() => {}}
          autoControl={{ state: { token: 1, busy: false, configured: true, active: enabled }, pending: false, ready: true, toggle: async () => setEnabled(!enabled) }}
          media={{ state, pending: false, error: "", masked: false, control: async action => { if (action === "next") setTrack(track + 1); else setPlaying(action === "play"); } }} />
      </div>
      {tray && <div className="preview-tray"><TrayMenu initialSnapshot={{ generation: 0, gui: { ...defaultGuiSettings, followSystemTheme: false, nightMode: dark },
        labels: ["发现一首歌", "显示手牌", "歌单与设置", "暂停全局快捷键", "重启网易云音乐", "重启 DiscoAS", "退出 DiscoAS"], paused: false, handEnabled: true, clientPlatform: "NeteaseCloudMusic", clientRestartEnabled: true }} /></div>}
    </div>
  </main>;
}
createRoot(document.getElementById("root")!).render(<Preview />);
