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
| 酷我 / 汽水公开音乐快照与客户端播放链接 | `crates/discoas-core/src/platforms/kuwo/`、同级 `qishui/` 与 `public_music.rs`；桌面二次校验在 `app/src-tauri/src/commands.rs` |
| 调整随机抽歌和神秘歌曲规则 | `crates/discoas-core/src/core/discover.rs` |
| 调整预加载、取消、当前批次或来源校验 | `crates/discoas-core/src/discovery_service.rs`、`core/cache.rs` |
| 调整歌曲封面准备和缓存 | `crates/discoas-core/src/image_cache.rs` |
| 历史记录、近期排除、固定记录保留与加权 | `crates/discoas-core/src/history.rs`、`weighting.rs`、`discovery_service.rs`；前端 `app/src/features/history/`、`app/src/features/settings/WeightingSettings.tsx`、`weightingCalculator.ts` |
| 单卡替换、每批预算、状态 DTO 与信息条 | `crates/discoas-core/src/model.rs`、`discovery_service.rs`、`core/cache.rs`；前端 `app/src/features/discovery/` |
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
| 应用风格的托盘右键菜单 | `app/src-tauri/src/desktop/tray_menu.rs`、`app/src/features/tray/`、`app/src-tauri/capabilities/tray-menu.json` |
| Windows 安装兼容检查与 WebView2 准备 | `app/src-tauri/installer/installer.nsi`、`hooks.nsh`、`app/src-tauri/tauri.conf.json`、`app/scripts/` |
| 关于页的项目、作者链接与更新入口 | `app/src/features/about/`、`app/src/About.tsx`、`app/src/features/updates/` |
| 歌单、发现、记录或设置的页面与交互 | `app/src/features/library/`、`discovery/`、`history/`、`settings/` |
| 多页面共用的界面组件 | `app/src/components/` |
| 日夜主题、共用颜色和旧默认浮窗配色兼容 | `app/src/styles/tokens.css`、`app/src/App.css`、`app/src/features/discovery/themePalette.ts` |
| 应用快照同步、草稿保留和通知 | `app/src/hooks/` |
| 前端桌面命令、事件或窗口调用 | `app/src/services/desktop.ts` |
| 调试版独立数据目录与原生 QA | `app/src-tauri/src/paths.rs` 的 `DISCOAS_TEST_ROOT` 分支 |

`App.tsx` 只组合页面和控制导航。`app/src-tauri/src/commands.rs`、`library.rs`、`desktop_preferences.rs` 保留现有命令名，负责接入；旧 `core/`、`platforms/` 路径中的少量文件是转发入口，业务实现以独立库为准。

## 依赖规则

- `discoas-core` 不依赖 Tauri、WebView 或 Windows 原生 API。调用者传入数据目录和当前来源快照；核心返回元数据、批次或播放 URI，不打开窗口和播放器。
- `app/src-tauri/src/services` 接收明确的文件路径和普通数据，不接收 `AppHandle`，不发送窗口事件。`desktop` 负责解析应用路径、原生操作和调用顺序。
- 前端只有 `app/src/services/desktop.ts` 导入 Tauri。视图通过语义回调执行操作，Hook 负责请求、状态和取消流程；共用组件不承担歌单和发现业务。
- 共用歌曲与播放参数定义在核心的 `model.rs`；前端对应类型在 `app/src/types.ts`。JSON 字段、命令名或事件载荷变更时，必须同时更新两端及兼容检查。
- 不为新功能复制一份平台读取、抽歌或缓存算法。非桌面程序也应复用核心入口，见 [核心说明与示例](../crates/discoas-core/README.md)。

## 修改时必须保持的行为

普通与神秘歌曲合计最多 15 首，预加载最多 5 批。歌曲批次返回前准备封面字节，前端整批解码后显示；来源或设置变化后旧预加载不能重新入队。歌单/专辑封面只在导入和更新时读取网络，快照读取只用本地缓存。

