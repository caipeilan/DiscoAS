"""
DiscoverASong - 音乐选择器GUI

使用PyQt6构建的图形界面
"""

import sys
import os
import threading
from typing import Optional, List

from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QPushButton, QScrollArea, QFrame, QGridLayout,
    QMessageBox, QSystemTrayIcon, QMenu
)
from PyQt6.QtGui import QPixmap, QImage, QIcon, QFont
from PyQt6.QtNetwork import QNetworkAccessManager, QNetworkRequest, QNetworkReply
from PyQt6.QtCore import Qt, QThread, pyqtSignal, QTimer, QUrl, QSize, QPoint

# 添加项目根目录到路径
sys.path.append(os.path.dirname(__file__))


class ImageLoaderThread(QThread):
    """图片异步加载线程"""
    image_loaded = pyqtSignal(str, QImage)  # (url, image)
    load_failed = pyqtSignal(str)  # url
    
    def __init__(self, url: str, parent=None):
        super().__init__(parent)
        self.url = url
        
    def run(self):
        try:
            from urllib.request import urlopen
            from PIL import Image
            import io
            
            # 下载图片
            with urlopen(self.url, timeout=10) as response:
                data = response.read()
                
            # 使用PIL处理图片
            img = Image.open(io.BytesIO(data))
            img = img.convert("RGBA")
            
            # 转换为QImage
            img_data = img.tobytes("raw", "RGBA")
            qimage = QImage(img_data, img.width, img.height, QImage.Format.Format_RGBA8888)
            
            self.image_loaded.emit(self.url, qimage)
            
        except Exception as e:
            print(f"图片加载失败: {e}")
            self.load_failed.emit(self.url)


