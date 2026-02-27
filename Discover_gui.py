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
from PyQt6.QtCore import Qt, QThread, pyqtSignal, QTimer, QTimerEvent, QObject
from PyQt6.QtNetwork import QNetworkAccessManager, QNetworkRequest
from PyQt6.QtCore import QUrl

# 添加项目根目录到路径
sys.path.append(os.path.dirname(__file__))

# 全局引用，用于托盘控制
_main_window = None
_tray_icon = None
_shortcut_enabled = True
_hotkey_id = None
_shortcut_action = None  # 托盘菜单中的快捷键开关项
_network_manager = None  # 全局网络管理器


class ImageLoader(QObject):
    """使用QNetworkAccessManager异步加载图片"""
    image_loaded = pyqtSignal(str, QPixmap)
    load_failed = pyqtSignal(str)
    
    def __init__(self, url: str, parent=None):
        super().__init__(parent)
        self.url = url
        self.reply = None
        
    def load(self):
        global _network_manager
        if _network_manager is None:
            _network_manager = QNetworkAccessManager()
        
        request = QNetworkRequest(QUrl(self.url))
        self.reply = _network_manager.get(request)
        self.reply.finished.connect(self._on_finished)
        
    def _on_finished(self):
        if self.reply:
            if self.reply.error() == 0:
                data = self.reply.readAll()
                pixmap = QPixmap()
                if pixmap.loadFromData(data):
                    self.image_loaded.emit(self.url, pixmap)
                else:
                    self.load_failed.emit(self.url)
            else:
                self.load_failed.emit(self.url)
            self.reply.deleteLater()
            self.reply = None


