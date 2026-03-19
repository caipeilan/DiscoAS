import sys
import os
import webbrowser
import threading
import time
import logging
import traceback
import pygetwindow as gw
from typing import Optional

# 添加项目根目录到路径
sys.path.append(os.path.dirname(__file__))

# 导入日志模块（必须在其他模块之前初始化）
# 这会自动全局替换 print 函数，使所有模块的 print 都写入日志
from log import logger

from Discover import DiscoverASong
from settings.music_setting import PASetting
from settings.gui_setting import GuiSetting

# GUI导入
try:
    from PyQt6.QtWidgets import QApplication, QSystemTrayIcon, QMenu, QWidget, QLabel
    from PyQt6.QtGui import QIcon, QAction, QPixmap
    from PyQt6.QtCore import QTimer, Qt, QPropertyAnimation, QEasingCurve, QByteArray, QEventLoop
    GUI_AVAILABLE = True
except ImportError:
    GUI_AVAILABLE = False
    print("警告: PyQt6未安装")


class SplashScreen(QWidget):
    """启动画面：全屏浮动窗口显示图片，快速淡入淡出"""
    
    def __init__(self, image_path: str, fade_duration: int = 750, show_duration: int = 500):
        """
        初始化启动画面
        
        Args:
            image_path: 图片路径
            fade_duration: 淡入淡出持续时间(毫秒)
            show_duration: 图片显示持续时间(毫秒)
        """
        super().__init__()
        self.fade_duration = fade_duration
        self.show_duration = show_duration
        self.animation_finished = False
        self.qApp = QApplication.instance()
        self.image_path = image_path
        
        # 设置窗口属性：无边框、置顶、不在任务栏显示
        # 使用 FramelessWindowHint 确保无边框，Tool 隐藏任务栏图标
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint | 
            Qt.WindowType.WindowStaysOnTopHint |
            Qt.WindowType.Tool
        )
        
        # 设置背景完全透明（必须在窗口显示之前设置）
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_NoSystemBackground)
        
        # 先创建标签并加载图片
        self.label = QLabel(self)
        self.label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        # 加载图片
        self.load_image(image_path)
        
        # 全屏显示
        self.showFullScreen()
        
        # 确保标签在窗口中央
        self.label.setGeometry(self.rect())
        
        # 初始显示（透明度0），然后淡入
        self.setWindowOpacity(0)
        
        # 强制刷新显示
        self.qApp.processEvents()
        
        # 开始淡入动画（从1到1其实不动画，直接淡出）
        # 为了视觉效果，我们直接从1开始淡出
        self.start_animation()
    
    def load_image(self, image_path: str):
        """加载并显示图片"""
        print(f"[SplashScreen] 尝试加载图片: {image_path}")
        print(f"[SplashScreen] 文件存在: {os.path.exists(image_path)}")
        
        if os.path.exists(image_path):
            # 使用绝对路径
            abs_path = os.path.abspath(image_path)
            print(f"[SplashScreen] 绝对路径: {abs_path}")
            
            # 尝试加载图片
            pixmap = QPixmap(abs_path)
            if pixmap.isNull():
                print(f"[SplashScreen] 图片加载失败，尝试其他方式...")
                # 尝试用文件 URL
                pixmap = QPixmap(f"file:///{abs_path.replace(os.sep, '/')}")
            
            print(f"[SplashScreen] 图片是否为空: {pixmap.isNull()}")
            print(f"[SplashScreen] 图片尺寸: {pixmap.width()}x{pixmap.height()}")
            
            if not pixmap.isNull():
                # 获取屏幕尺寸
                screen = QApplication.primaryScreen()
                if screen:
                    screen_geometry = screen.geometry()
                    # 缩放到合适大小（屏幕的80%）
                    scaled = pixmap.scaled(
                        int(screen_geometry.width() * 0.45) ,
                        int(screen_geometry.height() * 0.45),
                        Qt.AspectRatioMode.KeepAspectRatio,
                        Qt.TransformationMode.SmoothTransformation
                    )
                    print(f"[SplashScreen] 缩放后尺寸: {scaled.width()}x{scaled.height()}")
                    self.label.setPixmap(scaled)
                else:
                    self.label.setPixmap(pixmap)
            else:
                print(f"[SplashScreen] 错误：无法加载图片")
        else:
            print(f"[SplashScreen] 启动画面图片不存在: {image_path}")
    
    def start_animation(self):
        """开始淡入淡出动画"""
        self.fade_in()
    
    def fade_in(self):
        """淡入动画"""
        self.anim = QPropertyAnimation(self, QByteArray(b"windowOpacity"))
        self.anim.setDuration(self.fade_duration)
        self.anim.setStartValue(0.0)
        self.anim.setEndValue(1.0)
        self.anim.setEasingCurve(QEasingCurve.Type.InOutQuad)
        self.anim.finished.connect(self.fade_out)
        self.anim.start()
    
    def fade_out(self):
        """淡出动画"""
        QTimer.singleShot(self.show_duration, self._do_fade_out)
    
    def _do_fade_out(self):
        """执行淡出"""
        self.anim = QPropertyAnimation(self, QByteArray(b"windowOpacity"))
        self.anim.setDuration(self.fade_duration)
        self.anim.setStartValue(1.0)
        self.anim.setEndValue(0.0)
        self.anim.setEasingCurve(QEasingCurve.Type.InOutQuad)
        self.anim.finished.connect(self._on_animation_finished)
        self.anim.start()
    
    def _on_animation_finished(self):
        """动画全部完成"""
        self.animation_finished = True
        self.close()
    
    def wait_for_finish(self):
        """等待动画完成（非阻塞方式，允许事件循环处理后台任务）"""
        # 使用 QEventLoop 而非 time.sleep，让事件循环可以处理后台任务
        loop = QEventLoop()
        # 动画总时长后退出
        QTimer.singleShot(self.fade_duration * 2 + self.show_duration, loop.quit)
        loop.exec()


def get_splash_image_path() -> str:
    """获取启动画面图片路径，支持打包后环境"""
    # 先尝试常规路径
    image_path = os.path.join(os.path.dirname(__file__), "src", "DiscoAS.png")
    if os.path.exists(image_path):
        return image_path
    
    # 尝试打包后的路径
    try:
        from settings.user_data_path import get_resource_dir
        image_path = os.path.join(get_resource_dir(), "src", "DiscoAS.png")
        if os.path.exists(image_path):
            return image_path
    except ImportError:
        pass
    
    return ""


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
            traceback.print_exc()
            # 弹窗提示用户
            try:
                from settings.i18n import gettext as _
                from PyQt6.QtWidgets import QMessageBox
                from PyQt6.QtCore import QApplication
                app = QApplication.instance()
                if app:
                    QMessageBox.critical(
                        None,
                        _("playlist_load_failed"),
                        _("playlist_load_failed_msg").format(error=str(e))
                    )
            except:
                pass
        
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

            time.sleep(4)  # 等待窗口出现

            title_prefix = song_card.get_window_name()
            print(f"[调试] 前缀查找窗口: {title_prefix}")
            all_windows = gw.getAllWindows()
            windows = [w for w in all_windows if w.title[:len(title_prefix)].lower() == title_prefix.lower()] if title_prefix else []
            print(f"[调试] 找到 {len(windows)} 个匹配窗口")
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

    # 运行 GUI（Discover_gui.run_gui 会处理所有初始化）
    from Discover_gui import run_gui
    run_gui()


if __name__ == "__main__":
    main()
