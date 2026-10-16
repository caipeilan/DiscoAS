本文档由ChatGPT-6.1生成

# 功能边界与开发入口

应用由可独立运行的 Rust 核心、桌面适配层和按功能拆分的前端组成。桌面版直接调用核心库，示例和测试也调用同一份实现。

```text
前端视图 → 功能 Hook → services/desktop.ts → Tauri 命令
                                              ↓
                                      desktop / services
                                              ↓
                                         discoas-core
```

## 从哪里开始修改

下表完整路径相对于仓库根；同一行后续的文件短名沿用该行已列出的模块目录。桌面命令在 `app/` 中执行；核心和扩展源码各自位于 `crates/` 与 `extensions/`。

| 工作内容 | 入口 |
| --- | --- |
| 添加平台、解析分享链接、歌曲详情或播放 URI | `crates/discoas-core/src/platforms/` |
| 本地音频、目录与 M3U 扫描、标签和内嵌封面 | `crates/discoas-core/src/platforms/local.rs`；桌面导入复用 `desktop/library.rs` 的取消和提交流程，`desktop/dialogs.rs` 选择路径，`desktop/playback.rs` 打开默认播放器 |
| 酷我 / 汽水公开音乐快照与客户端播放链接 | `crates/discoas-core/src/platforms/kuwo/`、同级 `qishui/` 与 `public_music.rs`；桌面二次校验在 `app/src-tauri/src/commands.rs` |
| 调整随机抽歌和神秘歌曲规则 | `crates/discoas-core/src/core/discover.rs` |
| 调整预加载、取消、当前批次或来源校验 | `crates/discoas-core/src/discovery_service.rs`、`core/cache.rs` |
| 调整歌曲封面准备和缓存 | `crates/discoas-core/src/image_cache.rs` |
| 历史记录、近期排除、固定记录保留与加权 | `crates/discoas-core/src/history.rs`、`weighting.rs`、`discovery_service.rs`；前端 `app/src/features/history/`、`app/src/features/settings/WeightingSettings.tsx`、`weightingCalculator.ts` |
| 单卡替换、每批预算、状态 DTO 与信息条 | `crates/discoas-core/src/model.rs`、`discovery_service.rs`、`core/cache.rs`；前端 `app/src/features/discovery/` |
| 手牌保存、收牌、持有排除与打出 | `crates/discoas-core/src/hand.rs`、`discovery_service.rs`；桌面 `app/src-tauri/src/desktop/hand.rs`、`playback.rs`；前端 `app/src/features/hand/`、`features/settings/HandSettings.tsx` |
| 自动打出手牌（实验） | 核心 `auto_hand.rs` 的观察状态、去重与提前量；桌面 `desktop/auto_hand.rs`、`auto_hand_native.rs`；普通手牌 `hand_actions.rs` 与共用 `playback.rs`；网页 `services/browser_bridge.rs`、`extensions/browser/` |
| 迷你发现 | `HandDiscoverySettings`、`DiscoveryService::for_hand`、`Cache::independent`；桌面 `desktop/hand_discovery.rs`；共享手牌窗口、Logo 浮窗和设置预览 |
| 穿透尺寸预览、原生指针和按键观察 | `app/src-tauri/src/desktop/discovery_preview.rs`、`preview_keyboard.rs`；前端 `app/src/features/settings/useDiscoveryPreview.ts`、`app/src/features/discovery/` |
| 导入进度、取消与网络任务提交 | `platforms::PlaylistFetcher::fetch_with_progress`、`app/src-tauri/src/services/library_jobs.rs`、`desktop/library.rs` |
| Spotify 切歌、可选扩展安装与已有配置恢复 | `app/src-tauri/src/spotify_playback.rs`、`spotify_setup.rs`、`services/spotify_setup.rs`、`installer/`、`extensions/spotify/` |
| 浏览器播放页复用与续播 | `app/src-tauri/src/browser_playback.rs`、`services/browser_bridge.rs`、`extensions/browser/` |
| 兼容歌单 JSON 或改变缓存存储 | `crates/discoas-core/src/platforms/storage.rs`、`core/playlist.rs` |
| 调整歌单管理、迁移或设置保存规则 | `app/src-tauri/src/services/` |
| 新用户默认作者歌单与中断恢复 | `app/src-tauri/src/services/bootstrap.rs`、`app/src-tauri/resources/default-playlist.json`、`app/src-tauri/src/paths.rs` |
| 批量删除来源、取消对应网络任务 | `app/src-tauri/src/services/library.rs`、`desktop/library.rs`、`app/src/features/library/` |
| 枚举用户系统字体与多语言字体名 | `app/src-tauri/src/desktop/fonts.rs`、`app/src/components/FontPicker.tsx`、`fontOptions.ts` |
| 紧凑下拉框、隐藏帮助及缩放定位 | `app/src/components/Select.tsx`、`Popover.tsx`、`SettingsControls.tsx`、`popoverPlacement.ts` |
| 键盘选歌、按键录制与卡片选中反馈 | `app/src/features/discovery/keyboardSelection.ts`、`useKeyboardSelection.ts`、`app/src/features/settings/KeyboardSettings.tsx`、`discoveryKeybindings.ts` |
| 窗口、托盘、快捷键、系统对话框、自启或客户端最小化 | `app/src-tauri/src/desktop/`、`lib.rs`、`client_window.rs` |
| 前九张手牌的全局快捷键 | `hand::HandSettings.play_shortcuts`、`desktop/shortcuts.rs` 的身份映射与按住抑制、`desktop/hand_actions.rs` 的当前位置读取；沿用 `play_hand_card` 的失败保留／回牌 |
| 应用风格的托盘右键菜单 | `app/src-tauri/src/desktop/tray_menu.rs`、`app/src/features/tray/`、`app/src-tauri/capabilities/tray-menu.json` |
| Windows 安装兼容检查与 WebView2 准备 | `app/src-tauri/installer/installer.nsi`、`hooks.nsh`、`app/src-tauri/tauri.conf.json`、`app/scripts/` |
| 关于页的项目、作者链接与更新入口 | `app/src/features/about/`、`app/src/About.tsx`、`app/src/features/updates/` |
| 歌单、发现、记录或设置的页面与交互 | `app/src/features/library/`、`discovery/`、`history/`、`settings/` |
| 多页面共用的界面组件 | `app/src/components/` |
| 日夜主题、共用颜色和旧默认浮窗配色兼容 | `app/src/styles/tokens.css`、`app/src/App.css`、`app/src/features/discovery/themePalette.ts` |
| 应用快照同步、草稿保留和通知 | `app/src/hooks/` |
| 主窗口自绘顶栏与窗口控制 | `app/src/components/MainWindowFrame.tsx`、`mainWindow.css`、`app/src/services/desktop.ts`、`app/src-tauri/capabilities/main-window.json` |
| 前端桌面命令、事件或窗口调用 | `app/src/services/desktop.ts` |
| 调试版独立数据目录与原生 QA | `app/src-tauri/src/paths.rs` 的 `DISCOAS_TEST_ROOT` 分支 |

