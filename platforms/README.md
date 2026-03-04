
# 该文档由Minimax - M2.5模型编写

## Platforms 目录说明

本目录用于存放各音乐平台的适配代码。每个平台一个子目录，子目录名称即为平台标识符（如 `NeteaseCloudMusic`、`QQMusic`）。

## 目录结构

每个平台目录下必须包含以下两个文件：

| 文件 | 说明 |
|------|------|
| `card.py` | 歌曲卡片类，实现单首歌曲的信息获取和播放唤起 |
| `get_json.py` | 歌单/专辑获取类，实现从平台API获取歌曲列表 |

其余文件（如签名算法、测试脚本等）根据各平台实际需要自行添加。


## JSON 数据文件

### 文件路径

歌单/专辑数据JSON文件的存放路径：
```
platforms/{平台名}/user_data/{playlist|album}/{id}.json
```

例如：
- `platforms/NeteaseCloudMusic/user_data/playlist/8285082830.json`

### JSON 数据格式

`PlaylistAlbumJson.save()` 保存的数据格式如下：

```json
{
    "playlist_album_id": "123456",
    "playlist_album_name": "歌单名称",
    "playlist_album_type": "playlist",
    "song_ids": [111, 222, 333, ...]
}
```

- `playlist_album_id`：歌单/专辑ID
- `playlist_album_name`：歌单/专辑名称
- `playlist_album_type`：类型 `"playlist"` 或 `"album"`
- `song_ids`：歌曲ID列表

## 核心类接口规范

### SongCard 类（card.py）

负责处理单首歌曲的信息展示和播放唤起。

```python
class SongCard:
    def __init__(self, song_id: int, mystery_mode: bool = False, mystery_pic_url: str = None):
        """
        构造函数
        - song_id: 歌曲在平台中的唯一标识
        - mystery_mode: 是否为秘密歌曲模式（隐藏详细信息）
        - mystery_pic_url: 秘密歌曲模式下使用的封面图URL
        """

    def load_song_detail(self) -> None:
        """从平台API加载歌曲详细信息"""

    # ---------- 以下为必须实现的方法 ----------

    def get_id(self) -> int:
        """返回歌曲ID"""

    def get_name(self) -> str:
        """返回歌曲名称"""

    def get_artist_names(self) -> List[str]:
        """返回艺术家名称列表"""

    def get_window_name(self) -> str:
        """返回窗口显示名称，格式如：歌曲名 - 艺术家1/艺术家2"""

    def get_album_pic_url(self) -> str:
        """返回专辑封面图片URL"""

    def get_scheme_url(self) -> str:
        """返回用于唤起对应音乐APP的scheme URL"""

    # ---------- 以下为必须实现的属性 ----------
    
    have_loaded: bool  # 标记歌曲详情是否已加载
```

#### 秘密歌曲模式(mystery_mode)行为说明

当 `mystery_mode=True` 时：
- `get_name()` 返回 `"秘密歌曲"`
- `get_artist_names()` 返回 `["??????????"]`
- `get_album_pic_url()` 返回 `mystery_pic_url`
- `have_loaded` 仍设为 `True`（避免重复加载）

### PlaylistAlbumJson 类（get_json.py）

负责从平台获取歌单/专辑的歌曲列表。

```python
class PlaylistAlbumJson:
    def __init__(self, playlist_album_id: str, typename: str):
        """
        构造函数
        - playlist_album_id: 歌单或专辑的ID
        - typename: 类型标识，"playlist" 或 "album"
        """

    # ---------- 以下为必须实现的方法 ----------

    def get_id(self) -> str:
        """返回歌单/专辑ID"""

    def get_name(self) -> str:
        """返回歌单/专辑名称"""

    def get_songs(self) -> List[int]:
        """返回歌曲ID列表"""

    def save(self) -> None:
        """将歌曲ID列表保存到本地JSON文件"""
```

## 模块动态导入机制

主程序通过平台名称动态导入对应的模块：

```python
# 导入歌单/专辑获取类
module_path = f"platforms.{platform}.get_json"
get_json_module = __import__(module_path, fromlist=['PlaylistAlbumJson'])
PlaylistAlbumJson = get_json_module.PlaylistAlbumJson

# 导入歌曲卡片类
platform_path = os.path.join("platforms", platform)
card_module = importlib.import_module('card', platform_path)
SongCard = getattr(card_module, 'SongCard')
```

## 添加新平台步骤

1. 在 `platforms/` 下创建新平台目录（如 `MyMusicPlatform/`）

2. 创建 `card.py`，实现 `SongCard` 类

3. 创建 `get_json.py`，实现 `PlaylistAlbumJson` 类

4. 创建 `user_data/` 目录，用于存放歌单/专辑数据

5. （可选）创建 `run.py` 用于本地测试 scheme URL

6. 在 `settings/setting_gui.py` 的平台选择下拉框中添加新平台名称

7. 在 `main.py` 中根据平台名称动态导入对应的模块（参考现有实现）

## 注意事项

- `run.py` 仅用于开发者本地调试，切勿在正式代码中调用
- 遵循各平台的使用规范，避免频繁请求导致账号被封
- 妥善处理网络异常，提供友好的错误提示
- JSON 文件路径必须遵循 `platforms/{平台}/user_data/{playlist|album}/{id}.json` 格式
