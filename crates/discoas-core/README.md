本文档由ChatGPT-6.1生成

# discoas-core

桌面版实际使用的 Rust 库。无 Tauri、WebView、托盘或 Windows API 依赖，可用于命令行和其他程序。沿用项目现有许可证。

## 模块职责

| 模块 | 职责 |
|---|---|
| `model` | 共用歌曲卡片和播放参数；保持桌面端 JSON 字段不变 |
| `platforms` | 八平台元数据抓取、歌曲详情解析与播放 URL 生成；返回数据，不打开播放器 |
| `platforms::source` | 分享文本、链接和 ID 规范化；校验平台与歌单/专辑/视频来源类型 |
| `storage::LibraryStore` | 用调用者传入的 `user_data` 目录读取、原子保存平台缓存 |
| `settings::music_setting` | 发现设置与歌单配置的兼容读写 |
| `core::discover` | 最多 15 首的均匀或加权抽取、神秘模式和卡片组装 |
| `core::cache` | 最多 5 批的缓存、取消重入、过期预加载隔离 |
| `image_cache` | 独立图像缓存及整批封面准备 |
| `history` | 固定 10,000 首真实入选/选择记录、原子状态编辑与按平台/歌曲排除 |
| `weighting` | 发现轮次、入选/选择扣权、线性恢复和久未出现加权；与历史原子保存 |
| `discovery_service` | 发现、取消、预加载和播放来源校验；调用者负责展示和打开播放目标 |

歌曲详情只读取调用者传入的当前歌单/专辑快照，不扫描其他歌单。网易云和 QQ 的详情继续请求平台接口；酷狗、酷我、汽水、Spotify 与视频平台使用当前快照，酷狗缺封面时仍有原来的专辑元数据回退。

详情请求并发最多 4 个，共享缓存会合并相同请求，并保留最近 256 项成功详情（5 分钟）；来源快照参与缓存标识，缓存真实详情后在卡片投影时隐藏神秘信息。完整批次准备的 45 秒期限涵盖详情与封面，超时不发布半批卡片。图片字节缓存按最近使用顺序逐项淘汰，预算为 32 MiB、单项最多 5 MiB；此预算不包含批次中的 Base64 字符串、前端解码图片或整个应用的内存。

`SongDetailLoader` 可通过 `supports_batch_details` / `load_batch_details` 提供批量详情；默认仍走单曲入口，已有实现不必改动。网易云合并同批未缓存的歌曲为一次请求，按歌曲 ID 对应响应，不能依赖返回顺序；批量缺失一首不会污染其他成功结果。共享请求锁按完整缓存标识排序后取得，重查成功缓存再占用并发额度，重叠批次不重复请求。连接、超时及临时服务错误最多重试一次，间隔 250 毫秒；明确的访问限制、来源不存在、限流或无效平台数据不自动重试，失败不存入详情缓存。

失败详情不能进入预加载队列；前台仍可提供含错误标志的可播放占位卡片，但再次打开同一批次会按原身份补取失败详情，保留神秘位置、替换预算和展示轮次，仍失败则返回错误。取消复用批次补取失败时保留其队列位置与会话。设置或来源失效通过 generation 通知中止旧准备任务，释放请求锁并读取最新配置；预加载运行标记由 `PreloadGuard` 在任务退出或取消时释放。封面网络失败仍采用占位，不阻塞整批；批量保存本地历史封面先验证全部字节，逐项原子写入后只执行一次目录裁剪。

`PlaylistFetcher::fetch_with_progress` 接收 `FetchProgressCallback`（线程安全回调），报告 `completed`、可空的 `total` 和 `pages`。Spotify、QQ、酷狗和酷我在每个通过校验的分页后报告进度，单次目录请求报告开始和完成。进度不是保存成功通知；只有完整抓取成功后才能提交快照。

## 本地运行

在此目录执行：

```powershell
cargo test --offline
cargo run --offline --example local_discovery
```

`local_discovery` 创建临时的 Spotify 格式元数据，通过桌面版同一个 `DiscoveryService` 发现一首普通歌曲和一首神秘歌曲，打印 JSON 后清理临时数据。此示例无需联网、音乐客户端或桌面运行时，也不读写已有用户数据。

按需访问平台的示例：

```powershell
cargo run --example fetch_source -- NeteaseCloudMusic album 32311
```

默认只打印接口结果。传入第四个参数 `user_data` 目录后才保存平台缓存，例如：

