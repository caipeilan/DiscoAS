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
| 缓存设置、磁盘复用与清理 | `npm run test:cache`、`test:preferences`、`test:transport`；核心 `image_cache`、`history`、`core::cache`；桌面 `services::desktop_preferences`、`services::library` |
| 手牌布局/动画/区域 | `npm run test:hand`，或分开跑 `test:hand:layout`、`test:hand:motion`，以及 `node --test tests/handRegions.test.mjs` |
| 自动出牌 | `npm run test:auto-hand`、`test:browser`、`test:preferences`，以及手牌动画/区域检查 |
| 浮窗播放栏与封面大小 | `npm run test:media`、`test:hand`、`test:transport`、`test:preferences`、`test:settings-layout`；桌面 `desktop::media_session` 与 `settings::gui_setting`；原生提示层级、拖动与穿透 |
| 控制台 | `npm run test:console`；需要本机接口集成时用 `npm run test:console:native` |
| 使用引导/创建手牌 | `npm run test:onboarding`；核心 `platforms::track_input`、`hand`，桌面 `desktop::onboarding` |
| 托盘、更新、安装器 | `npm run test:tray`、`test:updates`、`test:installer` |
| 浏览器/Spotify 扩展 | `npm run test:browser` / `npm run test:spotify` |
| 启动动画 | `npm run test:startup` |

启动焦点修改需在隔离 Windows 实例覆盖普通启动、仅托盘启动和 `--background`：启动前在其他程序输入，连续记录从窗口创建、开屏出现与消失到 6 秒兜底之后的前台窗口，确认无瞬时激活或键盘输入转移。检查开屏鼠标穿透、托盘和单实例二次启动主动唤起、主窗口点击输入；前端动画测试只覆盖配置及动画完成逻辑，不能替代原生焦点验收。

2026-11-12 原生焦点验收：普通、仅托盘、`--background`、使用引导、暂停开屏脚本后触发原生 6 秒兜底，共五种隔离场景各监听 12 秒。前台切换事件中 DiscoAS 激活次数均为 0，结束时前台窗口与键盘焦点仍在原编辑器；普通及引导启动正常播放动画，托盘及后台模式播放后保持主窗口隐藏。主动二次启动产生 1 次预期激活；托盘“歌单与设置”点击恢复主窗口聚焦，搜索框实际输入通过。启动配置与原动画测试 5 项、桌面偏好相关测试 11 项通过；记录保存在本地 `.local-qa/`。

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

2.1.3 游戏窗口适配：

- 按改动范围执行 `test:keyboard`、`test:discovery`、`test:hand`、`test:transport`、`test:media`、`test:motion`、`test:startup`，以及桌面 Rust `desktop::` 筛选；新增检查覆盖普通发现的原生按键确认/替换、全屏窗口边界和临时落点恢复。前端构建确认 Tauri 命令类型与调用保持一致。
- 原生对照覆盖 Terraria 低于桌面分辨率的全屏，以及 Stardew Valley、The Binding of Isaac 的无边框全屏。记录前台窗口、游戏位置/最小化状态、实际显示模式、发现窗口和浮窗的尺寸与缩放。检查连续开关、关闭动画中重开、动画恢复、方向键选歌、空格收牌、Escape 退出和退出后的键盘释放。
- 分辨率往返检查常驻浮窗回到可见区域、恢复桌面后回到原物理落点，且 `hand/dock.json` 内容不变；屏幕边缘的原有吸附继续保持。多屏 DPI、拖动中切换显示器与真实播放器唤起仍需对应硬件和客户端验收。
- 本轮使用独立应用标识与数据目录，播放来源使用专用本地音频；原始记录保存在本地 `.local-qa/game-ux-213-20261113/`。完整交付的安装器验收沿用发布流程；本轮按用户要求只本地提交，暂不打包。

2.1.1 浮窗与播放器验收：

