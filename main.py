import sys
import os
import webbrowser
import threading
import time
from typing import Optional

# 添加项目根目录到路径
sys.path.append(os.path.dirname(__file__))

from Discover import DiscoverASong
from settings.music_setting import PASetting
from settings.gui_setting import GuiSetting

# GUI导入（可选，如果PyQt6未安装则跳过）
try:
    from PyQt6.QtWidgets import QApplication, QSystemTrayIcon, QMenu
    from PyQt6.QtGui import QIcon, QAction
    from PyQt6.QtCore import QTimer
    GUI_AVAILABLE = True
except ImportError:
    GUI_AVAILABLE = False
    print("警告: PyQt6未安装，GUI功能不可用")


class DiscoverApp:
    """DiscovAS应用主类"""

    def __init__(self):
        # 加载设置
        self.music_setting = PASetting()
        self.music_setting.load()
        
        self.gui_setting = GuiSetting()
        self.gui_setting.load()
        
        # 启动时自动更新启用的歌单
        self._update_enabled_playlist()
        
        # 应用设置
        self._apply_settings()
    
    def _update_enabled_playlist(self) -> None:
        """启动时自动更新唯一被启用的歌单"""
        # 查找启用的歌单
        enabled_playlist = None
        for pl in self.music_setting.playlist_albums:
            if pl.enabled:
                enabled_playlist = pl
                break
        
        if not enabled_playlist:
            print("没有启用的歌单，跳过更新")
            return
        
        print(f"正在更新启用的歌单: {enabled_playlist.playlist_album_id} ({enabled_playlist.typename})")
        
        try:
            # 根据平台动态导入对应的 get_json 模块
            platform = enabled_playlist.name
            module_path = f"platforms.{platform}.get_json"
            get_json_module = __import__(module_path, fromlist=['PlaylistAlbumJson'])
            PlaylistAlbumJson = get_json_module.PlaylistAlbumJson
            
            # 获取并保存歌单数据
            playlist_json = PlaylistAlbumJson(
                enabled_playlist.playlist_album_id, 
                enabled_playlist.typename
            )
            playlist_json.save()
            print(f"歌单更新完成: {playlist_json.get_name()}")
        except Exception as e:
            print(f"更新歌单失败: {e}")
            import traceback
            traceback.print_exc()
        
    def _apply_settings(self) -> None:
        """应用设置"""
        # 查找启用的歌单
        enabled_playlist = None
        for pl in self.music_setting.playlist_albums:
            if pl.enabled:
                enabled_playlist = pl
                break
                
        if enabled_playlist:
            self.platform = enabled_playlist.name
            self.playlist_type = enabled_playlist.typename
            self.playlist_id = enabled_playlist.playlist_album_id
        else:
            # 默认使用网易云音乐
            self.platform = "NeteaseCloudMusic"
            self.playlist_type = "playlist"
            self.playlist_id = "8285082830"
            
    def discover_songs(self, number: Optional[int] = None) -> list:
        """
        发现歌曲
        
        Args:
            number: 指定歌曲数量，默认使用设置中的数量
            
        Returns:
            歌曲卡片列表
        """
        if number is None:
            number = self.music_setting.number_of_discovered_songs
            
        discover = DiscoverASong(self.platform, self.playlist_type, self.playlist_id)
        songs, total, mystery_count = discover.get_songs(
            number,
            self.music_setting.have_mystery_song,
            self.music_setting.num_of_mystery_song,
            False,                                 # 不再允许歌曲重复
            self.music_setting.mystery_song_cover  # 传递自定义秘密封面
        )
        
        return songs
    
    def play_song(self, song_card) -> bool:
        """
        播放歌曲
        
        Args:
            song_card: 歌曲卡片对象
            
        Returns:
            是否成功播放
        """
        try:
            # 确保加载歌曲详情
            if not song_card.have_loaded:
                song_card.load_song_detail()
                
            # 获取播放URL
            url = song_card.get_scheme_url()
            
            # 打开URL唤起音乐播放器
            webbrowser.open(url, new=0, autoraise=False)

            # 尝试最小化播放器窗口
            self._minimize_player_window(song_card)
            
            return True
        except Exception as e:
            print(f"播放失败: {e}")
            return False
            
    def _minimize_player_window(self, song_card) -> None:
        """最小化播放器窗口"""
        try:
            import pygetwindow as gw
            import time
            
            time.sleep(4)  # 等待窗口出现
            
            windows = gw.getWindowsWithTitle(song_card.get_window_name())
            if windows:
                window = windows[0]
                window.minimize()
        except Exception as e:
            print(f"最小化窗口失败: {e}")
            
    def run_gui(self) -> None:
        """GUI模式运行"""
        if not GUI_AVAILABLE:
            print("错误: GUI模式需要安装PyQt6")
            print("请运行: pip install PyQt6")
            return
            
        # 导入GUI模块
        from Discover_gui import run_gui
        
        run_gui()


def handle_scheme_url(url: str) -> bool:
    """
    处理Scheme URL调用
    
    Args:
        url: scheme URL
        
    Returns:
        是否成功处理
    """
    # 解析URL参数
    try:
        from urllib.parse import urlparse, parse_qs
        
        parsed = urlparse(url)
        
        # 检查是否是DiscoverASong的scheme
        if parsed.scheme == "discoverasong":
            params = parse_qs(parsed.query)
            
            # 创建应用实例
            app = DiscoverApp()
            
            if "action" in params:
                action = params["action"][0]
                
                if action == "discover":
                    # 发现歌曲
                    number = int(params.get("count", [3])[0])
                    songs = app.discover_songs(number)
                    
                    print(f"发现 {len(songs)} 首歌曲:")
                    for i, song in enumerate(songs):
                        song.load_song_detail()
                        print(f"  {i+1}. {song.get_name()} - {'/'.join(song.get_artist_names())}")
                    return True
                    
                elif action == "play":
                    # 直接播放指定歌曲
                    song_index = int(params.get("song", [0])[0])
                    songs = app.discover_songs()
                    if 0 <= song_index < len(songs):
                        app.play_song(songs[song_index])
                        return True
                        
        return False
        
    except Exception as e:
        print(f"处理URL失败: {e}")
        return False


def main():
    """主入口函数"""
    # 检查是否是scheme URL
    if len(sys.argv) > 1 and sys.argv[1].startswith("discoverasong://"):
        handle_scheme_url(sys.argv[1])
        return
    
    # 默认启动GUI模式
    if not GUI_AVAILABLE:
        print("错误: GUI模式需要安装PyQt6")
        print("请运行: pip install PyQt6")
        return
        
    app = DiscoverApp()
    app.run_gui()


if __name__ == "__main__":
    main()
