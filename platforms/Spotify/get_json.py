"""
Spotify API 模块 - 使用 Embed 页面抓取 Entity 数据

无需开发者 token，通过 Spotify Embed 页面的 __NEXT_DATA__ 提取歌单/专辑/歌曲信息
"""

import json
import os
import re
import sys

import requests

# 添加 settings 目录到路径，导入统一的路径管理模块
sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(__file__)), 'settings'))
from settings.user_data_path import ensure_dir, get_album_dir, get_playlist_dir

# Spotify 配置
SPOTIFY_USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"

# 创建全局 Session 用于连接复用
_session: requests.Session | None = None


def get_session() -> requests.Session:
    """获取全局 Session，复用 TCP 连接"""
    global _session
    if _session is None:
        _session = requests.Session()
        _session.headers.update({
            "User-Agent": SPOTIFY_USER_AGENT,
            "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
        })
    return _session


class PlaylistAlbumJson:
    """Spotify 歌单/专辑 JSON 获取类"""

    def __init__(self, playlist_album_id: str, typename: str):
        self.playlist_album_id = playlist_album_id
        self.typename = typename  # "playlist" | "album"
        self.playlist_album_name: str = ""
        self.playlist_album_json: dict = {}

        self._fetch_data()

    def _fetch_data(self) -> None:
        """从 Spotify Embed 页面抓取 Entity 数据"""
        if self.typename not in ("playlist", "album"):
            raise ValueError("typename must be 'playlist' or 'album'")

        session = get_session()
        embed_url = f"https://open.spotify.com/embed/{self.typename}/{self.playlist_album_id}"

        try:
            response = session.get(embed_url, timeout=10)
            response.raise_for_status()
        except requests.exceptions.RequestException as e:
            raise RuntimeError(f"网络请求失败，请检查网络环境或代理设置！具体报错: {e}")

        # 从 HTML 中提取 __NEXT_DATA__ JSON
        match = re.search(
            r'<script id="__NEXT_DATA__" type="application/json">(.*?)</script>',
            response.text
        )
        if not match:
            raise RuntimeError(
                f"无法在页面源码中找到 __NEXT_DATA__ 节点，Spotify 可能更改了网页结构。\n"
                f"请求的URL: {embed_url}"
            )

        try:
            data = json.loads(match.group(1))
        except json.JSONDecodeError:
            raise RuntimeError("解析内部 JSON 失败。")

        # 提取 entity
        try:
            entity = data["props"]["pageProps"]["state"]["data"]["entity"]
        except KeyError:
            raise RuntimeError("JSON 结构有变，无法找到 entity 数据。")

        self.playlist_album_json = entity
        self.playlist_album_name = entity.get("name", "")

        print(f"已获取 {self.typename}: {self.playlist_album_name}")

    def get_id(self) -> str:
        return self.playlist_album_id

    def get_name(self) -> str:
        return self.playlist_album_name

    def get_songs(self) -> list[str]:
        """获取歌曲 ID 列表（纯 track ID，不带 spotify:track: 前缀）"""
        songs: list[str] = []

        track_list = self.playlist_album_json.get("trackList", [])
        for track in track_list:
            uri = track.get("uri", "")
            # URI 格式: spotify:track:3T0UCGe1Vrfh57fM1B0Mgi
            if uri.startswith("spotify:track:"):
                track_id = uri.replace("spotify:track:", "")
                songs.append(track_id)

        return songs

    def save(self) -> None:
        """保存到本地 JSON 文件"""
        if self.typename == "playlist":
            path = get_playlist_dir("Spotify")
        else:
            path = get_album_dir("Spotify")
        ensure_dir(path)

        song_ids = self.get_songs()

        track_list = self.playlist_album_json.get("trackList", [])
        tracks_info = []
        for track in track_list:
            uri = track.get("uri", "")
            track_id = uri.replace("spotify:track:", "") if uri.startswith("spotify:track:") else uri
            tracks_info.append({
                "id": track_id,
                "title": track.get("title", ""),
                "subtitle": track.get("subtitle", ""),
                "duration": track.get("duration", 0),
            })

        # 获取封面 URL
        cover_art = self.playlist_album_json.get("coverArt", {})
        sources = cover_art.get("sources", [{}])
        cover_url = sources[0].get("url", "") if sources else ""

        # 专辑封面为空时，从第一首歌的 track embed 页面获取
        if not cover_url and self.typename == "album":
            track_list = self.playlist_album_json.get("trackList", [])
            if track_list:
                first_uri = track_list[0].get("uri", "")
                if first_uri.startswith("spotify:track:"):
                    track_id = first_uri.replace("spotify:track:", "")
                    cover_url = self._fetch_track_cover(track_id)

        data = {
            "playlist_album_id": self.playlist_album_id,
            "playlist_album_name": self.playlist_album_name,
            "playlist_album_type": self.typename,
            "song_ids": song_ids,
            "tracks_info": tracks_info,
            "coverUrl": cover_url,
        }

        filepath = os.path.join(path, f"{self.playlist_album_id}.json")
        with open(filepath, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=4)
            print(f"已保存 {self.typename} {self.playlist_album_id} {self.playlist_album_name} 到 {path}")

    def _fetch_track_cover(self, track_id: str) -> str:
        """从 Spotify track embed 页面抓取专辑封面 URL"""
        session = get_session()
        url = f"https://open.spotify.com/embed/track/{track_id}"
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


if __name__ == '__main__':
    # 测试代码
    if len(sys.argv) > 2:
        playlist_id = sys.argv[1]
        typename = sys.argv[2]
    else:
        playlist_id = "37i9dQZF1EIZ9u9vIT9NHT"
        typename = "playlist"

    playlist = PlaylistAlbumJson(playlist_id, typename)
    print(f"名称: {playlist.get_name()}")
    print(f"歌曲数量: {len(playlist.get_songs())}")
    playlist.save()