class SongCardWidget(QFrame):
    """歌曲卡片widget"""
    
    play_requested = pyqtSignal(object)  # 发送歌曲对象
    
    def __init__(self, song_card, index: int, gui_setting=None, parent=None):
        super().__init__(parent)
        self.song_card = song_card
        self.index = index
        self.image_loaded = False
        self.current_pixmap: Optional[QPixmap] = None
        self.gui_setting = gui_setting
        self.image_loader = None
        
        self._setup_ui()
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
                "border": "#76e8fd",
                "font_color": "#000000"
            }
        
        bg = card_config.get("background", "#FFFFFF")
        bg_hover = card_config.get("background_hover", "#d0ebf0")
        border = card_config.get("border", "#76e8fd")
        
        return f"""
            QFrame {{
                background-color: {bg};
                border: 1px solid {border};
                border-radius: 16px;
            }}
            QFrame:hover {{
                background-color: {bg_hover};
                border: 2px solid {border};
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
        self.setFixedSize(200, 260)
        self.setFrameStyle(QFrame.Shape.NoFrame)
        
        # 布局
        layout = QVBoxLayout(self)
        layout.setSpacing(8)
        layout.setContentsMargins(15, 15, 15, 15)
        
        font_color = self._get_font_color()
        
        # 封面
        self.cover_label = QLabel()
        self.cover_label.setFixedSize(170, 170)
        self.cover_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        # 强制设置透明背景
        self.cover_label.setAutoFillBackground(False)
        palette = self.cover_label.palette()
        palette.setColor(QPalette.ColorRole.Window, Qt.GlobalColor.transparent)
        self.cover_label.setPalette(palette)
        self.cover_label.setStyleSheet("""
            background-color: rgba(200, 200, 200, 0.3);
            border-radius: 12px;
        """)
        layout.addWidget(self.cover_label)
        
        # 歌曲名 - 无边框，强制透明背景
        self.name_label = QLabel(self.song_card.get_name())
        self.name_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.name_label.setWordWrap(True)
        font = QFont()
        font.setBold(True)
        font.setPointSize(11)
        self.name_label.setFont(font)
        # 强制设置透明背景
        self.name_label.setAutoFillBackground(False)
        palette = self.name_label.palette()
        palette.setColor(QPalette.ColorRole.Window, Qt.GlobalColor.transparent)
        self.name_label.setPalette(palette)
        self.name_label.setStyleSheet(f"color: {font_color}; background-color: transparent;")
        layout.addWidget(self.name_label)
        
        # 艺术家 - 无边框，强制透明背景
        artist_names = self.song_card.get_artist_names()
        secondary_color = self._get_secondary_font_color()
        self.artist_label = QLabel("/".join(artist_names))
        self.artist_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.artist_label.setWordWrap(True)
        font_small = QFont()
        font_small.setPointSize(9)
        self.artist_label.setFont(font_small)
        # 强制设置透明背景
        self.artist_label.setAutoFillBackground(False)
        palette = self.artist_label.palette()
        palette.setColor(QPalette.ColorRole.Window, Qt.GlobalColor.transparent)
        self.artist_label.setPalette(palette)
        self.artist_label.setStyleSheet(f"color: {secondary_color}; background-color: transparent;")
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
            scaled = pixmap.scaled(
                170, 170, 
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation
            )
            self.cover_label.setPixmap(scaled)
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
    
    def __init__(self, discover_app, parent=None):
        super().__init__(parent)
        self.discover_app = discover_app
        self.songs: List = []
        self.next_songs: List = []  # 缓存下一批歌曲
        self.load_thread = None
        
        # 获取GUI设置
        self.gui_setting = discover_app.gui_setting
        
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
        else:
            # 默认配置
            btn_config = {
                "background": "#FFFFFF",
                "background_hover": "#f5d5d0",
                "border": "#d6533e",
                "font_color": "#000000"
            }
        
        bg = btn_config.get("background", "#FFFFFF")
        bg_hover = btn_config.get("background_hover", "#f5d5d0")
        font_color = btn_config.get("font_color", "#000000")
        
        return f"""
            QPushButton {{
                background-color: {bg};
                color: {font_color};
                border: none;
                border-radius: 25px;
                font-size: 20px;
                font-weight: bold;
            }}
            QPushButton:hover {{
                background-color: {bg_hover};
            }}
        """
        
    def _setup_ui(self):
        """设置极简UI"""
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
        
        # 中央widget - 完全透明
        central = QWidget()
        central.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        central.setStyleSheet("background: transparent;")
        self.setCentralWidget(central)
        
        # 主布局 - 全屏铺满
        main_layout = QVBoxLayout(central)
        main_layout.setContentsMargins(50, 50, 50, 50)
        
        # 右上角关闭按钮 - 应用配置
        close_btn = QPushButton("✕")
        close_btn.setFixedSize(50, 50)
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
        self.song_layout.setSpacing(25)
        
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
        
    def _load_songs_async(self):
        """异步加载歌曲"""
        try:
            # 加载显示用歌曲
            self.songs = self.discover_app.discover_songs()
            
            # 加载下一批缓存
            self.next_songs = self.discover_app.discover_songs()
            
            # 使用信号槽更新UI
            QTimer.singleShot(0, lambda: self.songs_loaded.emit(self.songs))
        except Exception as e:
            print(f"加载歌曲失败: {e}")
            QTimer.singleShot(0, lambda: self.songs_loaded.emit([]))
            
    def _on_songs_loaded(self, songs):
        """歌曲加载完成回调"""
        self.songs = songs
        self._display_songs()
        
    def _display_loading(self):
        """显示加载中状态"""
        # 清空现有卡片
        while self.song_layout.count():
            item = self.song_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
                
        # 显示简单加载提示
        font_color = self._get_font_color_for_label()
        loading_label = QLabel("加载中...")
        # 强制透明背景
        loading_label.setAutoFillBackground(False)
        palette = loading_label.palette()
        palette.setColor(QPalette.ColorRole.Window, Qt.GlobalColor.transparent)
        loading_label.setPalette(palette)
        loading_label.setStyleSheet(f"color: {font_color}; font-size: 18px; background-color: transparent;")
        loading_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.song_layout.addWidget(loading_label, 0, 0)
        
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
                
        columns = 5
        for i, song in enumerate(self.songs):
            if not song.have_loaded:
                song.load_song_detail()
                
            # 传入gui_setting
            card = SongCardWidget(song, i, self.gui_setting)
            card.play_requested.connect(self._on_song_play)
            
            row = i // columns
            col = i % columns
            self.song_layout.addWidget(card, row, col)
            
    def _on_song_play(self, song_card):
        """播放歌曲"""
        # 播放
        self.discover_app.play_song(song_card)
        
        # 延迟隐藏窗口
        QTimer.singleShot(500, self._on_close)
        
    def _on_close(self):
        """关闭/退出时触发"""
        self.hide()


def create_tray_icon(app, discover_app):
    """创建系统托盘"""
    global _tray_icon, _main_window, _shortcut_enabled, _shortcut_action
    
    _main_window = discover_app
    
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
    
    # 使用QTimer延迟显示窗口，避免在keyboard回调线程中阻塞
    def create_and_show():
        # 每次都创建新窗口，确保全新的歌曲列表
        _main_window = DiscoverOverlay(discover_app)
        _main_window.showFullScreen()
        _main_window.raise_()
        _main_window.activateWindow()
    
    QTimer.singleShot(100, create_and_show)


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
