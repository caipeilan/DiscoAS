"""
QQ音乐 API 模块 - 纯HTTP实现

使用同步requests库替代异步第三方库，提升响应速度
"""

import json
import os
import requests
from typing import Any, Dict, List, Optional, Union

# QQ音乐API配置
QQMUSIC_BASE_URL = "https://u.y.qq.com/cgi-bin/musicu.fcg"
QQMUSIC_REFERER = "https://y.qq.com/"
QQMUSIC_USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
QQMUSIC_VERSION = "13.2.5.8"

# 创建全局Session用于连接复用
_session: Optional[requests.Session] = None


def get_session() -> requests.Session:
    """获取全局Session，复用TCP连接"""
    global _session
    if _session is None:
        _session = requests.Session()
        _session.headers.update({
            "User-Agent": QQMUSIC_USER_AGENT,
            "Referer": QQMUSIC_REFERER,
            "Content-Type": "application/json",
        })
    return _session


class PlaylistAlbumJson:
    """QQ音乐歌单/专辑JSON获取类"""

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
            url = "https://c.y.qq.com/v8/fcg-bin/fcg_v8_playlist_cp.fcg"
            params = {
                "id": self.playlist_album_id,
                "format": "json",
                "inCharset": "utf8",
                "outCharset": "utf8",
                "notice": 0,
                "platform": "yqq.json",
                "needNewCode": 0,
                "type": 0,
                "json": 1,
                "onlysong": 0,
                "new_format": 1,
            }
            try:
                response = session.get(url, params=params, timeout=10)
                data = response.json()
                
                if "cdlist" in data and len(data["cdlist"]) > 0:
                    self.playlist_album_name = data["cdlist"][0].get("dissname", "")
                    self.playlist_album_json = data
                else:
                    raise ValueError("无法获取歌单信息")
            except Exception as e:
                print(f"获取歌单详情失败: {e}")
                raise
                
        elif self.typename == "album":
            # 获取专辑详情
            url = "https://c.y.qq.com/v8/fcg-bin/fcg_v8_album_info_cp.fcg"
            params = {
                "albummid": self.playlist_album_id,
                "format": "json",
                "inCharset": "utf8",
                "outCharset": "utf8",
                "notice": 0,
                "platform": "yqq.json",
                "needNewCode": 0,
            }
            try:
                response = session.get(url, params=params, timeout=10)
                data = response.json()
                
                if "data" in data:
                    self.playlist_album_name = data["data"].get("name", "")
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
            if "cdlist" in self.playlist_album_json:
                for cd in self.playlist_album_json.get("cdlist", []):
                    for song in cd.get("songlist", []):
                        if "id" in song:
                            songs.append(song["id"])
                            
        elif self.typename == "album":
            # 从专辑中提取歌曲ID
            if "data" in self.playlist_album_json:
                for song in self.playlist_album_json["data"].get("getSongInfo", []):
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
        playlist_id = "9595891286"
        typename = "playlist"
    
    playlist = PlaylistAlbumJson(playlist_id, typename)
    print(f"名称: {playlist.get_name()}")
    print(f"歌曲数量: {len(playlist.get_songs())}")
    playlist.save()
