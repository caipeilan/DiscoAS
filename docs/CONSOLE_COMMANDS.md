此文档由GPT-6.1sol生成

# DiscoAS 2.1.0 控制台与本机接口

控制台位于主窗口左侧的「控制台」。本版提供 147 个命令、111 个设置键。命令目录、参数校验、补全和本表使用同一份登记数据。

## 开始使用

在 DiscoAS 控制台中输入以下命令：

```text
help
app version
source list
settings get music.normalSongCount
schema source import
```

↑ / ↓ 浏览本窗口的输入历史，Tab 补全命令与设置键，Enter 执行。补全列表打开时，Enter 先接受选项。Ctrl+C 取消当前任务，或停止本控制台的事件订阅；选中文字时保留复制操作。导航离开控制台会停止该页面建立的订阅。

`help` 的别名是 `?`，`clear` 的别名是 `cls`，`console close` 的别名是 `exit`。所有命令均支持 `--help` 与 `--json`；表中标为「支持」的命令可使用 `--async`，立即返回任务记录。

命令使用空格路径，也接受对应的点号方法名，例如 `source list` 与 `source.list`。`<参数>` 必填，`[参数]` 可选；参数也可以写成 `--参数名 值` 或 `--参数名=值`。界面参数使用短横线，如 `--batch-epoch`；JSON 参数使用 camelCase，如 `batchEpoch`。

字符串可使用单引号或双引号保护空格，Windows 路径保留反斜线。JSON 对象与数组保留内部引号，支持空格及嵌套。设置布尔值使用 `true` / `false`；明确声明为布尔的普通参数也接受 `on` / `off`。字符串 `true`、`123` 等若需要按文字保存，可使用外层单引号包裹 JSON 字符串，例如 `'"123"'`。

```text
source import-local "D:\音乐\测试卡池" --remark "我的歌单" --async
settings patch music {"normalSongCount":3,"hand":{"enabled":true}}
settings set gui.fontFamily "Microsoft YaHei UI"
history get NeteaseCloudMusic 00123
```

控制台解释 DiscoAS 命令。播放与窗口操作调用应用现有流程，播放请求仍需等待官方播放器的状态确认。

## 身份、批次与结果

| 字段 | 含义 |
| --- | --- |
| `source` | 完整来源对象 `{platform,sourceKind,sourceId}`，或 `platform:sourceKind:sourceId` 字符串。 |
| `song` / `card` | 当前批次中的 songId 或完整歌曲身份对象。mini 命令的 card 也接受迷你卡片自己的 id。 |
| `batchEpoch` | 从 `discovery state` 或 `mini state` 取得。发现批次的播放、替换、收牌、展示确认，以及迷你发现的重排均要求此值；批次已改变时返回 `STALE_STATE`。 |
| `cardId` | 普通手牌自己的 id；与歌曲的 songId 不同。`hand play-position` 按执行时的当前牌序读取第 1–9 张。需要操作固定手牌时使用 cardId。 |
| `id` | 调用方的请求标识，用于关联一次响应；相同 id 的再次调用仍会再次执行。 |
| `operationId` | 长任务标识，可查询、等待或取消。任务最多保留 128 条，仅在本次运行中有效。 |
| `requestId` / `libraryRequestId` | 歌单网络／扫描任务的取消标识。未指定时自动分配；同一时间须保持唯一。TaskRecord 中会返回实际使用的 libraryRequestId。 |
| `token` | 现有手牌播放与确认流程的标识；用于诊断，不替代 operationId。 |
| `subscriptionId` / `after` | 订阅标识与事件序号。第三方通过 `event.poll` 取得事件。 |

平台标识可由 `platform list` 查询：NeteaseCloudMusic、QQMusic、KugouMusic、KuwoMusic、QishuiMusic、Spotify、AppleMusic、YouTube、Bilibili、LocalMusic。所有 ID 保持字符串，包括带前导零的 ID。来源类型由 `source kinds <platform>` 查询。

默认省略歌单、发现、手牌、历史及事件中的封面 data URI。需要封面时使用 `cover get`、`history covers`，或命令支持的 `--include-covers true`。

播放类命令返回 `status: accepted` 与 `confirmed: false` 表示已派发。实际结果通过 `playback-result`、`auto-hand-state-changed` 或 `playback state` 查询。`auto on/off/toggle` 调整临时 AUTO 状态；保存的开关使用 `settings set music.hand.autoPlayEnabled ...`。自动与手动出牌共用原有锁，忙时返回错误。创建手牌沿用设置、容量与已启用来源平台的限制。

托盘的 `pause` 暂停全局快捷键；音乐暂停使用 `playback pause`。不带 platform 时暂停当前 Windows 媒体会话，再回退至已绑定的浏览器或 Spotify 扩展；指定 platform 时按对应会话或扩展定向暂停。`discovery choose` 按当前模式播放或收牌，`discovery play` 直接走播放流程，`discovery collect` 走收牌流程。`mini play` 与迷你发现现有操作一致，派发后准备下一批。

## 本机 HTTP/JSON 接口

通过控制台工具栏的 API 按钮或 `api start` 开启。接口只监听 `127.0.0.1`，默认关闭；开启状态跨重启保留。端口在每次启动时分配，第三方连接前读取配置文件中的 address。`api info` 返回实际地址和配置文件路径。

连接配置位于当前数据目录的 `settings/console_api.json`。正常安装默认为 `%APPDATA%/com.caipeilan.discoas/user_data/settings/console_api.json`。其中 token 用于 `Authorization: Bearer ...`；普通命令结果与日志不会输出 token。仅把该文件交给需要控制 DiscoAS 的本机程序。接口拒绝带有浏览器 Origin 的请求。

| 请求 | 用途 |
| --- | --- |
| `GET /v1/info` | 当前接口状态与版本。 |
| `GET /v1/catalog` | 完整命令、设置、事件目录。 |
| `POST /v1/execute` | 执行命令，Content-Type 为 application/json。 |

