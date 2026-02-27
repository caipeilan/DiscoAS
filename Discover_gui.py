"""
DiscoverASong - 音乐选择器GUI (极简透明浮窗)

- 全屏透明浮窗，无边框
- 只显示歌曲卡片和关闭按钮
- 缓存机制：预加载下一批歌曲
"""

import sys
import os
import threading
import time
from typing import Optional, List

from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QPushButton, QScrollArea, QFrame, QGridLayout,
    QMenu, QSystemTrayIcon
)
from PyQt6.QtGui import QPixmap, QImage, QIcon, QFont, QAction, QKeySequence, QShortcut, QPainter, QBrush, QColor, QPalette
from PyQt6.QtWidgets import QGraphicsDropShadowEffect
from PyQt6.QtCore import Qt, QThread, pyqtSignal, QTimer, QTimerEvent, QObject, QPropertyAnimation, QEasingCurve
from PyQt6.QtNetwork import QNetworkAccessManager, QNetworkRequest
from PyQt6.QtCore import QUrl

# 添加项目根目录到路径
sys.path.append(os.path.dirname(__file__))
sys.path.append(os.path.join(os.path.dirname(__file__), 'settings'))

# 全局引用，用于托盘控制
_main_window = None
_tray_icon = None
_shortcut_enabled = True
_hotkey_id = None
_shortcut_action = None  # 托盘菜单中的快捷键开关项
_network_manager = None  # 全局网络管理器

# 全局图片缓存 {url: bytes}
_image_cache = {}

# 全局 discover_app 实例，用于预加载
_global_discover_app_instance = None

# 预加载线程
_preload_thread = None


def preload_images_background(discover_app):
    """在后台线程预加载图片"""
    global _preload_thread
    
    if _preload_thread and _preload_thread.is_alive():
        print("预加载线程已在运行中，跳过")
        return
    
    def _preload():
        import requests
        try:
            print("开始预加载图片...")
            # 获取歌曲列表
            songs = discover_app.discover_songs()
            # 加载详情
            for song in songs:
                if not song.have_loaded:
                    song.load_song_detail()
            # 下载图片
            count = 0
            for song in songs:
                url = song.get_album_pic_url()
                if url and url not in _image_cache:
                    try:
                        response = requests.get(url, timeout=10)
                        if response.status_code == 200:
                            _image_cache[url] = response.content
                            count += 1
                    except:
                        pass
            print(f"预加载完成，新增 {count} 张图片，缓存共 {len(_image_cache)} 张")
        except Exception as e:
            print(f"预加载失败: {e}")
    
    _preload_thread = threading.Thread(target=_preload, daemon=True)
    _preload_thread.start()


class ImageLoader(QObject):
    """使用requests异步加载图片"""
    image_loaded = pyqtSignal(str, QPixmap)
    load_failed = pyqtSignal(str)
    
    def __init__(self, url: str, parent=None):
        super().__init__(parent)
        self.url = url
        self._thread = None
        
    def load(self):
        # 使用线程加载图片
        self._thread = threading.Thread(target=self._load_image, daemon=True)
        self._thread.start()
        
    def _load_image(self):
        import requests
        try:
            response = requests.get(self.url, timeout=10)
            if response.status_code == 200:
                pixmap = QPixmap()
                if pixmap.loadFromData(response.content):
                    self.image_loaded.emit(self.url, pixmap)
                else:
                    self.load_failed.emit(self.url)
            else:
                self.load_failed.emit(self.url)
        except Exception as e:
            print(f"图片加载异常: {self.url} - {e}")
            self.load_failed.emit(self.url)