```powershell
cargo run --example fetch_source -- NeteaseCloudMusic album 32311 ./example-data
```

## 复用入口

```rust,ignore
let store = LibraryStore::new(user_data);
let data = fetcher_for(platform)?.fetch(id, kind).await?;
store.save_playlist_json(platform, id, kind, &data)?;

let service = DiscoveryService::new(store.root(), Cache::new());
let batch = service.discover(false).await?;
// 展示 batch.songs；取消时调用 service.cancel().await。
// 播放 URI 由 platforms::build_scheme_url 生成，播放器唤起由调用者实现。
```

传入的目录是 `user_data` 本身，歌单路径为 `<user_data>/<平台>/<来源类型>/<ID>.json`。使用现有 `settings/music_setting.json` 中启用的来源进行发现。平台请求受网络状态与平台访问规则影响；日常测试不调用公网，原有公网探针仍默认为忽略。

## 本地记录接入

`history_exclusion`（`off` / `selected` / `discovered`，默认 `off`）决定排除的记录状态；兼容字段 `history_limit` 现在只表示当前平台最近排除多少首（默认 200，范围 0–10,000）。保存容量固定为两个状态合并后的 10,000 首唯一歌曲；减少排除数量不会删除记录。排除标识使用平台与歌曲 ID，同一首歌换歌单后仍生效。记录仍在 `history/discovery_history.json`，旧 `discovered` / `selected` 数组保留；兼容新增 `weighting` 字段与实际展示/选择事件一次原子保存，裁剪、修改和清空同时更新。全部歌曲被排除时与空歌单返回相同错误，不自动放宽；取消后保留原批次的偏好仍然有效。

桌面或其他有并发操作的宿主，在 `Cache::operation` 中完成以下调用：

- 卡片封面整批解码且界面显示后，调用 `record_discovery_displayed(&[PlaySongArgs])`；与当前批次不匹配的迟到通知返回 `false`，预加载和取消的未展示批次不计入记录。
- `playback_target` 验证当前歌曲来源、宿主接受选择后，调用 `record_selection(&PlaySongArgs)`，再释放当前批次。选择操作也会补记已展示的整批卡片，防止显示通知与点击的竞态。此记录表示用户选择，不能证明客户端实际切歌。
- `history()` 返回至多 10,000 首合并记录，包含真实封面 URL 和本地缓存身份，不整批读取图片。`history_covers(&[HistoryIdentity])` 按可见页加载最多 64 首，4 并发、单项 8 秒、网络阶段总计 20 秒，准备字节和响应各限 8 MiB。它自行加操作锁合并，调用时不能在外部持锁；清空或编辑后旧请求不能恢复记录。封面存于 `history/covers`，前端不能指定路径或任意图片 URL。
- `mutate_history(&HistoryMutationDto).await` 在宿主操作锁内批量删除，或设置 / 取消入选、选择状态；两种状态独立，两者都取消则删除条目。整组先验证再原子保存。`clear_history().await` 同时清空权重状态并更新预加载版本；旧 `trim_history(limit)` 只规范固定容量，不按排除上限删记录。

也可独立使用 `HistoryStore` 的读写入口。宿主负责序列化写操作；核心库不引入窗口、账户、播放器或系统媒体接口。歌单封面的网络准备可用 `download_library_cover` 在操作锁外进行，核对来源仍有效后，用 `save_library_cover` 提交已准备的字节。

## 视频与秘密信息

YouTube 支持 `playlist` / `video`；Bilibili 支持 `video` / `favorites` / `collection` / `series`。使用 `supported_kinds(platform)` 检查能力，`normalize_source` 规范化分享来源。公开视频来源缓存 `tracks_info`，发现详情不再读取网络；Bilibili 曲目身份是 `BV号_pN`。核心只生成 HTTPS 播放地址，绑定浏览器标签页、续播和播放确认由宿主负责。

`SongCardDto.real_metadata` 保存内部真实名称 / 作者，Serde 序列化与反序列化都跳过；浮窗字段保持隐藏，`record_discovery_displayed` / `record_selection` 写入真实历史。宿主可在读出历史后后台调用 `repair_history_metadata(100).await` 补全旧问号行；该函数自行加锁合并，不能在外部持有操作锁时调用。最多 4 并发、单条 8 秒、总计 20 秒，失败行保留原值，清空或修改过的记录不会被恢复。

## 键盘设置与宿主边界