class SongCardWidget(QFrame):
    """歌曲卡片widget"""
    
    clicked = pyqtSignal(int)  # 点击信号
    
    def __init__(self, song_card, index: int, parent=None):
        super().__init__(parent)
        self.song_card = song_card
        self.index = index
        self.image_loaded = False
        self.current_image: Optional[QImage] = None
        
        self._setup_ui()
        self._load_cover_image()
        
    def _setup_ui(self):
        """设置UI"""
        self.setFixedSize(180, 220)
        self.setFrameStyle(QFrame.Shape.Box | QFrame.Shadow.Raised)
        
        # 布局
        layout = QVBoxLayout(self)
        layout.setSpacing(5)
        layout.setContentsMargins(10, 10, 10, 10)
        
        # 封面
        self.cover_label = QLabel()
        self.cover_label.setFixedSize(160, 160)
        self.cover_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.cover_label.setStyleSheet("background-color: #f0f0f0; border-radius: 8px;")
        layout.addWidget(self.cover_label)
        
        # 歌曲名
        self.name_label = QLabel(self.song_card.get_name())
        self.name_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.name_label.setWordWrap(True)
        font = QFont()
        font.setBold(True)
        font.setPointSize(10)
        self.name_label.setFont(font)
        layout.addWidget(self.name_label)
        
        # 艺术家
        artist_names = self.song_card.get_artist_names()
        self.artist_label = QLabel("/".join(artist_names))
        self.artist_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.artist_label.setWordWrap(True)
        self.artist_label.setStyleSheet("color: #666;")
        font_small = QFont()
        font_small.setPointSize(8)
        self.artist_label.setFont(font_small)
        layout.addWidget(self.artist_label)
        
        # 样式
        self.setStyleSheet("""
            QFrame {
                background-color: white;
                border: 1px solid #e0e0e0;
                border-radius: 8px;
            }
            QFrame:hover {
                background-color: #f5f5f5;
                border: 2px solid #76e8fd;
            }
        """)
        
        # 点击事件
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        
    def _load_cover_image(self):
        """异步加载封面图"""
        url = self.song_card.get_album_pic_url()
        
        # 使用线程加载
        self.loader = ImageLoaderThread(url, self)
        self.loader.image_loaded.connect(self._on_image_loaded)
        self.loader.start()
        
    def _on_image_loaded(self, url: str, image: QImage):
        """图片加载完成"""
        if url == self.song_card.get_album_pic_url():
            self.current_image = image
            pixmap = QPixmap.fromImage(image).scaled(
                160, 160, 
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation
            )
            self.cover_label.setPixmap(pixmap)
            self.image_loaded = True
            
    def mousePressEvent(self, event):
        """鼠标点击事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            self.clicked.emit(self.index)
        super().mousePressEvent(event)


class DiscoverWindow(QMainWindow):
    """发现歌曲主窗口"""
    
    def __init__(self, discover_app, parent=None):
        super().__init__(parent)
        self.discover_app = discover_app
        self.songs: List = []
        
        self._setup_ui()
        self._load_settings()
        
    def _setup_ui(self):
        """设置UI"""
        self.setWindowTitle("DiscoverASong - 发现音乐")
        self.setMinimumSize(800, 600)
        
        # 中央widget
        central_widget = QWidget()
        self.setCentralWidget(central_widget)
        
        # 主布局
        main_layout = QVBoxLayout(central_widget)
        
        # 标题
        title = QLabel("🎵 随机歌曲发现器")
        title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        title_font = QFont()
        title_font.setPointSize(20)
        title_font.setBold(True)
        title.setFont(title_font)
        main_layout.addWidget(title)
        
        # 刷新按钮
        btn_layout = QHBoxLayout()
        btn_layout.addStretch()
        
        self.refresh_btn = QPushButton("🔄 刷新发现")
        self.refresh_btn.setFixedSize(150, 40)
        self.refresh_btn.clicked.connect(self._on_refresh_clicked)
        btn_layout.addWidget(self.refresh_btn)
        
        # 设置按钮
        self.settings_btn = QPushButton("⚙️ 设置")
        self.settings_btn.setFixedSize(100, 40)
        self.settings_btn.clicked.connect(self._on_settings_clicked)
        btn_layout.addWidget(self.settings_btn)
        
        btn_layout.addStretch()
        main_layout.addLayout(btn_layout)
        
        # 歌曲卡片区域
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        
        self.song_container = QWidget()
        self.song_layout = QGridLayout(self.song_container)
        self.song_layout.setSpacing(20)
        
        scroll.setWidget(self.song_container)
        main_layout.addWidget(scroll)
        
        # 状态栏
        self.status_label = QLabel("准备就绪")
        self.status_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        main_layout.addWidget(self.status_label)
        
        # 加载样式
        self._apply_style()
        
    def _apply_style(self):
        """应用样式"""
        self.setStyleSheet("""
            QMainWindow {
                background-color: #f5f5f5;
            }
            QPushButton {
                background-color: #76e8fd;
                border: none;
                border-radius: 8px;
                padding: 8px 16px;
                font-size: 14px;
            }
            QPushButton:hover {
                background-color: #5dd4ec;
            }
            QPushButton:pressed {
                background-color: #4ac0dc;
            }
            QLabel {
                color: #333;
            }
            QScrollArea {
                border: none;
            }
        """)
        
    def _load_settings(self):
        """加载设置"""
        self.gui_setting = self.discover_app.gui_setting
        
    def _on_refresh_clicked(self):
        """刷新按钮点击"""
        self.refresh_btn.setEnabled(False)
        self.status_label.setText("正在发现歌曲...")
        
        # 异步加载歌曲
        thread = threading.Thread(target=self._load_songs_async)
        thread.start()
        
    def _load_songs_async(self):
        """异步加载歌曲"""
        try:
            self.songs = self.discover_app.discover_songs()
            
            # 在主线程更新UI
            QTimer.singleShot(0, self._display_songs)
            
        except Exception as e:
            QTimer.singleShot(0, lambda: self._show_error(str(e)))
            
    def _display_songs(self):
        """显示歌曲"""
        # 清除旧卡片
        while self.song_layout.count():
            item = self.song_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
                
        # 创建新卡片
        columns = 4  # 每行4个
        for i, song in enumerate(self.songs):
            # 确保加载详情
            if not song.have_loaded:
                song.load_song_detail()
                
            card = SongCardWidget(song, i)
            card.clicked.connect(self._on_song_clicked)
            
            row = i // columns
            col = i % columns
            self.song_layout.addWidget(card, row, col)
            
        self.status_label.setText(f"发现 {len(self.songs)} 首歌曲，点击卡片播放")
        self.refresh_btn.setEnabled(True)
        
    def _on_song_clicked(self, index: int):
        """歌曲卡片点击"""
        if 0 <= index < len(self.songs):
            song = self.songs[index]
            self.status_label.setText(f"正在播放: {song.get_name()} - {'/'.join(song.get_artist_names())}")
            
            # 播放歌曲
            self.discover_app.play_song(song)
            
    def _on_settings_clicked(self):
        """设置按钮点击"""
        try:
            from settings.setting_gui import SettingsWindow
            settings_window = SettingsWindow()
            settings_window.show()
        except Exception as e:
            QMessageBox.warning(self, "错误", f"无法打开设置: {str(e)}")
            
    def _show_error(self, message: str):
        """显示错误"""
        QMessageBox.critical(self, "错误", message)
        self.status_label.setText("加载失败")
        self.refresh_btn.setEnabled(True)


# 测试代码
if __name__ == "__main__":
    from main import DiscoverApp
    
    app = QApplication(sys.argv)
    
    discover_app = DiscoverApp()
    window = DiscoverWindow(discover_app)
    window.show()
    
    sys.exit(app.exec())
