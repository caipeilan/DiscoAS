"""
网易云音乐歌曲卡片模块 - 性能优化版

使用requests.Session复用连接，提升响应速度
"""

import json
import base64
import requests
from typing import List, Optional

# 导入共享的Session
import os
import sys
sys.path.append(os.path.dirname(__file__))
from get_json import get_session


class SongCard:
    """网易云音乐歌曲卡片类"""

    # 默认神秘歌曲封面
    DEFAULT_MYSTERY_PIC = "https://p1.music.126.net/sFzdxi9EMPV0q4IuWEy-og==/17792297160856759.jpg"

    def __init__(
        self, 
        song_id: int,
        mystery_mode: bool = False,
        mystery_pic_url: Optional[str] = None
    ):
        self.song_id = song_id
        self.mystery_mode = mystery_mode
        self.mystery_pic_url = mystery_pic_url or self.DEFAULT_MYSTERY_PIC
        
        # 歌曲详情数据
        self.song_detail_json: Optional[dict] = None
        self.song_name: Optional[str] = None
        self.song_artists: List[dict] = []
        self.song_artist_names: List[str] = []
        self.window_name: Optional[str] = None
        self.album_pic_url: Optional[str] = None
        self.have_loaded: bool = False

    def load_song_detail(self) -> None:
        """加载歌曲详情"""
        if self.have_loaded:
            return
            
        if self.mystery_mode:
            # 神秘模式不需要加载详情
            self.song_name = "秘密歌曲"
            self.song_artist_names = ["??????????"]
            self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
            self.album_pic_url = self.mystery_pic_url
            self.have_loaded = True
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
                self.song_name = song_info.get("name", "未知")
                
                # 获取艺术家信息
                self.song_artists = song_info.get("artists", [])
                self.song_artist_names = [artist.get("name", "未知") for artist in self.song_artists]
                
                # 构建窗口名
                self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
                
                # 获取专辑信息
                album = song_info.get("album", {})
                self.album_pic_url = album.get("blurPicUrl", self.mystery_pic_url)
                
                self.have_loaded = True
            else:
                raise ValueError("无法获取歌曲详情")
                
        except Exception as e:
            print(f"歌曲详情加载失败: {e}")
            self._set_error_defaults()

    def _set_error_defaults(self) -> None:
        """设置错误默认值为未知"""
        self.song_name = "未知"
        self.song_artist_names = ["未知艺术家"]
        self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
        self.album_pic_url = self.mystery_pic_url
        self.have_loaded = False

    def get_id(self) -> int:
        return self.song_id

    def get_name(self) -> str:
        if self.mystery_mode:
            return "秘密歌曲"
        return self.song_name or "未知"
    
    def get_artist_names(self) -> List[str]:
        if self.mystery_mode:
            return ["??????????"]
        return self.song_artist_names or ["未知艺术家"]

    def get_window_name(self) -> str:
        return self.window_name or ""

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


# 测试代码
if __name__ == '__main__':
    import time
    import pygetwindow as gw
    
    song = SongCard(2121980421)
    song.load_song_detail()
    
    print(f"歌曲ID: {song.get_id()}")
    print(f"歌曲名: {song.get_name()}")
    print(f"艺术家: {song.get_artist_names()}")
    print(f"窗口名: {song.get_window_name()}")
    print(f"专辑封面: {song.get_album_pic_url()}")
    print(f"Scheme URL: {song.get_scheme_url()}")
