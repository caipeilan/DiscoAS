import os,sys,webbrowser,time,pygetwindow as gw,json,base64

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
        # webbrowser.open(self.song_card.get_scheme_url(), new=0, autoraise=False)
        os.startfile(self.song_card.get_scheme_url())
        # 等待客户端窗口弹出后再检索窗口
        time.sleep(self.sleep_time)
        windows = gw.getWindowsWithTitle(self.song_card.get_window_name())
        if windows:
            window = windows[0]
            window.minimize()

# class ToPlaySongList(object):

#     def __init__(self, song_list_id,song_list_type):
#         self.song_list_id = song_list_id
#         self.song_list_type = song_list_type
#         self.sleep_time = 3
#     def to_play(self):
#         # {"type":"playlist","id":"8285082830","cmd":"play"}
#         prefix = "orpheus://"
#         the_json = {"type":self.song_list_type,"id":self.song_list_id,"cmd":"play"}
#         json_str = json.dumps(the_json)
#         encoded_json = base64.b64encode(json_str.encode('utf-8')).decode('ascii')
#         url = prefix + encoded_json
#         webbrowser.open(url, new=0, autoraise=False)
        time.sleep(self.sleep_time)

if __name__ == '__main__':
    song = ToPlaySong(2689926862).to_play()
    # songlist = ToPlaySongList(8285082830,'playlist').to_play()
    # album = ToPlaySongList(34614503,'album').to_play()