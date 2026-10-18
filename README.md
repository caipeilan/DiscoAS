<p align="center">
  <img src="assets/DiscoAS.png" width="200" alt="DiscoAS">
</p>


# DiscoAS - 发现一首歌！

DiscoAS是蔡某人特地为了他那[5000+曲数小众歌单](https://music.163.com/playlist?id=8285082830)写的一个「选歌器」。

<p align="center">
  <a href="https://github.com/caipeilan/DiscoAS/releases/latest"><img src="https://img.shields.io/github/v/release/caipeilan/DiscoAS?label=release" alt="release"></a>
  <img src="https://img.shields.io/badge/platform-Windows%2010%2F11%20x64-0078D6" alt="platform">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/caipeilan/DiscoAS" alt="license"></a>
</p>

<p align="center">
  <img src="assets/演示.webp" width="85%" alt="按下Alt+D以发现一首歌">
</p>



## 这东西是怎么来的

在~~某个bug每天都不重样的~~卡牌游戏《炉石传说》里，「发现」特指一种「从多个选项中挑选想要的一个」的操作。这种多选一操作正是这款~~垃圾骗氪~~游戏最吸引人的特点之一。

**DiscoAS** 便是项目作者边炉边听歌的产物。在他一次次点击音乐软件的「下一首」按钮，试图在他那[放了5000+曲目的小众歌单](https://music.163.com/playlist?id=8285082830)里随机到一首想听的曲子，结果手滑把金铜须卖了，最后组件没凑齐被纯海盗阵营当路边一条踢到第5，人叫得比金木研还痛之后，这个小项目借助大语言模型的力量，生まれた……

---
## 所以，怎么用？


非常简单：

- 导入一个公开歌单、专辑或视频合集作为来源，并启用它
- 按下快捷键 *（或者通过系统托盘）*，**发现**一首歌！
- 选择一首歌
- Enjoy！

如果突然不想选了，可以通过 Esc 或右上角的取消键退出「发现」界面。

---

## 所以，怎么用上呢？

能看到右边的 [Releases](https://github.com/caipeilan/DiscoAS/releases/latest) 吗？就从那下载最新版本的就行。
只有十几MB的安装包是在线安装包喵，两百多MB那个是离线的，看您的网络情况选安装包喵。
目前仅支持Win 64系统喵。

---

## 那啥，前置？

目前适配以下平台：

- 网易云音乐
- QQ音乐
- 酷狗音乐
- 酷我音乐
- 汽水音乐
- Spotify
- YouTube（通过网页）
- Bilibili（通过网页）

就像前面说的那样，这个小项目本质是个「选歌器」，本身没有播放器功能，就只是唤起本地应用而已（更别说什么播放权限、地区限制了），所以还请先安装你所使用的音乐平台对应的桌面软件捏。

---

## 开发

<details>
<summary>环境、构建与测试命令</summary>

环境：Node.js、Rust（MSVC 工具链）、Visual Studio C++ 生成工具、WebView2，参见 [Tauri Windows 前置条件](https://v2.tauri.app/start/prerequisites/#windows)。

在仓库根目录执行：

```powershell
cd app
npm ci
npm run tauri dev
```

打包：

```powershell
npm run tauri build -- --bundles nsis
```

测试、发布流程和完整命令见 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) 与 [docs/RELEASE_GUIDE.md](docs/RELEASE_GUIDE.md)。

</details>

- 功能边界与开发入口：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 核心库说明与示例：[crates/discoas-core/README.md](crates/discoas-core/README.md)
- 更新记录：[CHANGELOG.md](CHANGELOG.md)


## 数据放在哪？

默认在 `%APPDATA%\com.caipeilan.discoas\user_data`，设置里可以直接打开：

```text
user_data/
├── settings/    # 偏好设置
├── pic/         # 封面缓存
├── logs/        # 日志
├── history/     # 发现历史
└── <平台>/{playlist,album}/<id>.json   # 歌单 / 专辑缓存
```

旧版1.x的`user_data/`可以在设置里迁移导入。

---

## Q&A

Q:没有支持我使用的平台/软件诶

A: 目前已接入网易云、QQ音乐、酷狗、酷我、汽水、Spotify以及YouTube和Bilibili。其他平台看反馈后续再考虑。

Q:界面尺寸太大/太小了

A: 设置里面可以分别调整的说

Q:怎么歌曲播放完后直接停止播放/重复播放/播放歌单外歌曲？

A:这个项目管不了音乐平台的播放列表的说（没有那个威能）。请检查客户端本身的播放列表与播放模式。

Q:为什么不支持同时启用多个歌单

A:一是做起来太麻烦，二是没什么人会同时用多个音乐平台。同平台的多个歌单，可以先在原平台合并，再导入。

Q:怎么做到唤起本地应用的

A:通过Scheme协议唤起客户端，至于Youtube和哔哩哔哩则是通过浏览器，至于作者怎么知道每家的scheme，请不要追问谢谢喵。

Q:web接口是怎么扒的？

A:参考现有第三方实现、公开网页接口和相关文档。

Q:安装时提示「未知发布者」怎么办？

A:安装包暂时没有代码签名，点「更多信息 → 仍要运行」。介意的话可以自己按`docs/RELEASE_GUIDE.md`从源码构建。

Q:想清空数据重来？

A:关掉程序，删掉上面对应的`user_data`目录即可；只想重置歌单缓存删`<平台>/`就够了喵

Q:PR怎么说？

A:无论是人写的还是AI写的都接受，只要能说清楚改动目的、验证结果这类的就行。

Q:我要反馈

A:直接私信骚扰[bilibili@蔡佩兰](https://space.bilibili.com/29285623  "点我去作者的B站空间")，如果想他人也参与讨论，请到相关视频评论区中。

~~Q:大切なものって、なあに？~~

~~A:充電器~~

---

## 目前已知

- Spotify 当前采用匿名网页读取方式，正式授权与记住登录信息什么的后面再看吧。

---

## 最后

本项目使用 [GPLv3 协议](LICENSE)。

项目作者：[bilibili@蔡佩兰](https://space.bilibili.com/29285623  "点我去作者的B站空间")