详情缓存只保存成功结果，临时 HTTP 错误最多重试一次，间隔 250 毫秒；访问限制、来源不存在、限流与无效平台数据不自动重试。网易云通过可选的 `SongDetailLoader` 批量入口合并同批缺失详情，按 ID 映射响应，保留单曲兼容入口。批量请求按缓存标识排序取得单曲锁，等待后重查成功缓存，再占用全局详情额度；不得反向取得锁。失败详情不进入预加载队列，当前失败卡片在再次打开时按原身份修复，不重抽或重复推进历史轮次。预加载任务通过 generation watch 取消过期准备；`PreloadGuard` 在取消或退出时释放运行标记，不能仅在正常返回路径清理。

历史固定保留最多 10,000 个平台 / 歌曲身份，兼容字段 `history_limit` 仅表示近期排除上限，范围 0–10,000；调整排除上限不裁掉历史。选择 / 入选模式按同平台最近符合状态的歌曲排除，排除后为零按无歌曲处理，不自动放宽。历史与加权的轮次、歌曲事件一同存入 `history/discovery_history.json`，由 `HistoryStore` 原子写入一次；`WeightStore` 只读取同一快照。删除、清空或修改状态必须同时更新对应权重事件，验证或写入失败不得出现部分更新。固定保留上限同时约束加权身份，避免另一份状态无限增长。

新批次实际展示后才写入历史并推进所属平台的加权轮次。同一批次重新打开或替换单张卡片不推进轮次；替补实际展示后仍记录入选。普通候选保持同批去重，权重最低为 1；入选与选择减权分别随发现次数衰减，达到恢复次数后恢复原始权重，之后按配置的等待次数开始加权并受最高权重约束。默认不开启加权。可选 `preload_deduplication` 同时排除当前批次与已入队预加载中的歌曲，候选不足时停止补充队列，未展示批次不提前写入记录。

发现、取消、来源/设置修改与播放共享 `Cache::operation`。桌面命令用 `discover_in_operation` / `cancel_in_operation`，在同一个操作锁内修改状态并发送事件；不能在锁外发送迟到的发现结果。普通调用者可直接用自持锁的 `discover` / `cancel`。播放前验证当前批次的完整来源身份，酷狗文件名取可信缓存；唤起失败不能提前释放批次。

网络获取完整成功后才能替换缓存。旧 JSON、来源互斥、未保存草稿和迁移预检必须保留。数据目录与仓库根 `assets/` 的原始与彩蛋资源不能因重构移动或清空。

`MusicSetting.has_same_discovery_configuration` 统一界定发现相关变更：启用来源的完整身份及发现参数参与比较，备注、未启用来源、全局快捷键和本地选歌按键不使批次失效。桌面 `library-changed` 载荷为 `{ discoveryInvalidated: boolean }`，前端对 `false` 只刷新来源快照，旧无载荷事件仍按失效处理；替换提交使用同一比较规则，继续检查 epoch / generation。`get_app_state` 在后台线程读取磁盘，列表摘要只解析歌曲 ID 和来源封面，跳过完整 `tracks_info`。前端快照版本防止迟到读取覆盖后续保存，未保存草稿仍保留。

秘密歌曲的真实元数据保留在内部 `real_metadata`，Serde 双向跳过该字段；发现卡片保持隐藏，历史使用真实信息。旧记录补全最多 100 项、4 并发、单项 8 秒、总计 20 秒；网络不持有操作锁，短暂合并只修改仍与原快照一致的行，不能恢复已经清空的记录。

八平台能力由核心 `supported_kinds` / `validate_kind` 约束。酷我、汽水及视频来源快照含完整 `tracks_info`，发现详情从快照读取；Bilibili 分 P 用 `BV号_pN` 标识。酷我校验官方 UTF-8 分页的来源 ID、页码、总数和进度；汽水读取官方分享页 SSR，实际歌曲数量不符时拒绝提交。两者复用 `public_music.rs` 的快照与详情逻辑，不另建抽歌或封面缓存机制，核心继续不依赖 Tauri。

