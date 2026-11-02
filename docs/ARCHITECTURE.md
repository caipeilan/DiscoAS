# DiscoAS 架构与行为契约

本文档描述当前实现的架构、边界和不能随意改变的行为。只记录当前事实；版本历史见 [CHANGELOG](../CHANGELOG.md)，历史设计与旧验证记录已归档到 `.local-docs/archive/`，不作为当前规格。

版本以 [app/package.json](../app/package.json)、[crates/discoas-core/Cargo.toml](../crates/discoas-core/Cargo.toml) 和 [extensions/browser/manifest.json](../extensions/browser/manifest.json) 为准。本文基线：应用 2.1.0、core 2.0.13、浏览器扩展 0.5.1（2026-10-29）。

## 文档分工

| 文档 | 内容 |
| --- | --- |
| 根 `README.md` | 用户维护的安装、使用与项目介绍；助手只报告问题，不直接改写 |
| `docs/ARCHITECTURE.md` | 当前架构、模块边界、数据路径、行为契约 |
| `docs/DEVELOPMENT.md` | 开发环境、测试、构建、发布、原生 QA |
| `docs/CONSOLE_COMMANDS.md` | 控制台与本机 HTTP/JSON API 的生成参考，禁止手改 |
| `crates/discoas-core/README.md` | 核心库 API、复用方式和核心侧数值 |
| `CHANGELOG.md` | 面向用户的版本历史 |

## 目录与依赖方向

```text
app/                         React 前端 + Tauri 桌面适配 + 安装器
crates/discoas-core/         无桌面依赖的 Rust 核心
extensions/browser/          Chrome/Edge/Firefox 播放扩展
extensions/spotify/          Spicetify 播放桥
assets/                      Logo、默认问号封面、彩蛋资源
docs/                        当前文档
```

运行时依赖方向：

```text
前端视图 → features Hook → services/desktop.ts → Tauri 命令
                                              ↓
                                   desktop / services
                                              ↓
                                         discoas-core
```

- `discoas-core` 不依赖 Tauri、WebView、窗口、托盘或 Windows API。调用者传入数据目录和来源快照；核心返回元数据、批次、历史或播放 URI，不打开窗口和播放器。
- `app/src-tauri/src/services/` 接收明确的文件路径和普通数据，不接收 `AppHandle`，不发送窗口事件。
- `app/src-tauri/src/desktop/` 负责应用路径、原生窗口、托盘、快捷键、事件编排和 Win32 调用。
- 前端只有 `app/src/services/desktop.ts` 可以直接导入 `@tauri-apps/*`。其他视图和 Hook 通过语义函数调用。
- 不为新功能复制平台读取、抽歌、缓存或封面算法；桌面版与其他调用者共用核心实现。

## 启动、窗口与托盘

### 启动顺序

`lib.rs` 注册插件：single-instance、opener、dialog、global-shortcut。启动时：

1. 初始化 `user_data` 目录；新数据目录执行首次预设与使用引导初始化。
2. 读取音乐设置、GUI 设置和桌面设置；清理不可用的自动出牌开关。
3. 按 `tauri.conf.json` 创建主窗口、发现浮窗和启动窗口。
4. 安装透明浮窗的原生处理；按设置启动 Spotify/浏览器扩展桥。
5. 注册全局快捷键，创建托盘菜单，启动控制台与自动出牌服务。
6. 异步刷新主窗口数据；最长 6 秒后兜底结束启动等待。

版本升级不移动用户数据目录。`--background` 只抑制启动时显示主窗口；`--discover` 在单实例二次启动时打开发现浮窗。调试构建可用 `DISCOAS_TEST_ROOT` 指向已存在的绝对隔离目录，正式包不读取该变量。

### 窗口

| 窗口 | 说明 |
| --- | --- |
| `main` | 主窗口；歌单、发现记录、设置、控制台、关于 |
| `overlay` | 独立发现浮窗；透明、置顶、无边框 |
| `splash` | 启动动画；透明、置顶、鼠标穿透 |
| `hand` | 手牌与预览卡片 |
| `hand-dock` | Logo/数量/清空/刷新的独立浮窗 |
| `hand-drag` | 拖动中的单张卡片，独立小窗跟随物理指针 |
| `tray-menu` | 托盘右键菜单，独立 WebView，不是原生 Menu |