`main.tsx` 按窗口用途延迟加载入口。`App.tsx` 组合主窗口页面与导航；`features/discovery/DiscoverWindow.tsx` 单独组合发现浮窗，只读取歌曲和外观设置。手牌、Logo 浮窗与托盘菜单各自加载自己的视图。`app/src-tauri/src/commands.rs`、`library.rs`、`desktop_preferences.rs` 保留现有命令名，负责接入；旧 `core/`、`platforms/` 路径中的少量文件是转发入口，业务实现以独立库为准。

手牌前端的 `HandWindow.tsx` 负责渲染，`useHandController.ts` 管理快照与操作，`useHandInput.ts` 管理输入，`useHandMotion.ts` 管理动画，`useHandSurface.ts` 管理原生布局确认和绘制区域。静止卡片的矩形由 `regionUpdates.ts` 缓存，运动中的卡片测量至最后一帧；区域提交一次只执行一个请求，并合并期间的新状态。桌面对应 `hand_actions.rs`（持久操作与播放）、`hand.rs`（窗口生命周期）、`hand_input.rs`（原生观察）、`geometry.rs`（坐标、DPI 与绘制范围）和 `hand_dock.rs`（独立浮窗及位置保存）。普通卡牌窗口不接收浮窗位置事件，拖动浮窗不重建卡片；预览中的控制条仍由预览视图显示。

拖动开始时捕获分组位置，鼠标移动只保留最新坐标，每帧合并计算重排、入手位置及播放判定。松手使用最终坐标补算并提交；原生绘制区域准备完成前保持等待，避免快速拖动提前绘制或提交旧位置。

## 依赖规则

- `discoas-core` 不依赖 Tauri、WebView 或 Windows 原生 API。调用者传入数据目录和当前来源快照；核心返回元数据、批次或播放 URI，不打开窗口和播放器。
- `app/src-tauri/src/services` 接收明确的文件路径和普通数据，不接收 `AppHandle`，不发送窗口事件。`desktop` 负责解析应用路径、原生操作和调用顺序。
- 前端只有 `app/src/services/desktop.ts` 导入 Tauri。视图通过语义回调执行操作，Hook 负责请求、状态和取消流程；共用组件不承担歌单和发现业务。
- 共用歌曲与播放参数定义在核心的 `model.rs`；前端对应类型在 `app/src/types.ts`。JSON 字段、命令名或事件载荷变更时，必须同时更新两端及兼容检查。
- 不为新功能复制一份平台读取、抽歌或缓存算法。非桌面程序也应复用核心入口，见 [核心说明与示例](../crates/discoas-core/README.md)。

## 修改时必须保持的行为

普通与神秘歌曲合计最多 15 首，预加载最多 5 批。歌曲批次返回前准备封面字节，前端整批解码后显示；来源或设置变化后旧预加载不能重新入队。歌单/专辑封面只在导入和更新时读取网络，快照读取只用本地缓存。

每次批次准备通过 `SamplingInputs` 捕获来源和一份历史快照，近期排除与加权从同一快照计算；预加载去重直接使用这些输入，再加入已预留歌曲，不重复读盘。下一批重新读取快照，保留 generation 隔离与实时记录更新。

当前来源的 JSON、歌曲 ID 视图与内容指纹由 `core/source_snapshot.rs` 按完整路径、文件长度、修改和创建时间复用，只保留一个当前来源。文件版本变化时重建，当前来源导入或更新提交成功后主动清除该快照；抽歌设置变化保留来源快照。已捕获的只读快照可继续完成在途准备，提交仍检查原有 epoch / generation。主发现与迷你发现通过 `Cache::independent` 共用来源、成功详情、封面及请求锁和四个详情并发额度；批次、替换预算和预加载取消各自独立。封面字节与 Base64 结果一起缓存，共享预算为 32 MiB，发送给前端的载荷和解码图像另占内存。

详情缓存只保存成功结果，临时 HTTP 错误最多重试一次，间隔 250 毫秒；访问限制、来源不存在、限流与无效平台数据不自动重试。网易云通过可选的 `SongDetailLoader` 批量入口合并同批缺失详情，按 ID 映射响应，保留单曲兼容入口。批量请求按缓存标识排序取得单曲锁，等待后重查成功缓存，再占用全局详情额度；不得反向取得锁。失败详情不进入预加载队列，当前失败卡片在再次打开时按原身份修复，不重抽或重复推进历史轮次。预加载任务通过 generation watch 取消过期准备；`PreloadGuard` 在取消或退出时释放运行标记，不能仅在正常返回路径清理。

历史固定保留最多 10,000 个平台 / 歌曲身份，兼容字段 `history_limit` 仅表示近期排除上限，范围 0–10,000；调整排除上限不裁掉历史。选择 / 入选模式按同平台最近符合状态的歌曲排除，排除后为零按无歌曲处理，不自动放宽。历史与加权的轮次、歌曲事件一同存入 `history/discovery_history.json`，由 `HistoryStore` 原子写入一次；`WeightStore` 只读取同一快照。删除、清空或修改状态必须同时更新对应权重事件，验证或写入失败不得出现部分更新。固定保留上限同时约束加权身份，避免另一份状态无限增长。

新批次实际展示后才写入历史并推进所属平台的加权轮次。同一批次重新打开或替换单张卡片不推进轮次；替补实际展示后仍记录入选。普通候选保持同批去重，权重最低为 1；入选与选择减权分别随发现次数衰减，达到恢复次数后恢复原始权重，之后按配置的等待次数开始加权并受最高权重约束。默认不开启加权。可选 `preload_deduplication` 同时排除当前批次与已入队预加载中的歌曲，候选不足时停止补充队列，未展示批次不提前写入记录。

抽样先用借用的歌曲 ID 过滤空值、重复与排除项，只复制最终选中的 ID。普通抽样使用 `rand::SliceRandom::choose_multiple`；加权抽样只构建一次 `WeightedIndex`，每次选中后将该项权重设为零，保留按剩余权重连续无放回抽取的概率及普通／神秘卡片顺序。

选择先于展示确认时，`HistoryStore::record_events` 在同一份快照内记录入选、选择及权重事件，只规范化并原子保存一次，再用提交后的快照更新近期排除。保存成功后才标记批次已展示；写入失败保留重试能力，迟到展示确认不会再推进轮次。

发现、取消、来源/设置修改与播放共享 `Cache::operation`。桌面命令用 `discover_in_operation` / `cancel_in_operation`，在同一个操作锁内修改状态并发送事件；不能在锁外发送迟到的发现结果。普通调用者可直接用自持锁的 `discover` / `cancel`。播放前验证当前批次的完整来源身份，酷狗文件名取可信缓存；唤起失败不能提前释放批次。

