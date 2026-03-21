"""
酷狗音乐 API 模块

使用 requests.Session 复用连接，提升响应速度
"""

import json
import os
import sys

import requests

# 添加 settings 目录到路径，导入统一的路径管理模块
sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(__file__)), 'settings'))
from settings.user_data_path import ensure_dir, get_album_dir, get_playlist_dir

# 酷狗音乐 API 配置
KUGOU_BASE_URL = "http://mobilecdn.kugou.com"
KUGOU_USER_AGENT = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1"

# 创建全局 Session 用于连接复用
_session: requests.Session | None = None


def get_session() -> requests.Session:
    """获取全局 Session，复用 TCP 连接"""
    global _session
    if _session is None:
        _session = requests.Session()
        _session.headers.update({
            "User-Agent": KUGOU_USER_AGENT,
        })
    return _session


class PlaylistAlbumJson:
    """酷狗音乐歌单/专辑 JSON 获取类"""

    def __init__(self, playlist_album_id: str, typename: str):
        self.playlist_album_id = playlist_album_id  # 原始分享码
        self.typename = typename
        self.specialid: str = ""  # 解析后的 specialid
        self.playlist_album_name: str = ""
        self.playlist_album_json: dict | list = {}

        self._fetch_data()

    def _fetch_data(self) -> None:
        """获取歌单/专辑数据"""
        session = get_session()

        if self.typename == "playlist":
            # 判断是分享码还是 specialid（纯数字为 specialid）
            if self.playlist_album_id.isdigit():
                specialid = self.playlist_album_id
            else:
                specialid = self._resolve_share_code(self.playlist_album_id)
                if not specialid:
                    raise ValueError("无法解析歌单分享码")

            # 自动翻页拉取歌单
            all_songs = []
            page = 1
            pagesize = 500

            while True:
                url = f"{KUGOU_BASE_URL}/api/v3/special/song"
                params = {
                    "specialid": specialid,
                    "page": page,
                    "plat": 2,
                    "pagesize": pagesize,
                    "version": 8400,
                }
                try:
                    response = session.get(url, params=params, timeout=10)
                    data = response.json()

                    if data.get('status') != 1:
                        break

                    # 收集歌曲（从 data.info 获取）
                    songs_list = data.get('data', {}).get('info', [])
                    all_songs.extend(songs_list)

                    if len(songs_list) < pagesize:
                        break
                    page += 1
                except Exception as e:
                    print(f"获取歌单详情失败: {e}")
                    raise

            # 保存到 playlist_album_json，结构与 test.py 一致
            self.playlist_album_json = {
                "data": {
                    "info": all_songs
                }
            }
            # 歌单名称用 specialid 代替
            self.playlist_album_name = specialid
            # 保存解析后的 specialid
            self.specialid = specialid

        elif self.typename == "album":
            # 判断是分享码还是 album_id（纯数字为 album_id）
            if self.playlist_album_id.isdigit():
                album_id = self.playlist_album_id
            else:
                album_id = self._resolve_album_share_code(self.playlist_album_id)
                if not album_id:
                    raise ValueError("无法解析专辑分享码")

            # 获取专辑封面和名称
            album_info_url = f"{KUGOU_BASE_URL}/api/v3/album/info"
            album_info_params = {
                "albumid": album_id,
                "plat": 2,
                "version": 8400,
            }
            try:
                info_response = session.get(album_info_url, params=album_info_params, timeout=10)
                info_data = info_response.json()

                if info_data.get('status') == 1:
                    self.playlist_album_name = info_data.get('data', {}).get('albumname', '未知专辑')
                else:
                    raise ValueError("无法获取专辑信息")
            except Exception as e:
                print(f"获取专辑详情失败: {e}")
                raise

            # 翻页获取专辑内的所有歌曲
            all_songs = []
            page = 1
            pagesize = 500

            while True:
                album_songs_url = f"{KUGOU_BASE_URL}/api/v3/album/song"
                songs_params = {
                    "albumid": album_id,
                    "page": page,
                    "plat": 2,
                    "pagesize": pagesize,
                    "version": 8400,
                }
                try:
                    songs_response = session.get(album_songs_url, params=songs_params, timeout=10)
                    songs_data = songs_response.json()

                    if songs_data.get('status') != 1:
                        break

                    songs_list = songs_data.get('data', {}).get('info', [])
                    all_songs.extend(songs_list)

                    if len(songs_list) < pagesize:
                        break
                    page += 1
                except Exception as e:
                    print(f"翻页拉取专辑歌曲异常: {e}")
                    break

            # 保存到 playlist_album_json，结构与 playlist 分支对齐
            self.playlist_album_json = {
                "data": {
                    "info": all_songs
                }
            }
            self.specialid = album_id
        else:
            raise ValueError("typename must be 'playlist' or 'album'")

        print(f"已获取 {self.typename}: {self.playlist_album_name}")

    def _resolve_share_code(self, share_code: str) -> str | None:
        """解析歌单分享码获取 specialid"""
        import re

        share_url = f"https://t.kugou.com/song.html?id={share_code}"
        try:
            session = get_session()
            response = session.get(share_url, allow_redirects=True, timeout=10)
            response.raise_for_status()

            # 从 URL 中提取 specialid
            url_match = re.search(r'/(?:plist/list|songlist)/(\d+)', response.url)
            if url_match:
                return url_match.group(1)

            # 从 HTML 中提取
            html_match = re.search(r'["\']?(?:special[_]?id|global_specialid)["\']?\s*[:=]\s*["\']?(\d+)["\']?', response.text, re.IGNORECASE)
            if html_match:
                return html_match.group(1)

            return None
        except Exception as e:
            print(f"解析分享码失败: {e}")
            return None

    def _resolve_album_share_code(self, share_code: str) -> str | None:
        """解析专辑分享码获取 album_id"""
        import re

        share_url = f"https://t.kugou.com/song.html?id={share_code}"
        try:
            session = get_session()
            response = session.get(share_url, allow_redirects=True, timeout=10)
            response.raise_for_status()

            # 从 URL 中提取 album_id（如 /album/info/12345）
            url_match = re.search(r'/album/(?:info/)?(\d+)', response.url)
            if url_match:
                return url_match.group(1)

            # 从 HTML 中提取 albumid
            html_match = re.search(r'["\']?album[_]?id["\']?\s*[:=]\s*["\']?(\d+)["\']?', response.text, re.IGNORECASE)
            if html_match:
                return html_match.group(1)

            return None
        except Exception as e:
            print(f"解析专辑分享码失败: {e}")
            return None

    def get_id(self) -> str:
        return self.playlist_album_id

    def get_name(self) -> str:
        return self.playlist_album_name

    def get_songs(self) -> list[dict]:
        """获取歌曲信息列表（包含 hash 和 album_id）"""
        songs = []

        if self.typename == "playlist":
            # 与 test.py 一致，从 data.info 获取
            for song in self.playlist_album_json.get("data", {}).get("info", []):
                songs.append({
                    "hash": song.get("hash", ""),
                    "album_id": song.get("album_id", song.get("albumid", "")),
                    "name": song.get("filename", song.get("name", "")),
                })

        elif self.typename == "album" and "data" in self.playlist_album_json:
            for song in self.playlist_album_json.get("data", {}).get("info", []):
                songs.append({
                    "hash": song.get("hash", ""),
                    "album_id": song.get("album_id", song.get("albumid", "")),
                    "name": song.get("filename", song.get("name", "")),
                })

        return songs

    def save(self) -> None:
        """保存到本地 JSON 文件"""
        # 使用统一的路径管理
        path = get_playlist_dir("KugouMusic") if self.typename == "playlist" else get_album_dir("KugouMusic")
        ensure_dir(path)

        # 获取歌曲信息列表
        all_songs = self.playlist_album_json.get("data", {}).get("info", [])
        song_ids = [song.get("hash", "") for song in all_songs]
        # 保存完整信息供 card.py 使用
        songs_info = [
            {
                "hash": song.get("hash", ""),
                "album_id": song.get("album_id", song.get("albumid", "")),
                "filename": song.get("filename", "")
            }
            for song in all_songs
        ]

        data = {
            "playlist_album_id": self.playlist_album_id,
            "specialid": self.specialid,
            "playlist_album_name": self.playlist_album_name,
            "playlist_album_type": self.typename,
            "song_ids": song_ids,
            "songs_info": songs_info
        }

        filepath = os.path.join(path, f"{self.playlist_album_id}.json")
        with open(filepath, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=4)
            print(f"已保存 {self.typename} {self.playlist_album_id} {self.playlist_album_name} 到 {path}")


if __name__ == '__main__':
    # 测试代码
    import sys
    if len(sys.argv) > 2:
        playlist_id = sys.argv[1]
        typename = sys.argv[2]
    else:
        playlist_id = "7hXh101FZV2"  # 测试用分享码
        typename = "playlist"

    playlist = PlaylistAlbumJson(playlist_id, typename)
    print(f"名称: {playlist.get_name()}")
    print(f"歌曲数量: {len(playlist.get_songs())}")
    playlist.save()
