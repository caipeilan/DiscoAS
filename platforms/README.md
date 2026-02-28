# 平台开发指南

本文档为新增音乐平台支持提供开发模板和接口规范。

## 目录结构

每个平台应放在 `platforms/` 目录下，结构如下：

```
platforms/
└── 平台名称/                  # 例如: Spotify, AppleMusic
    ├── card.py               # 歌曲卡片模块（必需）
    ├── get_json.py           # 歌单/专辑API获取（必需）
    ├── run.py                # 平台运行入口
    └── user_data/            # 本地缓存目录
        ├── album/            # 专辑缓存
        └── playlist/         # 歌单缓存
```

---

## 1. SongCard 类（歌曲卡片）

负责单个歌曲的信息获取和展示。

### 必需的文件结构

```python
# card.py
from typing import List, Optional

class SongCard:
    """平台歌曲卡片类"""
    
    # 必须定义默认封面
    DEFAULT_MYSTERY_PIC = "默认封面URL"
    
    def __init__(
        self, 
        song_id: int,
        mystery_mode: bool = False,
        mystery_pic_url: Optional[str] = None
    ):
        self.song_id = song_id
        self.mystery_mode = mystery_mode
        self.mystery_pic_url = mystery_pic_url or self.DEFAULT_MYSTERY_PIC
        
        # 内部状态
        self.song_detail_json: Optional[dict] = None
        self.song_name: Optional[str] = None
        self.song_artists: List[dict] = []
        self.song_artist_names: List[str] = []
        self.window_name: Optional[str] = None
        self.album_pic_url: Optional[str] = None
        self.have_loaded: bool = False

    def load_song_detail(self) -> None:
        """加载歌曲详情（必需方法）
        
        流程：
        1. 如果已加载，直接返回
        2. 如果是神秘模式，设置默认值并返回
        3. 调用API获取歌曲详情
        4. 解析响应，填充内部状态
        5. 失败时调用 _set_error_defaults()
        """
        pass

    def _set_error_defaults(self) -> None:
        """设置错误时的默认值"""
        pass

    # ---- 以下为必需的 getter 方法 ----
    
    def get_id(self) -> int:
        """获取歌曲ID"""
        return self.song_id

    def get_name(self) -> str:
        """获取歌曲名称"""
        if self.mystery_mode:
            return "秘密歌曲"
        return self.song_name or "未知"
    
    def get_artist_names(self) -> List[str]:
        """获取艺术家名称列表"""
        if self.mystery_mode:
            return ["??????????"]
        return self.song_artist_names or ["未知艺术家"]

    def get_window_name(self) -> str:
        """获取窗口标题（歌曲名 - 艺术家）"""
        return self.window_name or ""

    def get_album_pic_url(self) -> str:
        """获取专辑封面URL"""
        if self.mystery_mode:
            return self.mystery_pic_url
        return self.album_pic_url or self.mystery_pic_url

    def get_scheme_url(self) -> str:
        """生成平台专用的scheme URL，用于唤起APP播放"""
        pass
```

### 接口说明

| 方法 | 返回类型 | 说明 |
|------|----------|------|
| `load_song_detail()` | None | 加载歌曲详情，必须实现 |
| `get_id()` | int | 返回歌曲ID |
| `get_name()` | str | 返回歌曲名称 |
| `get_artist_names()` | List[str] | 返回艺术家名称列表 |
| `get_window_name()` | str | 返回窗口标题，格式：`歌曲名 - 艺术家1/艺术家2` |
| `get_album_pic_url()` | str | 返回专辑封面URL |
| `get_scheme_url()` | str | 返回唤起APP的scheme URL |

### 神秘模式

神秘模式用于为选择困难症用户提供秘密歌曲选项，不加载真实歌曲信息：
- 歌曲名称显示为 "秘密歌曲"
- 艺术家名称显示为 "??????????"
- 封面使用默认图片

---

## 2. PlaylistAlbumJson 类（歌单/专辑）