网络获取完整成功后才能替换缓存。旧 JSON、来源互斥、未保存草稿和迁移预检必须保留。数据目录与仓库根 `assets/` 的原始与彩蛋资源不能因重构移动或清空。

安装包通过资源映射将 `assets/最伟大的作品.png` 直接安装为 `$INSTDIR/最伟大的作品.png`，与程序主文件同级。

`MusicSetting.has_same_discovery_configuration` 统一界定发现相关变更：启用来源的完整身份及发现参数参与比较，备注、未启用来源、全局快捷键和本地选歌按键不使批次失效。桌面 `library-changed` 载荷为 `{ discoveryInvalidated: boolean }`，前端对 `false` 只刷新来源快照，旧无载荷事件仍按失效处理；替换提交使用同一比较规则，继续检查 epoch / generation。`get_app_state` 在后台线程读取磁盘，列表摘要只解析歌曲 ID 和来源封面，跳过完整 `tracks_info`。前端快照版本防止迟到读取覆盖后续保存，未保存草稿仍保留。

秘密歌曲的真实元数据保留在内部 `real_metadata`，Serde 双向跳过该字段；发现卡片保持隐藏，历史使用真实信息。旧记录补全最多 100 项、4 并发、单项 8 秒、总计 20 秒；网络不持有操作锁，短暂合并只修改仍与原快照一致的行，不能恢复已经清空的记录。

八平台能力由核心 `supported_kinds` / `validate_kind` 约束。酷我、汽水及视频来源快照含完整 `tracks_info`，发现详情从快照读取；Bilibili 分 P 用 `BV号_pN` 标识。酷我校验官方 UTF-8 分页的来源 ID、页码、总数和进度；汽水读取官方分享页 SSR，实际歌曲数量不符时拒绝提交。两者复用 `public_music.rs` 的快照与详情逻辑，不另建抽歌或封面缓存机制，核心继续不依赖 Tauri。

酷我原生播放 URI 在官网歌曲页取得，并核对固定协议、字段和目标 `MUSIC_<id>`，保留官方 Base64 的字面 `+`。桌面先在操作锁内验证来源并记录 `current_batch_epoch`，释放锁后联网，重新获得锁后二次核对完整来源与批次标记，再读取最新播放设置并唤起客户端。只影响预加载的 `generation` 变化不使当前可见批次失效；取消、换批、选择和来源变化仍不能让旧请求播放。汽水使用官方 Windows 客户端 `luna://luna.com/playing?track_id=...` 协议；客户端识别仅匹配 `SodaMusic.exe`，酷我匹配 `KwMusic.exe`，不把同名辅助进程作为播放器。URI 构造或元数据读取通过不能代替真实客户端切歌验收。

浏览器扩展只控制绑定页面，确认首次播放后结束比较；自动续播不写入历史或触发发现事件。Spicetify 安装 / 配置在操作锁外运行，校验固定资源、保留已有配置和备份。首次安装仅由设置中的明确操作发起；启动、保存模式和选歌的自动入口必须先检查有效且未移除的 DiscoAS 安装记录，服务锁内再检查一次。NSIS 没有安装时自动部署钩子，升级保留配置，卸载仅撤销本应用内容。

## 发现状态、替换与预览

`discover_batch` / `get_discovery_state` 返回 `DiscoveryStateDto`，前端对应 `DiscoveryState`：`songs`、`batchEpoch`、`remainingSongs`、`replacementsRemaining`、`exclusionEnabled`、`replacementEnabled`、`preview`。批次或元数据改变时，在操作锁内发送完整 `discovery-state-changed`；仅计数、排除与预算改变时发送不含 `songs` 的 `discovery-status-changed`。核心分别维护批次 epoch 和元数据 revision，同身份的详情修复仍触发完整更新。前端按 epoch 合并解码期间收到的轻量状态，不重启未变卡片的动画。歌曲设置保留 snake_case，跨桌面的 DTO 使用 camelCase。

`get_discovery_preferences` 只返回歌曲和外观设置。三类保存命令返回并通过对应事件发送 `PreferencePatch`，只携带本次保存的设置组，不读取歌单摘要和封面。主窗口按组保留未保存草稿；全量读取期间收到保存结果时，保留已保存的组并接受其它来源数据。普通发现设置变化只有涉及手牌、按键或秘密封面时才刷新手牌窗口。

卡池计数通过歌单 ID 视图读取，跳过完整 `tracks_info`；已有 JSON 快照以借用方式解析 ID，避免克隆整份元数据。即时抽取和尺寸预览使用同一份 `SamplingInputs` 同时获得候选与剩余数量；预加载批次展示时重新计算排除，反映准备后新增的历史与手牌。

`replace_discovery_song` 接收完整来源身份与 `batchEpoch`。锁内获取替换票据，锁外准备元数据和封面，再持锁复核来源、当前批次、卡片位置、已移出身份与剩余预算并提交。失败或过时的替换不消耗次数，成功只替换指定位置并更新状态；神秘位置仍保持隐藏，替补也可再替换。预算默认 1，0 关闭，每次发现最多 100 次。启用预加载去重时，替补不能与后续预加载同时占用同一身份。

`mutate_discovery_history` 按 `HistoryIdentity` 执行删除或入选 / 选择布尔状态修改；整个请求先校验后一次保存。历史前端搜索、状态筛选和跨页多选使用平台 / 歌曲组合身份，每页 25 条；`get_history_covers` 单次最多 64 项，按本页懒取，封面 data URI 不写回历史快照。旧元数据修复、封面请求与删除 / 清空共用版本检查，迟到结果不能恢复已删记录。历史界面和新增加权、去重、替换选项不加说明小字或问号。

发现卡片网格每行最多 5 张，列数随当前可用宽度与卡片比例减少；键盘移动仍以实际 slot 行位置为准。独立 `.discovery-status-bar` 定位在卡片组下方，不参与全屏居中计算。开启排除时第一行显示剩余卡池，开启替换时第二行显示次数；功能关闭跳过对应行，两项关闭则不产生信息条。`GuiSetting` 的 `replacement_button_size`、`discovery_bar_size` 分别独立缩放，范围 0.50–3.00。

普通发现浮窗淡入期间禁用卡片操作，卡片本身保持 100% 不透明度，由外层统一淡入；尺寸预览继续使用自身的 25% 透明度规则。

`start_discovery_preview` 使用设置 / 外观草稿调用 `prepare_preview`，保存独立 `PreviewState`，不修改真实当前批次、预加载队列、历史、权重轮次或替换预算。`update_discovery_preview` 更新同一会话的外观及按键，不重新抽歌、不保存草稿；`end_discovery_preview` 或 `preview-closed` 结束预览并同步设置开关。预览卡片默认 25% 不透明度，指针悬停或键盘选中变为 100%，前后端均阻止播放和替换。会话 generation 与生命周期锁同时约束准备、窗口效果、退出动画及观察任务，关闭后的旧准备结果不能重新打开浮窗。