- 主窗口关闭只隐藏到托盘，同时停止快捷键录制；托盘“退出”才结束进程。
- 发现浮窗关闭走取消流程；手牌窗口关闭走隐藏流程；托盘菜单失焦或 Esc 关闭。
- 托盘左键打开发现浮窗，右键打开菜单。菜单六项：发现、显示/关闭手牌、歌单与设置、暂停全局快捷键、重启、退出。
- 托盘“暂停”暂停的是全局快捷键，不是音乐播放；音乐暂停使用 `playback pause` 或设置中的切歌前暂停。
- 手牌、拖拽层和 Logo 浮窗不抢外部程序焦点；手牌窗口不覆盖整个桌面，只按绘制范围调整大小。
- 主窗口自绘标题栏；边缘缩放、贴靠和最大化仍由 Tauri/Windows 处理。

## 数据路径

应用根目录为 Tauri `app_config_dir`，Windows 上是 `%APPDATA%\com.caipeilan.discoas`；用户数据在 `<app_root>/user_data`。

```text
user_data/
├── settings/
│   ├── music_setting.json
│   ├── gui_setting.json
│   ├── desktop_setting.json
│   ├── onboarding.json
│   └── console_api.json
├── <平台>/<来源类型>/<id>.json
├── pic/library/                 # 歌单/专辑封面
├── history/
│   ├── discovery_history.json    # discovered / selected / weighting
│   └── covers/                   # 历史封面
├── hand/
│   ├── cards.json
│   └── dock.json
└── logs/
    ├── errors.log
    ├── errors.previous.log
    └── client-isolation.log
```

- 平台目录使用稳定平台标识和 `playlist` / `album` / `video` / `favorites` / `collection` / `series`。
- 写入使用临时文件 + 原子替换；读取失败或损坏不应覆盖旧文件。
- 旧版 1.x 数据可在设置中选择目录迁移；迁移只提交经过校验的内容，不删除原目录。
- 不清理未知本地文件、根 `user_data/` 或系统应用数据。

## 平台与来源

平台标识和来源类型：

| 平台 | 支持的来源类型 |
| --- | --- |
| `NeteaseCloudMusic` / `QQMusic` / `KugouMusic` / `KuwoMusic` / `QishuiMusic` / `Spotify` / `AppleMusic` | playlist、album |
| `YouTube` | playlist、video |
| `Bilibili` | video、favorites、collection、series |
| `LocalMusic` | playlist |

- 来源身份是 `platform + sourceId + sourceKind`；歌曲身份是 `platform + songId`。同一歌曲换来源歌单后仍按平台/歌曲识别。
- 同时只允许一个来源 `enabled`；启用/导入/删除必须保持互斥。
- 来源缓存保存歌曲 ID、标题、歌手、封面和来源元数据；不保存音频流、账号 Cookie 或播放器凭据。
- 网易云、QQ 使用批量详情接口；酷狗、酷我、汽水、Spotify、Apple Music 和视频平台使用当前来源快照。缺详情时保留可播放占位，不伪造成功。
- Spotify 导入使用公开 Embed 页面建立匿名会话，不登录账号、不保存用户令牌。
- Apple Music 读取公开页 `serialized-server-data`，要求页头曲目数与实际曲目数一致；播放只走浏览器扩展。
- 酷我播放前读取官网歌曲页的 `kuwo://` URI；汽水使用 `luna://luna.com/playing`；本地音频交给系统默认播放器。
- 封面下载只接受真实图片字节；网络失败使用占位，不阻塞整批。

## 发现流程

### 抽卡与批次

- 一次发现最多 15 首，普通与神秘歌曲合计计入上限；普通数量至少 1，神秘卡排在普通卡之后。
- 只从当前启用来源抽卡；排除空 ID、重复 ID、历史排除和手牌持有 ID。
- 未加权时均匀无放回抽取；加权时按配置权重无放回抽取。
- 批次队列最多 5 批；预加载批数 0–5；可选“批次间去重”，候选不足时停止补充，不循环重抽。
- 单批准备期限 45 秒，包含详情、重试和封面准备；超时不发布半批。
- 详情并发 4；最近 256 项成功详情缓存 5 分钟；临时网络错误重试一次，间隔 250 ms；访问限制、来源不存在、限流和无效平台数据不重试。
- 图片字节缓存总预算 32 MiB；单张只受总预算限制，不再有 5 MiB 单项上限。

### 状态与提交

- `batchEpoch` 标识当前展示批次；`generation` 标识预加载/配置代次。替换、取消、重新展示都必须二次校验，不能只比较歌曲 ID。
- 发现、取消、替换、收牌、播放和来源变更共享操作锁；不能在锁外发布迟到的发现结果。
- 前端整批解码封面并实际显示后，调用展示确认；展示确认才写入历史并推进加权轮次。
- 选择动作记录一次选择；播放或收牌失败不能提前释放当前批次。
- 替换预算默认 1，范围 0–100；失败/过期不扣次数；替补仍保持神秘位置，且可以继续替换。
- 取消后是否换一批由 `refreshOnCancel` 决定；复用已取消批次时保留替换预算和展示轮次。
- 预览使用独立状态，不占当前批次、不写历史/权重、不消耗替换次数，也不能播放或收牌。