- 对外观调整先展示实际 Windows 浮窗截图，再进入测试与打包。`tests/fixtures/hand-dock-preview.html` 仅供开发检查四象限、换侧、长标题、空会话和主题；设置页以保存后的实际浮窗为调整依据。
- 在 Windows 检查顶部与任务栏上沿落点、位置恢复、100%/125%/150%/200% DPI、多屏负坐标与跨屏拖动。确认两行及动画完整绘制、间隙可点击到后方窗口、换侧和快速反向不移动 Logo 锚点。
- 在设置调整封面大小，检查设置页已移除示意预览、保存后浮窗同步、放弃草稿、重新启动后的值和旧配置默认 100%；文字和按钮大小保持稳定。将播放栏移到提示/对话框所在区域，检查“设置已保存”等信息可见、退出过渡后层级恢复，以及失焦不抢前台、不跳桌面。
- 对实际媒体客户端检查原始歌名/作者、封面更新、暂停与播放互斥、只在 AUTO 开启时显示下一首，以及客户端退出/重启后重新绑定；支持能力以上报为准。验证迟到封面不会覆盖新曲目。
- 模拟锁定期间连续曲目信息、解锁刷新未返回又进入新锁、旧封面迟到和刷新失败：播放栏保留当前配置的神秘封面与 `???`，只有当前解锁后的新采样可以揭晓。`test:media` 覆盖刷新竞态、轻量事件封面合并及锁定展示；原有切歌判断代码保持不变。
- 性能检查先审区域扫描、IPC、封面读取和拖动渲染路径：静止时停止动画扫描，按钮颜色/Logo 动画不持续触发区域测量；暂停/继续不重读或重传封面；拖动位置不刷新整棵播放栏。需要量化帧率时再记录同机负载和帧时间，代码优化不能替代帧率实测结论。
- 托盘播放器项分别检查未运行显示“打开”、已运行（含托盘驻留）显示“重启”、平台图标、真实路径、无目标禁用和失败反馈；通用协议指向其他程序时不得启动错误客户端。检查菜单显示“打开”后客户端自行启动仍只打开，以及菜单展开后切换来源不操作其他平台。Rust 筛选 `desktop::client_restart`、`desktop::tray_menu`，前端执行 `test:tray`、`test:transport`。可用只读忽略探针检查本机注册入口；原生打开/重启探针仅接受 `DISCOAS_PLAYER_FIXTURE_ROOT` 下的专用进程夹具，按完整路径隔离真实播放器。网易云独立桌面规避步骤需在真实桌面切换场景验证，并单独记录其他客户端效果。
- 平台图标的 mask 样式由 `PlatformIcon.css` 随组件加载；检查独立托盘入口，不依赖主窗口 `App.css` 的副作用。

## 性能复测

固定同一机器、显示器刷新率、DPI、渲染模式和测试数据；采用优化编译及正式前端。原生 QA 构建使用独立应用标识，避免与已安装版本的单实例互相唤起。除 `DISCOAS_TEST_ROOT` 外，同时设置 `WEBVIEW2_USER_DATA_FOLDER` 到测试目录，隔离 WebView 缓存。

在仓库根准备已有的专用数据目录后，可编译优化 QA 实例：

```powershell
$env:TAURI_CONFIG = '{"identifier":"com.caipeilan.discoas.performance-qa","productName":"DiscoAS Performance QA"}'
cargo build --manifest-path app/src-tauri/Cargo.toml --release --features tauri/custom-protocol --offline --config 'profile.release.package.discoas.debug-assertions=true'
Remove-Item Env:TAURI_CONFIG
$env:DISCOAS_TEST_ROOT = "D:\some\existing\absolute\qa"
$env:WEBVIEW2_USER_DATA_FOLDER = "$env:DISCOAS_TEST_ROOT\webview"
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9234"
Start-Process app/src-tauri/target/release/discoas.exe -WindowStyle Hidden
```

先按上面的前端构建步骤准备 `app/dist`。该 QA 构建仅为应用包启用调试隔离入口，核心仍优化编译；实际发布执行正常 NSIS 流程，正式包不读取 `DISCOAS_TEST_ROOT`。

- 测试数据：10/100 张普通手牌、10,000 条历史、10,000 首本地来源快照、10 张不同封面；另保留真实数据副本作对照。关闭 AUTO、全局快捷键、启动联网更新及预加载，抽歌使用本地元数据和封面。
- 每次连续开合 20 次，操作间隔 300ms；记录实际总时长、WebView rAF 间隔 P95/最大值、超过 33.4ms 的次数、长任务、布局时间，以及区域 IPC 请求数。静止时检查没有持续几何扫描，结束后检查最终命中范围和穿透。
- 完整与带版本的状态/历史各读 30 次，分别记录首次、后续耗时和序列化大小；历史页首访与再次进入分开，文字出现与封面完成分开。原有完整接口和新接口都要验证。
- 内存按应用及其 WebView 子进程组统计私有提交内存；工作集之和会重复计入共享页，不能当作独占内存。对比同一实例的卡牌动画结束、收起、拖动结束与隐藏状态，检查临时合成提示及拖拽 DOM 已释放。手工 GC 结果单列。
- CPU、内存与耗时采样依次进行，避免构建、自动测试和截图同时干扰。rAF 表示渲染线程调度间隔，不能直接替代 GPU 实际呈现统计。