Windows 预览让卡片和背景穿透；右上退出按钮是唯一临时接受鼠标点击的命中区域。`desktop/discovery_preview.rs` 使用 `GetPhysicalCursorPos`、窗口屏幕原点和当前 DPI 比例转换指针，避免后台线程的 DPI 虚拟化，发送 `preview-pointer`。`preview_keyboard.rs` 在 Tauri UI 消息循环中安装临时 `WH_KEYBOARD_LL`，只保留配置键、修饰键和 Esc 的瞬时状态，始终转发输入；回调只发送有界事件，不等待生命周期锁、不执行窗口或文件操作。退出时先原子停用，再按会话卸载，旧卸载不能移除新会话。Esc 有独立优先通道，打开时按住的键不会触发，修改按键后缓冲的旧事件被拒绝。前端区分 native / DOM 来源防止重复处理。`preview-appearance` 只作用于预览会话，退出后恢复正式外观。原生调用不能下移至核心库。

预览指针观察保持 24ms 间隔，每次只复制关闭按钮区域；按键事件在会话锁内核对当前绑定，两条观察路径均直接读取会话，不复制整批歌曲与封面。

原生窗口显示及关闭回调先转入后台执行，统一按 `operation → lifecycle` 的顺序提交；UI 线程不能反向等待生命周期锁。安装按键观察前释放这两把锁，再等待 UI 确认。预览关闭期间的查询和播放继续隔离，正常浮窗打开时才清除关闭标记；重复退出为最新 generation 保留隐藏兜底。

## 界面与交互约定

### 手牌模式（2.0.1）

`MusicSetting.hand` 缺失时使用关闭、底部、10 张、非驻留、Alt+H、不揭晓神秘信息的默认值。发现确认在开启模式时调用 `collect_hand_card`，验证当前完整身份和批次 epoch，写入 `HandStore` 并记录一次选择，返回手牌 ID 与发现状态。默认先淡出其余发现内容，保留选中卡片，过渡完成后调用 `show_collected_hand_card`。手牌窗口先将收牌动画暂停在来源位置，解码封面并显示；下一帧通过 `hand_arrival_ready` 确认接住卡片，再隐藏发现浮窗并继续收牌动画。该确认只作用于对应 generation，收纳或关闭会结束等待。开启 `keep_discovery_open` 后直接收牌，只移走选中的卡片，保留本轮历史和替换预算。手牌满额不消费当前卡片。

`hand/cards.json` 保存独立身份、原平台播放参数和真实元数据，不存封面 data URI；前端投影按收牌时的揭晓选项隐藏信息。切换或删除来源、重新启动或关闭模式均保留手牌。持有身份按平台 / 歌曲排除发现和替补，收牌使旧预加载准备失效；打出和弃牌重新计算持有集合。历史排除继续独立生效。打出通过共用 `desktop/playback.rs` 唤起客户端，不再记录选择或重复减权。明确播放失败将卡片放回原位置；普通协议的唤起成功沿用现有播放接口含义。

2.0.8 的 `hand.auto_play` 默认为关闭，仅在普通手牌开启时工作；`auto_play_position` 为 `first` / `last`，读取已保存的普通手牌顺序，合并布局中的迷你发现不参与选择。`auto_play_detection` 为 `system` / `window` / `mixed`，旧配置默认 `mixed`。网易云、QQ、酷狗、Spotify 可选三种方式；酷我仅窗口名，汽水、网页和本地仅系统接口。设置页禁用当前启用歌单平台不适用的选项，保存设置和切换来源时也按同一规则纠正选择。QQ、Spotify、汽水的有效进度用于曲终前切歌，初始提前约 1 秒，随后按本次运行中请求到新曲确认的延迟调整，范围 0.5–4 秒；网易云、酷狗在当前客户端只提供曲目变化。暂停不触发，重复曲目通知不连续出牌。原平台直接切歌同样触发自动打出。启动或重新开启后先建立当前播放基线，空手牌消费当前信号但不反复重试。

窗口名和混合方式读取已验证客户端主窗口的曲名，忽略应用名称、空标题等过渡状态；酷我循环滚动的长标题按应用后缀还原，滚动不视为切歌，酷狗和 Spotify 按歌手在前的标题布局匹配。首次有效窗口变化先临时静音该平台的全部已识别音频会话，再屏蔽触发并打出；读取仍继续，确认目标曲目的第二次有效变化后立即恢复原静音状态并建立基线。酷我包含同一安装目录下真正出声的 `KwService.exe`。混合方式为接口与窗口分别保存观察基线，等待另一路追上同一次打出，防止延迟通知重复消费手牌。曲名匹配仅去掉末尾的媒体用途注释，保留演唱者及录音版本括号。共用播放入口为发现、手牌、迷你发现及快捷键请求设置来源标记，所有 DiscoAS 自身切歌均被屏蔽。失败、关闭功能、预览、退出及确认超时释放临时静音；自动播放未确认时沿用失败回牌并暂停自动打出，下一次手动播放恢复。普通确认窗口 20 秒，网页广告延长等待但总计不超过 120 秒。

观察线程平时每 250ms 采样，接近结尾和临时静音期间每 100ms 采样；未启用时等待唤醒。媒体会话枚举缓存 1 秒，媒体属性变化事件与每秒刷新共同更新曲名；客户端主窗口缓存 HWND/PID，并在句柄失效后重新查找。系统接口方式不读取窗口名，窗口方式不读取媒体会话元数据。

浏览器扩展 0.4.0 在首次播放确认后继续报告主视频进度、`ended` 和受控标签页内的曲目变化，保留首次选择的确认结果；通知绑定请求代次、标签页、顶层 frame 和浏览器提供的 URL，旧页面和 DiscoAS 发出的暂停不产生自动出牌信号。复用标签页切歌时由新文档主动取得绑定，避免尚未卸载的旧文档使用新请求编号上报曲终进度。YouTube / Bilibili 的自动打出需要扩展播放方式，升级后重新加载扩展与已有播放页。本地音频继续使用系统关联播放器；先以请求歌曲的标题确认媒体会话，再持续跟踪该会话，避免将仍存在的浏览器会话当作本地播放器。连续观察取决于默认播放器是否公开可匹配的 Windows 媒体会话。

