"""
Spotify播放模块

负责唤起Spotify播放歌曲并最小化窗口
Spotify的scheme在本地已播放时无法跳转，需先暂停再播放
"""

import ctypes
import time
import webbrowser

import pygetwindow as gw

# Windows 媒体按键码
VK_MEDIA_PLAY_PAUSE = 0xB3


def play_song(song_card) -> bool:
    """播放歌曲并最小化窗口"""
    # 1. 检测 Spotify 是否在运行（通过窗口名判断）
    all_windows = gw.getAllWindows()
    spotify_running = any(
        "spotify free" in w.title.lower() or "spotify premium" in w.title.lower()
        for w in all_windows
    )

    # 2. 如果 Spotify 在播放中（窗口不存在），先暂停
    if not spotify_running:
        ctypes.windll.user32.keybd_event(VK_MEDIA_PLAY_PAUSE, 0, 0, 0)
        time.sleep(0.5)

    # 3. 使用 scheme URL 播放目标歌曲
    url = song_card.get_scheme_url()
    webbrowser.open(url, new=0, autoraise=False)

    # 4. 等待窗口出现并最小化
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
