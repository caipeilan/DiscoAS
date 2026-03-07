"""
Spotify API 模块 - 匿名 Token 版

使用 Spotify 网页端的匿名 Token，无需开发者账号
"""

import json
import os
import requests
from typing import Any, Dict, List, Optional, Union
from functools import lru_cache

# Spotify API 配置
SPOTIFY_BASE_URL = "https://api.spotify.com/v1"
SPOTIFY_TOKEN_URL = "https://open.spotify.com/get_access_token"
SPOTIFY_USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36"

# 全局变量
_access_token: Optional[str] = None


def get_access_token() -> str:
    """
    获取 Spotify 匿名 Token
    无需登录，无需开发者账号
    """
    global _access_token
    
    if _access_token:
        return _access_token
    
    headers = {
        "User-Agent": SPOTIFY_USER_AGENT,
        "Accept": "application/json"
    }
    
    try:
        response = requests.get(SPOTIFY_TOKEN_URL, headers=headers, timeout=10)
        if response.status_code == 200:
            data = response.json()
            _access_token = data.get("accessToken")
            if _access_token:
                print("成功获取 Spotify 匿名 Token")
                return _access_token
    except Exception as e:
        print(f"获取 Spotify Token 失败: {e}")
    
    raise Exception("无法获取 Spotify 访问令牌")


def get_session() -> Dict[str, str]:
    """获取带有 Token 的请求头"""
    token = get_access_token()
    return {
        "Authorization": f"Bearer {token}",
        "User-Agent": SPOTIFY_USER_AGENT
    }


class PlaylistAlbumJson:
    """Spotify 歌单 JSON 获取类"""

    def __init__(self, playlist_album_id: str, typename: str):
        self.playlist_album_id = playlist_album_id
        self.typename = typename  # playlist 或 album
        self.playlist_album_name: str = ""
        self.playlist_album_json: Union[Dict, List] = {}
        
        self._fetch_data()

    def _fetch_data(self) -> None:
        """获取歌单/专辑数据"""
        headers = get_session()
        
        if self.typename == "playlist":
            # 获取歌单详情
            url = f"{SPOTIFY_BASE_URL}/playlists/{self.playlist_album_id}"
            # 只获取必要的字段，减少返回数据量
            params = {
                "fields": "name,tracks.items(track(id)),tracks.next"
            }
            try:
                response = requests.get(url, headers=headers, params=params, timeout=10)
                response.raise_for_status()
                data = response.json()
                
                self.playlist_album_name = data.get("name", "")
                self.playlist_album_json = data
                print(f"已获取歌单: {self.playlist_album_name}")
                
            except Exception as e:
                print(f"获取歌单详情失败: {e}")
                raise
                
        elif self.typename == "album":
            # 获取专辑详情
            url = f"{SPOTIFY_BASE_URL}/albums/{self.playlist_album_id}"
            params = {
                "fields": "name,tracks.items(track(id)),tracks.next"
            }
            try:
                response = requests.get(url, headers=headers, params=params, timeout=10)
                response.raise_for_status()
                data = response.json()
                
                self.playlist_album_name = data.get("name", "")
                self.playlist_album_json = data
                print(f"已获取专辑: {self.playlist_album_name}")
                
            except Exception as e:
                print(f"获取专辑详情失败: {e}")
                raise
        else:
            raise ValueError("typename must be 'playlist' or 'album'")

    def get_id(self) -> str:
        return self.playlist_album_id

    def get_name(self) -> str:
        return self.playlist_album_name

    def get_songs(self) -> List[str]:
        """获取歌曲ID列表"""
        songs: List[str] = []
        
        try:
            # 递归获取所有分页
            self._collect_tracks(self.playlist_album_json, songs)
        except Exception as e:
            print(f"获取歌曲列表失败: {e}")
        
        return songs
    
    def _collect_tracks(self, data: Dict, songs: List[str]) -> None:
        """递归收集所有歌曲ID"""
        if self.typename == "playlist":
            tracks_data = data.get("tracks", {})
        elif self.typename == "album":
            tracks_data = data.get("tracks", {})
        else:
            return
        
        # 获取当前页的歌曲
        for item in tracks_data.get("items", []):
            track = item.get("track")
            if track and track.get("id"):
                songs.append(track["id"])
        
        # 检查下一页
        next_url = tracks_data.get("next")
        if next_url:
            headers = get_session()
            try:
                response = requests.get(next_url, headers=headers, timeout=10)
                response.raise_for_status()
                next_data = response.json()
                self._collect_tracks(next_data, songs)
            except Exception as e:
                print(f"获取下一页失败: {e}")

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
        # 默认测试歌单：Today's Top Hits
        playlist_id = "37i9dQZF1DXcBWIGoYBM5M"
        typename = "playlist"
    
    playlist = PlaylistAlbumJson(playlist_id, typename)
    print(f"名称: {playlist.get_name()}")
    print(f"歌曲数量: {len(playlist.get_songs())}")
    playlist.save()
