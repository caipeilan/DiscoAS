import fs from "node:fs";
import { fileURLToPath } from "node:url";

const commands = JSON.parse(fs.readFileSync(new URL("../src/features/console/commands.json", import.meta.url)));
const settings = JSON.parse(fs.readFileSync(new URL("../src/features/console/settings.json", import.meta.url)));
const cell = value => String(value).replaceAll("|", "\\|").replaceAll("\n", " ");
const code = value => `\`${cell(value)}\``;
const lines = [
  "此文档由GPT-6.1sol生成", "", "# DiscoAS 2.1.0 控制台与本机接口", "",
  `控制台位于主窗口左侧的「控制台」。本版提供 ${commands.length} 个命令、${settings.length} 个设置键。命令目录、参数校验、补全和本表使用同一份登记数据。`, "",
  "## 开始使用", "",
  "在 DiscoAS 控制台中输入以下命令：", "", "```text", "help", "app version", "source list", "settings get music.normalSongCount", "schema source import", "```", "",
  "↑ / ↓ 浏览本窗口的输入历史，Tab 补全命令与设置键，Enter 执行。补全列表打开时，Enter 先接受选项。Ctrl+C 取消当前任务，或停止本控制台的事件订阅；选中文字时保留复制操作。导航离开控制台会停止该页面建立的订阅。", "",
  "`help` 的别名是 `?`，`clear` 的别名是 `cls`，`console close` 的别名是 `exit`。所有命令均支持 `--help` 与 `--json`；表中标为「支持」的命令可使用 `--async`，立即返回任务记录。", "",
  "命令使用空格路径，也接受对应的点号方法名，例如 `source list` 与 `source.list`。`<参数>` 必填，`[参数]` 可选；参数也可以写成 `--参数名 值` 或 `--参数名=值`。界面参数使用短横线，如 `--batch-epoch`；JSON 参数使用 camelCase，如 `batchEpoch`。", "",
  "字符串可使用单引号或双引号保护空格，Windows 路径保留反斜线。JSON 对象与数组保留内部引号，支持空格及嵌套。设置布尔值使用 `true` / `false`；明确声明为布尔的普通参数也接受 `on` / `off`。字符串 `true`、`123` 等若需要按文字保存，可使用外层单引号包裹 JSON 字符串，例如 `'\"123\"'`。", "",
  "```text", 'source import-local "D:\\音乐\\测试卡池" --remark "我的歌单" --async', 'settings patch music {"normalSongCount":3,"hand":{"enabled":true}}', 'settings set gui.fontFamily "Microsoft YaHei UI"', 'history get NeteaseCloudMusic 00123', "```", "",
  "控制台解释 DiscoAS 命令。播放与窗口操作调用应用现有流程，播放请求仍需等待官方播放器的状态确认。", "",
  "## 身份、批次与结果", "",
  "| 字段 | 含义 |", "| --- | --- |",
  "| `source` | 完整来源对象 `{platform,sourceKind,sourceId}`，或 `platform:sourceKind:sourceId` 字符串。 |",
  "| `song` / `card` | 当前批次中的 songId 或完整歌曲身份对象。mini 命令的 card 也接受迷你卡片自己的 id。 |",
  "| `batchEpoch` | 从 `discovery state` 或 `mini state` 取得。发现批次的播放、替换、收牌、展示确认，以及迷你发现的重排均要求此值；批次已改变时返回 `STALE_STATE`。 |",
  "| `cardId` | 普通手牌自己的 id；与歌曲的 songId 不同。`hand play-position` 按执行时的当前牌序读取第 1–9 张。需要操作固定手牌时使用 cardId。 |",
  "| `id` | 调用方的请求标识，用于关联一次响应；相同 id 的再次调用仍会再次执行。 |",
  "| `operationId` | 长任务标识，可查询、等待或取消。任务最多保留 128 条，仅在本次运行中有效。 |",
  "| `requestId` / `libraryRequestId` | 歌单网络／扫描任务的取消标识。未指定时自动分配；同一时间须保持唯一。TaskRecord 中会返回实际使用的 libraryRequestId。 |",
  "| `token` | 现有手牌播放与确认流程的标识；用于诊断，不替代 operationId。 |",
  "| `subscriptionId` / `after` | 订阅标识与事件序号。第三方通过 `event.poll` 取得事件。 |", "",
  "平台标识可由 `platform list` 查询：NeteaseCloudMusic、QQMusic、KugouMusic、KuwoMusic、QishuiMusic、Spotify、AppleMusic、YouTube、Bilibili、LocalMusic。所有 ID 保持字符串，包括带前导零的 ID。来源类型由 `source kinds <platform>` 查询。", "",
  "默认省略歌单、发现、手牌、历史及事件中的封面 data URI。需要封面时使用 `cover get`、`history covers`，或命令支持的 `--include-covers true`。", "",
  "播放类命令返回 `status: accepted` 与 `confirmed: false` 表示已派发。实际结果通过 `playback-result`、`auto-hand-state-changed` 或 `playback state` 查询。`auto on/off/toggle` 调整临时 AUTO 状态；保存的开关使用 `settings set music.hand.autoPlayEnabled ...`。自动与手动出牌共用原有锁，忙时返回错误。创建手牌沿用设置、容量与已启用来源平台的限制。", "",
  "托盘的 `pause` 暂停全局快捷键；音乐暂停使用 `playback pause`。不带 platform 时暂停当前 Windows 媒体会话，再回退至已绑定的浏览器或 Spotify 扩展；指定 platform 时按对应会话或扩展定向暂停。`discovery choose` 按当前模式播放或收牌，`discovery play` 直接走播放流程，`discovery collect` 走收牌流程。`mini play` 与迷你发现现有操作一致，派发后准备下一批。", "",
  "## 本机 HTTP/JSON 接口", "",
  "通过控制台工具栏的 API 按钮或 `api start` 开启。接口只监听 `127.0.0.1`，默认关闭；开启状态跨重启保留。端口在每次启动时分配，第三方连接前读取配置文件中的 address。`api info` 返回实际地址和配置文件路径。", "",
  "连接配置位于当前数据目录的 `settings/console_api.json`。正常安装默认为 `%APPDATA%/com.caipeilan.discoas/user_data/settings/console_api.json`。其中 token 用于 `Authorization: Bearer ...`；普通命令结果与日志不会输出 token。仅把该文件交给需要控制 DiscoAS 的本机程序。接口拒绝带有浏览器 Origin 的请求。", "",
  "| 请求 | 用途 |", "| --- | --- |", "| `GET /v1/info` | 当前接口状态与版本。 |", "| `GET /v1/catalog` | 完整命令、设置、事件目录。 |", "| `POST /v1/execute` | 执行命令，Content-Type 为 application/json。 |", "",
  "请求可以使用 command，或使用 method + params：", "", "```json", '{"id":"request-1","command":"source list --sort name"}', '{"id":"request-2","method":"source.list","params":{"sort":"name"}}', "```", "",
  "成功／失败的统一响应：", "", "```json", '{"id":"request-2","ok":true,"data":[]}', '{"id":"request-3","ok":false,"error":{"code":"INVALID_ARGUMENT","message":"缺少参数。"}}', "```", "",
  "合法请求的业务失败以 HTTP 200 与 `ok:false` 表达。鉴权失败为 401，浏览器 Origin 为 403，请求格式错误为 400，未知路径为 404。未识别的参数与错误类型会被拒绝。`schema <command>` 返回该命令参数、返回值说明，以及统一请求／响应结构。HTTP 响应始终为 JSON，`--json` 只控制界面显示偏好。", "",
  "PowerShell 调用示例：", "", "```powershell", "$configPath = Join-Path $env:APPDATA 'com.caipeilan.discoas/user_data/settings/console_api.json'", "$connection = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json", "$body = @{ id = 'example-1'; method = 'source.list'; params = @{} } | ConvertTo-Json -Depth 20", 'Invoke-RestMethod -Uri "$($connection.address)/v1/execute" -Method Post `', '  -Headers @{ Authorization = "Bearer $($connection.token)" } `', "  -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($body))", "```", "",
  "## 任务与订阅", "",
  "```text", "source refresh NeteaseCloudMusic:playlist:8285082830 --async", "task list", "task get task-1", "task wait task-1 --timeout 30000", "task cancel task-1", "```", "",
  "TaskRecord 包含 operationId、requestId、libraryRequestId、method、status、startedAt、finishedAt、progress、data、error、cancellable、cancelRequested。status 为 running / succeeded / failed / cancelled。任务标识以实际返回值为准。", "",
  "取消沿用歌单任务的提交边界：已完成的保存会保留，已进入提交的单项继续完成；批量更新在取消后停止剩余项。`source refresh-many` 与 `settings reset` 返回逐项 results；外层成功表示批处理已完成，需逐项检查 ok。`source retry-failed` 从原批量任务的结果中重新执行失败来源。", "",
  "控制台中的 `event watch` 与 `log follow` 会自动输出后续事件。第三方使用以下顺序，并将返回的 after 作为下一次 poll 的游标：", "", "```text", "event watch playback-result", "event poll watch-1 --after 0 --timeout 30000", "event unwatch watch-1", "```", "",
  "每个订阅保留最近 256 个事件，事件包含 sequence、time、event、payload。`droppedBefore` 表示此前事件已离开缓冲区。最多同时保留 64 个订阅，使用完毕后应 unwatch。`task wait` 等待上限 120000ms；`event poll` 等待上限 30000ms。", "",
  "公开事件：`library-changed`、`library-progress`、`discovery-state-changed`、`discovery-status-changed`、`discovery-history-changed`、`hand-state-changed`、`hand-visibility-changed`、`hand-error`、`auto-hand-state-changed`、`playback-result`、`preferences-changed`、`gui-changed`、`desktop-changed`、`onboarding-changed`、`client-window-result`。`log` 由 log follow 建立。filter 使用 payload 顶层键的精确 JSON 值匹配。", "",
  "`log tail` 每个文件最多返回最近 1000 行，category 按行内容筛选。`log follow` 输出后续完整行。命令输入历史最多保留 200 条，当前版本关闭程序后清空。", "",
  "## 完整命令表", "",
];
for (const group of [...new Set(commands.map(c => c.group))]) {
  lines.push(`### ${group}`, "", "| 命令 | JSON 方法 | 参数（* 为必填） | 用途／结果 | 异步 |", "| --- | --- | --- | --- | --- |");
  for (const command of commands.filter(c => c.group === group)) {
    const params = command.parameters.map(p => `${code(p.name)}${p.required ? "*" : ""}: ${p.type}`).join("；") || "—";
    lines.push(`| ${code(command.usage)} | ${code(command.method)} | ${params} | ${cell(command.description)}；${cell(command.result)} | ${command.async ? "支持" : "—"} |`);
  }
  lines.push("");
}
lines.push("## 完整设置键", "", "set / patch / reset 立即保存，并发送原有设置事件。music、gui、desktop 是三组设置；可使用 settings validate 检查而不保存。修改 music／gui／desktop 使用同一组原有校验和生效流程，按字段影响更新发现、窗口或连接。渲染方式重启后生效。", "",
  "settings draft get/apply/discard 操作主窗口尚未保存的草稿；settings get 读取已保存值。局部 patch 在操作锁内与最新保存值合并。多组 reset 按组返回保存结果。", "",
  "sources 由来源命令维护；兼容字段及旧颜色只读。请用 weightingMode 切换加权模式。自动检测方式由程序选择。", "",
  "| 设置键 | 范围／类型 | 含义 | 权限 |", "| --- | --- | --- | --- |");
