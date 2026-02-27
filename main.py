"""
DiscoverASong - 音乐选择器主程序

通过Scheme URL协议唤起本地音乐播放器，从歌单中随机挑选歌曲供用户选择
"""

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
    """DiscoverASong应用主类"""

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
            self.music_setting.overlap  # 传递允许重复设置
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
            
            time.sleep(2)  # 等待窗口出现
            
            windows = gw.getWindowsWithTitle(song_card.get_window_name())
            if windows:
                window = windows[0]
                window.minimize()
        except Exception as e:
            print(f"最小化窗口失败: {e}")
            
    def run_cli(self) -> None:
        """命令行模式运行"""
        print("=== DiscoverASong 音乐选择器 ===\n")
        
        # 发现歌曲
        print("正在发现歌曲...")
        songs = self.discover_songs()
        
        # 显示歌曲列表
        print("\n发现的歌曲:")
        for i, song in enumerate(songs):
            song.load_song_detail()
            print(f"{i}. {song.get_name()} - {'/'.join(song.get_artist_names())}")
            
        # 用户选择
        while True:
            try:
                choice = input("\n请选择要播放的歌曲序号 (输入q退出): ")
                if choice.lower() == 'q':
                    break
                    
                index = int(choice)
                if 0 <= index < len(songs):
                    self.play_song(songs[index])
                else:
                    print("无效的序号")
            except ValueError:
                print("请输入有效的数字")
            except Exception as e:
                print(f"错误: {e}")
                
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
    # 检查命令行参数
    if len(sys.argv) > 1:
        # 检查是否是scheme URL
        if sys.argv[1].startswith("discoverasong://"):
            handle_scheme_url(sys.argv[1])
            return
            
        # 检查命令行模式
        if sys.argv[1] == "--cli":
            app = DiscoverApp()
            app.run_cli()
            return
            
    # 默认尝试GUI模式
    if GUI_AVAILABLE:
        app = DiscoverApp()
        app.run_gui()
    else:
        # 回退到命令行模式
        print("PyQt6未安装，使用命令行模式...")
        app = DiscoverApp()
        app.run_cli()


if __name__ == "__main__":
    main()
