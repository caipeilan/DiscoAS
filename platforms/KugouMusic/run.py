"""
酷狗音乐播放模块

负责唤起酷狗音乐播放歌曲并最小化窗口
"""

import time
import webbrowser

import pygetwindow as gw


def play_song(song_card) -> bool:
    """播放歌曲并最小化窗口"""
    url = song_card.get_scheme_url()
    webbrowser.open(url, new=0, autoraise=False)

    time.sleep(4)

    title_prefix = song_card.get_window_name()
    windows = (
        [w for w in gw.getAllWindows() if w.title[:len(title_prefix)].lower() == title_prefix.lower()]
        if title_prefix
        else []
    )
    if windows:
        windows[0].minimize()

    return True