请求可以使用 command，或使用 method + params：

```json
{"id":"request-1","command":"source list --sort name"}
{"id":"request-2","method":"source.list","params":{"sort":"name"}}
```

成功／失败的统一响应：

```json
{"id":"request-2","ok":true,"data":[]}
{"id":"request-3","ok":false,"error":{"code":"INVALID_ARGUMENT","message":"缺少参数。"}}
```

合法请求的业务失败以 HTTP 200 与 `ok:false` 表达。鉴权失败为 401，浏览器 Origin 为 403，请求格式错误为 400，未知路径为 404。未识别的参数与错误类型会被拒绝。`schema <command>` 返回该命令参数、返回值说明，以及统一请求／响应结构。HTTP 响应始终为 JSON，`--json` 只控制界面显示偏好。

PowerShell 调用示例：

```powershell
$configPath = Join-Path $env:APPDATA 'com.caipeilan.discoas/user_data/settings/console_api.json'
$connection = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$body = @{ id = 'example-1'; method = 'source.list'; params = @{} } | ConvertTo-Json -Depth 20
Invoke-RestMethod -Uri "$($connection.address)/v1/execute" -Method Post `
  -Headers @{ Authorization = "Bearer $($connection.token)" } `
  -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($body))
```

## 任务与订阅

```text
source refresh NeteaseCloudMusic:playlist:8285082830 --async
task list
task get task-1
task wait task-1 --timeout 30000
task cancel task-1
```

TaskRecord 包含 operationId、requestId、libraryRequestId、method、status、startedAt、finishedAt、progress、data、error、cancellable、cancelRequested。status 为 running / succeeded / failed / cancelled。任务标识以实际返回值为准。

取消沿用歌单任务的提交边界：已完成的保存会保留，已进入提交的单项继续完成；批量更新在取消后停止剩余项。`source refresh-many` 与 `settings reset` 返回逐项 results；外层成功表示批处理已完成，需逐项检查 ok。`source retry-failed` 从原批量任务的结果中重新执行失败来源。

控制台中的 `event watch` 与 `log follow` 会自动输出后续事件。第三方使用以下顺序，并将返回的 after 作为下一次 poll 的游标：

```text
event watch playback-result
event poll watch-1 --after 0 --timeout 30000
event unwatch watch-1
```

每个订阅保留最近 256 个事件，事件包含 sequence、time、event、payload。`droppedBefore` 表示此前事件已离开缓冲区。最多同时保留 64 个订阅，使用完毕后应 unwatch。`task wait` 等待上限 120000ms；`event poll` 等待上限 30000ms。

公开事件：`library-changed`、`library-progress`、`discovery-state-changed`、`discovery-status-changed`、`discovery-history-changed`、`hand-state-changed`、`hand-visibility-changed`、`hand-error`、`auto-hand-state-changed`、`playback-result`、`preferences-changed`、`gui-changed`、`desktop-changed`、`onboarding-changed`、`client-window-result`。`log` 由 log follow 建立。filter 使用 payload 顶层键的精确 JSON 值匹配。

`log tail` 每个文件最多返回最近 1000 行，category 按行内容筛选。`log follow` 输出后续完整行。命令输入历史最多保留 200 条，当前版本关闭程序后清空。

## 完整命令表

### 控制台基础能力

| 命令 | JSON 方法 | 参数（* 为必填） | 用途／结果 | 异步 |
| --- | --- | --- | --- | --- |
| `help [command]` | `help` | `command`: string | 列出命令、参数、示例及对应方法名；commands: CommandSpec[] | — |
| `schema [command]` | `schema` | `command`: string | 输出参数、结果与错误的结构；commands、request、reply、errors 的结构 | — |
| `capabilities [platform]` | `capabilities` | `platform`: string | 列出当前平台、播放连接和接口版本支持的能力；操作结果对象；失败返回 error | — |
| `platform list` | `platform.list` | — | 列出稳定平台标识、来源类型与当前可用能力；操作结果对象；失败返回 error | — |
| `clear` | `clear` | — | 清空控制台显示；操作结果对象；失败返回 error | — |
| `console history [limit]` | `console.history` | `limit`: integer | 查看命令输入历史；操作结果对象；失败返回 error | — |
| `console output <format>` | `console.output` | `format`*: string | 切换人工可读或机器可读结果；{format: text\|json} | — |
| `console close` | `console.close` | — | 关闭控制台面板；操作结果对象；失败返回 error | — |
| `task list [status]` | `task.list` | `status`: string | 列出当前会话内任务；TaskRecord[] | — |
| `task get <operationId>` | `task.get` | `operationId`*: string | 查看进度、结果或错误；TaskRecord | — |
| `task wait <operationId> [--timeout timeout]` | `task.wait` | `operationId`*: string；`timeout`: integer | 等待任务完成，用于连续测试与第三方调用；TaskRecord；超时返回仍在运行的记录 | — |
| `task cancel <operationId>` | `task.cancel` | `operationId`*: string | 取消任务，并返回是否已接受取消请求；操作结果对象；失败返回 error | — |
| `event watch [event] [--filter filter]` | `event.watch` | `event`: string；`filter`: object | 订阅状态、进度、手牌和播放事件；{subscriptionId, events, after} | — |
| `event unwatch <subscriptionId>` | `event.unwatch` | `subscriptionId`*: string | 停止订阅；操作结果对象；失败返回 error | — |
| `event poll <subscriptionId> [--after after] [--timeout timeout]` | `event.poll` | `subscriptionId`*: string；`after`: integer；`timeout`: integer | 读取订阅中尚未取走的事件，支持 after 和 timeout；{subscriptionId, events, after, droppedBefore, closed} | — |

### 歌单与来源

