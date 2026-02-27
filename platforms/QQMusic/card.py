"""
QQ音乐歌曲卡片模块 - 纯HTTP实现

使用同步requests库替代异步第三方库，提升响应速度
"""

import webbrowser
import requests
from typing import List, Optional

# 导入共享的Session
from .get_json import get_session


class SongCard:
    """QQ音乐歌曲卡片类"""

    # 默认神秘歌曲封面
    DEFAULT_MYSTERY_PIC = "https://y.qq.com/music/photo_new/T002R300x300M000004RT1Bi1Ee6r5_1.jpg"

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
        self.album_mid: Optional[str] = None
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
            # 直接通过ID获取歌曲信息
            song_info = self._fetch_song_info_by_id()
            
            if song_info:
                self.song_detail_json = song_info
                self.song_name = song_info.get("name", "未知")
                
                # 获取艺术家信息
                self.song_artists = song_info.get("singer", [])
                self.song_artist_names = [artist.get("name", "未知") for artist in self.song_artists]
                
                # 构建窗口名
                self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
                
                # 获取专辑信息
                album = song_info.get("album", {})
                self.album_mid = album.get("mid", "")
                
                # 构建专辑封面URL
                if self.album_mid:
                    self.album_pic_url = f"https://y.qq.com/music/photo_new/T002R300x300M000{self.album_mid}_1.jpg"
                else:
                    self.album_pic_url = self.mystery_pic_url
                
                self.have_loaded = True
            else:
                # 备用方案：尝试通过songmid获取
                song_mid = self._get_song_mid_from_id()
                self._load_by_mid(song_mid)
                
        except Exception as e:
            print(f"歌曲详情加载失败: {e}")
            self._set_error_defaults()
    
    def _load_by_mid(self, song_mid: str) -> None:
        """通过songmid加载歌曲详情"""
        try:
            session = get_session()
            
            url = "https://c.y.qq.com/v8/fcg-bin/fcg_play_single_song.fcg"
            params = {
                "songmid": song_mid,
                "format": "json",
                "inCharset": "utf8",
                "outCharset": "utf8",
                "notice": 0,
                "platform": "yqq.json",
                "needNewCode": 0,
            }
            
            response = session.get(url, params=params, timeout=10)
            data = response.json()
            
            if "data" in data and len(data["data"]) > 0:
                song_info = data["data"][0]
                
                self.song_detail_json = song_info
                self.song_name = song_info.get("name", "未知")
                
                self.song_artists = song_info.get("singer", [])
                self.song_artist_names = [artist.get("name", "未知") for artist in self.song_artists]
                
                self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
                
                album = song_info.get("album", {})
                self.album_mid = album.get("mid", "")
                
                if self.album_mid:
                    self.album_pic_url = f"https://y.qq.com/music/photo_new/T002R300x300M000{self.album_mid}_1.jpg"
                else:
                    self.album_pic_url = self.mystery_pic_url
                
                self.have_loaded = True
            else:
                self._set_error_defaults()
                
        except Exception as e:
            print(f"通过mid加载歌曲详情失败: {e}")
            self._set_error_defaults()

    def _get_song_mid_from_id(self) -> str:
        """
        通过歌曲ID获取songmid
        
        Returns:
            歌曲的mid字符串
        """
        try:
            session = get_session()
            
            # 使用QQ音乐歌曲详情API，通过songmid参数传递歌曲ID
            # 但我们需要先获取songmid，所以这里用另一种方式
            # 调用歌曲搜索接口或详情接口
            url = "https://c.y.qq.com/v8/fcg-bin/fcg_v8_song_detail.fcg"
            params = {
                "songmid": "",  # 先留空
                "songid": self.song_id,
                "format": "json",
                "inCharset": "utf8",
                "outCharset": "utf8",
                "notice": 0,
                "platform": "yqq.json",
                "needNewCode": 0,
            }
            
            response = session.get(url, params=params, timeout=10)
            data = response.json()
            
            if "data" in data and "song" in data["data"]:
                song_info = data["data"]["song"]
                return song_info.get("mid", str(self.song_id))
                
            return str(self.song_id)
            
        except Exception as e:
            print(f"获取songmid失败，使用备用方案: {e}")
            return str(self.song_id)
    
    def _fetch_song_info_by_id(self) -> dict:
        """
        通过歌曲ID直接获取歌曲信息（包含mid）
        
        Returns:
            歌曲信息字典
        """
        try:
            session = get_session()
            
            # 使用歌曲详情接口
            url = "https://c.y.qq.com/v8/fcg-bin/fcg_v8_song_detail.fcg"
            params = {
                "songmid": "",  
                "songid": self.song_id,
                "format": "json",
                "inCharset": "utf8",
                "outCharset": "utf8", 
                "notice": 0,
                "platform": "yqq.json",
                "needNewCode": 0,
            }
            
            response = session.get(url, params=params, timeout=10)
            data = response.json()
            
            if "data" in data and "song" in data["data"]:
                return data["data"]["song"]
                
            return {}
            
        except Exception as e:
            print(f"通过ID获取歌曲信息失败: {e}")
            return {}

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
        """生成QQ音乐scheme URL用于唤起播放"""
        url = f"tencent://QQMusic/?version==1173&&cmd_count==1&&cmd_0==playsong&&id_0=={self.song_id}&&songtype_0==0"
        return url


# 测试代码
if __name__ == '__main__':
    import time
    import pygetwindow as gw
    
    song = SongCard(284218927)
    song.load_song_detail()
    
    print(f"歌曲ID: {song.get_id()}")
    print(f"歌曲名: {song.get_name()}")
    print(f"艺术家: {song.get_artist_names()}")
    print(f"窗口名: {song.get_window_name()}")
    print(f"专辑封面: {song.get_album_pic_url()}")
    print(f"Scheme URL: {song.get_scheme_url()}")
    
    # 尝试打开QQ音乐
    # webbrowser.open(song.get_scheme_url(), new=0, autoraise=False)
    # time.sleep(2.5)
    # windows = gw.getWindowsWithTitle(song.get_window_name())
    # if windows:
    #     window = windows[0]
    #     window.minimize()