for (const field of settings) lines.push(`| ${code(field.path)} | ${cell(field.range)} | ${cell(field.description)} | ${field.readOnly ? "只读" : "读写"} |`);
lines.push("", "## 常用测试流程", "", "```text", "source list", "source enable NeteaseCloudMusic:playlist:8285082830", "discovery prepare", "discovery state", "discovery replace 2107861047 --batch-epoch 2", "hand list", "auto state", "diagnostic snapshot auto", "playback state", "```", "",
  "示例中的歌曲和批次编号使用实际返回值替换。引导 next 只推进界面提供的普通继续步骤；发现、导入、启用等步骤需要执行对应业务操作，back 与 exit 会校验提供的 expectedStep。", "",
  "常见错误：INVALID_ARGUMENT 参数或类型错误，UNKNOWN_COMMAND 未知命令，READ_ONLY_SETTING 只读设置，BUSY 操作锁占用，STALE_STATE 批次或来源过时，CANCELLED 已取消，TASK_NOT_FOUND / SUBSCRIPTION_NOT_FOUND 标识失效，UI_UNAVAILABLE / UI_TIMEOUT 主窗口无法执行该动作，OPERATION_FAILED 现有业务流程失败。错误的 details 可含已完成的部分结果。", "");
const output = lines.join("\n");
const target = new URL("../../docs/CONSOLE_COMMANDS.md", import.meta.url);
if (process.argv.includes("--check")) {
  if (fs.readFileSync(target, "utf8") !== output) throw new Error("Console documentation is out of date; run npm run docs:console");
} else { fs.writeFileSync(target, output); }
console.log(`${commands.length} commands, ${settings.length} settings: ${fileURLToPath(target)}`);