| 命令 | JSON 方法 | 参数（* 为必填） | 用途／结果 | 异步 |
| --- | --- | --- | --- | --- |
| `source list [--search search] [--platform platform] [--sort sort]` | `source.list` | `search`: string；`platform`: string；`sort`: string | 列出来源、启用状态、曲数、缺失数、缓存错误；排序支持导入顺序、名称、更新时间；SourceSummary[]（默认省略封面 data URI） | — |
| `source get <source> [--include-covers includeCovers]` | `source.get` | `source`*: identity；`includeCovers`: boolean | 查看来源详情与本地缓存摘要；SourceSummary | — |
| `source identify <input>` | `source.identify` | `input`*: string | 识别链接或分享文本中的平台与来源类型；{platform, sourceKind} | — |
| `source kinds <platform>` | `source.kinds` | `platform`*: string | 列出该平台支持的来源类型；操作结果对象；失败返回 error | — |
| `source import <input> [--platform platform] [--source-kind sourceKind] [--remark remark] [--request-id requestId]` | `source.import` | `input`*: string；`platform`: string；`sourceKind`: string；`remark`: string；`requestId`: string | 导入或更新来源；操作结果对象；失败返回 error | 支持 |
| `source import-default [--request-id requestId]` | `source.import-default` | `requestId`: string | 导入/刷新资源文件中的作者默认歌单；操作结果对象；失败返回 error | 支持 |
| `source import-local <paths> [--remark remark] [--request-id requestId]` | `source.import-local` | `paths`*: array；`remark`: string；`requestId`: string | 从本地音频文件、目录或 M3U 导入来源；操作结果对象；失败返回 error | 支持 |
| `source choose-local <kind>` | `source.choose-local` | `kind`*: string | 选择本地音频文件或目录；操作结果对象；失败返回 error | 支持 |
| `source refresh <source> [--request-id requestId]` | `source.refresh` | `source`*: identity；`requestId`: string | 按完整身份更新一个来源，保留备注和启用状态；操作结果对象；失败返回 error | 支持 |
| `source refresh-many <sources> [--request-id requestId]` | `source.refresh-many` | `sources`*: array；`requestId`: string | 依次更新多个来源，保留每项结果；操作结果对象；失败返回 error | 支持 |
| `source retry-failed <operationId> [--request-id requestId]` | `source.retry-failed` | `operationId`*: string；`requestId`: string | 重试批量更新中失败的来源；操作结果对象；失败返回 error | 支持 |
| `source enable <source>` | `source.enable` | `source`*: identity | 启用一个来源，同时取消其他来源的启用；操作结果对象；失败返回 error | — |
| `source remark <source> <remark>` | `source.remark` | `source`*: identity；`remark`*: string | 编辑备注，不使无关发现批次失效；操作结果对象；失败返回 error | — |
| `source remove <source>` | `source.remove` | `source`*: identity | 移除配置项，保留原有缓存和记录；操作结果对象；失败返回 error | — |
| `source remove-many <sources>` | `source.remove-many` | `sources`*: array | 批量移除，同时取消对应在途更新；操作结果对象；失败返回 error | — |
| `source cancel <requestId>` | `source.cancel` | `requestId`*: string | 调用 `cancel_library_operation`；操作结果对象；失败返回 error | — |
| `source import-dialog` | `source.import-dialog` | — | 打开“添加歌单”界面；操作结果对象；失败返回 error | — |

### 发现与单曲播放

| 命令 | JSON 方法 | 参数（* 为必填） | 用途／结果 | 异步 |
| --- | --- | --- | --- | --- |
| `discovery state` | `discovery.state` | — | 读取当前批次、epoch、替换预算与状态；DiscoveryState | — |
| `discovery preferences` | `discovery.preferences` | — | 读取发现界面所需设置；操作结果对象；失败返回 error | — |
| `discovery prepare [--force force]` | `discovery.prepare` | `force`: boolean | 准备发现批次，可强制重新抽取；DiscoveryState | 支持 |
| `discovery refresh [--force force]` | `discovery.refresh` | `force`: boolean | 请求新批次，沿用原有排除、预加载与批次隔离；DiscoveryState | 支持 |
| `discovery show` | `discovery.show` | — | 打开实际发现窗口；操作结果对象；失败返回 error | — |
| `discovery cancel` | `discovery.cancel` | — | 退出发现并按“取消后换一批”设置处理；操作结果对象；失败返回 error | — |
| `discovery replace <song> --batch-epoch <batchEpoch> [--platform platform] [--source-id sourceId] [--source-kind sourceKind]` | `discovery.replace` | `song`*: identity；`batchEpoch`*: integer；`platform`: string；`sourceId`: string；`sourceKind`: string | 替换指定歌曲并消耗本批替换次数；操作结果对象；失败返回 error | — |
| `discovery choose <song> --batch-epoch <batchEpoch> [--platform platform] [--source-id sourceId] [--source-kind sourceKind]` | `discovery.choose` | `song`*: identity；`batchEpoch`*: integer；`platform`: string；`sourceId`: string；`sourceKind`: string | 按当前模式收牌或播放，并记录选择；操作结果对象；失败返回 error | — |
| `discovery play <song> --batch-epoch <batchEpoch> [--platform platform] [--source-id sourceId] [--source-kind sourceKind]` | `discovery.play` | `song`*: identity；`batchEpoch`*: integer；`platform`: string；`sourceId`: string；`sourceKind`: string | 手牌模式关闭时，校验批次与来源后播放指定发现歌曲；操作结果对象；失败返回 error | — |
| `discovery collect <song> --batch-epoch <batchEpoch> [--platform platform] [--source-id sourceId] [--source-kind sourceKind]` | `discovery.collect` | `song`*: identity；`batchEpoch`*: integer；`platform`: string；`sourceId`: string；`sourceKind`: string | 将发现卡加入手牌，遵守容量与锁；操作结果对象；失败返回 error | — |
| `discovery displayed <batchEpoch> [--identities identities]` | `discovery.displayed` | `batchEpoch`*: integer；`identities`: array | 确认批次已实际展示，写入入选记录与加权轮次；操作结果对象；失败返回 error | — |
| `discovery preload` | `discovery.preload` | — | 开始准备后续发现批次；操作结果对象；失败返回 error | — |
| `cover get <url>` | `cover.get` | `url`*: string | 读取或准备指定封面；操作结果对象；失败返回 error | — |
| `playback state` | `playback.state` | — | 读取媒体会话、派发结果、确认状态和播放连接；{native, lastResult, auto, browser, browserObservation}（只读） | — |
| `playback pause [platform]` | `playback.pause` | `platform`: string | 暂停当前媒体会话或指定平台的可控播放连接；{paused: true}；暂停失败返回错误 | — |