酷我原生播放 URI 在官网歌曲页取得，并核对固定协议、字段和目标 `MUSIC_<id>`，保留官方 Base64 的字面 `+`。桌面先在操作锁内验证来源并记录 `current_batch_epoch`，释放锁后联网，重新获得锁后二次核对完整来源与批次标记，再读取最新播放设置并唤起客户端。只影响预加载的 `generation` 变化不使当前可见批次失效；取消、换批、选择和来源变化仍不能让旧请求播放。汽水使用官方 Windows 客户端 `luna://luna.com/playing?track_id=...` 协议；客户端识别仅匹配 `SodaMusic.exe`，酷我匹配 `KwMusic.exe`，不把同名辅助进程作为播放器。URI 构造或元数据读取通过不能代替真实客户端切歌验收。

浏览器扩展只控制绑定页面，确认首次播放后结束比较；自动续播不写入历史或触发发现事件。Spicetify 安装 / 配置在操作锁外运行，校验固定资源、保留已有配置和备份。首次安装仅由设置中的明确操作发起；启动、保存模式和选歌的自动入口必须先检查有效且未移除的 DiscoAS 安装记录，服务锁内再检查一次。NSIS 没有安装时自动部署钩子，升级保留配置，卸载仅撤销本应用内容。

## 发现状态、替换与预览

新入口 `discover_batch` / `get_discovery_state` 返回 `DiscoveryStateDto`，前端对应 `DiscoveryState`：`songs`、`batchEpoch`、`remainingSongs`、`replacementsRemaining`、`exclusionEnabled`、`replacementEnabled`、`preview`。状态通过 `discovery-state-changed` 在操作锁内同步；旧 `discover_songs` 和 `discovery-changed` 继续兼容。歌曲设置保留 snake_case，跨桌面的 DTO 使用 camelCase，不能混用字段形式。

`replace_discovery_song` 接收完整来源身份与 `batchEpoch`。锁内获取替换票据，锁外准备元数据和封面，再持锁复核来源、当前批次、卡片位置、已移出身份与剩余预算并提交。失败或过时的替换不消耗次数，成功只替换指定位置并更新状态；神秘位置仍保持隐藏，替补也可再替换。预算默认 1，0 关闭，每次发现最多 100 次。启用预加载去重时，替补不能与后续预加载同时占用同一身份。

`mutate_discovery_history` 按 `HistoryIdentity` 执行删除或入选 / 选择布尔状态修改；整个请求先校验后一次保存。历史前端搜索、状态筛选和跨页多选使用平台 / 歌曲组合身份，每页 25 条；`get_history_covers` 单次最多 64 项，按本页懒取，封面 data URI 不写回历史快照。旧元数据修复、封面请求与删除 / 清空共用版本检查，迟到结果不能恢复已删记录。历史界面和新增加权、去重、替换选项不加说明小字或问号。

发现卡片网格每行最多 5 张，列数随当前可用宽度与卡片比例减少；键盘移动仍以实际 slot 行位置为准。独立 `.discovery-status-bar` 定位在卡片组下方，不参与全屏居中计算。开启排除时第一行显示剩余卡池，开启替换时第二行显示次数；功能关闭跳过对应行，两项关闭则不产生信息条。`GuiSetting` 的 `replacement_button_size`、`discovery_bar_size` 分别独立缩放，范围 0.50–3.00。

`start_discovery_preview` 使用设置 / 外观草稿调用 `prepare_preview`，保存独立 `PreviewState`，不修改真实当前批次、预加载队列、历史、权重轮次或替换预算。`update_discovery_preview` 更新同一会话的外观及按键，不重新抽歌、不保存草稿；`end_discovery_preview` 或 `preview-closed` 结束预览并同步设置开关。预览卡片默认 25% 不透明度，指针悬停或键盘选中变为 100%，前后端均阻止播放和替换。会话 generation 与生命周期锁同时约束准备、窗口效果、退出动画及观察任务，关闭后的旧准备结果不能重新打开浮窗。

