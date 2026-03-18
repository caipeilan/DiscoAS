"""
调试脚本：查看酷狗 API 返回的 JSON 格式
"""
import requests
import json

KUGOU_USER_AGENT = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1"


def main():
    import os
    specialid = "7365552"  # 直接使用 specialid

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

    # 保存完整 JSON 到文件
    output_dir = os.path.dirname(__file__)
    output_path = os.path.join(output_dir, f"debug_{specialid}.json")
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
    print(f"✅ 完整 JSON 已保存到: {output_path}")


if __name__ == "__main__":
    main()