手牌使用两个独立的原生窗口：`hand` 只绘制卡牌，`hand-dock` 只显示 Logo、数量和清空操作。布局沿用桌面画布坐标，卡牌原生窗口按绘制范围调整尺寸；入手与拖动扩展范围，动画结束缩小右侧／下侧范围并保留可见窗口的原点，收纳或最后一张牌消失后隐藏。隐藏后再次打开可重新定位。浮窗根据内容宽度预留左右伸展空间，伸缩只改变内容和绘制区域，拖动使用 Windows 原生窗口移动。`desktop/hand_dock.rs` 与 `HandDockWindow.tsx` 负责浮窗，不接收歌曲封面；`desktop/hand.rs` 与 `HandWindow.tsx` 处理卡牌、持有状态和播放。`desktop/floating_window.rs` 统一透明窗口的框架可见性与原生无边框处理，过滤底层框架重加的标题栏样式，并禁止非客户区绘制。手牌与 Logo 浮窗采用非激活交互，显示和鼠标操作保留外部程序焦点。前端 `surface` 描述窗口相对桌面画布的偏移与 DPI 比例，改变后先提交布局，再显示。卡牌原生裁剪范围覆盖完整绘制路径，去掉保留窗口原点产生的空白；仅绘制包围范围变化时更新裁剪，逐帧命中矩形不参与裁剪。唯一的指针观察任务按卡牌命中范围切换穿透；鼠标捕获负责区域外拖动。已显示的窗口更新卡牌时不再次显示；定位等待原生线程执行完成，再通过命令回复同步实际坐标，前端提交布局后显示卡牌。`handDrawingBounds.ts` 提前计算静止、悬停、收纳与完整入手路径，避免逐帧调整原生窗口尺寸；入手动画开始、结束和取消时均主动更新区域。

Logo 浮窗创建时将卡牌窗口设为 Windows owner，由系统保持浮窗始终高于卡牌。两者仍为独立顶层窗口，位置与显示状态分别管理，隐藏卡牌窗口后浮窗继续显示。

手牌按键复用发现方向键和确认键，另支持 Delete 弃牌；Esc 取消卡牌拖动，否则收纳，预览中则退出预览。快捷键或托盘展开可单独进入键盘选牌状态，普通收牌和鼠标展开不接管按键。原生 Hook 仅消费已识别的手牌按键，其它输入继续传给外部程序；前台窗口变化后的下一次按键立即透传并退出接管，不轮询前台状态。尺寸预览继续观察按键并全部透传。发现窗口按所选显示器的工作区定位，采用无边框普通窗口，排除任务栏占用；每次唤出重新读取工作区，层级高于手牌和 Logo 浮窗。

手牌分为展开、收纳、完全关闭三种状态。独立 Logo 浮窗可以拖到任意显示器的位置，靠近边缘时允许部分藏到屏幕外，贴边侧圆角平滑变为直角；位置以物理坐标保存在 `hand/dock.json`，只在松手时写入。收纳时只显示圆形 Logo，展开时按所在半屏延伸成显示“手牌 X/Y”和删除按钮的长条。展开、收纳均固定 Logo 圆心，跨越半屏时先缩回圆形，再向另一侧延伸；快速往返只更新目标方向，在圆形时采用最后的位置，不排队播放。数量和删除按钮在缩回时隐藏、延伸时淡入，Logo 始终跟随指针。`HandDock.tsx` 与 `floatingDock.ts` 负责交互和几何，卡牌布局继续由 `handLayout.ts` 处理。点击 Logo 或按手牌快捷键切换展开／收纳，快捷键展开有牌手牌时进入键盘选牌状态；托盘完全关闭两个窗口。`hand-visibility-changed` 复用卡牌快照，`hand-dock-state-changed` 只携带数量、布局、外观和位置；浮窗拖动的 `hand-dock-moved` 不触发卡牌重绘，松手后同步浮窗位置；卡牌布局不避让浮窗。清空按钮通过前端手牌事件调用原清空操作，保留逐张消失动画与操作锁。收纳动画完成后卸载卡片、隐藏卡牌窗口并停止原生指针和按键观察；空手牌保留浮窗。背景刷新保留收纳状态，手动关闭后不因常驻设置重新打开；收进新卡时自动展开并播放收牌动画。布局预览临时隐藏独立浮窗，沿用穿透画布内的退出按钮，退出后恢复此前状态与浮窗位置。

展开和调位动画使用 CSS transform；`handMotion.ts` 让收牌与回牌动画独占变换，结束或中断时先保留当前可见帧，再交回最新布局。打出复用原卡牌节点和已解码封面。悬停、调位和收纳的绘制包围范围按卡牌逐张计算，不为每个悬停位置重算整副布局。拖动位置按动画帧直接更新，只有插入位置或可打出状态改变时才更新 React；卡片内容单独复用。松手后提交顺序并更新原生快照，避免重新读取整副手牌的封面；取消拖动恢复原排列。打出动画在松手时捕获卡片实际位置、角度和缩放，原地缩小淡出，不等待播放结果才启动；播放失败后恢复手牌。弃牌和清空立即提交原操作，保留正在消失的卡片节点，在原位置轻微缩小、下移、淡出，清空时使用短暂错峰。神秘揭晓的背面沿用来源发现卡片已准备的封面，支持用户自定义图片。边缘距离允许 -500–500px，负值只改变靠边偏移，不改变卡片尺寸或沿边位置。悬停或键盘选中的卡片按放大后的尺寸完整移入工作区；取消选中后恢复原有布局。收牌、展开、打出、弃牌、回牌、取消及神秘揭晓遵从减弱动画设置。

命中区域在布局变化及实际动画期间更新，按物理像素对齐并合并重复结果，动画结束后停止测量。原生绘制范围覆盖完整动画路径，不用迟到的逐帧矩形裁切画面；已有指针观察根据卡牌命中切换穿透，空白区域可以操作背后的程序。拖动按下时保留当前悬停帧，原生层预留完整桌面画布及边缘余量，前端等待绘制区域和布局提交后才移动卡片；等待期间累计指针位移，准备完成后直接绘制最新位置，快速松手也等待这次确认。拖动结束恢复紧凑范围，指针捕获接收窗口外的拖动；原生层跳过相同区域并暂停重复指针事件。封面缓存共享只读字节，不在每次读取时复制整张图片；总缓存预算用于淘汰旧图，单张封面不再按 5 MB 拒绝。网易云下载请求使用 640×640 参数，Spotify 从返回的图片列表优先选取不超过 640px 的版本；已有 QQ、酷狗和汽水封面请求沿用各自的缩略图尺寸。

手牌预览使用已有手牌或四张离线示例牌，操作设置草稿而不保存或播放；卡片默认 25% 不透明度，悬停 / 键盘选中恢复 100%，卡片和空白区域穿透。预览按键复用同一原生观察实现，与发现预览互斥。手牌模式关闭、预览退出与重新唤出使用 generation 隔离迟到的显示和隐藏操作。

### 迷你发现

`MusicSetting.hand_discovery` 默认关闭、合并布局，具有独立的左右／底部布局参数。`DiscoveryService::for_hand` 使用独立批次、epoch、替换预算和预加载缓存，与普通发现共用采样、持有排除及历史规则；两个缓存通过同一操作锁串行更新持久数据。来源和发现偏好变化同时使两个缓存失效；布局和文字外观修改保留当前发现批次。

