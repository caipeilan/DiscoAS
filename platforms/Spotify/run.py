"""
Spotify 播放模块

使用 spotify:track: URI scheme 唤起 Spotify 播放
"""

import os
import sys
import time

sys.path.append(os.path.dirname(__file__))
from card import SongCard


class ToPlaySong:
    def __init__(self, song_id):
        self.song_id = song_id
        self.song_card = SongCard(song_id)
        self.sleep_time = 2

    def to_play(self):
        # 预先加载歌曲详情，确保窗口名不是 None
        if not self.song_card.have_loaded:
            self.song_card.load_song_detail()

        # 使用 os.startfile 打开 Spotify URI
        scheme_url = self.song_card.get_scheme_url()
        os.startfile(scheme_url)

        # 等待 Spotify 客户端启动
        time.sleep(self.sleep_time)

        # 尝试最小化 Spotify 窗口
        try:
            import pygetwindow as gw
            windows = gw.getWindowsWithTitle(self.song_card.get_window_name())
            if windows:
                window = windows[0]
                window.minimize()
        except Exception:
            # 如果最小化失败，忽略错误（不同版本的 Spotify 窗口名可能不同）
            pass


if __name__ == '__main__':
    # 测试播放：Shape of You
    song = ToPlaySong("4cOdK2wGLETKBW3PvgPWqT")
    song.to_play()