`MusicSetting.discovery_keybindings` 保存六个本地选歌动作的按键，默认 `up=W`、`left=A`、`down=S`、`right=D`、`select=Enter`、`replace=R`。`DiscoveryKeybindings::normalized()` 统一修饰键顺序与键名，并检查无效 / 重复按键；允许字母、数字、方向键、Enter、Space、F1–F12 与 Ctrl / Alt / Shift 组合，Esc 保留给宿主退出。旧 JSON 缺少整个对象或其中字段时使用对应默认值。

核心只保存与校验按键数据，不读取键盘、不录制、不控制焦点或动画。桌面前端按实际卡片网格导航，首次方向输入选择第一张卡片，换批、加载和关闭清空选择，再通过原播放校验入口提交当前卡片。卡片反馈动画、圆形帮助、系统多语言字体枚举和批量删除来源均由相应桌面 / 前端层处理，不能为这些功能引入 Tauri 或 Windows 依赖到核心。

## 酷我与汽水

`KuwoMusic` 与 `QishuiMusic` 支持 `playlist` / `album`。酷我读取官网分页元数据，汽水读取官方分享页 SSR；导入校验完整列表，`tracks_info` 保存歌名、歌手与封面，后续发现和历史修复只读取当前快照，不获取音频流。

酷我的同步 `build_scheme_url` 返回官网歌曲页；需要桌面入口的宿主可在操作锁外调用 `kuwo::native_playback_url(song_id).await`，获取官网现有的完整 URI。准备前后在 `Cache::operation` 内调用 `playback_target`，并比较 `current_batch_epoch`，避免取消后再次展示相同歌曲仍接受旧请求；预加载的 `generation` 不能代替该版本。汽水直接返回 `luna://luna.com/playing?track_id=...`；核心仅生成目标，系统协议注册与唤起由宿主负责。

常规测试不访问公网，按需实测的探针默认忽略。桌面开发入口及 Windows / 浏览器验证边界见 [架构说明](../../docs/ARCHITECTURE.md)。


## 发现状态、替换与预览

`DiscoveryBatch.state` / `get_state()` 提供 `DiscoveryStateDto`：歌曲、当前批次 epoch、当前来源经历史排除后的唯一歌曲数、剩余替换次数及对应功能开关。卡池数量不减去预加载临时预留。`preload_deduplication` 开关默认关闭；开启后当前和等待批次之间避重，小卡池不足一批时保留已准备队列并停止填充，不循环重抽、不提前记历史。

`discovery_weighting` 默认为关闭的均匀抽样。真实新批次展示推进当前平台轮次；选择与实际显示替补记录事件，替补不推进整轮。预加载、预览和取消后复用不推进；关闭抽样开关时仍记录真实事件，以保证再次开启时已按发现次数恢复。基础权重、入选/选择扣减、恢复发现次数、久未出现加权起点、每次加权和最大权重均可配置。扣减线性恢复，权重始终为正且有上限；数值范围及 `boost_after_batches >= recovery_batches`、`max_weight >= base_weight` 由核心校验。

连续权重字段与 `select_batch_weighted` 的权重映射采用 `f64`，旧 JSON 的整数值仍兼容。线性恢复和抽样不截断小数；发现轮次字段保持整数，非有限参数不能通过校验。桌面按歌曲基数计算推荐参数的公式位于仓库根的 `app/src/features/settings/weightingCalculator.ts`，核心只应用保存后的参数，不依赖桌面界面。

`replacement_limit` 默认 1，范围 0–100，0 禁用。宿主在操作锁内调用 `replacement_ticket_in_operation(args, epoch, guard)` 捕获完整身份和来源快照；释放锁后 `prepare_replacement(ticket)` 准备详情和封面，再重新获得锁调用 `commit_replacement_in_operation(prepared, guard)`。来源、设置、队列 generation 和批次 epoch 二次校验通过后才替换，失败不耗次数。保留卡片位置、神秘模式；避开当前其他卡片及本次已替掉的歌曲，替补可再替换。新发现重置额度，取消复用保持额度。替补实际展示后仍需完整批次 display acknowledgment。

`prepare_preview(&draft_settings)` 只读抽样和准备卡片，不占用当前批次，不写历史/权重，不取用或修改预加载队列，也不消耗替换额度；返回 `preview=true`。宿主负责透明度、键鼠反馈、禁止播放、点击穿透与 Esc 退出。预览调用自身不持操作锁，宿主需隔离预览与真实发现窗口状态。
