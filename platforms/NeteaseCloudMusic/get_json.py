import json
import requests
import os

class PlaylistAlbumJson(object):
    def __init__(self, playlist_album_id,typename):
        self.playlist_album_id = playlist_album_id
        self.typename = typename
        if typename == "playlist":
            self.playlist_album_json = requests.get(
                f"https://music.163.com/api/v6/playlist/detail?id={self.playlist_album_id}&limit=20000"
            )
            self.playlist_album_name = self.playlist_album_json.json()["playlist"]["name"]
        elif typename == "album":
            self.playlist_album_json = requests.get(
                f"https://music.163.com/api/album/{self.playlist_album_id}?limit=20000"
            )
            self.playlist_album_name = self.playlist_album_json.json()["album"]["name"]
        else:
            raise ValueError("typename must be 'playlist' or 'album'")
        print(self.playlist_album_json.json())
        
    
    def get_id(self):
        return self.playlist_album_id
    
    def get_name(self):
        return self.playlist_album_name
    
    def get_songs(self):  # 只读歌曲id
        songs = []
        if self.typename == "playlist":
            # 检查是否有完整的trackIds列表
            if "trackIds" in self.playlist_album_json.json()["playlist"]:
                # 使用trackIds获取所有歌曲ID
                for track in self.playlist_album_json.json()["playlist"]["trackIds"]:
                    songs.append(track["id"])
            else:
                # 回退到原来的tracks方法
                for song in self.playlist_album_json.json()["playlist"]["tracks"]:
                    songs.append(song["id"])
        elif self.typename == "album":
            for song in self.playlist_album_json.json()["album"]["songs"]:
                songs.append(song["id"])
        else:
            raise ValueError("typename must be 'playlist' or 'album'")
        return songs
    
    def save(self):
        # 保存在与本文件同一目录的子文件夹user_data/playlist(album)下，名字{playlist_id}.json
        path = os.path.join(os.path.dirname(__file__), "user_data",f"{self.typename}")
        
        song_ids = self.get_songs()
        
        data = {
            "playlist_album_id": self.playlist_album_id,
            "playlist_album_name": self.playlist_album_name,
            "playlist_album_type": self.typename,
            "song_ids": song_ids
        }
        
        with open(os.path.join(path, f'{self.playlist_album_id}' + ".json"), "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=2)
            print(f"已保存歌单{self.playlist_album_name}到{path}")

            
if __name__ == '__main__':
    playlist = PlaylistAlbumJson(8285082830,"playlist")
    # playlist = PlaylistAlbumJson(34614503,"album")
    playlist.save()