### 手牌、迷你发现与 AUTO

| 命令 | JSON 方法 | 参数（* 为必填） | 用途／结果 | 异步 |
| --- | --- | --- | --- | --- |
| `hand list` | `hand.list` | — | 列出卡片 ID、顺序、元数据与当前锁状态；{hand, auto} | — |
| `hand show` | `hand.show` | — | 显示当前配置的手牌窗口；操作结果对象；失败返回 error | — |
| `hand hide` | `hand.hide` | — | 隐藏手牌窗口；操作结果对象；失败返回 error | — |
| `hand toggle` | `hand.toggle` | — | 切换手牌显示；操作结果对象；失败返回 error | — |
| `hand expand` | `hand.expand` | — | 展开手牌；操作结果对象；失败返回 error | — |
| `hand collapse` | `hand.collapse` | — | 收纳手牌；操作结果对象；失败返回 error | — |
| `hand play <cardId>` | `hand.play` | `cardId`*: string | 打出一张牌；保留原有失败丢牌及准备失败保牌规则；操作结果对象；失败返回 error | — |
| `hand play-position <position>` | `hand.play-position` | `position`*: integer | 按当前顺序打出第 1–9 张；优先普通手牌，其次迷你发现；操作结果对象；失败返回 error | — |
| `hand discard <cardId>` | `hand.discard` | `cardId`*: string | 丢弃一张牌；操作结果对象；失败返回 error | — |
| `hand clear` | `hand.clear` | — | 清空手牌，遵守当前操作锁；操作结果对象；失败返回 error | — |
| `hand reorder <cardIds>` | `hand.reorder` | `cardIds`*: array | 调整普通手牌顺序；操作结果对象；失败返回 error | — |
| `hand create <input>` | `hand.create` | `input`*: string | 开启创建手牌选项后，创建对应单曲；继续限制为启用来源所属平台；操作结果对象；失败返回 error | 支持 |
| `hand create-dialog` | `hand.create-dialog` | — | 打开创建手牌界面；操作结果对象；失败返回 error | — |
| `hand show-collected <cardId> [--origin origin] [--mystery-cover mysteryCover]` | `hand.show-collected` | `cardId`*: string；`origin`: object；`mysteryCover`: string | 显示已入手的卡片，可指定起点矩形和神秘封面；操作结果对象；失败返回 error | — |
| `mini state` | `mini.state` | — | 读取迷你发现批次与卡片；{discovery, cards}（省略封面 data URI） | — |
| `mini refresh` | `mini.refresh` | — | 刷新迷你发现；操作结果对象；失败返回 error | — |
| `mini replace <card> --batch-epoch <batchEpoch> [--platform platform] [--source-id sourceId] [--source-kind sourceKind]` | `mini.replace` | `card`*: identity；`batchEpoch`*: integer；`platform`: string；`sourceId`: string；`sourceKind`: string | 单卡替换，沿用本批替换预算；操作结果对象；失败返回 error | — |
| `mini collect <card> --batch-epoch <batchEpoch> [--platform platform] [--source-id sourceId] [--source-kind sourceKind] [--insert insert]` | `mini.collect` | `card`*: identity；`batchEpoch`*: integer；`platform`: string；`sourceId`: string；`sourceKind`: string；`insert`: integer | 从迷你发现收入普通手牌；操作结果对象；失败返回 error | — |
| `mini play <card> --batch-epoch <batchEpoch> [--platform platform] [--source-id sourceId] [--source-kind sourceKind]` | `mini.play` | `card`*: identity；`batchEpoch`*: integer；`platform`: string；`sourceId`: string；`sourceKind`: string | 播放选中曲目并准备下一批迷你发现；操作结果对象；失败返回 error | — |
| `mini reorder <cardIds> --batch-epoch <batchEpoch>` | `mini.reorder` | `cardIds`*: array；`batchEpoch`*: integer | 调整迷你发现顺序；操作结果对象；失败返回 error | — |
| `mini displayed <batchEpoch> [--identities identities]` | `mini.displayed` | `batchEpoch`*: integer；`identities`: array | 确认当前迷你发现批次已展示，并写入展示记录；操作结果对象；失败返回 error | — |
| `auto state` | `auto.state` | — | 返回 configured、active、busy、revision、token；AutoHandState | — |
| `auto on` | `auto.on` | — | 临时开启 AUTO，要求保存设置允许自动打出；回复包含控制修订号；操作结果对象；失败返回 error | — |
| `auto off` | `auto.off` | — | 临时关闭 AUTO，在途播放继续确认与解锁；回复包含控制修订号；操作结果对象；失败返回 error | — |
| `auto toggle` | `auto.toggle` | — | 基于当前服务状态切换，沿用修订号过滤迟到回复；操作结果对象；失败返回 error | — |

### 发现记录