## 历史、排除与加权

- 历史文件 `history/discovery_history.json` 固定保留最多 10,000 个平台/歌曲身份；旧 `discovered` / `selected` 数组继续兼容。
- `history_limit` 现在只表示排除范围，默认 200，范围 0–10,000；减少排除数量不删除历史。
- 排除模式：`off` / `selected` / `discovered`；按平台与歌曲生效，同平台切换歌单后仍保留。全部被排除时返回与空歌单相同的错误，不自动放宽。
- 加权模式：`none` / `traditional` / `ranked` / `rotation`。旧 `discovery_weighting.enabled` 加载时迁移为 traditional。
- 传统模式由桌面计算器给出默认参数：以来源曲数 N、每批总数 K，T=min(10000, ceil(N/min(N,K)))；原始权重 100，入选减权 25，选择额外减权 50，恢复轮次 ceil(T/4)，开始加权轮次 T，每轮 100/T，最高 300。核心只应用保存后的参数。
- 排序模式：前 L 首硬排除，随后软带按 smoothstep 恢复；可选时间冷却和久未出现加权。
- 轮换模式：周期 C=ceil(N/min(N,K))；入选按 C 轮恢复，选择按 2C 轮恢复，权重 `1 + 99 × p²`；未出现歌曲为 100。
- 只有真实展示的新批次推进轮次；替换、未展示预加载和取消复用不推进。
- 历史封面按页加载，最多 64 项、4 并发、20 秒总期限；缓存目标 100 项 / 16 MiB 编码字符，当前可见页受保护。

## 手牌、迷你发现与自动出牌

### 普通手牌

- 默认关闭；默认底部、10 张、`Alt+H`、不揭晓神秘信息。
- 支持 1–100 张；位置、缩放、边缘距离、沿边位置、重叠和倾角可调；布局与动画参数在前后端保持一致。
- 卡牌保存独立 ID、真实元数据、播放参数和神秘状态，不保存封面 Base64；`hand/cards.json` 使用原子写入。
- 收牌会记录一次选择并参与发现/预加载排除；手牌满额或被自动锁拒绝时不消费卡片。
- 手动/自动打出都走共用播放入口。派发失败时按现有规则处理：`BUSY_ERROR`、`PREPARATION_ERROR`、预览/手牌模式关闭等前置拒绝保留卡牌，其他正常派发失败会移除卡牌；不是“播放失败后恢复手牌”。
- 位置快捷键 `Alt+1`–`Alt+9` 在普通手牌开启时**始终打普通手牌**；普通手牌关闭时才回退到迷你发现。快捷键数量固定 9，不随手牌容量变化。

### 迷你发现

- 与主发现共用采样、历史、排除和封面缓存，但有独立批次、epoch、替换预算和预加载缓存。
- 可合并显示或分开布局；合并时迷你发现卡在前，普通手牌在后，但位置快捷键的目标规则仍按上一条。
- 迷你发现卡可直接打出，也可在普通手牌开启时拖入手牌；收牌/打出后刷新整批。
- Liquid 效果默认 `static`；可切 `blur`；描边固定开启，`text_outline_width` 默认 1px、允许 0.05–5px。

### 自动出牌

- 实验功能；需要普通手牌开启和保存设置中的自动出牌开启。存在启用来源时，平台不支持会自动禁用；没有启用来源时不做该过滤。
- 观察线程按当前平台的可用媒体会话/窗口标题选一个主观察源；旧 `autoPlayDetection` 字段不再决定固定策略。
- 每 100ms 兜底观察，媒体属性/播放状态事件会提前唤醒。
- 有效外部换曲触发后，先对产生换曲的客户端做定向 `TryPauseAsync`，实际安静 200ms 后确认；不补反向 Toggle，避免 Paused→Playing→Paused。暂停预算 3 秒。
- 有牌时立即建立自动请求并发布锁定状态；手牌、发现、拖拽、位置快捷键和清空/重排由同一 token/锁串行。
- 普通确认窗口 20 秒；网页广告可续期，但单次请求硬上限 120 秒。超时或失败结束本轮，下一次外部换曲继续。
- 自动与手动共用一个播放锁；手牌在忙锁、后台桌面准备失败等前置拒绝时保留，正常派发失败会丢牌。

## 播放与客户端

