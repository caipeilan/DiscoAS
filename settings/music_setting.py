import json, os, sys
from functools import lru_cache
import datetime

class PlaylistAlbum(object):
    def __init__(self, p_a_setting):
        self.name = p_a_setting.get("name", "")
        self.playlist_album_id = p_a_setting.get("playlist_album_id", "")
        self.typename = p_a_setting.get("typename", "playlist")
        self.playlist_album_name = p_a_setting.get("playlist_album_name", "")
        self.playlist_album_remark = p_a_setting.get("playlist_album_remark", "")
        self.update_time = p_a_setting.get("update_time", "")
        self.enabled = p_a_setting.get("enabled", False)

    def get_dict(self):
        return self.__dict__

    def get(self, ParameterName):
        return getattr(self, ParameterName, None)

    def set(self, ParameterName, ParameterValue):
        setattr(self, ParameterName, ParameterValue)

class PASetting(object):
    def __init__(self):
        self.file_path = os.path.join(os.path.dirname(__file__), "music_setting.json")
        if not os.path.exists(self.file_path):
            self.create_default_setting()
        self.settings = None
        self.playlist_albums = []
        self.number_of_discovered_songs = 3
        self.have_mystery_song = True
        self.num_of_mystery_song = 1
        self.mystery_song_cover = ""  # 秘密歌曲封面，空字符串=使用平台默认
        self.refreshing_after_cancel = False
        self.shortcut_key = "Alt+D"

    def load(self):
        if os.path.exists(self.file_path):
            with open(self.file_path, "r", encoding="utf-8") as f:
                self.settings = json.load(f)
        else:
            self.settings = {}
            
        # 防止 key missing
        self.playlist_albums = [PlaylistAlbum(p_a_setting) for p_a_setting in self.settings.get("playlist_albums", [])]
        self.number_of_discovered_songs = self.settings.get("number_of_discovered_songs", 3)
        self.have_mystery_song = self.settings.get("have_mystery_song", True)
        self.num_of_mystery_song = self.settings.get("num_of_mystery_song", 1)
        self.mystery_song_cover = self.settings.get("mystery_song_cover", "")
        self.refreshing_after_cancel = self.settings.get("refreshing_after_cancel", False)
        self.shortcut_key = self.settings.get("shortcut_key", "Alt+D")

    def save(self):
        # 逻辑：确保只有一个启用的歌单，或者没有
        playlist_albums_dicts = [p_a.get_dict() for p_a in self.playlist_albums]
        have_a_unique_enabled_playlist_album = False
        for p_a_dict in playlist_albums_dicts:
            if p_a_dict["enabled"]:
                if have_a_unique_enabled_playlist_album:
                    p_a_dict["enabled"] = False
                else:
                    have_a_unique_enabled_playlist_album = True
        
        settings = {
            "number_of_discovered_songs": self.number_of_discovered_songs,
            "have_mystery_song": self.have_mystery_song,
            "num_of_mystery_song": self.num_of_mystery_song,
            "mystery_song_cover": self.mystery_song_cover,
            "refreshing_after_cancel": self.refreshing_after_cancel,
            "shortcut_key": self.shortcut_key,
            "playlist_albums": playlist_albums_dicts
        }
        
        with open(self.file_path, "w", encoding="utf-8") as f:
            json.dump(settings, f, ensure_ascii=False, indent=4)
            print("保存Music设置成功")

    def create_default_setting(self):
        # 保持原有的默认创建逻辑
        default_setting = {
            "number_of_discovered_songs": 3,     
            "have_mystery_song": True,
            "num_of_mystery_song": 1,
            "refreshing_after_cancel": False,
            "shortcut_key": "Alt+D",
            "playlist_albums": [
                {
                    "name": "NeteaseCloudMusic",
                    "playlist_album_id": "8285082830",
                    "typename": "playlist",
                    "playlist_album_name": "",
                    "playlist_album_remark": "作者的小众亚逼二次元歌单",
                    "update_time": "",
                    "enabled": True
                }
            ]
        }
        with open(self.file_path, "w", encoding="utf-8") as f:
            json.dump(default_setting, f, ensure_ascii=False, indent=4)
            print("创建默认设置成功")