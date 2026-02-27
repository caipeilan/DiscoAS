"""
DiscoverASong - 音乐选择器GUI (Kardo风格全屏浮窗)

- 全屏透明浮窗，无边框
- 毛玻璃背景效果
- 系统托盘 + 全局快捷键
"""

import sys
import os
import threading
import webbrowser
from typing import Optional, List

from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QPushButton, QScrollArea, QFrame, QGridLayout,
    QMenu, QSystemTrayIcon
)
from PyQt6.QtGui import QPixmap, QImage, QIcon, QFont, QAction, QKeySequence, QShortcut
from PyQt6.QtCore import Qt, QThread, pyqtSignal, QTimer, QUrl, QSize, QTimerEvent

# 添加项目根目录到路径
sys.path.append(os.path.dirname(__file__))

# 全局引用，用于托盘控制
_main_window = None
_tray_icon = None
_shortcut_enabled = True
_hotkey_id = None


class ImageLoaderThread(QThread):
    """图片异步加载线程"""
    image_loaded = pyqtSignal(str, QImage)
    load_failed = pyqtSignal(str)
    
    def __init__(self, url: str, parent=None):
        super().__init__(parent)
        self.url = url
        
    def run(self):
        try:
            from urllib.request import urlopen
            from PIL import Image
            import io
            
            with urlopen(self.url, timeout=10) as response:
                data = response.read()
                
            img = Image.open(io.BytesIO(data))
            img = img.convert("RGBA")
            img_data = img.tobytes("raw", "RGBA")
            qimage = QImage(img_data, img.width, img.height, QImage.Format.Format_RGBA8888)
            
            self.image_loaded.emit(self.url, qimage)
            
        except Exception as e:
            self.load_failed.emit(self.url)