- `play_song` / 手牌 / 位置快捷键 / 自动出牌最终都走 `desktop/playback.rs`。
- 可选“切歌前暂停”：先暂停当前 Windows 媒体会话，再回退到受控浏览器或 Spotify 扩展；自动出牌已建立的暂停不重复执行。
- 普通平台走客户端协议；酷我先解析官网原生 URI；本地音频交给系统默认播放器；Spotify 支持 `extension` / `scheme` / `pause_then_scheme`；浏览器支持 `extension` / `direct`，Apple Music 仅扩展。
- 浏览器扩展只控制一个绑定页：新选择暂停旧页并导航到新目标，不搜索或暂停用户自己打开的其它页面。
- Spotify 扩展通过 Spicetify 的 `playUri` / `pause`；原生确认只能证明标题/歌手匹配时报告 `confirmed=false`，不能冒充精确身份。
- 客户端隔离：`off` / `temporary` / `persistent`，可配合“独立桌面”。准备失败时阻止 URI 派发并保留手牌，不退回会抢焦点的普通 opener。临时模式结束后最小化或恢复原隐藏状态；长期模式保持后台窗口。
- 关闭主窗口不会退出；托盘“退出”才结束进程。

## 控制台与本机 API

- 应用内控制台与第三方本机接口共用同一语义分发器；完整命令和设置表见 [CONSOLE_COMMANDS.md](CONSOLE_COMMANDS.md)，该文件由脚本生成，禁止手改。
- 当前基线 147 个命令、111 个设置键。
- 本机接口只监听 `127.0.0.1`，每次启动分配临时端口；连接配置在 `user_data/settings/console_api.json`，使用 64 位十六进制 Bearer token。
- 只有 `GET /v1/info`、`GET /v1/catalog`、`POST /v1/execute`；任何带浏览器 `Origin` 的请求返回 403；业务失败仍以 HTTP 200 + `ok:false` 返回。
- 长任务返回 `operationId`，最多保留 128 条；订阅最多 64 个，每个保留最近 256 个事件。命令输入历史最多 200 条。
- `docs/CONSOLE_COMMANDS.md` 的生成检查：`node app/scripts/generate-console-docs.mjs --check`。

## 前端

- 主窗口页面：歌单、发现记录、设置、控制台、关于；发现是独立浮窗。
- 设置分类：通用、发现、手牌与迷你发现、播放连接、快捷键、高级。
- 设置使用三组草稿（music / gui / desktop）；保存只提交变化的组，失败保留草稿。外观草稿即时应用到当前窗口，保存后同步其他窗口。
- 发现浮窗按真实 DOM 列宽键盘导航；方向键不循环，Enter 确认需要键盘选中，R 可替换鼠标悬停卡。
- 手牌/迷你发现/拖拽层使用独立窗口；命中区域只在实际布局/动画变化时提交，颜色和阴影变化不触发扫描。
- 前端持久化字段通过 `services/desktopWire.ts` 转换；Rust 用 Serde 保留旧字段名。改接口必须同时改 Rust、TypeScript 和兼容测试。
- 默认本地选歌键：`W/A/S/D/Enter/R`；默认发现全局快捷键 `Alt+D`；默认手牌 `Alt+H`；默认位置键 `Alt+1`–`Alt+9`。

## 修改时必须保持的行为

- 旧数据格式、来源互斥、预加载版本隔离、批次封面准备、事件操作锁顺序、设置草稿和发现动画。
- 发现/取消/替换/收牌/播放的状态提交必须在操作锁内完成；迟到结果不能覆盖新批次。
- 神秘歌曲的真实元数据只留在内部，Serde 双向跳过；发现 IPC 不能读到或伪造。
- 来源身份始终包含 `platform + sourceId + sourceKind`；歌曲身份始终保留字符串 ID。
- 核心不得引入 Tauri 或 Windows 依赖；`services/desktop.ts` 之外不得导入 Tauri。
- 根 `README.md` 由用户维护，助手只报告问题。
- 行为变化必须更新本文档或 `DEVELOPMENT.md`，并写入 `CHANGELOG.md`；不要新建按任务拆分的文档。

## 验证入口

开发、测试和发布命令见 [DEVELOPMENT.md](DEVELOPMENT.md)。最低要求：

- 前端：`npx tsc --noEmit`、`npm run build`，以及受影响的 `node --test`。
- Rust：对应的 `cargo test --offline`；改动桌面层时同时跑 core 与 desktop。
- 控制台：`node app/scripts/generate-console-docs.mjs --check`。
- 完成交付前编译 Windows NSIS；浏览器预览不等于 Windows 原生验收。
