"""
播放列表加载模块

负责从本地JSON文件加载歌单/专辑数据，并提供随机歌曲选择功能
"""

import json
import os
import random
from functools import lru_cache
from typing import List, Optional


class Playlist:
    """播放列表类，用于管理歌单/专辑数据"""

    def __init__(self, platform: str, playlist_type: str, playlist_id: str):
        """
        初始化播放列表

        Args:
            platform: 平台名称 (NeteaseCloudMusic, QQMusic)
            playlist_type: 类型 (playlist 或 album)
            playlist_id: 歌单/专辑ID
        """
        self.platform = platform
        self.playlist_type = playlist_type
        self.playlist_id = playlist_id
        self.json_file = self._get_json_file_path()
        self.playlist: dict = self._load_playlist_data()
        self.songs: List[int] = self.playlist.get("song_ids", [])
        self.song_count: int = len(self.songs)
        
    def _get_json_file_path(self) -> str:
        """获取JSON文件路径"""
        return os.path.join(
            os.path.dirname(__file__),
            "platforms",
            self.platform,
            "user_data",
            self.playlist_type,
            f"{self.playlist_id}.json"
        )
    
    @lru_cache(maxsize=32)
    def _load_playlist_data(self) -> dict:
        """
        加载播放列表数据并缓存
        
        Returns:
            包含歌曲ID列表的字典
        """
        if not os.path.exists(self.json_file):
            raise FileNotFoundError(f"播放列表文件不存在: {self.json_file}")
        
        with open(self.json_file, "r", encoding="utf-8") as f:
            return json.load(f)

    def get_random_song(self, number: int) -> tuple[List[int], int]:
        """
        获取随机不重复的歌曲ID列表

        Args:
            number: 需要获取的歌曲数量

        Returns:
            (歌曲ID列表, 实际歌曲数量)
        """
        if number > self.song_count:
            number = self.song_count
        
        if number <= 0:
            return [], 0
            
        return random.sample(self.songs, number), number
    
    def get_playlist_name(self) -> str:
        """获取播放列表名称"""
        return self.playlist.get("playlist_album_name", "")
    
    @classmethod
    def clear_cache(cls) -> None:
        """清除Playlist类的所有缓存"""
        if hasattr(cls._load_playlist_data, 'cache_clear'):
            cls._load_playlist_data.cache_clear()
            print("Playlist数据缓存已清除")


if __name__ == "__main__":
    # 测试代码
    playlist = Playlist("NeteaseCloudMusic", "playlist", "8285082830")
    print(f"歌单名称: {playlist.get_playlist_name()}")
    print(f"歌曲总数: {playlist.song_count}")
    print(f"随机3首: {playlist.get_random_song(3)}")