| 命令 | JSON 方法 | 参数（* 为必填） | 用途／结果 | 异步 |
| --- | --- | --- | --- | --- |
| `history list [--search search] [--platform platform] [--status status] [--page page] [--page-size pageSize]` | `history.list` | `search`: string；`platform`: string；`status`: string；`page`: integer；`pageSize`: integer | 查询入选或选择记录，支持筛选与分页（默认每页 25 项）；操作结果对象；失败返回 error | — |
| `history get <platform> <songId>` | `history.get` | `platform`*: string；`songId`*: string | 查看一个歌曲身份的记录；操作结果对象；失败返回 error | — |
| `history covers <identities>` | `history.covers` | `identities`*: array | 读取历史封面；操作结果对象；失败返回 error | — |
| `history repair` | `history.repair` | — | 补齐旧记录元数据，沿用并发与超时限制；操作结果对象；失败返回 error | 支持 |
| `history mark <identities> <action> <value>` | `history.mark` | `identities`*: array；`action`*: string；`value`*: boolean | 批量修改一种状态，同时更新权重记录；操作结果对象；失败返回 error | — |
| `history remove <identities>` | `history.remove` | `identities`*: array | 删除指定记录，同时更新权重记录；操作结果对象；失败返回 error | — |
| `history clear` | `history.clear` | — | 清空记录与对应权重记录；操作结果对象；失败返回 error | — |

### 设置、计算器与快捷键

| 命令 | JSON 方法 | 参数（* 为必填） | 用途／结果 | 异步 |
| --- | --- | --- | --- | --- |
| `settings list [group]` | `settings.list` | `group`: string | 列出设置及当前值；SettingSpec[] + value/default | — |
| `settings get <path>` | `settings.get` | `path`*: string | 读取单项或设置组；指定设置的 JSON 值 | — |
| `settings schema [path]` | `settings.schema` | `path`: string | 列出类型、范围、枚举、只读状态与生效时机；SettingSpec[] + value/default | — |
| `settings defaults [group]` | `settings.defaults` | `group`: string | 查看默认值；指定组或全部默认设置 | — |
| `settings set <path> <value>` | `settings.set` | `path`*: string；`value`*: any | 校验并保存一个设置项；操作结果对象；失败返回 error | — |
| `settings patch <group> <patch>` | `settings.patch` | `group`*: string；`patch`*: object | 校验整组再保存，保留来源互斥与跨字段约束；操作结果对象；失败返回 error | — |
| `settings validate <group> <patch>` | `settings.validate` | `group`*: string；`patch`*: object | 校验一组设置修改；{valid, group, settings} | — |
| `settings reset <paths>` | `settings.reset` | `paths`*: array | 对选中项应用当前默认值；操作结果对象；失败返回 error | — |
| `settings draft get` | `settings.draft.get` | — | 查看主界面当前草稿，用于 UI 测试；操作结果对象；失败返回 error | — |
| `settings draft apply` | `settings.draft.apply` | — | 保存主界面草稿；操作结果对象；失败返回 error | — |
| `settings draft discard` | `settings.draft.discard` | — | 放弃未保存草稿；操作结果对象；失败返回 error | — |
| `font list [search]` | `font.list` | `search`: string | 枚举系统字体及别名；操作结果对象；失败返回 error | — |
| `monitor list` | `monitor.list` | — | 枚举显示器与稳定 ID；操作结果对象；失败返回 error | — |
| `cover choose-mystery` | `cover.choose-mystery` | — | 选择本地神秘封面；操作结果对象；失败返回 error | — |
| `weighting traditional-calculate <n> <k>` | `weighting.traditional-calculate` | `n`*: integer；`k`*: integer | 计算传统加权参数；操作结果对象；失败返回 error | — |
| `weighting ranked-recommend [songCount]` | `weighting.ranked-recommend` | `songCount`: integer | 计算排序相对加权的软衰减带宽建议；操作结果对象；失败返回 error | — |
| `weighting inspect [identities] [--limit limit]` | `weighting.inspect` | `identities`: array；`limit`: integer | 读取当前来源的权重、轮次和排除状态；{source, mode, remainingSongs, total, songs: [{songId, weight, excluded}]} | — |
| `shortcut state` | `shortcut.state` | — | 查看全局快捷键注册状态、冲突与暂停状态；操作结果对象；失败返回 error | — |
| `shortcut pause` | `shortcut.pause` | — | 暂停全局快捷键，沿用托盘行为；操作结果对象；失败返回 error | — |
| `shortcut resume` | `shortcut.resume` | — | 恢复全局快捷键；操作结果对象；失败返回 error | — |
| `shortcut recording <enabled>` | `shortcut.recording` | `enabled`*: boolean | 沿用 `set_shortcut_recording`；操作结果对象；失败返回 error | — |

### 窗口、客户端与播放连接

