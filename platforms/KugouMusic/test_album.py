import requests
import time
import re
import json
import base64

def resolve_album_share_code(share_code, headers):
    """
    解析专辑的分享码，提取 album_id
    """
    share_url = f"https://t.kugou.com/song.html?id={share_code}"
    print(f"🔍 正在解析专辑分享码: {share_code} ...")
    try:
        response = requests.get(share_url, headers=headers, allow_redirects=True, timeout=10)
        response.raise_for_status()
        
        # 兼容匹配专辑的 URL 路由 (例如 /album/info/12345 或 /album/12345)
        url_match = re.search(r'/album/(?:info/)?(\d+)', response.url)
        if url_match: return url_match.group(1)
        
        # 从网页源码中提取 albumid
        html_match = re.search(r'["\']?album[_]?id["\']?\s*[:=]\s*["\']?(\d+)["\']?', response.text, re.IGNORECASE)
        if html_match: return html_match.group(1)
        
        # 有些直接分享链接带了 album_id 参数
        query_match = re.search(r'album_id=(\d+)', response.url)
        if query_match: return query_match.group(1)
        
        print("❌ 解析分享码失败，未找到 album_id")
        return None
    except Exception as e:
        print(f"❌ 解析异常: {e}")
        return None

def generate_minimal_scheme(song):
    """
    ⭐ 核心：根据您的伟大发现，使用最小化必要参数生成 Base64 Scheme
    """
    filename = song.get('filename', song.get('name', '未知歌曲'))
    # 确保加上 .mp3 后缀（酷狗内部识别需要）
    if not filename.lower().endswith('.mp3'):
        filename += ".mp3"

    song_hash = song.get('hash', '')

    # 最小化核心 JSON 结构
    payload = {
        "Files":[
            {
                "filename": filename,
                "hash": song_hash
            }
        ]
    }
    
    json_str = json.dumps(payload, separators=(',', ':'), ensure_ascii=False)
    b64_str = base64.b64encode(json_str.encode('utf-8')).decode('utf-8')
    
    return f"kugou://play?p={b64_str}"

def test_kugou_album_ultimate(share_code):
    headers = {
        "User-Agent": "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1"
    }

    # 1. 获取 Album ID
    album_id = resolve_album_share_code(share_code, headers)
    if not album_id:
        # 如果您平时直接有纯数字专辑ID，也可以直接跳过解析，直接赋值 album_id = "纯数字"
        return

    print(f"\n========== 开始获取专辑信息 (album_id: {album_id}) ==========")
    
    # 2. 获取专辑封面 (仅需请求一次！)
    album_cover_url = None
    album_name = "未知专辑"
    album_info_url = f"http://mobilecdn.kugou.com/api/v3/album/info?albumid={album_id}"
    
    try:
        info_res = requests.get(album_info_url, headers=headers, timeout=10).json()
        if info_res.get('status') == 1 and info_res.get('data'):
            album_name = info_res['data'].get('albumname', '未知专辑')
            raw_cover = info_res['data'].get('sizable_cover') or info_res['data'].get('imgurl', '')
            if raw_cover:
                album_cover_url = raw_cover.replace('{size}', '400') # 替换为 400x400 高清图
    except Exception as e:
        print(f"⚠️ 获取专辑封面时发生异常: {e}")

    print(f"💿 专辑名称: 《{album_name}》")
    print(f"🖼️ 专辑封面 (全局通用): {album_cover_url if album_cover_url else '无封面'}")
    print("=" * 60)

    # 3. 翻页获取专辑内的所有歌曲
    all_songs =[]
    page = 1
    pagesize = 500  
    
    while True:
        # 专辑歌曲拉取接口 (与歌单接口类似，只是 specialid 变成了 albumid)
        album_songs_url = f"http://mobilecdn.kugou.com/api/v3/album/song?albumid={album_id}&page={page}&plat=2&pagesize={pagesize}&version=8400"
        try:
            response = requests.get(album_songs_url, headers=headers, timeout=10)
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

    print(f"✅ 专辑曲目拉取完毕！共获取到 {len(all_songs)} 首歌曲\n")
    
    # 4. 遍历打印并生成神级 Scheme
    # 取前 5 首作为测试展示
    test_limit = 5
    for idx, song in enumerate(all_songs[:test_limit]):
        song_name = song.get('filename', song.get('name', '未知歌曲'))
        
        print(f"[第 {idx+1} 首] {song_name}")
        
        # 👑 调用刚刚优化的最小化极简 Scheme 生成器
        perfect_scheme = generate_minimal_scheme(song)
        print(f"  ├─ 📱 极简唤起链接: {perfect_scheme}")
        
        # 封面直接使用上面获取到的公共封面
        print(f"  └─ 🖼️ 对应封面图: {album_cover_url}")
        print("-" * 40)

if __name__ == "__main__":
    # 您可以传入专辑的分享码，或者稍加修改直接传入数字的 album_id
    # 这里先放一个占位符，您替换为您实际想要抓取的专辑分享码即可
    TEST_ALBUM_SHARE_CODE = "7DYln48FZV2" 
    test_kugou_album_ultimate(TEST_ALBUM_SHARE_CODE)