"""
调试脚本：查看酷狗 API 返回的 JSON 格式
"""
import requests
import re

KUGOU_USER_AGENT = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1"

def resolve_share_code(share_code):
    share_url = f"https://t.kugou.com/song.html?id={share_code}"
    print(f"🔍 正在解析分享码: {share_code} ...")
    try:
        response = requests.get(share_url, headers={"User-Agent": KUGOU_USER_AGENT}, allow_redirects=True, timeout=10)
        response.raise_for_status()

        url_match = re.search(r'/(?:plist/list|songlist)/(\d+)', response.url)
        if url_match:
            return url_match.group(1)

        html_match = re.search(r'["\']?(?:special[_]?id|global_specialid)["\']?\s*[:=]\s*["\']?(\d+)["\']?', response.text, re.IGNORECASE)
        if html_match:
            return html_match.group(1)

        return None
    except Exception as e:
        print(f"❌ 解析异常: {e}")
        return None

def main():
    share_code = "7iuxla3FZV2"  # 测试用分享码

    specialid = resolve_share_code(share_code)
    if not specialid:
        print("❌ 获取 specialid 失败")
        return

    print(f"\n✅ specialid: {specialid}")

    # 获取第一页
    url = f"http://mobilecdn.kugou.com/api/v3/special/song"
    params = {
        "specialid": specialid,
        "page": 1,
        "plat": 2,
        "pagesize": 10,
        "version": 8400,
    }

    response = requests.get(url, params=params, timeout=10)
    data = response.json()

    print("\n📋 JSON 格式:")
    print("=" * 60)

    import json
    print(json.dumps(data, ensure_ascii=False, indent=2))

    print("\n" + "=" * 60)

    # 检查关键字段
    print("\n🔍 关键字段检查:")
    print(f"  status: {data.get('status')}")
    print(f"  info (顶层): {type(data.get('info'))}")
    print(f"  data: {type(data.get('data'))}")

    if data.get('info'):
        print(f"\n  info[0]: {data['info'][0] if data['info'] else '空'}")
        if data['info']:
            print(f"  specialname: {data['info'][0].get('specialname')}")

if __name__ == "__main__":
    main()
