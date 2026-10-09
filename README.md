# DiscoAS - 发现一首歌！

<img src="assets/DiscoAS.png" width="30%" alt="DiscoAS Logo">

## 前言

在~~某个bug每天都不重样的~~卡牌游戏《炉石传说》里，「发现」特指一种「从多个选项中挑选想要的一个」的操作。这种多选一操作正是这款~~垃圾骗氪~~游戏最吸引人的特点之一。

**DiscoAS** 便是项目作者边炉边听歌的产物。在他一次次点击音乐软件的「下一首」按钮，试图在他那放了4500+曲目的歌单里随机到一首想听的曲子，结果手滑把金铜须卖了，最后组件没凑齐被纯海盗阵营当路边一条踢到第5，人叫得比金木研还痛之后，这个小项目借助大语言模型的力量，生まれた……

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

能看到右边的Release吗？就从那下载。
目前仅支持Win64系统。

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

这个小项目本身没有播放器功能，就只是通过Scheme唤起本地应用（更别说什么播放权限、地区限制了），所以还请先安装你所使用的音乐平台对应的桌面软件捏。

---

## 开发环境下运行

```
2026.10.09 记：
原项目的1.X.X版本使用PyQT实现，现已使用Tauri+React进行重构。旧版本的Python代码现已清除。
```

环境准备参见 [Tauri Windows 开发环境](https://v2.tauri.app/start/prerequisites/#windows)。需 Node.js/npm、Rust MSVC 工具链、Windows C++构建工具和WebView2，在仓库根目录执行：

```powershell
cd app
npm ci
npm run tauri dev
```

开发入口见GPT6.1写的[「功能边界」](docs/ARCHITECTURE.md)文档，独立运行和复用方法见GPT6.1写的[「核心说明与示例」](crates/discoas-core/README.md)文档。

---

## 打包

在 `app/` 中执行：

```powershell
npm run tauri build -- --bundles nsis
```

日常检查可以执行：

```powershell
npm run build
npm run test:motion
npm run test:preferences
npm run test:covers
npm run test:discovery
npm run test:source
npm run test:controls
npm run test:library
npm run test:keyboard
npm run test:history
node --test ../extensions/spotify/bridge.test.mjs
node --test ../extensions/browser/bridge.test.mjs
cargo test --manifest-path ../crates/discoas-core/Cargo.toml --offline
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --lib --offline
```

`--offline` 要求Rust依赖已在本机缓存，首次准备依赖时可省略。

## 数据放在哪？

新版将歌单缓存和偏好保存在系统应用配置目录下的 `user_data/`，可在设置中打开。

旧版的 `user_data/` 可以通过设置中的迁移入口导入。

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