Windows 预览让卡片和背景穿透；右上退出按钮是唯一临时接受鼠标点击的命中区域。`desktop/discovery_preview.rs` 使用 `GetPhysicalCursorPos`、窗口屏幕原点和当前 DPI 比例转换指针，避免后台线程的 DPI 虚拟化，发送 `preview-pointer`。`preview_keyboard.rs` 在 Tauri UI 消息循环中安装临时 `WH_KEYBOARD_LL`，只保留配置键、修饰键和 Esc 的瞬时状态，始终转发输入；回调只发送有界事件，不等待生命周期锁、不执行窗口或文件操作。退出时先原子停用，再按会话卸载，旧卸载不能移除新会话。Esc 有独立优先通道，打开时按住的键不会触发，修改按键后缓冲的旧事件被拒绝。前端区分 native / DOM 来源防止重复处理。`preview-appearance` 只作用于预览会话，退出后恢复正式外观。原生调用不能下移至核心库。

原生窗口显示及关闭回调先转入后台执行，统一按 `operation → lifecycle` 的顺序提交；UI 线程不能反向等待生命周期锁。安装按键观察前释放这两把锁，再等待 UI 确认。预览关闭期间的查询和播放继续隔离，正常浮窗打开时才清除关闭标记；重复退出为最新 generation 保留隐藏兜底。

## 界面与交互约定

仓库目录按 `app/`、`crates/`、`extensions/`、`assets/`、`docs/` 划分。源码目录搬迁不改变系统用户数据位置，浏览器与 Spotify 的已配对导出目录也保持原格式。

桌面 setup 首先调用 `seed_new_user`。只有不存在持久文件的用户数据目录才预设并启用作者网易云歌单 `8285082830`；随包快照仅包含来源字段和完整歌曲 ID，不包含账号、Cookie 或配对数据。已有设置（包括显式空来源）、缓存、历史、未知文件及符号链接均优先保留。初始化无需联网，之后沿用普通刷新与预加载。首次两文件提交通过明确的本应用事务标记处理进程中断；只在标记有效、没有用户设置、目录仅含未修改种子文件时恢复，不能用同名旧缓存猜测用户意图。

托盘右键菜单不绑定 Windows 原生 Menu，由单例 `tray-menu` 无边框 WebView 展示。前端先订阅事件再报告 ready，每次右键接收保存后的语言、日夜、字体和暂停状态，测量后再显示；过期 generation、失焦与 Esc 都不能重新打开旧菜单。五项动作只接受菜单本窗口请求，执行后隐藏。定位使用目标显示器的物理工作区域，处理负坐标、任务栏与 DPI；菜单前台期间低级预览按键观察继续转发系统输入，但不同时操作发现预览。浏览器预览只验证外观，实际托盘点击、失焦和多屏位置仍需 Windows 原生验收。