负责获取歌单或专辑中的歌曲列表。

### 必需的文件结构

```python
# get_json.py
from typing import List, Union, Dict

class PlaylistAlbumJson:
    """歌单/专辑JSON获取类"""
    
    def __init__(self, playlist_album_id: str, typename: str):
        """初始化
        
        Args:
            playlist_album_id: 歌单或专辑ID
            typename: 类型标识，"playlist" 或 "album"
        """
        self.playlist_album_id = playlist_album_id
        self.typename = typename  # "playlist" 或 "album"
        self.playlist_album_name: str = ""
        self.playlist_album_json: Union[Dict, List] = {}
        
        # 尝试从API获取，失败可回退到本地缓存
        try:
            self._fetch_data()
        except Exception as e:
            print(f"API获取失败: {e}")
            self._load_from_cache()

    def _fetch_data(self) -> None:
        """从API获取数据（必需方法）"""
        pass

    def _load_from_cache(self) -> None:
        """从本地缓存加载数据"""
        pass

    # ---- 以下为必需的方法 ----
    
    def get_id(self) -> str:
        """获取歌单/专辑ID"""
        return self.playlist_album_id

    def get_name(self) -> str:
        """获取歌单/专辑名称"""
        return self.playlist_album_name

    def get_songs(self) -> List[int]:
        """获取歌曲ID列表（必需方法）
        
        Returns:
            歌曲ID列表
        """
        songs: List[int] = []
        # 根据 self.typename 判断类型
        # 从 self.playlist_album_json 中提取歌曲ID
        return songs

    def save(self) -> None:
        """保存到本地JSON文件"""
        pass
```

### 接口说明

| 方法 | 返回类型 | 说明 |
|------|----------|------|
| `get_id()` | str | 返回歌单/专辑ID |
| `get_name()` | str | 返回歌单/专辑名称 |
| `get_songs()` | List[int] | 返回歌曲ID列表（必需） |
| `save()` | None | 保存到本地缓存 |

### 数据存储格式

保存到 `user_data/playlist/` 或 `user_data/album/` 目录：

```json
{
    "playlist_album_id": "123456",
    "playlist_album_name": "歌单名称",
    "playlist_album_type": "playlist",
    "song_ids": [123456789, 987654321, ...]
}
```

---

## 3. run.py（可选）

平台运行入口，如果平台需要特殊的初始化逻辑可以添加。

---

## 开发注意事项

### API 发现技巧

1. **浏览器开发者工具**
   - 打开目标音乐平台的网页版
   - F12 打开开发者工具
   - 切换到 Network 面板
   - 播放歌曲或加载歌单，观察网络请求

2. **常见API端点模式**
   - 歌单详情：`/playlist/detail` 或类似
   - 专辑详情：`/album/info` 或类似
   - 歌曲详情：`/song/detail` 或类似

3. **参数注意**
   - 有些API需要登录态或token
   - 有些API有频率限制
   - 有些API需要特定的请求头（如 User-Agent, Referer）

4. **签名问题**
   - 部分平台（如QQ音乐）需要签名算法
   - 可以尝试使用设备指纹、cookie等方式绕过

### 性能优化

1. **使用 Session**
   使用 `requests.Session()` 复用TCP连接：
   ```python
   _session = None
   
   def get_session():
       global _session
       if _session is None:
           _session = requests.Session()
           _session.headers.update({"User-Agent": "..."})
       return _session
   ```

2. **本地缓存**
   - API失败时回退到本地缓存
   - 定期更新缓存数据

### 测试

建议添加测试代码：
```python
if __name__ == '__main__':
    # 测试代码
    song = SongCard(歌曲ID)
    song.load_song_detail()
    print(f"歌曲名: {song.get_name()}")
    print(f"艺术家: {song.get_artist_names()}")
```

---

## 示例：参考实现

- **网易云音乐**: `platforms/NeteaseCloudMusic/`
- **QQ音乐**: `platforms/QQMusic/`