class SongCardWidget(QFrame):
    """歌曲卡片widget"""
    
    clicked = pyqtSignal(int)
    play_requested = pyqtSignal(object)  # 发送歌曲对象
    
    def __init__(self, song_card, index: int, parent=None):
        super().__init__(parent)
        self.song_card = song_card
        self.index = index
        self.image_loaded = False
        self.current_image: Optional[QImage] = None
        self.is_playing = False
        
        self._setup_ui()
        self._load_cover_image()
        
    def _setup_ui(self):
        """设置UI"""
        self.setFixedSize(200, 260)
        self.setFrameStyle(QFrame.Shape.NoFrame)
        
        # 布局
        layout = QVBoxLayout(self)
        layout.setSpacing(8)
        layout.setContentsMargins(15, 15, 15, 15)
        
        # 封面
        self.cover_label = QLabel()
        self.cover_label.setFixedSize(170, 170)
        self.cover_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.cover_label.setStyleSheet("""
            background-color: rgba(255, 255, 255, 0.1);
            border-radius: 12px;
        """)
        layout.addWidget(self.cover_label)
        
        # 歌曲名
        self.name_label = QLabel(self.song_card.get_name())
        self.name_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.name_label.setWordWrap(True)
        font = QFont()
        font.setBold(True)
        font.setPointSize(11)
        self.name_label.setFont(font)
        self.name_label.setStyleSheet("color: white;")
        layout.addWidget(self.name_label)
        
        # 艺术家
        artist_names = self.song_card.get_artist_names()
        self.artist_label = QLabel("/".join(artist_names))
        self.artist_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.artist_label.setWordWrap(True)
        font_small = QFont()
        font_small.setPointSize(9)
        self.artist_label.setFont(font_small)
        self.artist_label.setStyleSheet("color: rgba(255, 255, 255, 0.7);")
        layout.addWidget(self.artist_label)
        
        # 样式 - 毛玻璃卡片效果
        self.setStyleSheet("""
            QFrame {
                background-color: rgba(255, 255, 255, 0.15);
                border: 1px solid rgba(255, 255, 255, 0.2);
                border-radius: 16px;
            }
            QFrame:hover {
                background-color: rgba(255, 255, 255, 0.25);
                border: 2px solid rgba(255, 255, 255, 0.4);
            }
        """)
        
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        
    def _load_cover_image(self):
        """异步加载封面图"""
        url = self.song_card.get_album_pic_url()
        
        self.loader = ImageLoaderThread(url, self)
        self.loader.image_loaded.connect(self._on_image_loaded)
        self.loader.start()
        
    def _on_image_loaded(self, url: str, image: QImage):
        """图片加载完成"""
        if url == self.song_card.get_album_pic_url():
            self.current_image = image
            pixmap = QPixmap.fromImage(image).scaled(
                170, 170, 
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation
            )
            self.cover_label.setPixmap(pixmap)
            self.image_loaded = True
            
    def mousePressEvent(self, event):
        """鼠标点击事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            self.play_requested.emit(self.song_card)
        super().mousePressEvent(event)


class DiscoverOverlay(QMainWindow):
    """全屏透明浮窗主界面"""
    
    def __init__(self, discover_app, parent=None):
        super().__init__(parent)
        self.discover_app = discover_app
        self.songs: List = []
        
        self._setup_ui()
        self._apply_style()
        
        # 自动加载歌曲
        self._on_refresh_clicked()
        
    def _setup_ui(self):
        """设置UI"""
        # 全屏无边框透明窗口
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint | 
            Qt.WindowType.WindowStaysOnTopHint |
            Qt.WindowType.Tool
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_NoSystemBackground)
        
        # 全屏
        self.showFullScreen()
        
        # 中央widget - 使用透明背景
        central = QWidget()
        self.setCentralWidget(central)
        
        # 主布局
        main_layout = QVBoxLayout(central)
        main_layout.setContentsMargins(50, 80, 50, 80)
        
        # 标题栏区域
        header = QWidget()
        header.setFixedHeight(60)
        header_layout = QHBoxLayout(header)
        header_layout.setContentsMargins(0, 0, 0, 0)
        
        # 标题
        title = QLabel("🎵 随机歌曲发现")
        title.setStyleSheet("color: white; font-size: 28px; font-weight: bold;")
        header_layout.addWidget(title)
        
        header_layout.addStretch()
        
        # 关闭按钮
        close_btn = QPushButton("✕")
        close_btn.setFixedSize(40, 40)
        close_btn.setStyleSheet("""
            QPushButton {
                background-color: rgba(255, 255, 255, 0.1);
                color: white;
                border: 1px solid rgba(255, 255, 255, 0.3);
                border-radius: 20px;
                font-size: 18px;
            }
            QPushButton:hover {
                background-color: rgba(255, 100, 100, 0.5);
            }
        """)
        close_btn.clicked.connect(self.hide)
        header_layout.addWidget(close_btn)
        
        main_layout.addWidget(header)
        
        # 刷新按钮
        btn_layout = QHBoxLayout()
        btn_layout.addStretch()
        
        self.refresh_btn = QPushButton("🔄 刷新发现")
        self.refresh_btn.setFixedSize(160, 45)
        self.refresh_btn.setStyleSheet("""
            QPushButton {
                background-color: rgba(118, 232, 253, 0.3);
                color: white;
                border: 1px solid rgba(118, 232, 253, 0.5);
                border-radius: 22px;
                font-size: 16px;
            }
            QPushButton:hover {
                background-color: rgba(118, 232, 253, 0.5);
            }
        """)
        self.refresh_btn.clicked.connect(self._on_refresh_clicked)
        btn_layout.addWidget(self.refresh_btn)
        
        btn_layout.addStretch()
        main_layout.addLayout(btn_layout)
        
        # 歌曲卡片区域 - 可滚动
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        scroll.setStyleSheet("""
            QScrollArea {
                border: none;
                background: transparent;
            }
            QScrollBar:vertical {
                background: rgba(255, 255, 255, 0.1);
                width: 8px;
                border-radius: 4px;
            }
            QScrollBar::handle:vertical {
                background: rgba(255, 255, 255, 0.3);
                border-radius: 4px;
            }
        """)
        
        self.song_container = QWidget()
        self.song_layout = QGridLayout(self.song_container)
        self.song_layout.setSpacing(25)
        
        scroll.setWidget(self.song_container)
        main_layout.addWidget(scroll)
        
        # 状态栏
        self.status_label = QLabel("点击卡片播放歌曲 · 按 ESC 退出")
        self.status_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.status_label.setStyleSheet("color: rgba(255, 255, 255, 0.6); font-size: 14px;")
        main_layout.addWidget(self.status_label)
        
    def _apply_style(self):
        """应用毛玻璃样式"""
        self.setStyleSheet("""
            QMainWindow {
                background-color: transparent;
            }
        """)
        
    def paintEvent(self, event):
        """绘制毛玻璃背景"""
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        
        # 半透明深色背景
        from PyQt6.QtGui import QBrush, QColor, QPainterPath, QRadialGradient
        gradient = QRadialGradient(
            self.width() / 2, self.height() / 2, 
            max(self.width(), self.height()) * 0.8
        )
        gradient.setColorAt(0, QColor(30, 30, 50, 200))
        gradient.setColorAt(1, QColor(10, 10, 20, 220))
        painter.setBrush(QBrush(gradient))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRect(self.rect())
        
    def _on_refresh_clicked(self):
        """刷新按钮点击"""
        self.refresh_btn.setEnabled(False)
        self.status_label.setText("正在发现歌曲...")
        
        thread = threading.Thread(target=self._load_songs_async)
        thread.start()
        
    def _load_songs_async(self):
        """异步加载歌曲"""
        try:
            self.songs = self.discover_app.discover_songs()
            QTimer.singleShot(0, self._display_songs)
        except Exception as e:
            QTimer.singleShot(0, lambda: self._show_error(str(e)))
            
    def _display_songs(self):
        """显示歌曲"""
        while self.song_layout.count():
            item = self.song_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
                
        columns = 5
        for i, song in enumerate(self.songs):
            if not song.have_loaded:
                song.load_song_detail()
                
            card = SongCardWidget(song, i)
            card.play_requested.connect(self._on_song_play)
            
            row = i // columns
            col = i % columns
            self.song_layout.addWidget(card, row, col)
            
        self.status_label.setText(f"发现 {len(self.songs)} 首歌曲，点击卡片播放 · 按 ESC 退出")
        self.refresh_btn.setEnabled(True)
        
    def _on_song_play(self, song_card):
        """播放歌曲"""
        self.status_label.setText(f"正在播放: {song_card.get_name()} - {'/'.join(song_card.get_artist_names())}")
        
        # 播放
        self.discover_app.play_song(song_card)
        
        # 延迟隐藏窗口
        QTimer.singleShot(500, self.hide)
        
    def _show_error(self, message: str):
        """显示错误"""
        self.status_label.setText(f"错误: {message}")
        self.refresh_btn.setEnabled(True)
        
    def keyPressEvent(self, event):
        """键盘事件"""
        if event.key() == Qt.Key.Key_Escape:
            self.hide()
        super().keyPressEvent(event)
        
    def mousePressEvent(self, event):
        """点击窗口外部隐藏"""
        # 检查点击是否在内容区域外
        # 如果是，隐藏窗口
        super().mousePressEvent(event)


def create_tray_icon(app, discover_app):
    """创建系统托盘"""
    global _tray_icon, _main_window, _shortcut_enabled
    
    _main_window = discover_app
    
    # 创建托盘图标
    tray = QSystemTrayIcon()
    
    # 尝试加载图标，如果失败使用默认
    icon_path = os.path.join(os.path.dirname(__file__), "src", "Icon.ico")
    if os.path.exists(icon_path):
        tray.setIcon(QIcon(icon_path))
    else:
        # 创建一个简单的图标
        from PyQt6.QtGui import QPainter, QPixmap, QColor
        pixmap = QPixmap(64, 64)
        pixmap.fill(QColor(0, 0, 0, 0))
        painter = QPainter(pixmap)
        painter.setBrush(QColor(118, 232, 253))
        painter.drawEllipse(8, 8, 48, 48)
        painter.end()
        tray.setIcon(QPixmap(pixmap))
    
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
    self.shortcut_action = QAction("⏸️ 暂停快捷键", menu)
    self.shortcut_action.triggered.connect(lambda: toggle_shortcut(app, discover_app))
    menu.addAction(self.shortcut_action)
    
    menu.addSeparator()
    
    # 退出
    quit_action = QAction("❌ 退出", menu)
    quit_action.triggered.connect(app.quit)
    menu.addAction(quit_action)
    
    tray.setContextMenu(menu)
    
    # 左键点击显示浮窗
    tray.activated.connect(lambda reason: 
        show_overlay(app, discover_app) if reason == QSystemTrayIcon.ActivationReason.Trigger else None
    )
    
    tray.show()
    _tray_icon = tray
    
    return tray


def toggle_shortcut(app, discover_app):
    """切换快捷键启用状态"""
    global _shortcut_enabled, _hotkey_id
    
    _shortcut_enabled = not _shortcut_enabled
    
    if _shortcut_enabled:
        # 重新注册快捷键
        try:
            import keyboard
            shortcut = discover_app.music_setting.shortcut_key
            keyboard.add_hotkey(shortcut, lambda: show_overlay(app, discover_app))
            print(f"快捷键已启用: {shortcut}")
        except Exception as e:
            print(f"启用快捷键失败: {e}")
    else:
        # 移除快捷键
        try:
            import keyboard
            keyboard.unhook_all()
            print("快捷键已暂停")
        except Exception as e:
            print(f"暂停快捷键失败: {e}")
    
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
    
    # 创建或显示浮窗
    if _main_window is None or not hasattr(_main_window, 'showFullScreen'):
        _main_window = DiscoverOverlay(discover_app)
    
    _main_window.showFullScreen()
    _main_window.raise_()
    _main_window.activateWindow()


def open_settings():
    """打开设置窗口"""
    try:
        from settings.setting_gui import SettingsWindow
        settings_window = SettingsWindow()
        settings_window.show()
    except Exception as e:
        print(f"无法打开设置: {e}")


def register_global_shortcut(app, discover_app, shortcut="Alt+D"):
    """注册全局快捷键"""
    try:
        import keyboard
        keyboard.add_hotkey(shortcut, lambda: show_overlay(app, discover_app))
        print(f"全局快捷键已注册: {shortcut}")
    except ImportError:
        print("keyboard模块未安装，使用PyQt快捷键")
        # 使用PyQt的全局快捷键作为后备
        from PyQt6.QtWidgets import QShortcut
        from PyQt6.QtGui import QKeySequence
        shortcut_widget = QShortcut(QKeySequence(shortcut), None)
        shortcut_widget.activated.connect(lambda: show_overlay(app, discover_app))


def run_gui():
    """运行GUI模式"""
    from main import DiscoverApp
    
    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)
    
    # 创建应用实例
    discover_app = DiscoverApp()
    
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