class SongCardWidget(QFrame):
    """歌曲卡片widget"""
    
    play_requested = pyqtSignal(object)  # 发送歌曲对象
    
    def __init__(self, song_card, index: int, gui_setting=None, preloaded_pixmap=None, parent=None, card_size=1.0):
        super().__init__(parent)
        self.song_card = song_card
        self.index = index
        self.image_loaded = False
        self.current_pixmap: Optional[QPixmap] = None
        self.gui_setting = gui_setting
        self.image_loader = None
        self.card_size = card_size
        
        self._setup_ui()
        
        # 如果有预加载的图片，直接使用
        if preloaded_pixmap:
            self.current_pixmap = preloaded_pixmap
            # setScaledContents(True) 会让图片自动缩放填满整个 QLabel
            self.cover_label.setPixmap(preloaded_pixmap)
            self.image_loaded = True
        else:
            self._load_cover_image()
        
    def _get_card_style(self):
        """获取卡片样式"""
        if self.gui_setting:
            # 根据night_mode获取配置
            if self.gui_setting.night_mode:
                card_config = self.gui_setting.card_night_mode
            else:
                card_config = self.gui_setting.card
        else:
            # 默认配置
            card_config = {
                "background": "#FFFFFF",
                "background_hover": "#d0ebf0",
                "font_color": "#000000"
            }
        
        bg = card_config.get("background", "#FFFFFF")
        bg_hover = card_config.get("background_hover", "#d0ebf0")
        
        return f"""
            QFrame {{
                background-color: {bg};
                border: none;
                border-radius: 16px;
            }}
            QFrame:hover {{
                background-color: {bg_hover};
            }}
        """
    
    def _get_font_color(self):
        """获取字体颜色"""
        if self.gui_setting:
            if self.gui_setting.night_mode:
                return self.gui_setting.card_night_mode.get("font_color", "#ffffff")
            else:
                return self.gui_setting.card.get("font_color", "#000000")
        return "#000000"
        
    def _get_secondary_font_color(self):
        """获取次级字体颜色（作者名）"""
        if self.gui_setting:
            if self.gui_setting.night_mode:
                color = self.gui_setting.card_night_mode.get("font_color", "#ffffff")
            else:
                color = self.gui_setting.card.get("font_color", "#000000")
            # 使用半透明
            if color.startswith("#") and len(color) == 7:
                r = int(color[1:3], 16)
                g = int(color[3:5], 16)
                b = int(color[5:7], 16)
                return f"rgba({r}, {g}, {b}, 0.7)"
        return "rgba(0, 0, 0, 0.7)"
        
    def _setup_ui(self):
        """设置UI"""
        # 基础尺寸（1.0时的值）
        base_width = 200
        base_height = 280
        base_cover_size = 170
        
        # 应用 card_size 缩放
        width = int(base_width * self.card_size)
        height = int(base_height * self.card_size)
        cover_size = int(base_cover_size * self.card_size)
        
        self.setFixedSize(width, height)
        self.setFrameStyle(QFrame.Shape.NoFrame)
        
        # 布局 - 内部间距也按比例缩放，缩小间距
        spacing = int(8 * self.card_size)  # 从 12 缩小到 8
        margin = int(10 * self.card_size)  # 从 15 缩小到 10
        layout = QVBoxLayout(self)
        layout.setSpacing(spacing)
        layout.setContentsMargins(margin, margin, margin, margin)
        
        font_color = self._get_font_color()
        
        # 封面 - 添加右下角阴影
        self.cover_label = QLabel()
        self.cover_label.setFixedSize(cover_size, cover_size)
        self.cover_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.cover_label.setScaledContents(True)  # 改为True让图片铺满整个区域
        # 强制设置透明背景
        self.cover_label.setAutoFillBackground(False)
        palette = self.cover_label.palette()
        palette.setColor(QPalette.ColorRole.Window, Qt.GlobalColor.transparent)
        self.cover_label.setPalette(palette)
        # 圆角也按比例缩放
        radius = int(12 * self.card_size)
        self.cover_label.setStyleSheet(f"""
            background-color: rgba(200, 200, 200, 0.3);
            border-radius: {radius}px;
        """)
        
        # 添加右下角阴影
        shadow = QGraphicsDropShadowEffect()
        shadow.setBlurRadius(int(15 * self.card_size))
        shadow.setOffset(int(10 * self.card_size), int(10 * self.card_size))
        shadow.setColor(QColor(0, 0, 0, 80))  # 半透明黑色
        self.cover_label.setGraphicsEffect(shadow)
        
        layout.addWidget(self.cover_label)
        
        # 歌曲名 - 字体调小，随 card_size 缩放
        self.name_label = QLabel(self.song_card.get_name())
        self.name_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.name_label.setWordWrap(True)
        font = QFont()
        font.setBold(True)
        font.setPointSize(int(10 * self.card_size))  # 从 11 改为随 card_size 缩放
        self.name_label.setFont(font)
        # 强制设置透明背景
        self.name_label.setAutoFillBackground(False)
        palette = self.name_label.palette()
        palette.setColor(QPalette.ColorRole.Window, Qt.GlobalColor.transparent)
        self.name_label.setPalette(palette)
        self.name_label.setStyleSheet(f"color: {font_color}; background-color: transparent; border: none;")
        layout.addWidget(self.name_label)
        
        # 艺术家 - 字体调小，随 card_size 缩放
        artist_names = self.song_card.get_artist_names()
        secondary_color = self._get_secondary_font_color()
        self.artist_label = QLabel("/".join(artist_names))
        self.artist_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.artist_label.setWordWrap(True)
        font_small = QFont()
        font_small.setPointSize(int(8 * self.card_size))  # 从 9 改为随 card_size 缩放
        self.artist_label.setFont(font_small)
        # 强制设置透明背景
        self.artist_label.setAutoFillBackground(False)
        palette = self.artist_label.palette()
        palette.setColor(QPalette.ColorRole.Window, Qt.GlobalColor.transparent)
        self.artist_label.setPalette(palette)
        self.artist_label.setStyleSheet(f"color: {secondary_color}; background-color: transparent; border: none;")
        layout.addWidget(self.artist_label)
        
        # 样式 - 应用配置
        self.setStyleSheet(self._get_card_style())
        
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        
    def _load_cover_image(self):
        """异步加载封面图"""
        url = self.song_card.get_album_pic_url()
        
        self.image_loader = ImageLoader(url, self)
        self.image_loader.image_loaded.connect(self._on_image_loaded)
        self.image_loader.load_failed.connect(self._on_load_failed)
        
        # 延迟加载
        QTimer.singleShot(50, self.image_loader.load)
        
    def _on_image_loaded(self, url: str, pixmap: QPixmap):
        """图片加载完成"""
        if url == self.song_card.get_album_pic_url():
            self.current_pixmap = pixmap
            # setScaledContents(True) 会让图片自动缩放填满整个 QLabel
            # 不需要手动裁剪
            self.cover_label.setPixmap(pixmap)
            self.image_loaded = True
            
    def _on_load_failed(self, url: str):
        """图片加载失败"""
        print(f"图片加载失败: {url}")
            
    def mousePressEvent(self, event):
        """鼠标点击事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            self.play_requested.emit(self.song_card)
        super().mousePressEvent(event)


class DiscoverOverlay(QMainWindow):
    """极简全屏透明浮窗主界面"""
    
    # 自定义信号：歌曲加载完成
    songs_loaded = pyqtSignal(list)
    
    # 窗口透明度属性
    window_opacity = 1.0
    
    def __init__(self, discover_app, parent=None):
        super().__init__(parent)
        self.discover_app = discover_app
        self.songs: List = []
        self.next_songs: List = []  # 缓存下一批歌曲
        self.load_thread = None
        
        # 获取GUI设置
        self.gui_setting = discover_app.gui_setting
        
        # 动画相关
        self.fade_animation = None
        
        self._setup_ui()
        
        # 连接信号
        self.songs_loaded.connect(self._on_songs_loaded)
        
        # 初始加载歌曲（显示用 + 缓存用）
        self._load_songs()
        
    def _get_close_button_style(self):
        """获取关闭按钮样式"""
        if self.gui_setting:
            # 根据night_mode获取配置
            if self.gui_setting.night_mode:
                btn_config = self.gui_setting.cancel_button_night_mode
            else:
                btn_config = self.gui_setting.cancel_button
            cancel_btn_size = self.gui_setting.cancel_button_size
        else:
            # 默认配置
            btn_config = {
                "background": "#FFFFFF",
                "background_hover": "#f5d5d0",
                "border": "#d6533e",
                "font_color": "#000000"
            }
            cancel_btn_size = 1.0
        
        bg = btn_config.get("background", "#FFFFFF")
        bg_hover = btn_config.get("background_hover", "#f5d5d0")
        font_color = btn_config.get("font_color", "#000000")
        
        # 圆角和字体大小也按比例缩放
        radius = int(25 * cancel_btn_size)
        font_size = int(20 * cancel_btn_size)
        
        return f"""
            QPushButton {{
                background-color: {bg};
                color: {font_color};
                border: none;
                border-radius: {radius}px;
                font-size: {font_size}px;
                font-weight: bold;
            }}
            QPushButton:hover {{
                background-color: {bg_hover};
            }}
        """
        
    def _setup_ui(self):
        """设置极简UI"""
        # 全屏无边框透明窗口 - 使用 FramelessWindowHint + WindowStaysOnTopHint
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint | 
            Qt.WindowType.WindowStaysOnTopHint
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_NoSystemBackground)
        
        # 不在这里调用 showFullScreen()，由 show_overlay 控制
        
        # 中央widget - 完全透明
        central = QWidget()
        central.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        central.setStyleSheet("background: transparent;")
        self.setCentralWidget(central)
        
        # 主布局 - 全屏铺满
        main_layout = QVBoxLayout(central)
        main_layout.setContentsMargins(50, 50, 50, 50)
        
        # 右上角关闭按钮 - 应用 cancel_button_size 配置
        close_btn_size = int(50 * (self.gui_setting.cancel_button_size if self.gui_setting else 1.0))
        close_btn = QPushButton("✕")
        close_btn.setFixedSize(close_btn_size, close_btn_size)
        close_btn.setStyleSheet(self._get_close_button_style())
        close_btn.clicked.connect(self._on_close)
        
        # 关闭按钮位置 - 使用QHBoxLayout
        btn_container = QWidget()
        btn_layout = QHBoxLayout(btn_container)
        btn_layout.setContentsMargins(0, 0, 0, 0)
        btn_layout.addStretch()
        btn_layout.addWidget(close_btn)
        
        # 歌曲卡片区域 - 可滚动 - 完全透明
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        scroll.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        scroll.setStyleSheet("""
            QScrollArea {
                border: none;
                background: transparent;
            }
            QScrollArea QWidget QScrollBar:vertical {
                background: rgba(255, 255, 255, 0.1);
                width: 8px;
                border-radius: 4px;
            }
            QScrollArea QWidget QScrollBar::handle:vertical {
                background: rgba(255, 255, 255, 0.3);
                border-radius: 4px;
            }
        """)
        
        # 卡片容器 - 完全透明
        self.song_container = QWidget()
        self.song_container.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.song_container.setStyleSheet("background: transparent;")
        self.song_layout = QGridLayout(self.song_container)
        # 增加间距到 45 (原 25 + 额外 20)
        self.song_layout.setSpacing(45)
        
        scroll.setWidget(self.song_container)
        
        # 垂直布局：关闭按钮在上，歌曲卡片在下面
        main_layout.addWidget(btn_container)
        main_layout.addWidget(scroll)
        
    def paintEvent(self, event):
        """完全透明背景"""
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        # 完全透明
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRect(self.rect())
        
    def _load_songs(self):
        """加载歌曲（显示用 + 缓存用）"""
        # 显示加载状态
        self._display_loading()
        
        # 使用保守的线程管理：确保上一个线程完成后再启动新线程
        if self.load_thread and self.load_thread.is_alive():
            return
            
        # 后台线程加载
        self.load_thread = threading.Thread(target=self._load_songs_async, daemon=True)
        self.load_thread.start()
        
    def _preload_images(self):
        """预加载所有歌曲封面图片到缓存"""
        import requests
        
        for song in self.songs:
            url = song.get_album_pic_url()
            print(f"图片URL: {url}")
            if url and url not in _image_cache:
                try:
                    response = requests.get(url, timeout=10)
                    if response.status_code == 200:
                        _image_cache[url] = response.content
                        print(f"预加载成功: {url[:50]}...")
                except Exception as e:
                    print(f"预加载失败: {url[:50]}... - {e}")
        
        print(f"预加载完成，共 {len(_image_cache)} 张图片")
    
    def _load_songs_async(self):
        """异步加载歌曲"""
        try:
            print("开始加载歌曲...")
            # 加载显示用歌曲
            self.songs = self.discover_app.discover_songs()
            print(f"加载到 {len(self.songs)} 首歌曲")
            
            # 先加载歌曲详情（这样才能获取到图片URL）
            for song in self.songs:
                if not song.have_loaded:
                    song.load_song_detail()
            
            # 加载下一批缓存（只加载歌曲，不下载图片）
            self.next_songs = self.discover_app.discover_songs()
            print("缓存加载完成")
            
            # 使用信号槽更新UI - 确保使用 QueuedConnection
            self.songs_loaded.emit(self.songs)
            print("信号已发送")
        except Exception as e:
            import traceback
            print(f"加载歌曲失败: {e}")
            traceback.print_exc()
            self.songs_loaded.emit([])
            
    def _on_songs_loaded(self, songs):
        """歌曲加载完成回调"""
        self.songs = songs
        self._display_songs()
        
    def _get_loading_style(self):
        """获取加载中卡片的样式"""
        if self.gui_setting:
            if self.gui_setting.night_mode:
                card_config = self.gui_setting.card_night_mode
            else:
                card_config = self.gui_setting.card
        else:
            card_config = {
                "background": "#FFFFFF",
                "border": "#76e8fd",
                "font_color": "#000000"
            }
        
        bg = card_config.get("background", "#FFFFFF")
        border = card_config.get("border", "#76e8fd")
        
        return f"""
            QFrame {{
                background-color: {bg};
                border: 1px solid {border};
                border-radius: 16px;
            }}
        """
    
    def _display_loading(self):
        """显示加载中状态"""
        # 清空现有卡片
        while self.song_layout.count():
            item = self.song_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
                
        # 创建加载中卡片容器 - 匹配新卡片高度 280
        loading_card = QFrame()
        loading_card.setFixedSize(200, 280)
        loading_card.setFrameStyle(QFrame.Shape.NoFrame)
        loading_card.setStyleSheet(self._get_loading_style())
        
        # 加载中内容布局 - 增加间距
        layout = QVBoxLayout(loading_card)
        layout.setSpacing(12)
        layout.setContentsMargins(15, 15, 15, 15)
        
        # 获取字体颜色
        font_color = self._get_font_color_for_label()
        
        # 加载中封面占位
        cover_label = QLabel()
        cover_label.setFixedSize(170, 170)
        cover_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        cover_label.setStyleSheet("""
            background-color: rgba(250, 250, 250, 0.3);
            border-radius: 12px;
        """)
        layout.addWidget(cover_label)
        
        # 加载中文字
        loading_label = QLabel("加载中...")
        loading_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        font = QFont()
        font.setBold(True)
        font.setPointSize(11)
        loading_label.setFont(font)
        loading_label.setStyleSheet(f"color: {font_color}; background-color: transparent;")
        layout.addWidget(loading_label)
        
        # 占位艺术家文字
        artist_label = QLabel("...")
        artist_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        font_small = QFont()
        font_small.setPointSize(9)
        artist_label.setFont(font_small)
        artist_label.setStyleSheet(f"color: {font_color}; background-color: transparent; opacity: 0.7;")
        layout.addWidget(artist_label)
        
        self.song_layout.addWidget(loading_card, 0, 0)
        
    def _get_font_color_for_label(self):
        """获取标签字体颜色"""
        if self.gui_setting:
            if self.gui_setting.night_mode:
                return self.gui_setting.card_night_mode.get("font_color", "#ffffff")
            else:
                return self.gui_setting.card.get("font_color", "#000000")
        return "#ffffff"
        
    def _display_songs(self):
        """显示歌曲卡片"""
        # 清空现有卡片
        while self.song_layout.count():
            item = self.song_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
        
        # 等待预加载完成（最多等2秒）
        if _preload_thread and _preload_thread.is_alive():
            print("等待预加载完成...")
            _preload_thread.join(timeout=2)
                
        columns = 5
        # 获取卡片尺寸设置
        card_size = self.gui_setting.card_size if self.gui_setting else 1.0
        
        for i, song in enumerate(self.songs):
            if not song.have_loaded:
                song.load_song_detail()
            
            # 检查是否有缓存的图片数据
            url = song.get_album_pic_url()
            cached_data = _image_cache.get(url, None)
            
            # 如果有缓存，直接使用
            if cached_data:
                pixmap = QPixmap()
                pixmap.loadFromData(cached_data)
                card = SongCardWidget(song, i, self.gui_setting, pixmap, card_size=card_size)
            else:
                # 没有缓存，让卡片自己加载
                card = SongCardWidget(song, i, self.gui_setting, card_size=card_size)
            
            card.play_requested.connect(self._on_song_play)
            
            row = i // columns
            col = i % columns
            self.song_layout.addWidget(card, row, col)
            
    def _on_song_play(self, song_card):
        """播放歌曲"""
        # 播放
        self.discover_app.play_song(song_card)
        
        # 播放后立即触发下一批预加载
        preload_images_background(self.discover_app)
        
        # 延迟隐藏窗口
        QTimer.singleShot(500, self._on_close)
        
    def _on_close(self):
        """关闭/退出时触发"""
        print("关闭窗口被触发")
        self.hide()
        
    def closeEvent(self, event):
        """窗口关闭事件"""
        print("closeEvent 被触发")
        event.ignore()  # 忽略关闭事件，只隐藏
        self.hide()
        
    def keyPressEvent(self, event):
        """键盘按键事件"""
        print(f"按键被按下: {event.key()}")
        # ESC 键不关闭窗口，直接隐藏
        if event.key() == Qt.Key.Key_Escape:
            print("ESC 被按下，隐藏窗口")
            self.hide()
            return
        super().keyPressEvent(event)
        
    def focusOutEvent(self, event):
        """窗口失去焦点事件"""
        print("窗口失去焦点")
        # 不自动隐藏，只打印日志
        super().focusOutEvent(event)


# 全局app实例，用于快捷键回调
_global_app = None
_global_discover_app = None


def create_tray_icon(app, discover_app):
    """创建系统托盘"""
    global _tray_icon, _main_window, _shortcut_enabled, _shortcut_action, _global_app, _global_discover_app
    
    # _main_window 应该是 DiscoverOverlay 窗口对象，不是 discover_app
    # _main_window 在 show_overlay 中设置
    _global_app = app
    _global_discover_app = discover_app
    
    # 创建托盘图标
    tray = QSystemTrayIcon()
    
    # 尝试加载图标，如果失败使用默认
    icon_path = os.path.join(os.path.dirname(__file__), "src", "Icon.ico")
    if os.path.exists(icon_path):
        tray.setIcon(QIcon(icon_path))
    else:
        # 创建一个简单的图标
        pixmap = QPixmap(64, 64)
        pixmap.fill(QColor(0, 0, 0, 0))
        p = QPainter(pixmap)
        p.setBrush(QColor(118, 232, 253))
        p.drawEllipse(8, 8, 48, 48)
        p.end()
        tray.setIcon(QIcon(pixmap))
    
    tray.setToolTip("DiscoverASong - 音乐选择器")
    
    # 创建右键菜单
    menu = QMenu()
    
    # 发现歌曲
    discover_action = QAction("🎵 发现", menu)
    discover_action.triggered.connect(lambda: show_overlay(app, discover_app))
    menu.addAction(discover_action)
    
    menu.addSeparator()
    
    # 设置
    settings_action = QAction("⚙️ 设置", menu)
    settings_action.triggered.connect(open_settings)
    menu.addAction(settings_action)
    
    menu.addSeparator()
    
    # 暂停/启用快捷键
    _shortcut_action = QAction("⏸️ 暂停快捷键", menu)
    _shortcut_action.triggered.connect(lambda: toggle_shortcut(app, discover_app))
    menu.addAction(_shortcut_action)
    
    menu.addSeparator()
    
    # 退出
    quit_action = QAction("❌ 退出", menu)
    quit_action.triggered.connect(app.quit)
    menu.addAction(quit_action)
    
    tray.setContextMenu(menu)
    
    # 左键点击显示浮窗 - 使用队列连接确保线程安全
    def on_tray_clicked(reason):
        print(f"托盘被点击, reason={reason}")
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            print("准备调用 show_overlay")
            show_overlay(app, discover_app)
    
    tray.activated.connect(on_tray_clicked)
    
    tray.show()
    _tray_icon = tray
    
    return tray


# 全局快捷键widget
_shortcut_widget = None
_shortcut_parent = None  # 隐藏的父窗口


def toggle_shortcut(app, discover_app):
    """切换快捷键启用状态"""
    global _shortcut_enabled, _shortcut_widget
    
    _shortcut_enabled = not _shortcut_enabled
    
    if _shortcut_enabled:
        # 启用快捷键 - 使用PyQt全局快捷键
        shortcut = discover_app.music_setting.shortcut_key
        if _shortcut_widget:
            _shortcut_widget.setEnabled(True)
        print(f"快捷键已启用: {shortcut}")
    else:
        # 禁用快捷键
        if _shortcut_widget:
            _shortcut_widget.setEnabled(False)
        print("快捷键已暂停")
    
    # 更新托盘菜单文字
    if _tray_icon:
        menu = _tray_icon.contextMenu()
        if menu:
            # 找到快捷键动作并更新
            for action in menu.actions():
                if "暂停" in action.text() or "启用" in action.text():
                    if _shortcut_enabled:
                        action.setText("⏸️ 暂停快捷键")
                    else:
                        action.setText("▶️ 启用快捷键")
                    break


def show_overlay(app, discover_app):
    """显示全屏浮窗"""
    global _main_window
    
    print("show_overlay 被调用")
    
    # 每次显示浮窗时，重新加载 GUI 设置，确保使用最新设置
    discover_app.gui_setting.load()
    print(f"已重新加载 GUI 设置: card_size={discover_app.gui_setting.card_size}, night_mode={discover_app.gui_setting.night_mode}")
    
    # 检查是否已有窗口显示
    if _main_window is not None and _main_window.isVisible():
        print("窗口已存在，直接显示")
        _main_window.show()
        _main_window.raise_()
        _main_window.activateWindow()
        return
    
    # 每次都创建新窗口，确保全新的歌曲列表
    _main_window = DiscoverOverlay(discover_app)
    print("DiscoverOverlay 创建完成")
    
    # 直接显示窗口（无动画）
    _main_window.show()
    _main_window.showFullScreen()
    _main_window.raise_()
    _main_window.activateWindow()
    
    print("窗口已显示")


def open_settings():
    """打开设置窗口"""
    try:
        # 获取项目根目录
        root_dir = os.path.dirname(os.path.abspath(__file__))
        settings_dir = os.path.join(root_dir, 'settings')
        
        # 保存原始工作目录
        original_cwd = os.getcwd()
        
        try:
            # 切换到settings目录，这样相对导入就能正常工作
            os.chdir(settings_dir)
            
            # 添加settings目录到path
            if settings_dir not in sys.path:
                sys.path.insert(0, settings_dir)
            
            # 导入setting_gui（它会使用相对导入找到music_setting和gui_setting）
            import setting_gui
            SettingsWindow = setting_gui.SettingsWindow
            
            settings_window = SettingsWindow()
            settings_window.show()
        finally:
            # 恢复原始工作目录
            os.chdir(original_cwd)
    except Exception as e:
        print(f"无法打开设置: {e}")
        import traceback
        traceback.print_exc()


# 全局变量用于跨线程调用
_global_app_ref = None
_global_discover_app_ref = None

# 全局信号：设置已保存，需要重新加载
_settings_changed_signal = None


class ShortcutSignalEmitter(QObject):
    """用于在主线程中触发快捷键回调的信号发射器"""
    shortcut_triggered = pyqtSignal()
    
    def __init__(self):
        super().__init__()
        self.shortcut_triggered.connect(self._on_shortcut_triggered)
    
    def _on_shortcut_triggered(self):
        """在主线程中执行的回调"""
        if _global_app_ref and _global_discover_app_ref:
            show_overlay(_global_app_ref, _global_discover_app_ref)


# 全局信号发射器
_shortcut_emitter = None


def register_global_shortcut(app, discover_app, shortcut="alt+d"):
    """注册全局快捷键 - 使用keyboard库"""
    global _shortcut_enabled, _global_app_ref, _global_discover_app_ref, _shortcut_emitter, _shortcut_parent, _shortcut_widget
    
    # 保存全局引用
    _global_app_ref = app
    _global_discover_app_ref = discover_app
    
    # 创建信号发射器（在主线程中创建）
    _shortcut_emitter = ShortcutSignalEmitter()
    
    try:
        import keyboard
        
        def on_shortcut():
            if _shortcut_enabled:
                print(f"快捷键 {shortcut} 被触发")
                # 通过信号机制在主线程中调用
                # PyQt 信号发射是线程安全的
                _shortcut_emitter.shortcut_triggered.emit()
        
        # 注册全局快捷键
        keyboard.add_hotkey(shortcut, on_shortcut)
        print(f"全局快捷键已注册: {shortcut}")
    except Exception as e:
        print(f"注册全局快捷键失败: {e}")
        print("将使用PyQt快捷键作为后备方案")
        # 后备：使用 PyQt 的全局快捷键
        # 创建一个不可见但可接收事件的窗口
        _shortcut_parent = QWidget()
        _shortcut_parent.setWindowTitle("ShortcutHost")
        # 使用 FramelessWindowHint 和 始终在最前，但不显示
        _shortcut_parent.setWindowFlags(
            Qt.WindowType.FramelessWindowHint |
            Qt.WindowType.WindowStaysOnTopHint |
            Qt.WindowType.Tool
        )
        # 将窗口移到屏幕外但保持活动状态
        _shortcut_parent.move(-10000, -10000)
        _shortcut_parent.show()
        _shortcut_parent.hide()  # 立即隐藏但窗口仍然存在
        
        from PyQt6.QtGui import QKeySequence
        _shortcut_widget = QShortcut(QKeySequence("Alt+D"), _shortcut_parent)
        _shortcut_widget.activated.connect(lambda: show_overlay(app, discover_app))
        print(f"PyQt全局快捷键已注册: Alt+D")


def run_gui():
    """运行GUI模式"""
    from main import DiscoverApp
    
    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)
    
    # 创建应用实例
    discover_app = DiscoverApp()
    
    # 程序启动时立即开始预加载
    preload_images_background(discover_app)
    
    # 创建托盘
    create_tray_icon(app, discover_app)
    
    # 注册全局快捷键
    try:
        register_global_shortcut(app, discover_app, discover_app.music_setting.shortcut_key)
    except:
        register_global_shortcut(app, discover_app, "Alt+D")
    
    # 运行应用
    sys.exit(app.exec())


if __name__ == "__main__":
    run_gui()
