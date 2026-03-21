import os
import sys
import time
import webbrowser

import pygetwindow as gw

sys.path.append(os.path.dirname(__file__))
from card import SongCard


class ToPlaySong:
    def __init__(self, song_id):
        self.song_id = song_id
        self.song_card = SongCard(song_id)
        self.sleep_time = 3

    def to_play(self):
        # 预先加载歌曲详情，确保窗口名不是 None
        if not self.song_card.have_loaded:
            self.song_card.load_song_detail()
        webbrowser.open(self.song_card.get_scheme_url(), new=0, autoraise=False)
        # 等待客户端窗口弹出后再检索窗口
        time.sleep(self.sleep_time)
        windows = gw.getWindowsWithTitle(self.song_card.get_window_name())
        if windows:
            window = windows[0]
            window.minimize()




if __name__ == '__main__':
    song = ToPlaySong(284218927)
    song.to_play()
