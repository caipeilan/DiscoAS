"""
网易云音乐 API 模块 - 性能优化版

使用requests.Session复用连接，提升响应速度
"""

import json
import os
import requests
from typing import Any, Dict, List, Optional, Union
from functools import lru_cache

# 网易云音乐API配置
NETEASE_BASE_URL = "https://music.163.com"
NETEASE_USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"

# 创建全局Session用于连接复用
_session: Optional[requests.Session] = None


def get_session() -> requests.Session:
    """获取全局Session，复用TCP连接"""
    global _session
    if _session is None:
        _session = requests.Session()
        _session.headers.update({
            "User-Agent": NETEASE_USER_AGENT,
            "Referer": NETEASE_BASE_URL,
            "Origin": NETEASE_BASE_URL,
        })
    return _session


class PlaylistAlbumJson:
    """网易云音乐歌单/专辑JSON获取类"""

    def __init__(self, playlist_album_id: str, typename: str):
        self.playlist_album_id = playlist_album_id
        self.typename = typename
        self.playlist_album_name: str = ""
        self.playlist_album_json: Union[Dict, List] = {}
        
        self._fetch_data()

    def _fetch_data(self) -> None:
        """获取歌单/专辑数据"""
        session = get_session()
        
        if self.typename == "playlist":
            # 获取歌单详情
            url = f"{NETEASE_BASE_URL}/api/v6/playlist/detail"
            params = {
                "id": self.playlist_album_id,
                "limit": 20000,
                "offset": 0,
                "total": True,
            }
            try:
                response = session.get(url, params=params, timeout=10)
                data = response.json()
                
                if "playlist" in data:
                    self.playlist_album_name = data["playlist"].get("name", "")
                    self.playlist_album_json = data
                else:
                    raise ValueError("无法获取歌单信息")
            except Exception as e:
                print(f"获取歌单详情失败: {e}")
                raise
                
        elif self.typename == "album":
            # 获取专辑详情
            url = f"{NETEASE_BASE_URL}/api/album/{self.playlist_album_id}"
            params = {
                "limit": 20000,
            }
            try:
                response = session.get(url, params=params, timeout=10)
                data = response.json()
                
                if "album" in data:
                    self.playlist_album_name = data["album"].get("name", "")
                    self.playlist_album_json = data
                else:
                    raise ValueError("无法获取专辑信息")
            except Exception as e:
                print(f"获取专辑详情失败: {e}")
                raise
        else:
            raise ValueError("typename must be 'playlist' or 'album'")

        print(f"已获取{self.typename}: {self.playlist_album_name}")

    def get_id(self) -> str:
        return self.playlist_album_id

    def get_name(self) -> str:
        return self.playlist_album_name

    def get_songs(self) -> List[int]:
        """获取歌曲ID列表"""
        songs: List[int] = []
        
        if self.typename == "playlist":
            # 从歌单中提取歌曲ID
            if "playlist" in self.playlist_album_json:
                playlist = self.playlist_album_json.get("playlist", {})
                
                # 优先使用trackIds（更完整）
                track_ids = playlist.get("trackIds", [])
                for track in track_ids:
                    if "id" in track:
                        songs.append(track["id"])
                
                # 如果trackIds为空，使用tracks
                if not songs:
                    for track in playlist.get("tracks", []):
                        if "id" in track:
                            songs.append(track["id"])
                            
        elif self.typename == "album":
            # 从专辑中提取歌曲ID
            if "album" in self.playlist_album_json:
                for song in self.playlist_album_json["album"].get("songs", []):
                    if "id" in song:
                        songs.append(song["id"])
                        
        return songs

    def save(self) -> None:
        """保存到本地JSON文件"""
        path = os.path.join(
            os.path.dirname(__file__), 
            "user_data", 
            self.typename
        )
        os.makedirs(path, exist_ok=True)
        
        song_ids = self.get_songs()
        data = {
            "playlist_album_id": self.playlist_album_id,
            "playlist_album_name": self.playlist_album_name,
            "playlist_album_type": self.typename,
            "song_ids": song_ids
        }
        
        filepath = os.path.join(path, f"{self.playlist_album_id}.json")
        with open(filepath, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=4)
            print(f"已保存{self.typename} {self.playlist_album_id} {self.playlist_album_name} 到 {path}")


if __name__ == '__main__':
    # 测试代码
    import sys
    if len(sys.argv) > 2:
        playlist_id = sys.argv[1]
        typename = sys.argv[2]
    else:
        playlist_id = "8285082830"
        typename = "playlist"
    
    playlist = PlaylistAlbumJson(playlist_id, typename)
    print(f"名称: {playlist.get_name()}")
    print(f"歌曲数量: {len(playlist.get_songs())}")
    playlist.save()
