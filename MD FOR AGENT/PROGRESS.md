# DiscoAS 进度文档 (PROGRESS.md)

本文档记录项目的当前目标、进行中的任务和待办事项。

---

## 当前状态

### 进行中

| 任务 | 描述 | 状态 | 优先级 |
|------|------|------|--------|
| 暂无 | | | |

### 待处理

| 任务 | 描述 | 状态 | 优先级 |
|------|------|------|--------|
| 打包测试 | 测试打包后环境是否正常工作 | 🔄 待处理 | P1 |

---

## 最近完成


- [x] KugouMusic 专辑 JSON 获取支持：在 `get_json.py` 中新增 `_resolve_album_share_code()` 方法解析专辑分享码，重写 album 分支的 `_fetch_data()` 实现翻页拉取专辑歌曲（`/api/v3/album/song`），与 playlist 分支数据结构对齐
- [x] KugouMusic card.py 修复：`SongCard._find_song_info()` 原只搜索 playlist 目录，新增 album 目录搜索，解决专辑歌曲显示"？？？？？"和封面/播放链接错误的问题
- [x] 秘密歌曲调试信息：各平台 SongCard 新增 `get_debug_info()` 方法返回真实歌曲信息；修复 `load_song_detail()` 在 mystery_mode 下不加载真实数据的问题，确保 `_real_window_name` 始终保存真实窗口名用于播放器匹配
- [x] 播放器窗口匹配修复：将 `gw.getWindowsWithTitle()` 子串匹配改为前缀匹配，解决 QQMusic 窗口标题含注释导致匹配失败的问题
- [x] 设置界面导航栏添加 Logo：在 `setting_gui.py` 左侧导航栏顶部展示 `src/DiscoAS.png`

- [x] 修复开机自启动：使用 `conda run -n DiscoverASong python main.py` 替代直接 python 路径
- [x] 在 `Discover_gui.run_gui()` 中添加启动画面支持
- [x] 异步启动优化：使用 `QTimer.singleShot(0, ...)` 实现启动画面与程序加载并行
- [x] 修改 `SplashScreen.wait_for_finish()` 使用 `QEventLoop` 替代 `time.sleep`，允许事件循环处理后台任务
- [x] 更新 FIRST.md，添加文档更新规范
- [x] 修复设置界面主色调：将所有交互元素（按钮、复选框、滑块、表格头、导航栏选中）的配色从 `cancel_button.border` 改为 `card.border`
- [x] 更新默认配色：将 `gui_setting.py` 中的默认颜色改为与用户保存的配置一致
- [x] 统一设置界面配色规则：悬停用 card_hover，背景用 setting_bg（bg），边框用 card_border
- [x] 添加颜色调色盘功能：点击颜色预览方块弹出系统颜色选择器
- [x] 添加快捷键录制功能：点击"设置快捷键"按钮，通过按键设置快捷键
- [x] 修复托盘右键点击误触发发现界面的问题

---

## 技术债务

- [x] ~~启动画面与程序加载仍是同步的，可以优化为真正异步~~
- [ ] 打包后环境未完全测试
- [ ] 只支持scheme url
---

## 文档更新日志

| 日期 | 更新内容 |
|------|---------|
| 2026-03-19 | 新增 KugouMusic 专辑支持；秘密歌曲调试信息；播放器窗口前缀匹配修复；设置界面 Logo；文档同步更新 |