2026-11-11 的 2.1.1 基线：Windows 11 build 26300、Ryzen 7 7840H、RTX 4050 Laptop、165Hz、175% DPI。100 张手牌开合 P95 42.4ms、最大 127.3ms；浮窗开合 20 次约 621 个区域请求；10,000 条历史完整读取 P95 64.4ms、3.15MB。测试记录及本机数据副本保留在本地 `.local-qa/`，不进入公开源码。

2.1.2 同机复测：未变化的来源状态回包约 9.9KB，未变化的历史回包 29 字节、P95 约 2ms；浮窗开合 20 次约 267 个区域请求。十张手牌实际拖动 P95 约 6.2ms，未出现超过 33.4ms 的间隔；百张压力场景的一轮 P95 为 36.4ms、最大 78.9ms，仍有长帧。松手后原生拖拽窗口的卡牌 DOM 为零，动画结束后的临时合成提示已清除。

回归目标：100 卡开合 P95 尽量控制在 16.7ms 内，超过 33.4ms 的比例低于 1%；区域请求至少减半；相同版本历史不重传数组。按实际结果报告达到情况，不能用目标代替测量。

缓存验收增加以下场景：

- 10/30/100 卡各开合三轮；折叠结束后原生窗口隐藏、卡牌命中移除，2 秒后 DOM 释放；快速展开取消释放计时器，独立拖动、神秘翻面和迟到动画保持原规则。
- 10,000 条历史，25 张封面冷/热读分别记录；跨页面再次进入时分别测文字和封面完成时间，并记录重复请求数。刷新、删除、编辑和清理后验证旧结果不能写回。
- 本机 HTTP 夹具请求一次后停止服务，重建缓存实例再读同一 URL，验证命中磁盘；渲染读取在缓存缺失时不联网。验证本地图片替换、损坏图片、容量淘汰、0 MiB 关闭磁盘缓存和清理期间迟到下载。
- 在隔离数据目录通过设置保存上下限及不同缓存地址，检查草稿、保存失败、取消目录选择、当前目录清理、无所有权标记的已占用目录与无关文件保留。只对专用 QA 缓存点击清理。
- 用同一批封面和相同窗口状态记录进程树私有内存与工作集；工作集求和可能重复计入共享页。区分首次启动、热态和百卡压力后的状态；新增缓存默认预算 64 MiB，以同场景私有内存增量不超过约 100 MiB 为复测目标。

2026-11-12 缓存优化复测采用上述机器与 10,000 条历史、25 项可见记录、10 张不同图片的夹具，默认磁盘预算 1 GiB、可保留图片内存预算 64 MiB：

| 项目 | 优化前（`17e85d8`） | 本轮结果 |
|---|---|---|
| 未变化的来源状态，热读 P95 | 11ms | 3.6ms |
| 25 项历史封面传输量 | 2,188,729 字节 | 886,616 字节，同页图片按内容合并为 10 张 |
| 25 项历史封面热读，中位 / P95 | 40.5 / 44.1ms | 9.4 / 29.2ms（30 次）；P95 仍高于 20ms 目标 |
| 历史页再次进入，封面完成 | 97–110ms | 固定夹具末轮 37–50.2ms；本轮其他采样最高 65.2ms |
| 十张手牌实际拖动，rAF P95 | 6.2ms | 6.2ms，25 秒内无超过 33.4ms 的间隔 |

百张手牌采用同会话前端对照：旧前端 `17e85d8` 的三轮开合 P95 均为 48.4ms，新前端为 42.5–48.4ms；两者共用本轮原生后端。百卡仍有长帧，尚未达到 16.7ms 目标。跨天的负载差异较大，不能把不同日期的单轮值作为严格增益。

十张展开手牌与万条历史页面的热态进程树私有内存约 880.9 MiB，手动 GC 后约 790.4 MiB；工作集求和分别约 1147.5 / 1049.5 MiB。此前压力后记录为 802.4 / 760.5 MiB，采样阶段不同，不作为严格内存增量。配置预算约束可保留图片，当前页面、解码图片及 WebView 本身的内存另计。

原生验收确认：歌曲封面首次下载一次，应用重启后直接命中磁盘；更换缓存地址、容量限制、拒绝占用目录、清理后不联网回退和无关文件保留均通过。清理保持历史文件内容及递增读取版本；折叠结束原生窗口隐藏、十张卡牌无命中，保留期结束 DOM 归零，重新展开恢复命中。松手后拖动窗口卡牌 DOM 与临时合成提示归零。完整原始记录保留在本地 `.local-qa/`。

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
