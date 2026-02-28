"""
QQ音乐歌曲卡片模块 - 使用签名算法

基于 qqmusic-api-python 库的签名算法实现
"""

import webbrowser
import requests
from typing import List, Optional

# 延迟导入签名模块
def _get_make_api_request():
    import sys
    import importlib
    
    # 清除已缓存的qq_sign模块
    mods_to_remove = [k for k in sys.modules.keys() if 'qq_sign' in k]
    for mod in mods_to_remove:
        del sys.modules[mod]
    
    # 重新导入
    import platforms.QQMusic.qq_sign as qq_sign_module
    return qq_sign_module.make_api_request


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
        """加载歌曲详情，失败则使用默认值"""
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
            # 使用新的 API 获取歌曲详情（使用正确的module和method）
            params = {
                "types": [0],  # 0 表示用 ID 查询
                "ids": [self.song_id],
                "modify_stamp": [0],
                "ctx": 0,
                "client": 1,
            }
            
            # 正确的API调用方式（延迟导入）
            # 直接调用qq_sign模块的函数，绕过可能的导入问题
            import platforms.QQMusic.qq_sign as qs
            api_result = qs.make_api_request("music.trackInfo.UniformRuleCtrl", "CgiGetTrackInfo", params)
            
            # API返回结构: {"music.trackInfo.UniformRuleCtrl": {"code": 0, "data": {"tracks": [...]}}}
            tracks = []
            if isinstance(api_result, dict):
                # 获取嵌套的module结果
                module_result = api_result.get("music.trackInfo.UniformRuleCtrl", {})
                if isinstance(module_result, dict):
                    data = module_result.get("data", {})
                    if isinstance(data, dict):
                        tracks = data.get("tracks", [])
            
            if tracks and len(tracks) > 0:
                song_info = tracks[0]
                
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
                self._set_error_defaults()
                
        except Exception as e:
            print(f"歌曲详情加载失败: {e}")
            # API失败时使用默认值（基于歌曲ID）
            self._set_default_from_id()
    
    def _set_default_from_id(self) -> None:
        """根据歌曲ID设置默认值"""
        self.song_name = f"歌曲{self.song_id}"
        self.song_artist_names = ["未知艺术家"]
        self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
        # 使用默认封面
        self.album_pic_url = self.mystery_pic_url
        self.have_loaded = True

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
