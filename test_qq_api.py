"""测试QQ音乐各种API端点"""
import requests
import json

headers = {
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Referer": "https://y.qq.com/",
}

# 测试各种端点
endpoints = [
    # 搜索API
    ("GET", "https://u.y.qq.com/cgi-bin/musicu.fcg", {
        "format": "json",
        "inCharset": "utf8", 
        "outCharset": "utf8",
        "notice": 0,
        "platform": "yqq",
        "needNewCode": 0,
        "data": json.dumps({
            "search": {
                "method": "DoSearchForQQMusic",
                "module": "music.search.SearchService", 
                "param": {
                    "query": "晴天",
                    "search_type": 10,
                    "nummeta": 50,
                    "offset": 0,
                    "limit": 1
                }
            }
        })
    }),
    # 歌曲详情API (签名版本)
    ("POST", "https://u.y.qq.com/cgi-bin/musics.fcg", {
        "sign": "test",
        "comm": {"ct": 11, "cv": 13020508},
        "music.trackInfo.UniformRuleCtrl": {
            "types": [0],
            "ids": [214126913]
        }
    }),
]

for method, url, params in endpoints:
    try:
        if method == "GET":
            r = requests.get(url, params=params, headers=headers, timeout=10)
        else:
            r = requests.post(url, json=params, headers=headers, timeout=10)
        
        print(f"\n=== {url} ===")
        print(f"Method: {method}")
        print(f"Response: {r.text[:300]}")
    except Exception as e:
        print(f"\n=== {url} ===")
        print(f"Error: {e}")
