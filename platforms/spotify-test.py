import requests

def get_anonymous_token():
    """
    通过模拟浏览器访问 Spotify 网页端内部接口，白嫖一个临时 Token。
    完全不需要开发者账号、不需要登录。
    """
    # 这是 Spotify 网页端用来获取临时游客 Token 的内部接口
    url = "https://open.spotify.com/get_access_token?reason=transport&productType=web_player"
    
    # 必须伪装成普通浏览器，否则会被 Spotify 拦截 (返回 403 或 401)
    headers = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        "Accept": "application/json"
    }
    
    response = requests.get(url, headers=headers)
    if response.status_code == 200:
        data = response.json()
        print("成功获取匿名 Token！")
        return data.get("accessToken")
    else:
        print(f"获取匿名 Token 失败，状态码: {response.status_code}")
        return None

# ================= 测试白嫖 Token =================
if __name__ == "__main__":
    # 1. 拿到白嫖的 Token
    anonymous_token = get_anonymous_token()
    
    if anonymous_token:
        # 2. 构造请求头
        HEADERS = {
            "Authorization": f"Bearer {anonymous_token}"
        }
        
        # 3. 直接复用之前的官方 Web 接口！
        # （这里以获取单曲信息为例，获取歌单的代码与上一个回答完全一样）
        track_id = '4cOdK2wGLETKBW3PvgPWqT' # 一首歌的ID
        
        api_url = f"https://api.spotify.com/v1/tracks/{track_id}"
        res = requests.get(api_url, headers=HEADERS)
        
        if res.status_code == 200:
            data = res.json()
            song_name = data.get('name')
            artists = [artist['name'] for artist in data.get('artists',[])]
            cover_url = data.get('album', {}).get('images', [{}])[0].get('url', '无')
            
            print(f"\n--- 单曲详情 ---")
            print(f"歌名: {song_name}")
            print(f"歌手: {', '.join(artists)}")
            print(f"封面链接: {cover_url}")
        else:
            print("请求歌曲信息失败:", res.status_code)