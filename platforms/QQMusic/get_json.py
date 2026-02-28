"""
QQ音乐 API 模块 - 使用签名算法

基于 qqmusic-api-python 库的签名算法实现
支持本地缓存回退
"""

import json
import os
import requests
from typing import Any, Dict, List, Optional, Union

# 导入签名模块
import sys
import os
sys.path.append(os.path.dirname(__file__))


class PlaylistAlbumJson:
    """QQ音乐歌单/专辑JSON获取类"""

    def __init__(self, playlist_album_id: str, typename: str):
        self.playlist_album_id = playlist_album_id
        self.typename = typename
        self.playlist_album_name: str = ""
        self.playlist_album_json: Union[Dict, List] = {}
        
        # 尝试从API获取，失败则从本地缓存读取
        try:
            self._fetch_data()
        except Exception as e:
            print(f"API获取失败，尝试从本地缓存读取: {e}")
            self._load_from_cache()

    def _fetch_data(self) -> None:
        """获取歌单/专辑数据"""
        
        if self.typename == "playlist":
            # 获取歌单详情 - 使用 qzone-music API（需要 type=1 和 newcp=1 参数）
            url = "https://i.y.qq.com/qzone-music/fcg-bin/fcg_ucc_getcdinfo_byids_cp.fcg"
            params = {
                "disstid": int(self.playlist_album_id) if self.playlist_album_id.isdigit() else self.playlist_album_id,
                "json": 1,
                "utf8": 1,
                "noCache": 1,
                "loginUin": 0,
                "hostUin": 0,
                "format": "json",
                "inCharset": "utf8",
                "outCharset": "utf-8",
                "notice": 0,
                "platform": "yqq",
                "needNewCode": 0,
                "type": 1,
                "newcp": 1
            }
            
            headers = {
                "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Safari/537.36 Edg/116.0.1938.54",
                "Referer": "https://y.qq.com/"
            }
            
            response = requests.get(url, params=params, headers=headers, timeout=10)
            data = response.json()
            
            # 解析响应
            cdlist = data.get("cdlist", [])
            if cdlist:
                self.playlist_album_name = cdlist[0].get("dissname", "")
                self.playlist_album_json = {"songlist": cdlist[0].get("songlist", [])}
            else:
                raise ValueError("无法获取歌单信息")
                
        elif self.typename == "album":
            # 获取专辑详情 - 使用 v8 API
            url = "https://i.y.qq.com/v8/fcg-bin/fcg_v8_album_info_cp.fcg"
            params = {
                "albummid": self.playlist_album_id,
                "json": 1,
                "utf8": 1,
                "loginUin": 0,
                "hostUin": 0,
                "format": "json",
                "inCharset": "utf8",
                "outCharset": "utf-8",
                "notice": 0,
                "platform": "yqq",
                "needNewCode": 0
            }
            
            headers = {
                "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Safari/537.36 Edg/116.0.1938.54",
                "Referer": "https://y.qq.com/"
            }
            
            response = requests.get(url, params=params, headers=headers, timeout=10)
            data = response.json()
            
            # 解析响应
            album_data = data.get("data", {})
            if album_data:
                self.playlist_album_name = album_data.get("name", "")
                self.playlist_album_json = {"songlist": album_data.get("list", [])}
            else:
                raise ValueError("无法获取专辑信息")
        else:
            raise ValueError("typename must be 'playlist' or 'album'")

        print(f"已获取{self.typename}: {self.playlist_album_name}")

    def _load_from_cache(self) -> None:
        """从本地缓存加载数据"""
        cache_path = os.path.join(
            os.path.dirname(__file__),
            "user_data",
            self.typename,
            f"{self.playlist_album_id}.json"
        )
        
        if os.path.exists(cache_path):
            with open(cache_path, "r", encoding="utf-8") as f:
                cache_data = json.load(f)
                self.playlist_album_name = cache_data.get("playlist_album_name", "")
                self.playlist_album_json = {"songlist": []}
                # 从缓存恢复歌曲ID列表
                for song_id in cache_data.get("song_ids", []):
                    self.playlist_album_json["songlist"].append({"id": song_id})
            print(f"已从缓存加载{self.typename}: {self.playlist_album_name}")
        else:
            raise FileNotFoundError(f"本地缓存不存在: {cache_path}")

    def get_id(self) -> str:
        return self.playlist_album_id

    def get_name(self) -> str:
        return self.playlist_album_name

    def get_songs(self) -> List[int]:
        """获取歌曲ID列表"""
        songs: List[int] = []
        
        if self.typename == "playlist":
            # 从歌单中提取歌曲ID (支持 songid 或 id)
            if "songlist" in self.playlist_album_json:
                for song in self.playlist_album_json.get("songlist", []):
                    # 优先使用 songid（QQ音乐API返回的字段名）
                    if "songid" in song:
                        songs.append(song["songid"])
                    elif "id" in song:
                        songs.append(song["id"])
                            
        elif self.typename == "album":
            # 从专辑中提取歌曲ID
            if "songlist" in self.playlist_album_json:
                for song in self.playlist_album_json["songlist"]:
                    if "songid" in song:
                        songs.append(song["songid"])
                    elif "id" in song:
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
