import os
import sys
import time
import pygetwindow as gw

sys.path.append(os.path.dirname(__file__))
from card import SongCard


class ToPlaySong(object):
    def __init__(self, song_id):
        self.song_id = song_id
        self.song_card = SongCard(song_id)
        self.sleep_time = 3

    def to_play(self):
        # 预先加载歌曲详情，确保窗口名不是 None
        if not self.song_card.have_loaded:
            self.song_card.load_song_detail()
        # 使用 os.startfile 打开 scheme URL
        os.startfile(self.song_card.get_scheme_url())
        # 等待客户端窗口弹出后再检索窗口
        time.sleep(self.sleep_time)
        windows = gw.getWindowsWithTitle(self.song_card.get_window_name())
        if windows:
            window = windows[0]
            window.minimize()


if __name__ == '__main__':
    # 测试播放
    test_hash = "4809EE31DEF9945C7751E3FD7BF7C009"
    song = ToPlaySong(test_hash).to_play()
