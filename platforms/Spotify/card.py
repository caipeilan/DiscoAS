"""
Spotify 歌曲卡片模块

通过 saved_playlist.json 中存储的 tracks_info 获取歌曲信息和封面
"""

import json
import os
import re

from platforms.Spotify.get_json import get_session

# 模块级封面缓存，避免重复请求
_cover_cache: dict[str, str] = {}


class SongCard:
    """Spotify 歌曲卡片类"""

    # 默认神秘歌曲封面
    DEFAULT_MYSTERY_PIC = "https://i.scdn.co/image/ab67616d00001e01a0785b6a03795770e230c15f"

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
        self.song_name: str | None = None
        self.song_artist_names: list[str] = []
        self.window_name: str | None = None
        self._real_window_name: str | None = ""
        self.have_loaded: bool = False

    def load_song_detail(self) -> None:
        """从 saved_playlist.json 加载歌曲详情"""
        if self.have_loaded:
            return

        # 取第一位歌手（多歌手只取第一位）
        first_artist = "???"

        try:
            song_info = self._find_song_info()
            if not song_info:
                raise ValueError("未找到歌曲信息")

            self.song_name = song_info.get("title", "???")
            # subtitle 格式："歌手1, 歌手2, ..."，只取第一位
            subtitle = song_info.get("subtitle", "")
            if subtitle:
                first_artist = subtitle.split(",")[0].strip()
                artists = [first_artist]
            else:
                artists = ["???"]
            self.song_artist_names = artists

        except Exception as e:
            print(f"歌曲详情加载失败: {e}")
            self._set_error_defaults()

        # 神秘模式下覆盖为假数据用于显示
        if self.mystery_mode:
            self.song_name = "???"
            self.song_artist_names = ["???"]
            first_artist = "???"

        # Spotify 播放时窗口名为 "{第一位歌手} - {歌名}"
        self._real_window_name = f"{first_artist} - {self.song_name}"
        self.window_name = self._real_window_name
        self.have_loaded = True

    def _find_song_info(self) -> dict | None:
        """从保存的 playlist/album JSON 文件中查找歌曲信息"""
        try:
            from settings.user_data_path import get_album_dir, get_playlist_dir

            # 先搜索 playlist 目录
            playlist_dir = get_playlist_dir("Spotify")
            if os.path.exists(playlist_dir):
                for filename in os.listdir(playlist_dir):
                    if filename.endswith(".json"):
                        filepath = os.path.join(playlist_dir, filename)
                        with open(filepath, encoding="utf-8") as f:
                            data = json.load(f)
                        tracks_info = data.get("tracks_info", [])
                        for track in tracks_info:
                            if track.get("id", "") == self.song_id:
                                return track

            # 再搜索 album 目录
            album_dir = get_album_dir("Spotify")
            if os.path.exists(album_dir):
                for filename in os.listdir(album_dir):
                    if filename.endswith(".json"):
                        filepath = os.path.join(album_dir, filename)
                        with open(filepath, encoding="utf-8") as f:
                            data = json.load(f)
                        tracks_info = data.get("tracks_info", [])
                        for track in tracks_info:
                            if track.get("id", "") == self.song_id:
                                return track

            return None
        except Exception as e:
            print(f"查找歌曲信息失败: {e}")
            return None

    def _set_error_defaults(self) -> None:
        """设置错误默认值为未知"""
        self.song_name = "???"
        self.song_artist_names = ["???"]
        self.window_name = "??? - ???"
        self._real_window_name = "??? - ???"
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
        return self._real_window_name or ""

    def get_album_pic_url(self) -> str:
        if self.mystery_mode:
            return self.mystery_pic_url
        if not self.have_loaded:
            self.load_song_detail()

        # 优先从缓存获取
        cached = _cover_cache.get(self.song_id)
        if cached:
            return cached

        # 按需抓取真实专辑封面
        fetched = self._fetch_track_cover()
        if fetched:
            _cover_cache[self.song_id] = fetched
            return fetched

        return self.mystery_pic_url

    def _fetch_track_cover(self) -> str:
        """从 Spotify track embed 页面抓取专辑封面 URL"""
        session = get_session()
        url = f"https://open.spotify.com/embed/track/{self.song_id}"
        try:
            response = session.get(url, timeout=10)
            response.raise_for_status()
            match = re.search(
                r'<script id="__NEXT_DATA__" type="application/json">(.*?)</script>',
                response.text
            )
            if not match:
                return ""
            data = json.loads(match.group(1))
            entity = data["props"]["pageProps"]["state"]["data"]["entity"]
            images = entity.get("visualIdentity", {}).get("image", [])
            for img in images:
                if img.get("maxHeight") == 300:
                    return img.get("url", "")
        except Exception:
            pass
        return ""

    def get_scheme_url(self) -> str:
        """生成 Spotify scheme URL 用于唤起播放"""
        return f"spotify:track:{self.song_id}"

    def get_debug_info(self) -> str:
        """返回歌曲真实信息，用于后台调试输出"""
        song_info = self._find_song_info()
        if song_info:
            title = song_info.get("title", "???")
            subtitle = song_info.get("subtitle", "???")
            return f"{title} - {subtitle}"
        return f"{self.get_name()} - {'/'.join(self.get_artist_names())}"


# 测试代码
if __name__ == '__main__':
    # 使用 playlist 37i9dQZF1EIZ9u9vIT9NHT 中的第一首歌曲测试
    test_id = "4MMDJ0gmQOMaThXbk3ytiN"

    song = SongCard(test_id)
    song.load_song_detail()

    print(f"歌曲 ID: {song.get_id()}")
    print(f"歌曲名: {song.get_name()}")
    print(f"艺术家: {song.get_artist_names()}")
    print(f"封面: {song.get_album_pic_url()}")
    print(f"窗口名: {song.get_window_name()}")
    print(f"Scheme URL: {song.get_scheme_url()}")