| 命令 | JSON 方法 | 参数（* 为必填） | 用途／结果 | 异步 |
| --- | --- | --- | --- | --- |
| `window main show` | `window.main.show` | — | 显示主窗口；操作结果对象；失败返回 error | — |
| `window main hide` | `window.main.hide` | — | 隐藏主窗口；操作结果对象；失败返回 error | — |
| `window main minimize` | `window.main.minimize` | — | 最小化主窗口；操作结果对象；失败返回 error | — |
| `window main maximize` | `window.main.maximize` | — | 最大化主窗口；操作结果对象；失败返回 error | — |
| `window main restore` | `window.main.restore` | — | 恢复主窗口；操作结果对象；失败返回 error | — |
| `window state [window]` | `window.state` | `window`: string | 读取窗口显示、大小、位置与最大化状态；操作结果对象；失败返回 error | — |
| `window navigate <page> [category]` | `window.navigate` | `page`*: string；`category`: string | 切换主界面页面与设置分类；操作结果对象；失败返回 error | — |
| `window dock get` | `window.dock.get` | — | 查看当前 Logo 浮窗位置与坐标信息；操作结果对象；失败返回 error | — |
| `window dock set <point>` | `window.dock.set` | `point`*: object | 调整并保存浮窗位置，坐标使用当前画布范围；操作结果对象；失败返回 error | — |
| `tray show` | `tray.show` | — | 显示应用托盘菜单；操作结果对象；失败返回 error | — |
| `tray hide` | `tray.hide` | — | 关闭托盘菜单；操作结果对象；失败返回 error | — |
| `tray action <action>` | `tray.action` | `action`*: string | 执行发现、手牌、主窗口、快捷键暂停、重启或退出动作；操作结果对象；失败返回 error | — |
| `client show` | `client.show` | — | 显示本地客户端，返回找到与显示的窗口数；操作结果对象；失败返回 error | — |
| `client hide-state` | `client.hide-state` | — | 查询客户端隐藏是否启用；操作结果对象；失败返回 error | — |
| `client desktop status` | `client.desktop.status` | — | 查看支持、移动能力、已配置桌面 ID 和名称；操作结果对象；失败返回 error | — |
| `client desktop recheck` | `client.desktop.recheck` | — | 重新检测独立桌面；操作结果对象；失败返回 error | — |
| `client desktop create` | `client.desktop.create` | — | 手动创建独立桌面；操作结果对象；失败返回 error | — |
| `spotify status` | `spotify.status` | — | 查询 Spotify 扩展连接；操作结果对象；失败返回 error | — |
| `spotify setup-status` | `spotify.setup-status` | — | 查询安装与配置状态；操作结果对象；失败返回 error | — |
| `spotify configure [--repair]` | `spotify.configure` | `repair`: boolean | 配置 Spotify 扩展；--repair 保留副本后整理可修复的旧目录；操作结果对象；失败返回 error | 支持 |
| `spotify export` | `spotify.export` | — | 导出 Spotify 扩展；操作结果对象；失败返回 error | — |
| `spotify open-folder [--config]` | `spotify.open-folder` | `config`: boolean | 打开扩展目录；--config 打开配置目录；操作结果对象；失败返回 error | — |
| `browser status` | `browser.status` | — | 查询受控浏览器连接；操作结果对象；失败返回 error | — |
| `browser export [browser]` | `browser.export` | `browser`: string | 导出对应扩展；类型是操作参数；操作结果对象；失败返回 error | — |
| `browser open-folder [browser]` | `browser.open-folder` | `browser`: string | 打开对应扩展目录；操作结果对象；失败返回 error | — |

### 使用引导、数据、日志与应用

| 命令 | JSON 方法 | 参数（* 为必填） | 用途／结果 | 异步 |
| --- | --- | --- | --- | --- |
| `onboarding state` | `onboarding.state` | — | 查看步骤、revision、完成状态与目标来源；操作结果对象；失败返回 error | — |
| `onboarding start` | `onboarding.start` | — | 回顾引导，先刷新并启用作者默认歌单；操作结果对象；失败返回 error | 支持 |
| `onboarding next [expectedStep]` | `onboarding.next` | `expectedStep`: integer | 推进当前可继续的引导步骤；操作步骤由对应命令推进；操作结果对象；失败返回 error | — |
| `onboarding back [expectedStep]` | `onboarding.back` | `expectedStep`: integer | 返回允许回退的步骤；操作结果对象；失败返回 error | — |
| `onboarding exit [expectedStep]` | `onboarding.exit` | `expectedStep`: integer | 退出引导并保存状态；操作结果对象；失败返回 error | — |
| `data path` | `data.path` | — | 返回当前数据目录；操作结果对象；失败返回 error | — |
| `data open` | `data.open` | — | 打开数据目录；操作结果对象；失败返回 error | — |
| `data legacy-status` | `data.legacy-status` | — | 查看旧版数据候选路径与导入条件；操作结果对象；失败返回 error | — |
| `data import-legacy [path]` | `data.import-legacy` | `path`: string | 使用当前迁移预检与合并规则；操作结果对象；失败返回 error | 支持 |
| `log open` | `log.open` | — | 打开日志目录；操作结果对象；失败返回 error | — |
| `log tail [lines] [category]` | `log.tail` | `lines`: integer；`category`: string | 读取最近日志；[{file, line}] | — |
| `log follow [category] [--lines lines]` | `log.follow` | `category`: string；`lines`: integer | 订阅后续日志；{subscriptionId, events: [log], after} | — |
| `diagnostic snapshot <target>` | `diagnostic.snapshot` | `target`*: string | 读取观察源、目标身份、操作锁和播放确认状态；操作结果对象；失败返回 error | — |
| `app state [--include-covers includeCovers]` | `app.state` | `includeCovers`: boolean | 读取完整应用快照；AppState（默认省略封面 data URI） | — |
| `app version` | `app.version` | — | 读取程序版本；{version, apiVersion} | — |
| `app update-check` | `app.update-check` | — | 检查更新，返回版本、说明、在线包与完整包下载链接；操作结果对象；失败返回 error | — |
| `app open-link <name>` | `app.open-link` | `name`*: string | 打开项目、作者、许可、更新或 Spotify 帮助链接；操作结果对象；失败返回 error | — |
| `app restart` | `app.restart` | — | 重启 DiscoAS；操作结果对象；失败返回 error | — |
| `app quit` | `app.quit` | — | 退出 DiscoAS；操作结果对象；失败返回 error | — |

### 本机第三方接口

| 命令 | JSON 方法 | 参数（* 为必填） | 用途／结果 | 异步 |
| --- | --- | --- | --- | --- |
| `api info` | `api.info` | — | 查看本机接口地址、状态和连接配置路径；{enabled, address, configPath, apiVersion, authentication, error} | — |
| `api start` | `api.start` | — | 开启本机 HTTP/JSON 接口并保存连接配置；{enabled, address, configPath, apiVersion, authentication, error} | — |
| `api stop` | `api.stop` | — | 关闭本机接口并取消自动开启；{enabled, address, configPath, apiVersion, authentication, error} | — |

## 完整设置键

