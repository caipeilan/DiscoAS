# 开发、测试与发布

本文档只记录当前开发流程。架构与行为契约见 [ARCHITECTURE.md](ARCHITECTURE.md)；版本历史见 [CHANGELOG](../CHANGELOG.md)。历史验证记录已归档，不作为当前命令来源。

## 环境

- Windows 10 1809 或更新版本 / Windows 11，x64。
- Node.js 与 npm、Rust MSVC 工具链、Visual Studio C++ 生成工具、WebView2。
- Tauri Windows 前置条件：<https://v2.tauri.app/start/prerequisites/#windows>。

## 目录

| 目录 | 内容 |
| --- | --- |
| `app/` | React 前端、Tauri 桌面适配、安装器、构建脚本和前端测试 |
| `crates/discoas-core/` | 不依赖桌面环境的 Rust 核心及示例 |
| `extensions/browser/` | Chrome、Edge、Firefox 播放扩展 |
| `extensions/spotify/` | Spicetify 桥接模板与测试 |
| `assets/` | Logo、默认问号封面、彩蛋资源 |
| `docs/` | 架构、开发、控制台参考 |

`release/`、依赖、构建输出和 `.local-*` 是本地目录，不进入公开源码。仓库根 `user_data/` 是旧数据，不随源码整理移动；新版数据在系统应用配置目录。不得把账号、Cookie、配对 token、用户缓存或本地 AI 指引提交到公开源码。

## 运行

在仓库根执行：

```powershell
cd app
npm ci
npm run tauri dev
```

只做浏览器预览：

```powershell
cd app
npm run dev
```

浏览器预览可检查布局、设置草稿和外观；托盘、透明窗口、快捷键注册、自启、播放器唤起、客户端隔离和最小化仍需 Windows 原生验收。

## 类型检查与构建

```powershell
cd app
npx tsc --noEmit
npm run build
```

`npm run build` 会先执行 `tsc`，再用 Vite 打包前端。Rust 侧按改动范围执行：

```powershell
cargo check --manifest-path ../crates/discoas-core/Cargo.toml --offline
cargo test  --manifest-path ../crates/discoas-core/Cargo.toml --offline
cargo test  --manifest-path src-tauri/Cargo.toml --lib --offline
cargo fmt   --manifest-path src-tauri/Cargo.toml -- --check
```

`--offline` 要求依赖已缓存；首次下载依赖时可省略。公网探针默认按测试配置跳过，不要为了跑测试改真实用户数据。

## 测试选择

全部测试脚本以 [app/package.json](../app/package.json) 为准。常用分组：

| 改动范围 | 检查 |
| --- | --- |
| 共享类型、旧消息兼容 | `npm run test:transport`；Rust 模型/设置/缓存序列化测试 |
| 设置、控件、数值、主题、布局 | `npm run test:preferences`、`test:settings-layout`、`test:controls`、`test:numeric`、`test:theme` |
| 发现、本地选歌、动画 | `npm run test:discovery`、`test:keyboard`、`test:motion` |
| 封面、来源、歌单、历史 | `npm run test:covers`、`test:source`、`test:library`、`test:history` |
| 手牌布局/动画/区域 | `npm run test:hand`，或分开跑 `test:hand:layout`、`test:hand:motion`，以及 `node --test tests/handRegions.test.mjs` |
| 自动出牌 | `npm run test:auto-hand`、`test:browser`、`test:preferences`，以及手牌动画/区域检查 |
| 浮窗播放栏与封面大小 | `npm run test:media`、`test:hand`、`test:transport`、`test:preferences`、`test:settings-layout`；桌面 `desktop::media_session` 与 `settings::gui_setting`；原生提示层级、拖动与穿透 |
| 控制台 | `npm run test:console`；需要本机接口集成时用 `npm run test:console:native` |
| 使用引导/创建手牌 | `npm run test:onboarding`；核心 `platforms::track_input`、`hand`，桌面 `desktop::onboarding` |
| 托盘、更新、安装器 | `npm run test:tray`、`test:updates`、`test:installer` |
| 浏览器/Spotify 扩展 | `npm run test:browser` / `npm run test:spotify` |
| 启动动画 | `npm run test:startup` |

补充检查：

- 控制台生成物必须与代码一致：`node scripts/generate-console-docs.mjs --check`（在 `app/` 执行）。
- 修改共享流程时，跑受影响的调用方，不只跑单个页面。
- 样式/动画先做可交互预览或截图确认，再进入打包；浏览器预览不等于原生验收。

## 原生 QA

调试构建可把应用根和 `user_data` 指向隔离目录：

```powershell
$env:DISCOAS_TEST_ROOT = "D:\some\existing\absolute\dir"
```

