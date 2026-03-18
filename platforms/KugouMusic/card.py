"""
酷狗音乐歌曲卡片模块

用于获取歌曲信息和封面
"""

import json
import requests
from typing import List, Optional

# 导入共享的 Session
import os
import sys
sys.path.append(os.path.dirname(__file__))
from get_json import get_session


class SongCard:
    """酷狗音乐歌曲卡片类"""

    # 默认神秘歌曲封面
    DEFAULT_MYSTERY_PIC = ""

    def __init__(
        self,
        song_id: str,
        mystery_mode: bool = False,
        mystery_pic_url: Optional[str] = None
    ):
        # song_id 可能是 hash 值（酷狗用 hash 标识歌曲）
        self.song_id = song_id
        self.song_hash: str = ""
        self.song_album_id: str = ""

        # 解析 song_id，提取 hash 和 album_id
        if "|" in str(song_id):
            parts = str(song_id).split("|")
            self.song_hash = parts[0]
            self.song_album_id = parts[1] if len(parts) > 1 else ""
        else:
            # 假设传入的是 hash
            self.song_hash = str(song_id)

        self.mystery_mode = mystery_mode
        self.mystery_pic_url = mystery_pic_url or self.DEFAULT_MYSTERY_PIC

        # 歌曲详情数据
        self.song_detail_json: Optional[dict] = None
        self.song_name: Optional[str] = None
        self.song_artists: List[dict] = []
        self.song_artist_names: List[str] = []
        self.window_name: Optional[str] = None
        self.album_pic_url: Optional[str] = None
        self.have_loaded: bool = False

    def load_song_detail(self) -> None:
        """加载歌曲详情"""
        if self.have_loaded:
            return

        if self.mystery_mode:
            # 神秘模式不需要加载详情
            self.song_name = "神秘歌曲"
            self.song_artist_names = ["未知艺术家"]
            self.window_name = self.song_name + " - " + "/".join(self.song_artist_names)
            self.album_pic_url = self.mystery_pic_url
            self.have_loaded = True
            return

        try:
            session = get_session()

            # 使用酷狗音乐获取歌曲信息 API
            url = "http://m.kugou.com/app/i/getSongInfo.php"
            params = {
                "cmd": "playInfo",
                "hash": self.song_hash
            }

            response = session.get(url, params=params, timeout=10)
            data = response.json()

            if data:
                self.song_detail_json = data
                self.song_name = data.get("songName", data.get("filename", "未知歌曲"))

                # 获取艺术家信息（酷狗的格式）
                self.song_artists = []
                singer_name = data.get(" singer_name", "")
                if singer_name:
                    self.song_artist_names = [s.strip() for s in singer_name.split(",")]
                else:
                    # 从 filename 尝试提取
                    filename = data.get("filename", "")
                    if " - " in filename:
                        parts = filename.split(" - ")
                        if len(parts) >= 2:
                            self.song_artist_names = [parts[0].strip()]

                # 构建窗口名
                self.window_name = self.song_name + " - " + "/".join(self.song_artist_names) if self.song_artist_names else self.song_name

                # 获取封面
                self.album_pic_url = self._get_cover_url(data)

                self.have_loaded = True
            else:
                raise ValueError("无法获取歌曲信息")

        except Exception as e:
            print(f"加载歌曲详情失败: {e}")
            raise

    def _get_cover_url(self, data: dict) -> Optional[str]:
        """获取封面 URL"""
        # 尝试从多个字段获取封面
        cover_url = (
            data.get("imgUrl") or
            data.get("pic") or
            data.get("album_img") or
            ""
        )

        if cover_url:
            # 替换尺寸占位符
            return cover_url.replace("{size}", "400")

        # 如果没有封面，返回默认或空
        return self.mystery_pic_url or None

    def get_id(self) -> str:
        return f"{self.song_hash}|{self.song_album_id}"

    def get_name(self) -> str:
        return self.song_name or "未知歌曲"

    def get_artist_names(self) -> List[str]:
        return self.song_artist_names or ["未知艺术家"]

    def get_window_name(self) -> str:
        return self.window_name or ""

    def get_album_pic_url(self) -> str:
        if self.mystery_mode:
            return self.mystery_pic_url
        if not self.have_loaded:
            self.load_song_detail()
        return self.album_pic_url or self.mystery_pic_url

    def get_cover_url(self) -> str:
        """获取封面 URL（别名）"""
        return self.get_album_pic_url()

    def get_scheme_url(self) -> str:
        """生成酷狗音乐scheme URL用于唤起播放"""
        import json
        import base64

        # 获取歌曲详情
        if not self.have_loaded:
            self.load_song_detail()

        # 从 song_detail_json 获取详细信息
        song_info = self.song_detail_json or {}

        # 构建文件名
        filename = song_info.get('filename', self.song_name or '未知歌曲')
        if not filename.lower().endswith('.mp3'):
            filename += ".mp3"

        # 只需要两个必要参数：filename 和 hash
        payload = {
            "Files": [
                {
                    "filename": filename,
                    "hash": self.song_hash
                }
            ]
        }

        # 1. 转化为紧凑型 JSON 字符串
        json_str = json.dumps(payload, separators=(',', ':'), ensure_ascii=False)
        # 2. UTF-8 编码后 Base64 转换
        b64_str = base64.b64encode(json_str.encode('utf-8')).decode('utf-8')
        # 3. 拼接最终协议
        return f"kugou://play?p={b64_str}"


# 测试代码
if __name__ == '__main__':
    import time

    # 测试歌曲（需要有效的 hash）
    # 示例 hash: 从歌单中获取
    test_hash = "D7A154A55E80B3FA2CC4E9D80F2B5D7C"

    song = SongCard(test_hash)
    song.load_song_detail()

    print(f"歌曲 ID: {song.get_id()}")
    print(f"歌曲名: {song.get_name()}")
    print(f"艺术家: {song.get_artist_names()}")
    print(f"封面: {song.get_cover_url()}")
    print(f"窗口名: {song.get_window_name()}")
    print(f"Scheme URL: {song.get_scheme_url()}")