set / patch / reset 立即保存，并发送原有设置事件。music、gui、desktop 是三组设置；可使用 settings validate 检查而不保存。修改 music／gui／desktop 使用同一组原有校验和生效流程，按字段影响更新发现、窗口或连接。渲染方式重启后生效。

settings draft get/apply/discard 操作主窗口尚未保存的草稿；settings get 读取已保存值。局部 patch 在操作锁内与最新保存值合并。多组 reset 按组返回保存结果。

sources 由来源命令维护；兼容字段及旧颜色只读。请用 weightingMode 切换加权模式。自动检测方式由程序选择。

| 设置键 | 范围／类型 | 含义 | 权限 |
| --- | --- | --- | --- |
| `music.normalSongCount` | 整数 1–15 | 普通发现歌曲数 | 读写 |
| `music.mysterySongsEnabled` | bool | 包含神秘歌曲 | 读写 |
| `music.mysterySongCount` | 整数 0–14 | 神秘歌曲数；界面开启时通常从 1 起；与普通歌曲合计最多 15 | 读写 |
| `music.mysteryCover` | URL 或路径字符串 | 神秘歌曲封面 | 读写 |
| `music.preloadBatchCount` | 整数 0–5 | 预加载批数 | 读写 |
| `music.deduplicatePreloadedBatches` | bool | 预加载批次间去重 | 读写 |
| `music.replacementLimit` | 整数 0–100 | 每次发现替换数 | 读写 |
| `music.refreshOnCancel` | bool | 取消后换一批 | 读写 |
| `music.recentExclusionMode` | off / selected / discovered | 近期排除模式 | 读写 |
| `music.recentExclusionLimit` | 整数 0–10000 | 近期排除数量；历史存储固定最多 10000 个歌曲身份 | 读写 |
| `music.weightingMode` | none / traditional / ranked / rotation | 加权模式 | 读写 |
| `music.discoveryShortcut` | 快捷键字符串 | 唤出发现 | 读写 |
| `music.discoveryKeybindings.up` | 6 个按键字符串 | 发现内移动、选择与替换，检查互相冲突及全局快捷键冲突 | 读写 |
| `music.discoveryKeybindings.left` | 6 个按键字符串 | 发现内移动、选择与替换，检查互相冲突及全局快捷键冲突 | 读写 |
| `music.discoveryKeybindings.down` | 6 个按键字符串 | 发现内移动、选择与替换，检查互相冲突及全局快捷键冲突 | 读写 |
| `music.discoveryKeybindings.right` | 6 个按键字符串 | 发现内移动、选择与替换，检查互相冲突及全局快捷键冲突 | 读写 |
| `music.discoveryKeybindings.select` | 6 个按键字符串 | 发现内移动、选择与替换，检查互相冲突及全局快捷键冲突 | 读写 |
| `music.discoveryKeybindings.replace` | 6 个按键字符串 | 发现内移动、选择与替换，检查互相冲突及全局快捷键冲突 | 读写 |
| `music.sources` | 只读列表 | 通过 source 命令增删、更新和启用 | 只读 |
| `music.discoveryWeighting.enabled` | 兼容只读 bool | 由当前加权模式决定 | 只读 |
| `music.discoveryWeighting.baseWeight` | 1–10000 | 基础权重 | 读写 |
| `music.discoveryWeighting.discoveredPenalty` | 0–10000 | 入选减权 | 读写 |
| `music.discoveryWeighting.selectedPenalty` | 0–10000 | 选择减权 | 读写 |
| `music.discoveryWeighting.recoveryBatches` | 整数 1–10000 | 恢复发现次数 | 读写 |
| `music.discoveryWeighting.boostAfterBatches` | recoveryBatches–10000 | 开始增权等待次数 | 读写 |
| `music.discoveryWeighting.boostPerBatch` | 0–10000 | 每轮增权 | 读写 |
| `music.discoveryWeighting.maxWeight` | baseWeight–100000 | 最大权重 | 读写 |
| `music.rankedWeighting.softBand` | 整数 0–10000 | 软衰减宽度 | 读写 |
| `music.rankedWeighting.timeCooling` | bool | 时间冷却 | 读写 |
| `music.rankedWeighting.timeCoolingStrength` | 0–0.95 | 冷却强度 | 读写 |
| `music.rankedWeighting.timeCoolingBatches` | 整数 1–10000 | 冷却轮次 | 读写 |
| `music.rankedWeighting.revival` | bool | 久未出现增权 | 读写 |
| `music.rankedWeighting.revivalGain` | 0–2 | 增权幅度 | 读写 |
| `music.rankedWeighting.revivalWait` | 整数 0–10000 | 增权等待 | 读写 |
| `music.rankedWeighting.revivalSpan` | 整数 1–10000 | 增权跨度 | 读写 |
| `music.hand.enabled` | bool | 手牌模式 | 读写 |
| `music.hand.revealMystery` | bool | 收牌后揭示神秘歌曲 | 读写 |
| `music.hand.resident` | bool | 手牌常驻 | 读写 |
| `music.hand.keepDiscoveryOpen` | bool | 收牌后保留发现界面 | 读写 |
| `music.hand.autoPlayEnabled` | bool | 保存的自动打出设置 | 读写 |
| `music.hand.createCards` | bool | 创建手牌入口 | 读写 |
| `music.hand.autoPlayPosition` | first / last | 下一张使用首张或末张 | 读写 |
| `music.hand.autoPlayDetection` | 兼容只读 system / window / mixed | 程序自动选择检测方式 | 只读 |
| `music.hand.shortcut` | 快捷键字符串 | 唤出手牌 | 读写 |
| `music.hand.playShortcuts` | 9 个快捷键字符串 | 前九张快捷打出 | 读写 |
| `music.hand.side` | left / right / bottom | 手牌显示边缘 | 读写 |
| `music.hand.capacity` | 整数 1–100 | 手牌上限 | 读写 |
| `music.hand.scale` | 0.5–3 | 手牌大小 | 读写 |
| `music.hand.edgeOffsetPx` | -500–500 | 边缘距离，像素 | 读写 |
| `music.hand.positionPercent` | 0–100 | 沿屏幕边缘的位置 | 读写 |
| `music.hand.overlapPercent` | 0–90 | 重叠比例 | 读写 |
| `music.hand.tiltDegrees` | 0–45 | 卡片倾角 | 读写 |
| `music.handDiscovery.enabled` | bool | 迷你发现 | 读写 |
| `music.handDiscovery.merged` | bool | 合并到普通手牌 | 读写 |
| `music.handDiscovery.liquidMode` | static / blur | 背景模式 | 读写 |
| `music.handDiscovery.liquidStrength` | 0–100 | 背景强度 | 读写 |
| `music.handDiscovery.textOutlineWidth` | 0.05–5 | 文字描边宽度 | 读写 |
| `music.handDiscovery.side` | left / right / bottom | 迷你发现边缘 | 读写 |
| `music.handDiscovery.scale` | 0.5–3 | 迷你发现大小 | 读写 |
| `music.handDiscovery.edgeOffsetPx` | -500–500 | 边缘距离 | 读写 |
| `music.handDiscovery.positionPercent` | 0–100 | 沿边缘位置 | 读写 |
| `music.handDiscovery.overlapPercent` | 0–90 | 重叠比例 | 读写 |
| `music.handDiscovery.tiltDegrees` | 0–45 | 卡片倾角 | 读写 |
| `gui.language` | zh_CN / zh_TW / en_US | 界面语言 | 读写 |
| `gui.fontFamily` | 最多 128 字符，无控制字符 | 字体，空字符串使用系统字体 | 读写 |
| `gui.fontSize` | 10–24 | 字号，像素 | 读写 |
| `gui.followSystemTheme` | bool | 跟随系统主题 | 读写 |
| `gui.nightMode` | bool | 手动深浅模式 | 读写 |
| `gui.mainWindowScale` | 0.5–3 | 主界面大小 | 读写 |
| `gui.discoveryCardScale` | 0.5–3 | 发现卡片大小 | 读写 |
| `gui.cancelButtonScale` | 0.5–3 | 发现退出按钮大小 | 读写 |
| `gui.replacementButtonScale` | 0.5–3 | 单卡替换按钮大小 | 读写 |
| `gui.discoveryBarScale` | 0.5–3 | 发现信息条大小 | 读写 |
| `gui.userConfigured` | 管理字段，只读 | 外观是否由用户主动保存 | 只读 |
| `gui.card.background` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.card.backgroundHover` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.card.border` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.card.fontColor` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cancelButton.background` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cancelButton.backgroundHover` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cancelButton.border` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cancelButton.fontColor` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.setting.background` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.setting.backgroundHover` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.setting.border` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.setting.fontColor` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cardNightMode.background` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cardNightMode.backgroundHover` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cardNightMode.border` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cardNightMode.fontColor` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cancelButtonNightMode.background` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cancelButtonNightMode.backgroundHover` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cancelButtonNightMode.border` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.cancelButtonNightMode.fontColor` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.settingNightMode.background` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.settingNightMode.backgroundHover` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.settingNightMode.border` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `gui.settingNightMode.fontColor` | 兼容读取 | 各含 background、backgroundHover、border、fontColor；现有界面没有任意配色编辑器 | 只读 |
| `desktop.launchAtLogin` | bool | 开机自启动 | 读写 |
| `desktop.startHidden` | bool | 仅托盘启动 | 读写 |
| `desktop.updateOnStartup` | bool | 启动更新已启用歌单 | 读写 |
| `desktop.pauseBeforePlayback` | bool | 切歌时暂停当前播放 | 读写 |
| `desktop.overlayMonitor` | current / primary / 返回的显示器 ID | 发现界面显示器 | 读写 |
| `desktop.clientWindowMode` | off / temporary / persistent | 隐藏客户端方式 | 读写 |
| `desktop.clientSeparateDesktop` | bool | 创建独立虚拟桌面 | 读写 |
| `desktop.temporaryHideSeconds` | 0–30 | 临时隐藏延时，秒 | 读写 |
| `desktop.spotifyPlaybackMode` | scheme / pause_then_scheme / extension | Spotify 播放连接 | 读写 |
| `desktop.browserPlaybackMode` | extension / direct | 浏览器播放连接 | 读写 |
| `desktop.renderingMode` | auto / integrated / discrete / cpu | 渲染方式，重启生效 | 读写 |
| `desktop.spotifyDefaultsVersion` | 管理字段，只读 | 默认设置迁移版本 | 只读 |
| `desktop.minimizeAfterPlayback` | 兼容读取 bool | 旧设置，新界面由 clientWindowMode 管理 | 只读 |

## 常用测试流程

```text
source list
source enable NeteaseCloudMusic:playlist:8285082830
discovery prepare
discovery state
discovery replace 2107861047 --batch-epoch 2
hand list
auto state
diagnostic snapshot auto
playback state
```

示例中的歌曲和批次编号使用实际返回值替换。引导 next 只推进界面提供的普通继续步骤；发现、导入、启用等步骤需要执行对应业务操作，back 与 exit 会校验提供的 expectedStep。

常见错误：INVALID_ARGUMENT 参数或类型错误，UNKNOWN_COMMAND 未知命令，READ_ONLY_SETTING 只读设置，BUSY 操作锁占用，STALE_STATE 批次或来源过时，CANCELLED 已取消，TASK_NOT_FOUND / SUBSCRIPTION_NOT_FOUND 标识失效，UI_UNAVAILABLE / UI_TIMEOUT 主窗口无法执行该动作，OPERATION_FAILED 现有业务流程失败。错误的 details 可含已完成的部分结果。
