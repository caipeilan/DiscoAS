import requests
import time
import re
import json
import base64

def resolve_share_code(share_code, headers):
    share_url = f"https://t.kugou.com/song.html?id={share_code}"
    print(f"🔍 正在解析分享码: {share_code} ...")
    try:
        response = requests.get(share_url, headers=headers, allow_redirects=True, timeout=10)
        response.raise_for_status()
        url_match = re.search(r'/(?:plist/list|songlist)/(\d+)', response.url)
        if url_match: return url_match.group(1)
        html_match = re.search(r'["\']?(?:special[_]?id|global_specialid)["\']?\s*[:=]\s*["\']?(\d+)["\']?', response.text, re.IGNORECASE)
        if html_match: return html_match.group(1)
        return None
    except Exception as e:
        print(f"❌ 解析异常: {e}")
        return None

def generate_kugou_scheme(song):
    """
    核心黑科技：根据您拦截到的格式，组装酷狗 App 专属的 Base64 播放协议
    """
    # 获取参数并提供容错默认值
    filename = song.get('filename', song.get('name', '未知歌曲'))
    if not filename.lower().endswith('.mp3'):
        filename += ".mp3" # 拦截的示例中带有 .mp3 后缀

    song_hash = song.get('hash', '')
    album_id = str(song.get('album_id', song.get('albumid', '0')))
    
    # 酷狗的数据时长有时是秒，有时是毫秒，拦截的 163265 是毫秒
    duration_val = song.get('duration', 180000)
    if isinstance(duration_val, int) and duration_val < 10000:
        duration_val *= 1000  # 转为毫秒

    # 严格按照您拦截到的 JSON 结构，全字段转为 String 类型
    payload = {
        "Files":[
            {
                "filename": filename,
                "hash": song_hash,
                # 有且仅有以上参数是必要的
                # "size": str(song.get('filesize', song.get('size', '3000000'))),
                # "duration": str(duration_val),
                # "bitrate": str(song.get('bitrate', '128')),
                # "isfilehead": "100", # 固定值
                # "privilege": str(song.get('privilege', '8')), # 权限标识
                # "album_id": album_id
            }
        ]
    }
    
    # 1. 转化为紧凑型 JSON 字符串 (ensure_ascii=False 保留中文)
    json_str = json.dumps(payload, separators=(',', ':'), ensure_ascii=False)
    # 2. 将字符串 UTF-8 编码后进行 Base64 转换
    b64_str = base64.b64encode(json_str.encode('utf-8')).decode('utf-8')
    
    # 3. 拼接最终协议
    return f"kugou://play?p={b64_str}"

def test_kugou_ultimate(share_code):
    headers = {
        "User-Agent": "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1"
    }

    specialid = resolve_share_code(share_code, headers)
    if not specialid:
        print("❌ 获取 specialid 失败")
        return

    all_songs =[]
    page = 1
    pagesize = 500  
    
    print(f"\n========== 开始拉取歌单 (specialid: {specialid}) ==========")
    while True:
        playlist_url = f"http://mobilecdn.kugou.com/api/v3/special/song?specialid={specialid}&page={page}&plat=2&pagesize={pagesize}&version=8400"
        try:
            response = requests.get(playlist_url, headers=headers, timeout=10)
            data = response.json()
            if data.get('status') != 1: break
            songs_list = data.get('data', {}).get('info',[])
            all_songs.extend(songs_list)
            if len(songs_list) < pagesize: break
            page += 1
            time.sleep(0.5)
        except Exception as e:
            print(f"❌ 翻页拉取异常: {e}")
            break

    print(f"\n✅ 歌单拉取完毕！共获取到 {len(all_songs)} 首歌曲")
    print("=" * 60)
    
    # 测试前 5 首
    test_limit = 5
    for idx, song in enumerate(all_songs[:test_limit]):
        song_name = song.get('filename', song.get('name', '未知歌曲'))
        song_hash = song.get('hash', '')
        album_id = str(song.get('album_id', song.get('albumid', '')))
        
        print(f"\n[第 {idx+1} 首] {song_name}")
        
        # 👑 【核心需求1】：生成完美的 Base64 唤起 Scheme
        perfect_scheme = generate_kugou_scheme(song)
        print(f"  ├─ 📱 神级唤起链接: {perfect_scheme}")

        # 🖼️ 【核心需求2】：获取高清封面
        cover_url = None
        if album_id and album_id != '0':
            album_url = f"http://mobilecdn.kugou.com/api/v3/album/info?albumid={album_id}"
            try:
                album_res = requests.get(album_url, headers=headers, timeout=10).json()
                if album_res.get('status') == 1 and album_res.get('data'):
                    raw_cover = album_res['data'].get('sizable_cover') or album_res['data'].get('imgurl', '')
                    if raw_cover:
                        cover_url = raw_cover.replace('{size}', '400')
                        print(f"  └─ 🖼️ 提取封面成功: {cover_url}")
            except Exception: pass
                
        if not cover_url and song_hash:
            fallback_url = f"http://m.kugou.com/app/i/getSongInfo.php?cmd=playInfo&hash={song_hash}"
            try:
                fallback_res = requests.get(fallback_url, headers=headers, timeout=10).json()
                raw_cover = fallback_res.get('imgUrl', fallback_res.get('pic', ''))
                if raw_cover:
                    cover_url = raw_cover.replace('{size}', '400')
                    print(f"  └─ 🖼️ 提取封面成功: {cover_url}")
            except Exception: pass

        if not cover_url:
             print(f"  └─ ⚠️ 该曲目完全没有配置封面图")
             
        time.sleep(1)

if __name__ == "__main__":
    TEST_SHARE_CODE = "7dM1z93FZV2"
    test_kugou_ultimate(TEST_SHARE_CODE)