迷你发现卡直接打出播放，开启普通手牌后也可拖入手牌位置。合并时，迷你发现位于普通手牌之前，越过最后一张迷你发现卡进入普通手牌插入范围；分开时，拖入普通手牌区域收牌。入手和打出均刷新整个迷你发现批次，单卡替换只更换对应位置。入手沿用来源位置交接与不透明收牌动画，先翻转剩余迷你发现卡，再过渡补入一张卡背，最后解码并一起揭示新批次；补入和拖动插入时普通手牌平滑让位。卡背只显示 Logo，合并分界只画短虚线；拖动入手复用“松开以加入手牌”提示。

两组卡牌共享手牌窗口和焦点层级，合并布局按当前混合顺序响应前九位快捷键；分开时仅普通手牌响应这些快捷键。同侧分开的两组使用有间隔的卡列。迷你发现卡使用 Liquid Glass，浅色主题为白字黑描边，深色主题为黑字白描边；描边固定开启，旧配置中的 `text_outline_enabled` 被忽略。`text_outline_width` 默认 1px，以 14px 全局字号为基准同比缩放。“描边粗细”复用字号设置的 `NumberField`，允许 0.05–5px 小数，并随设置草稿即时预览。卡牌不附加类型小字，右上角为替换，入手后为删除。共用 Logo 浮窗增加刷新、卡池和剩余替换次数；普通手牌与迷你发现的控件随各自开关显示。

### 通用界面

手牌按卡片身份复用已准备的封面，原生快照使用共享只读卡片。后续 `hand-state-changed` 的 `cardOrder` 给出完整顺序，`cards` 只携带新增或变化的卡片；首次快照仍发送全部卡片。前端保留未变化的卡片对象，首次回复之前收到增量且缺少卡片时重新读取完整快照。布局预览复用当前卡片，连续更新只保留一个待执行请求。卡片高度为大字号增加两行歌曲名和一行歌手名的空间，布局、动画和命中区域使用同一尺寸；按钮随字号缩放，阴影使用共用日夜主题参数。

记录页在后台读取和合并完整历史，搜索文本、分页及选中统计按输入变化缓存。编辑、清空命令与对应历史事件携带页面 `source`，该页面直接使用命令结果，其他事件仍触发刷新；编辑期间收到外部事件在提交后合并刷新。Spotify 状态只在扩展模式、分区位于视口且页面可见时轮询，配置过程中暂停。

仓库目录按 `app/`、`crates/`、`extensions/`、`assets/`、`docs/` 划分。源码目录搬迁不改变系统用户数据位置，浏览器与 Spotify 的已配对导出目录也保持原格式。

桌面 setup 首先调用 `seed_new_user`。只有不存在持久文件的用户数据目录才预设并启用作者网易云歌单 `8285082830`；随包快照仅包含来源字段和完整歌曲 ID，不包含账号、Cookie 或配对数据。曲数由快照决定，更新歌单无需修改固定曲数；资源测试确认作者歌单身份、歌曲与公开字段，运行时直接读取内置 JSON。已有设置（包括显式空来源）、缓存、历史、未知文件及符号链接均优先保留。初始化无需联网，之后沿用普通刷新与预加载。首次两文件提交通过明确的本应用事务标记处理进程中断；只在标记有效、没有用户设置、目录仅含未修改种子文件时恢复。标记兼容 2.0.0 的旧哈希字段，新标记只记录应用、格式版本和歌单身份，缓存归属通过实际字节比较。

托盘右键菜单不绑定 Windows 原生 Menu，由单例 `tray-menu` 无边框 WebView 展示。前端先订阅事件再报告 ready，每次右键接收保存后的语言、日夜、字体、暂停及手牌状态，测量后再显示；过期 generation、失焦与 Esc 都不能重新打开旧菜单。六项动作只接受菜单本窗口请求，执行后隐藏，手牌模式关闭时对应动作禁用。定位使用目标显示器的物理工作区域，处理负坐标、任务栏与 DPI；菜单前台期间低级按键观察继续转发系统输入，但不同时操作浮窗卡片。浏览器预览只验证外观，实际托盘点击、失焦和多屏位置仍需 Windows 原生验收。

