import json,base64,requests

class SongCard():
    def __init__(self, song_id,
              mystery_mode=False,
              mystery_pic_url="https://p1.music.126.net/sFzdxi9EMPV0q4IuWEy-og==/17792297160856759.jpg"):
        self.song_id = song_id    #歌曲id
        self.mystery_mode = mystery_mode     #是否为秘密歌曲
        self.song_detail_json = None  #歌曲详情json（仅用于内部使用）
        self.song_name = None  #歌曲名
        self.song_artists = None  #歌手数组（仅用于内部使用）
        self.song_artist_names = None  #歌手名数组
        self.window_name = None  #窗口名（关闭窗口用，仅用于内部）
        self.album_pic_url = None  #歌曲所属专辑封面url
        self.mystery_pic_url = mystery_pic_url  #秘密歌曲封面url
        self.have_loaded = False

    def load_song_detail(self):
        if self.have_loaded == True:
            return
        try:
            self.song_detail_json = requests.get(
                "https://music.163.com/api/song/detail/?ids=" 
                + "[" +str(self.song_id) + "]"
                )

            self.song_name = self.song_detail_json.json()["songs"][0]["name"]
            self.song_artists = self.song_detail_json.json()["songs"][0]["artists"]
            self.song_artist_names = [artist["name"] for artist in self.song_artists]
            self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
            self.album_pic_url = self.song_detail_json.json()["songs"][0]["album"]["blurPicUrl"]
            self.have_loaded = True

        except Exception as e:
            print(f"歌曲详情加载失败{e}")
            self.song_name = "未知"
            self.song_artist_names = ["未知艺术家"]
            # 即使请求失败也设置一个可用的窗口名，避免 None 导致异常
            self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
            self.album_pic_url = self.mystery_pic_url
            self.have_loaded = False

    def get_id(self):
        return self.song_id

    def get_name(self):
        if self.mystery_mode:
            return "秘密歌曲"
        return self.song_name

    def get_artist_names(self):
        if self.mystery_mode:
            return ["??????????"]
        return self.song_artist_names

    def get_window_name(self):
        return self.window_name

    def get_album_pic_url(self):
        if self.mystery_mode:
            return self.mystery_pic_url
        return self.album_pic_url

    def get_scheme_url(self):
        prefix = "orpheus://"
        the_json = {"type":"song","id":self.song_id,"cmd":"play"}
        json_str = json.dumps(the_json)
        encoded_json = base64.b64encode(json_str.encode('utf-8')).decode('ascii')
        url = prefix + encoded_json
        return url


 
if __name__ == '__main__':
    import time
    import pygetwindow as gw
    import webbrowser
    song = SongCard(2121980421)
    # song = SongCard(410181110, True)
    song.load_song_detail()
    print(song.get_id())
    print(song.get_name())
    print(song.get_artist_names())
    print(song.get_window_name())
    print(song.get_album_pic_url())
    print(song.get_scheme_url())
    webbrowser.open(song.get_scheme_url(), new=0, autoraise=False)
    time.sleep(2.5)
    windows = gw.getWindowsWithTitle(song.get_window_name())
    if windows:
        window = windows[0]
        window.minimize()
