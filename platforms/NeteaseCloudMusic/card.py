"""
网易云音乐歌曲卡片模块 - 性能优化版

使用requests.Session复用连接，提升响应速度
"""

import base64
import json

# 导入共享的Session
from platforms.NeteaseCloudMusic.get_json import get_session


class SongCard:
    """网易云音乐歌曲卡片类"""

    # 默认神秘歌曲封面
    DEFAULT_MYSTERY_PIC = "https://p1.music.126.net/sFzdxi9EMPV0q4IuWEy-og==/17792297160856759.jpg"

    def __init__(
        self,
        song_id: int,
        mystery_mode: bool = False,
        mystery_pic_url: str | None = None
    ):
        self.song_id = song_id
        self.mystery_mode = mystery_mode
        self.mystery_pic_url = mystery_pic_url or self.DEFAULT_MYSTERY_PIC

        # 歌曲详情数据
        self.song_detail_json: dict | None = None
        self.song_name: str | None = None
        self.song_artists: list[dict] = []
        self.song_artist_names: list[str] = []
        self.window_name: str | None = None
        self._real_window_name: str | None = ""  # 用于播放器窗口匹配，初始为空字符串防止未初始化
        self.album_pic_url: str | None = None
        self.have_loaded: bool = False

    def load_song_detail(self) -> None:
        """加载歌曲详情"""
        if self.have_loaded:
            return

        try:
            session = get_session()

            # 使用网易云音乐歌曲详情API
            url = "https://music.163.com/api/song/detail/"
            params = {
                "ids": f"[{self.song_id}]"
            }

            response = session.get(url, params=params, timeout=10)
            data = response.json()

            if "songs" in data and len(data["songs"]) > 0:
                song_info = data["songs"][0]

                self.song_detail_json = song_info
                self.song_name = song_info.get("name", "???")
                self.song_artists = song_info.get("artists", [])
                self.song_artist_names = [artist.get("name", "???") for artist in self.song_artists]
                album = song_info.get("album", {})
                self.album_pic_url = album.get("blurPicUrl", self.mystery_pic_url)
            else:
                raise ValueError("无法获取歌曲详情")
        except Exception as e:
            print(f"歌曲详情加载失败: {e}")
            self._set_error_defaults()

        # 神秘模式下覆盖为假数据用于显示，但真实数据已保存在上述属性中
        if self.mystery_mode:
            self.song_name = "???"
            self.song_artist_names = ["???"]
            self.album_pic_url = self.mystery_pic_url

        # 始终构建真实窗口名（用于播放器窗口匹配，不受 mystery_mode 影响）
        self._real_window_name = self.song_name + " - " + "/".join(self.song_artist_names)
        # window_name 用于 UI 显示（mystery_mode 下为假数据）
        self.window_name = self._real_window_name
        self.have_loaded = True

    def _set_error_defaults(self) -> None:
        """设置错误默认值为未知"""
        self.song_name = "???"
        self.song_artist_names = ["???"]
        self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
        self._real_window_name = self.window_name
        self.album_pic_url = self.mystery_pic_url
        self.have_loaded = False

    def get_id(self) -> int:
        return self.song_id

    def get_name(self) -> str:
        if self.mystery_mode:
            return "???"
        return self.song_name or "???"

    def get_artist_names(self) -> list[str]:
        if self.mystery_mode:
            return ["???"]
        return self.song_artist_names or ["???"]

    def get_window_name(self) -> str:
        return self._real_window_name or ""

    def get_album_pic_url(self) -> str:
        if self.mystery_mode:
            return self.mystery_pic_url
        return self.album_pic_url or self.mystery_pic_url

    def get_scheme_url(self) -> str:
        """生成网易云音乐scheme URL用于唤起播放"""
        prefix = "orpheus://"
        the_json = {"type": "song", "id": self.song_id, "cmd": "play"}
        json_str = json.dumps(the_json)
        encoded_json = base64.b64encode(json_str.encode('utf-8')).decode('ascii')
        url = prefix + encoded_json
        return url

    def get_debug_info(self) -> str:
        """返回歌曲真实信息，用于后台调试输出"""
        if self.song_detail_json:
            real_name = self.song_detail_json.get("name", "???")
            real_artists = [a.get("name", "???") for a in self.song_detail_json.get("artists", [])]
            return f"{real_name} - {'/'.join(real_artists)}"
        return self.get_name() + " - " + "/".join(self.get_artist_names())


# 测试代码
if __name__ == '__main__':

    song = SongCard(2121980421)
    song.load_song_detail()

    print(f"歌曲ID: {song.get_id()}")
    print(f"歌曲名: {song.get_name()}")
    print(f"艺术家: {song.get_artist_names()}")
    print(f"窗口名: {song.get_window_name()}")
    print(f"专辑封面: {song.get_album_pic_url()}")
    print(f"Scheme URL: {song.get_scheme_url()}")