Logo 的矢量源为根 `assets/DiscoAS.svg`：黄、蓝、红从左至右叠放，三张同高卡片使用平滑圆角及明亮实体配色；黄色向下、红色向上各偏移 48 个矢量单位，蓝色居中。修改后在 `app/` 运行 `npm run icons`，通过 [Tauri 图标工具](https://v2.tauri.app/develop/icons/) 同步 `assets/` 中的 PNG / ICO、桌面图标及浏览器扩展图标；侧栏与页面图标直接读取 SVG。侧栏图标为 40px，窄窗口缩小品牌区间距以容纳文字。不要分别修改导出的位图，原始问号封面和彩蛋资源不受此流程影响。

`StartupScreen.tsx` 使用同源矢量路径将三块拆开，独立加载 `startup.css`，不依赖延迟加载的主窗口样式。原生 `startup_ready` 先按显示器物理范围定位，再显示透明启动窗口。启动窗口不抢焦点且鼠标穿透，点击外部程序不会打断动画。每次新进程启动均播放，结束后根据隐藏启动设置决定是否显示主窗口。黄块从左下偏上、蓝块从右下偏左、红块从右上偏左依次沿弧线进入；`startupMotion.ts` 采样归一化 tanh 曲线控制速度，并在垂直于起终点方向的轴上加入正弦弧度，整体放大淡出继续使用 tanh。图标为 `min(42vw, 42vh, 400px)`，总时长 2240ms；减弱动画时保留短淡入淡出。

`desktop/rendering.rs` 在创建窗口前固定当次进程的渲染策略，主窗口、发现、启动、手牌、Logo 浮窗与托盘共用相同 WebView2 参数。`rendering_mode` 默认 `auto`，支持 `integrated`、`discrete` 与 `cpu`；设置保存后重启生效。GPU 偏好为实验选项，取决于运行时及硬件。

迷你发现的 `liquid_strength` 默认 100，允许 0–100% 小数；0 关闭背景模糊，强度同时改变模糊、饱和度、底色透明度和高光，设置草稿即时预览。卡背 Logo 为 78px。手牌渲染预先分组并缓存 ID 与位置映射，按每帧变化复用布局，避免逐张重复筛选整副手牌。Logo 浮窗测量实际内容宽度，原生画布为左右伸展保留对应空间；原生拖动期间放开绘制裁剪，松手后恢复内容区域，跨中线时两侧绘制均完整。

当前界面采用紧凑、灰白层级的桌面主题：侧栏和列表以中性背景、细分隔线区分层级，青色用于主要操作、状态及键盘焦点。所有页面与挂到 body 的菜单共用 `styles/tokens.css` 的日夜语义颜色；面板和菜单不使用玻璃或背景模糊。`discovery/themePalette.ts` 只将完整匹配旧默认值的卡片 / 退出配色映射到新主题，自定义组整组保留，不能借主题更新改写已保存的数据。发现反馈放在 `features/discovery/discovery.css`：鼠标轻微抬起，键盘单一 2px 焦点框，卡片边框透明以免形成双框；替换按钮同步移动。卡片不允许选中文字或拖动封面。几何、五列上限、信息条独立定位和预览行为保持原约定。

主窗口使用自绘 `MainWindowFrame`：紧凑固定顶栏、右侧最小化／最大化／还原／关闭按钮，空白顶栏支持原生拖动与双击最大化；关闭调用原窗口关闭事件并收至托盘。Tauri 保留边缘缩放、贴靠和最大化工作区计算，主窗口不接入透明浮窗的非激活适配。窗口状态按事件同步，连续缩放合并查询，不持续轮询。

设置标题和保存按钮使用 `.settings-topbar` 固定在主内容滚动区域顶部，sticky inset 抵消主内容响应式顶部内距；日夜、字号和界面缩放共用原外观状态。顶栏层级低于 body 帮助浮层，不应用页面进入的位移动画。更改顶栏时应核验滚到本地数据仍可见且可保存，不能在顶栏上方露出正文。

浏览器扩展按 `ExtensionBrowser` 导出 Chromium / Firefox 清单，`export_browser_extension` 和 `open_browser_extension_folder` 的 browser 参数可省略，默认 Chromium。Firefox 115+ 使用持久模块后台，Chromium 116+ 使用 MV3 worker，共用绑定页、自动续播和请求隔离。DTO 保留旧 `extensionPath`，新增 `extensionPaths` 和经过认证的 `connectedBrowser`；UI 的浏览器选择只决定准备目录与教程，不能误将另一浏览器的连接显示为已连接。Firefox Origin 是严格 UUID，仍需私有 token；Logo 图像与主程序字节相同，旧 Chromium 根目录保持兼容。

Spicetify 私有配置与状态必须分开：移除子进程 SPICETIFY_STATE，使用应用私有 APPDATA 生成 Backup / Extracted，并明确真实 Spotify / prefs 路径；不能改变全局环境或盲目使用真实 APPDATA 的全局配置。既有全局安装保留原状态。旧混放和会触发上游迁移的目录阻止配置，保留原文件；没有干净备份时不得备份已修改的客户端。工具退出成功后还需核对扩展注册和注入字节，再报告 ready。

`LibraryJobs::begin_with_snapshot` 先登记取消标识，再可取消地等待操作锁读取来源快照；释放锁后联网。删除可使尚在等待的请求失效，同一来源被重新导入后旧请求也不能提交。桌面保存选歌按键时，用原生 `Shortcut.id()` 排除与全局唤出键的冲突；核心仅验证本地按键格式及互不重复。

发现设置在操作锁内校验并规范化一次，再注册快捷键和保存；注册失败或写入失败沿用原有恢复流程。修改近期排除上限只使预加载失效，历史的 10,000 条保留规则在记录写入时处理。桌面设置读取失败时，主窗口显示默认选项并通过 `desktopSettingsError` 提示，保留原文件；浏览器与 Spotify 状态查询错误在对应设置分区显示。

侧栏按歌单、发现记录、设置、关于排列；品牌小字在三种语言中统一为 `Discover A Song!`，底部只显示版本。关于页保持简洁，仅展示版本、手动更新及项目与作者链接，不展示根 README 的介绍或问答；安装教程仍位于对应设置分区。根 README 由用户维护，发现内容或链接问题只报告，不自行修改。页面标题周围不再显示说明小字，歌单当前来源标为“当前卡池”；发现入口继续使用全局快捷键和托盘。发现记录使用主内容的滚动区与固定顶栏，不另建固定高度对话框或内部列表滚动区；窄屏将状态操作移到歌曲信息下方。独立信息条使用日夜语义配色，不影响卡片居中。

展示确认、成功选择、清空和修改记录后，桌面在操作锁内发送 `discovery-history-changed`；一级记录页先订阅再读取初始快照，随后自动刷新，不必切换页面。修改期间收到的事件延后合并，保留搜索、筛选及仍有效的勾选；快照版本同时使迟到记录、修复和封面结果失效，同身份刷新也会重取被作废的在途封面。

发现状态事件比较 DTO 字段并复用已准备的未变卡片，不反复序列化大段封面字符串或解码同一封面；播放、替换和取消使在途状态交付失效，迟到解码不能覆盖新卡片。记录页封面缓存保留最近访问的项目，目标预算为 100 项 / 16 MiB 编码字符，当前可见页受保护；该预算不包含浏览器已解码图像，当前页本身超过预算时也不会被逐出。

加权设置以“歌曲基数”和“计算”为主要入口，手动参数收在可展开区域，不增加说明小字。基数默认读取完整启用来源的缓存曲数，仅作为计算草稿；切换来源或缓存曲数更新时重新取默认值，近期排除和预加载占用不改变基数。参数只有点击计算后写入设置草稿，仍需保存。以基数 N、每批普通和神秘歌曲合计 K，计算 T = min(10000, ceil(N / min(N, K)))；原始权重 100，入选减权 25，选择额外减权 50，恢复轮次 ceil(T / 4)，开始加权轮次 T，每轮加权 100 / T（保留四位小数），最高权重 300。T 是名义调整节奏，不保证随机过程覆盖所有歌曲。

权重连续参数与客户端最小化等待时间使用 f64，旧整数 JSON 仍兼容；抽样和减权恢复全过程保留小数，权重最低 1。歌曲数、批数、发现轮次仍是整数。`components/numberInput.ts` 区分两类数值，`NumberField` 的小数模式使用文本输入、范围验证和自定义步进；字号、等待时间和连续权重支持小数，方向键 / 步进按钮按 ±1 调整并保留小数部分。非有限值和越界值不能进入后端设置。

手动更新通过 `desktop/updates.rs` → `services/updates.rs` 请求本项目公开 GitHub Releases；只响应用户点击，不在启动或后台轮询。`release/` 只输出普通和完整两种 Windows x64 安装包，NSIS 提供简体、繁体、英语。安装器先检查 Windows 10 1809 及以上的 AMD64 客户端系统，再处理 WebView2；通过 NSIS `VersionCompare` 选择系统级或当前用户较新的运行时，最低要求为 `109.0.0.0`。普通包按需下载，完整包在生成的 NSIS 脚本中嵌入微软 WebView2 离线安装器。

必要的设置说明通过可悬停、聚焦或点击的圆形问号展开，空说明不产生按钮；键盘选歌、启动与后台运行、本地数据三个分区不显示说明或问号。`HelpHint` 使用同一个 `0 0 24 24` viewBox 中的圆圈、路径和圆点，不使用文字拼接，外尺寸为 `14px × --font-scale`，随界面 zoom 整体缩放；`currentColor` 延续日夜和悬停颜色。帮助按钮保留可访问名称与浮层关联，SVG 隐藏于辅助技术；触发器的 10px 基准字体只用于维持帮助浮层的原文字比例。设置 `fieldset` 内文字 / 数字输入的额外 `:focus-visible` outline 局部取消，按钮、滑块、下拉触发器及挂到 body 的搜索输入仍保留键盘反馈。外观和按键恢复按钮共用 `appearance-actions` 与 `button secondary`。浏览器和 Spicetify 安装教程使用 `features/settings/InstallationGuide.tsx`，准备后自动展开；复制操作通过 `services/desktop.ts`，失败时不能显示已复制。

共用下拉框采用应用样式，浮层挂到 `body`，根据触发控件的累计 CSS `zoom` 与屏幕边界定位，避免被滚动容器裁切。应保留方向键、确认、Esc 与 Tab 的焦点行为；Esc 关闭控件浮层时不能同时关闭外层导入对话框。禁用父 `fieldset` 后已打开的控件也不得继续写入。

Windows 字体使用 DirectWrite 只读枚举系统与当前用户安装的字体族，保留全部本地化别名用于搜索；显示用户系统语言名称，CSS 优先使用稳定的英语族名，没有英语名的字体仍可选择。此原生调用只在 `desktop/fonts.rs`，不能进入独立核心。旧设置中的本地化名称或当前未安装的自定义名称仍原样保留。

批量删除先校验整组选中的平台 / 类型 / ID，再原子替换一次来源设置；验证或写入失败不得部分删除。成功后在同一操作锁内使对应抓取任务及发现缓存失效，并发送一次变更。保留已有缓存文件与歌曲历史，不替用户清理磁盘数据。

本地选歌按键保存在核心 `MusicSetting.discovery_keybindings`，Rust 与 TypeScript 同步规范化和重复校验。默认 W / A / S / D 移动，Enter 确认、R 替换；六个按键均可录制，旧数据缺少 `replace` 时采用 R，Esc 保留退出。首次方向输入选择第一张卡片，之后按照当前实际网格移动，换批、加载和关闭时清空旧选择。只有当前可见批次接受输入；输入框、对话框及输入法组合过程不触发选歌。选中卡片的抬起、描边及替换动画保留减弱动画偏好，封面右下替换按钮与整卡播放按钮为独立命中目标。

发现卡片用 `data-input-mode` 区分鼠标与键盘，仅方向输入或 Tab 开启键盘反馈；真实指针移动或按下切回鼠标。程序聚焦和静止指针不切换模式，鼠标模式的确认键不播放残留隐藏选择。稳定的 `.song-card-slot` 负责鼠标悬停，内部按钮继续抬起；网格列数必须读取 slot 的行位置。键盘使用单条外环，不能与原生 `:focus-visible` outline 叠加。

## 开发环境与运行

桌面开发使用 Windows 10 1809 或更新版本／Windows 11 x64、Node.js/npm、Rust MSVC 工具链、Windows C++ 构建工具和 WebView2。首次准备可参考 [Tauri Windows 开发环境](https://v2.tauri.app/start/prerequisites/#windows)。

在仓库根执行：

```powershell
cd app
npm ci
npm run tauri dev
```

`npm run dev` 只启动浏览器预览。核心库不需要桌面运行环境，独立用法见 [核心说明与示例](../crates/discoas-core/README.md)。

## 验证和打包

先列本次对应检查，再按改动文件或功能选择执行。以下是入口，不是每次必须全跑的清单；外观和动画修改先展示预览、完成样式确认，再执行相关检查。修改共享逻辑时覆盖受影响的调用方。

| 修改范围 | 对应检查（在 `app/` 执行） |
| --- | --- |
| 前端类型或界面 | `npx tsc --noEmit`；相关页面、浅深色及窗口尺寸 |
| 手牌布局 | `npm run test:hand:layout` |
| 手牌动画 | `npm run test:hand:motion`；实际入手、展开收纳、打出和弃牌画面 |
| 启动动画 | `npm run test:startup`；Windows 下显示主界面与隐藏启动、居中和完整淡入淡出 |
| 手牌测量与区域提交 | `node --test tests/handRegions.test.mjs`；静止时停止测量、拖动期间合并提交 |
| 发现与本地选歌 | 按修改选择 `test:discovery`、`test:keyboard`、`test:motion` |
| 封面、来源、歌单、历史 | 按修改选择 `test:covers`、`test:source`、`test:library`、`test:history` |
| 设置、控件、数值、主题 | 按修改选择 `test:preferences`、`test:controls`、`test:numeric`、`test:theme` |
| 托盘、更新、安装器 | 按修改选择 `test:tray`、`test:updates`、`test:installer` |
| 浏览器或 Spotify 扩展 | 对应 `test:browser` 或 `test:spotify` |
| Rust 核心或桌面层 | 对应 crate 的 `cargo check --offline`，以及 `cargo test --offline <模块或用例名>` |
| 自动打出手牌 | 核心 `cargo test --offline auto_hand::`；桌面 `auto_hand_native` 与 `services::browser_bridge`；`test:browser`、`test:preferences`；真实客户端曲终前切歌、首／末张、窗口静音恢复、失败回牌与自身切歌屏蔽 |

`test:hand` 组合手牌布局、动画与区域提交测试。`test:preferences` 包含分组保存、部分失败与迟到快照合并。主窗口自绘控制单独验证最小化、最大化／还原、拖动、边缘缩放和关闭收至托盘；原生窗口改动核验任务栏、显示缩放、焦点与透明区域穿透。性能对比只覆盖被改动的交互和静止状态。完整交付前编译 Windows NSIS 安装包，复用已经完成的相关检查。

`--offline` 要求 Rust 依赖已缓存在本机；首次下载依赖时可省略。公网探针只在核验平台变化时单独运行。独立核心的离线示例不读写已有用户数据。

在 `app/` 执行 `npm run release:windows -- --normal-only` 可生成普通安装包。准备微软 WebView2 x64 离线安装器后，通过环境变量 `DISCOAS_WEBVIEW2_INSTALLER` 指定文件路径，再执行 `npm run release:windows` 生成普通和完整两种安装包，输出位于仓库根 `release/`；该目录不进入源码仓库。

WebView2 准备脚本核对本地安装器的微软签名。普通包完成后，构建脚本复用当次生成的 NSIS 脚本和同一应用文件，将完整包的运行时安装方式改为内置本地离线文件；生成完整包无需访问微软下载节点。两种包保留相同图标、语言和安装行为，安装阶段仍检查最低运行时版本。

原生 QA 可在调试构建设置 `DISCOAS_TEST_ROOT` 为一个已存在的绝对目录，`paths.rs` 将应用根及其 `user_data/` 指向该隔离目录；无效路径直接报错。此分支仅在 `debug_assertions` 编译，正式 Release / NSIS 不读取该变量，也不构成便携数据模式。测试应预置专用来源、偏好与封面，不借用仓库根或系统应用数据；结束时保留或清理的对象只限明确创建的测试文件。

浏览器预览可验证布局、设置草稿与外观；托盘、透明浮窗、快捷键注册、自启、播放器唤起和最小化仍需 Windows 实机验收。