Logo 的矢量源为根 `assets/DiscoAS.svg`：黄、蓝、红从左至右叠放，三张同高卡片使用平滑圆角及实体配色。修改后在 `app/` 运行 `npm run icons`，通过 [Tauri 图标工具](https://v2.tauri.app/develop/icons/) 同步 `assets/` 中的 PNG / ICO、桌面图标及浏览器扩展图标；侧栏与页面图标直接读取 SVG，启动动画读取同源 PNG。侧栏图标为 40px，窄窗口缩小品牌区间距以容纳文字；启动图标为 `min(42vw, 42vh, 400px)`，沿用原淡入淡出时序。不要分别修改导出的位图，原始问号封面和彩蛋资源不受此流程影响。

当前界面采用紧凑、灰白层级的桌面主题：侧栏和列表以中性背景、细分隔线区分层级，青色用于主要操作、状态及键盘焦点。所有页面与挂到 body 的菜单共用 `styles/tokens.css` 的日夜语义颜色；面板和菜单不使用玻璃或背景模糊。`discovery/themePalette.ts` 只将完整匹配旧默认值的卡片 / 退出配色映射到新主题，自定义组整组保留，不能借主题更新改写已保存的数据。发现反馈放在 `features/discovery/discovery.css`：鼠标轻微抬起，键盘单一 2px 焦点框，卡片边框透明以免形成双框；替换按钮同步移动。卡片不允许选中文字或拖动封面。几何、五列上限、信息条独立定位和预览行为保持原约定。

设置标题和保存按钮使用 `.settings-topbar` 固定在主内容滚动区域顶部，sticky inset 抵消主内容响应式顶部内距；日夜、字号和界面缩放共用原外观状态。顶栏层级低于 body 帮助浮层，不应用页面进入的位移动画。更改顶栏时应核验滚到本地数据仍可见且可保存，不能在顶栏上方露出正文。

浏览器扩展按 `ExtensionBrowser` 导出 Chromium / Firefox 清单，`export_browser_extension` 和 `open_browser_extension_folder` 的 browser 参数可省略，默认 Chromium。Firefox 115+ 使用持久模块后台，Chromium 116+ 使用 MV3 worker，共用绑定页、自动续播和请求隔离。DTO 保留旧 `extensionPath`，新增 `extensionPaths` 和经过认证的 `connectedBrowser`；UI 的浏览器选择只决定准备目录与教程，不能误将另一浏览器的连接显示为已连接。Firefox Origin 是严格 UUID，仍需私有 token；Logo 图像与主程序字节相同，旧 Chromium 根目录保持兼容。

Spicetify 私有配置与状态必须分开：移除子进程 SPICETIFY_STATE，使用应用私有 APPDATA 生成 Backup / Extracted，并明确真实 Spotify / prefs 路径；不能改变全局环境或盲目使用真实 APPDATA 的全局配置。既有全局安装保留原状态。旧混放和会触发上游迁移的目录阻止配置，保留原文件；没有干净备份时不得备份已修改的客户端。工具退出成功后还需核对扩展注册和注入字节，再报告 ready。

`LibraryJobs::begin_with_snapshot` 先登记取消标识，再可取消地等待操作锁读取来源快照；释放锁后联网。删除可使尚在等待的请求失效，同一来源被重新导入后旧请求也不能提交。桌面保存选歌按键时，用原生 `Shortcut.id()` 排除与全局唤出键的冲突；核心仅验证本地按键格式及互不重复。

侧栏按歌单、发现记录、设置、关于排列；品牌小字在三种语言中统一为 `Discover A Song!`，底部只显示版本。关于页保持简洁，仅展示版本、手动更新及项目与作者链接，不展示根 README 的介绍或问答；安装教程仍位于对应设置分区。根 README 由用户维护，发现内容或链接问题只报告，不自行修改。页面标题周围不再显示说明小字，歌单当前来源标为“当前卡池”；发现入口继续使用全局快捷键和托盘。发现记录使用主内容的滚动区与固定顶栏，不另建固定高度对话框或内部列表滚动区；窄屏将状态操作移到歌曲信息下方。独立信息条使用日夜语义配色，不影响卡片居中。

展示确认、成功选择、清空和修改记录后，桌面在操作锁内发送 `discovery-history-changed`；一级记录页先订阅再读取初始快照，随后自动刷新，不必切换页面。修改期间收到的事件延后合并，保留搜索、筛选及仍有效的勾选；快照版本同时使迟到记录、修复和封面结果失效，同身份刷新也会重取被作废的在途封面。

发现状态事件比较 DTO 字段并复用已准备的未变卡片，不反复序列化大段封面字符串或解码同一封面；播放、替换和取消使在途状态交付失效，迟到解码不能覆盖新卡片。记录页封面缓存保留最近访问的项目，目标预算为 100 项 / 16 MiB 编码字符，当前可见页受保护；该预算不包含浏览器已解码图像，当前页本身超过预算时也不会被逐出。

加权设置以“歌曲基数”和“计算”为主要入口，手动参数收在可展开区域，不增加说明小字。基数默认读取完整启用来源的缓存曲数，仅作为计算草稿；切换来源或缓存曲数更新时重新取默认值，近期排除和预加载占用不改变基数。参数只有点击计算后写入设置草稿，仍需保存。以基数 N、每批普通和神秘歌曲合计 K，计算 T = min(10000, ceil(N / min(N, K)))；原始权重 100，入选减权 25，选择额外减权 50，恢复轮次 ceil(T / 4)，开始加权轮次 T，每轮加权 100 / T（保留四位小数），最高权重 300。T 是名义调整节奏，不保证随机过程覆盖所有歌曲。

权重连续参数与客户端最小化等待时间使用 f64，旧整数 JSON 仍兼容；抽样和减权恢复全过程保留小数，权重最低 1。歌曲数、批数、发现轮次仍是整数。`components/numberInput.ts` 区分两类数值，`NumberField` 的小数模式使用文本输入、范围验证和自定义步进；字号、等待时间和连续权重支持小数，方向键 / 步进按钮按 ±1 调整并保留小数部分。非有限值和越界值不能进入后端设置。

手动更新通过 `desktop/updates.rs` → `services/updates.rs` 请求本项目公开 GitHub Releases；只响应用户点击，不在启动或后台轮询。`release/` 只输出普通和完整两种 Windows x64 安装包，NSIS 提供简体、繁体、英语。安装器先检查 Windows 10 1809 及以上的 AMD64 客户端系统，再处理 WebView2；通过 NSIS `VersionCompare` 选择系统级或当前用户较新的运行时，最低要求为 `109.0.0.0`。普通包按需下载，完整包由 `tauri.full.conf.json` 嵌入微软 WebView2 离线安装器。

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

在 `app/` 中执行：

```powershell
cargo test --manifest-path ../crates/discoas-core/Cargo.toml --offline
cargo run --manifest-path ../crates/discoas-core/Cargo.toml --offline --example local_discovery
cargo test --manifest-path src-tauri/Cargo.toml --lib --offline
npm run build
npm run test:motion
npm run test:preferences
npm run test:covers
npm run test:discovery
npm run test:spotify
npm run test:browser
npm run test:source
npm run test:controls
npm run test:library
npm run test:keyboard
npm run test:history
npm run test:theme
npm run test:numeric
npm run test:updates
npm run test:tray
npm run test:installer
```

`--offline` 要求 Rust 依赖已缓存在本机；首次下载依赖时可省略。按改动范围选择检查，公网探针默认忽略，只在需要核验平台变化时单独运行。独立核心的离线示例不读写已有用户数据。

在 `app/` 执行 `npm run release:windows -- --normal-only` 可生成普通安装包。准备微软 WebView2 x64 离线安装器后，通过环境变量 `DISCOAS_WEBVIEW2_INSTALLER` 指定文件路径，再执行 `npm run release:windows` 生成普通和完整两种安装包，输出位于仓库根 `release/`；该目录不进入源码仓库。

原生 QA 可在调试构建设置 `DISCOAS_TEST_ROOT` 为一个已存在的绝对目录，`paths.rs` 将应用根及其 `user_data/` 指向该隔离目录；无效路径直接报错。此分支仅在 `debug_assertions` 编译，正式 Release / NSIS 不读取该变量，也不构成便携数据模式。测试应预置专用来源、偏好与封面，不借用仓库根或系统应用数据；结束时保留或清理的对象只限明确创建的测试文件。

浏览器预览可验证布局、设置草稿与外观；托盘、透明浮窗、快捷键注册、自启、播放器唤起和最小化仍需 Windows 实机验收。
