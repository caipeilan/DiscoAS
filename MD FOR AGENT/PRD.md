# DiscoAS 项目需求文档 (PRD)

## 1. 项目概述

### 项目名称
DiscoAS (Discover A Song)

### 项目类型
桌面端音乐发现与播放工具

### 核心功能摘要
一款极简的音乐选择器，通过全屏透明浮窗展示歌曲卡片，支持从多个音乐平台（网易云音乐、QQ音乐、酷狗音乐）发现和播放音乐。

### 目标用户
- 不满足于当前主流随机播放音乐逻辑的用户
- 需要在工作间隙快速选择音乐的用户
- 追求简洁高效音乐体验的用户

---

## 2. 功能列表

### 2.1 核心功能

| 功能 | 描述 | 优先级 |
|------|------|--------|
| 歌曲发现 | 随机从已配置的歌单中抽取歌曲展示 | P0 |
| 歌曲播放 | 点击歌曲卡片即可播放音乐 | P0 |
| 多平台支持 | 支持网易云音乐、QQ音乐、酷狗音乐、Spotify（Spotify 通过 Embed 页面抓取，无需开发者 Token） | P0 |
| 系统托盘 | 常驻系统托盘，后台运行 | P0 |
| 全局快捷键 | 支持自定义快捷键唤出浮窗（默认 Alt+D） | P0 |
| 开机自启动 | 支持 Windows 注册表方式开机自启动 | P1 |
| 歌单管理 | 支持添加/编辑/删除/启用歌单 | P1 |
| 歌曲缓存 | 预加载下一批歌曲，提升体验 | P1 |

### 2.2 设置功能

| 功能 | 描述 |
|------|------|
| 语言设置 | 支持中文/英文 |
| 快捷键设置 | 自定义全局快捷键 |
| 歌单配置 | 配置各平台歌单ID |
| 开机自启动 | 开关开机自启动功能 |
| 主题设置 | 后续扩展 |

---

## 3. 技术栈

### 3.1 核心技术

| 技术 | 用途 |
|------|------|
| Python 3.x | 编程语言 |
| PyQt6 | GUI 框架 |
| Anaconda | Python 环境管理 |

### 3.2 依赖库

- `PyQt6` - GUI 框架
- `requests` - HTTP 请求
- `keyboard` - 全局快捷键

### 3.3 项目结构

```
DiscoAS/
├── main.py              # 主入口
├── Discover_gui.py      # GUI 主界面
├── Discover.py          # 核心业务逻辑
├── load_playlist_json.py # 歌单加载
├── settings/            # 设置模块
│   ├── setting_gui.py   # 设置界面
│   ├── gui_setting.py   # GUI 设置
│   ├── music_setting.py # 音乐设置
│   └── i18n.py          # 国际化
├── platforms/          # 音乐平台
│   ├── NeteaseCloudMusic/
│   │   ├── get_json.py  # 歌单 JSON 获取
│   │   ├── card.py      # 歌曲卡片
│   │   └── run.py       # 播放函数
│   ├── QQMusic/
│   │   ├── get_json.py
│   │   ├── card.py
│   │   └── run.py
│   ├── KugouMusic/
│   │   ├── get_json.py
│   │   ├── card.py
│   │   └── run.py
│   └── Spotify/
│       ├── get_json.py  # Embed 页面抓取
│       ├── card.py      # 封面延迟加载
│       └── run.py       # 媒体键暂停 scheme
├── src/                 # 资源文件
│   ├── Icon.ico
│   ├── DiscoAS.png
│   └── dark_mask.png   # 遮罩图
└── user_data/          # 用户数据
    ├── settings/
    ├── NeteaseCloudMusic/
    ├── QQMusic/
    ├── KugouMusic/
    └── Spotify/
```

---

## 4. 已知限制

1. **系统限制**：仅支持 Windows 系统
2. **平台依赖**：网易云音乐、QQ音乐、酷狗音乐、Spotify 均已完整支持

---

## 5. 后续规划

- [x] 酷狗音乐平台支持（含歌单和专辑）
- [x] Spotify 平台支持（通过 Embed 页面抓取，无需开发者 Token）
- [ ] 打包为独立 .exe
