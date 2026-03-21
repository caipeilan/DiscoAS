"""
Spotify 歌曲卡片模块

使用 Spotify 匿名 Token 获取歌曲详情
"""


import requests

# 导入共享的 Token 获取函数
from platforms.Spotify.get_json import get_session


class SongCard:
    """Spotify 歌曲卡片类"""

    # 默认神秘歌曲封面
    DEFAULT_MYSTERY_PIC = "https://i.scdn.co/image/ab67616d0000b2738863bc11d2aa12b54f5aeb36"

    def __init__(
        self,
        song_id: str,
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
        self.album_pic_url: str | None = None
        self.have_loaded: bool = False

    def load_song_detail(self) -> None:
        """加载歌曲详情"""
        if self.have_loaded:
            return

        if self.mystery_mode:
            # 神秘模式不需要加载详情
            self.song_name = "???"
            self.song_artist_names = ["???"]
            self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
            self.album_pic_url = self.mystery_pic_url
            self.have_loaded = True
            return

        try:
            headers = get_session()

            # 使用 Spotify API 获取歌曲详情
            url = f"https://api.spotify.com/v1/tracks/{self.song_id}"

            response = requests.get(url, headers=headers, timeout=10)
            response.raise_for_status()
            data = response.json()

            # 解析歌曲信息
            self.song_detail_json = data
            self.song_name = data.get("name", "???").strip()

            # 获取艺术家信息
            self.song_artists = data.get("artists", [])
            self.song_artist_names = [artist.get("name", "???").strip() for artist in self.song_artists]

            # 构建窗口名
            self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)

            # 获取专辑封面
            album = data.get("album", {})
            images = album.get("images", [])
            self.album_pic_url = images[0].get("url", self.mystery_pic_url) if images else self.mystery_pic_url

            self.have_loaded = True

        except Exception as e:
            print(f"歌曲详情加载失败: {e}")
            self._set_error_defaults()

    def _set_error_defaults(self) -> None:
        """设置错误默认值为未知"""
        self.song_name = "???"
        self.song_artist_names = ["???"]
        self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
        self.album_pic_url = self.mystery_pic_url
        self.have_loaded = False

    def get_id(self) -> str:
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
        return self.window_name or ""

    def get_album_pic_url(self) -> str:
        if self.mystery_mode:
            return self.mystery_pic_url
        return self.album_pic_url or self.mystery_pic_url

    def get_scheme_url(self) -> str:
        """生成 Spotify scheme URL 用于唤起播放"""
        return f"spotify:track:{self.song_id}"


# 测试代码
if __name__ == '__main__':
    # 测试歌曲：Shape of You
    song = SongCard("4cOdK2wGLETKBW3PvgPWqT")
    song.load_song_detail()

    print(f"歌曲ID: {song.get_id()}")
    print(f"歌曲名: {song.get_name()}")
    print(f"艺术家: {song.get_artist_names()}")
    print(f"窗口名: {song.get_window_name()}")
    print(f"专辑封面: {song.get_album_pic_url()}")
    print(f"Scheme URL: {song.get_scheme_url()}")
