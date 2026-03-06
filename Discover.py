import os
import json
import sys
import random
import importlib

#使用绝对路径导入模块
sys.path.append(os.path.dirname(__file__))
from load_playlist_json import Playlist

class DiscoverASong(object):
    def __init__(self, platform, playlist_type, playlist_id):
        self.playlist = Playlist(platform, playlist_type, playlist_id)
        self.platform = platform
        self.song_card_class = None
        platform_path = os.path.join(os.path.dirname(__file__), "platforms", platform)
        if not os.path.exists(platform_path):
            raise ValueError(f"{platform}平台不存在")
        sys.path.append(platform_path)
        # 使用importlib动态导入模块
        card_module = importlib.import_module('card')
        self.song_card_class = getattr(card_module, 'SongCard')
        
    def get_songs(self, number_of_songs, mystery_song, number_of_mystery_song=1, overlap=False, mystery_pic_url=""):
        """
        从Playlist中获取歌曲
        
        Args:
            number_of_songs: 发现的歌曲数量
            mystery_song: 是否包含神秘歌曲
            number_of_mystery_song: 神秘歌曲数量
            overlap: 是否允许重复填充（已废弃参数，现在固定为False）
            mystery_pic_url: 秘密歌曲封面URL或本地路径，空字符串=使用平台默认
            
        Returns:
            [歌曲列表, 歌曲总数, 神秘歌曲数量]
        """
        if mystery_song == False:
            number_of_mystery_song = 0
        
        # 获取实际歌曲数量
        actual_song_count = number_of_songs + number_of_mystery_song
        
        # 获取随机歌曲
        song_ids, sum_of_song = self.playlist.get_random_song(actual_song_count)
        
        # 注意：allow_overlap 设置已移除，现在固定不允许重复
        # 如果歌曲不足，直接返回可用歌曲（可能少于请求数量）
        
        # 自定义封面：空字符串时传 None，让 SongCard 使用平台默认
        custom_pic = mystery_pic_url if mystery_pic_url else None
        
        songs = []
        i = 0
        for song_id in song_ids:
            i = i+1
            if i > number_of_songs and self.song_card_class:
                songs.append(self.song_card_class(song_id, True, mystery_pic_url=custom_pic))
            elif self.song_card_class:
                songs.append(self.song_card_class(song_id))
        return [songs, sum_of_song, number_of_mystery_song]
    

if __name__ == "__main__":
    discover_a_song = DiscoverASong("NeteaseCloudMusic","playlist","8285082830")
    songs,sum,sum_mystery = discover_a_song.get_songs(5,True)
    for song in songs:
        song.load_song_detail()  # 确保在获取信息前加载歌曲详情
        print(song.get_name(),end=" ")
        print("- ",end="")
        artist = song.get_artist_names()
        artist_str = "/".join(artist)
        print(artist_str)
    print()
    import webbrowser
    # 导入run模块
    platform_path = os.path.join(os.path.dirname(__file__), "platforms", "NeteaseCloudMusic")
    sys.path.append(platform_path)
    # 使用importlib动态导入模块
    card_module = importlib.import_module('run')
    song_play = getattr(card_module, 'ToPlaySong')
    while True:
        test = input("请输入要播放曲子的序号（从0开始,按q退出）：")
        if test == "q":
            break
        selected_song = songs[int(test)]
        selected_song.load_song_detail()  # 确保在获取信息前加载歌曲详情
        url = selected_song.get_scheme_url()
        print(url)
        song_play(selected_song.get_id()).to_play()