要求目录已存在且为绝对路径；正式 Release 不读取该变量，也不构成便携模式。QA 数据必须预先放置专用来源、偏好和封面，结束时只清理本轮明确创建的文件。

调试入口还支持：

- `--qa-create-hand`：启动后打开创建手牌界面。
- `--qa-auto-off`：启动后临时关闭 AUTO。

原生验收重点：托盘真实点击/失焦/多屏 DPI、透明窗口穿透、快捷键注册、自启、客户端唤起/最小化/独立桌面、播放确认和手牌锁定。浏览器预览不能替代这些检查。

2.1.1 浮窗与播放器验收：

- 对外观调整先展示实际 Windows 浮窗截图，再进入测试与打包。`tests/fixtures/hand-dock-preview.html` 仅供开发检查四象限、换侧、长标题、空会话和主题；设置页以保存后的实际浮窗为调整依据。
- 在 Windows 检查顶部与任务栏上沿落点、位置恢复、100%/125%/150%/200% DPI、多屏负坐标与跨屏拖动。确认两行及动画完整绘制、间隙可点击到后方窗口、换侧和快速反向不移动 Logo 锚点。
- 在设置调整封面大小，检查设置页已移除示意预览、保存后浮窗同步、放弃草稿、重新启动后的值和旧配置默认 100%；文字和按钮大小保持稳定。将播放栏移到提示/对话框所在区域，检查“设置已保存”等信息可见、退出过渡后层级恢复，以及失焦不抢前台、不跳桌面。
- 对实际媒体客户端检查原始歌名/作者、封面更新、暂停与播放互斥、只在 AUTO 开启时显示下一首，以及客户端退出/重启后重新绑定；支持能力以上报为准。验证迟到封面不会覆盖新曲目。
- 模拟锁定期间连续曲目信息、解锁刷新未返回又进入新锁、旧封面迟到和刷新失败：播放栏保留当前配置的神秘封面与 `???`，只有当前解锁后的新采样可以揭晓。`test:media` 覆盖刷新竞态、轻量事件封面合并及锁定展示；原有切歌判断代码保持不变。
- 性能检查先审区域扫描、IPC、封面读取和拖动渲染路径：静止时停止动画扫描，按钮颜色/Logo 动画不持续触发区域测量；暂停/继续不重读或重传封面；拖动位置不刷新整棵播放栏。需要量化帧率时再记录同机负载和帧时间，代码优化不能替代帧率实测结论。
- 托盘重启项分别检查平台图标、真实路径、无目标禁用、重启失败反馈。网易云独立桌面规避步骤需在真实桌面切换场景验证，并单独记录其他客户端效果。
- 平台图标的 mask 样式由 `PlatformIcon.css` 随组件加载；检查独立托盘入口，不依赖主窗口 `App.css` 的副作用。

## 发布

在 `app/` 执行：

```powershell
npm run release:windows
```

脚本要求 Windows x64 和稳定的 `x.y.z` 版本号，执行 `tauri build --bundles nsis`，校验产物为 PE AMD64，并输出到仓库根：

```text
release/DiscoAS_<version>_x64-setup.exe
release/DiscoAS_<version>_x64-setup-full.exe
```

变体：

```powershell
npm run release:windows -- --normal-only
npm run release:windows -- --full-only
```

- 普通包：WebView2 缺失或低于最低版本时，从微软下载并安装/升级运行时。
- 完整包：嵌入微软 WebView2 x64 离线安装器，不需要现场下载运行时。
- 最低 WebView2：`109.0.0.0`。
- NSIS 为 currentUser 安装，不需要管理员权限；语言为简体中文、繁体中文、英语。
- 安装器在下载/运行 WebView2 或卸载旧版前检查 Windows 10 1809（build 17763）及以上、原生 AMD64；Windows Server 与 ARM64 不支持。
- 完整包准备默认读取 `.local-release/MicrosoftEdgeWebView2RuntimeInstallerX64.exe`，也可用环境变量指定：

```powershell
$env:DISCOAS_WEBVIEW2_INSTALLER = "C:\Downloads\MicrosoftEdgeWebView2RuntimeInstallerX64.exe"
npm run release:windows
```

准备脚本会校验微软 Authenticode 签名。构建需要网络下载依赖；发布/推送由维护者明确执行，构建脚本不会推送或发布。

## 贡献与 Git

- 说明改动目的、实际验证结果和仍待原生验收的行为。
- 保持旧数据兼容、来源互斥、预加载版本隔离、事件锁顺序和设置草稿。
- 核心不得依赖 Tauri 或 Windows API。
- 根 `README.md` 由用户维护；发现链接、平台列表或数据目录问题只报告，不自行修改。
- Git 默认只在本地保存和提交；未得到用户明确许可，不推送、不创建 PR、不上传 GitHub。
