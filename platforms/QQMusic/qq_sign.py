"""
QQ音乐签名算法

基于 qqmusic-api-python 库的签名算法实现
"""

import re
import base64
import hashlib
import orjson


# 签名参数
PART_1_INDEXES = [23, 14, 6, 36, 16, 40, 7, 19]
PART_2_INDEXES = [16, 1, 32, 12, 19, 27, 8, 5]
SCRAMBLE_VALUES = [89, 39, 179, 150, 218, 82, 58, 252, 177, 52, 186, 123, 120, 64, 242, 133, 143, 161, 121, 179]

# JavaScript quirks emulation - 过滤掉大于等于40的索引
PART_1_INDEXES = list(filter(lambda x: x < 40, PART_1_INDEXES))


def sign(request: dict) -> str:
    """
    QQ音乐 请求签名
    
    Args:
        request: 请求数据字典
        
    Returns:
        签名结果
    """
    # 将字典转换为 JSON 字符串并计算 SHA1 哈希
    json_str = orjson.dumps(request)
    hash_hex = hashlib.sha1(json_str).hexdigest().upper()
    
    # 提取 part1
    part1 = "".join(hash_hex[i] for i in PART_1_INDEXES)
    
    # 提取 part2
    part2 = "".join(hash_hex[i] for i in PART_2_INDEXES)
    
    # 计算 part3
    part3 = bytearray(20)
    for i, v in enumerate(SCRAMBLE_VALUES):
        value = v ^ int(hash_hex[i * 2 : i * 2 + 2], 16)
        part3[i] = value
    
    # Base64 编码并移除特殊字符
    b64_part = re.sub(rb"[\\/+=]", b"", base64.b64encode(part3)).decode("utf-8")
    
    # 组合最终签名
    return f"zzc{part1}{b64_part}{part2}".lower()


# API 配置
API_CONFIG = {
    "version": "13.2.5.8",
    "version_code": 13020508,
    "endpoint": "https://u.y.qq.com/cgi-bin/musicu.fcg",
    "enc_endpoint": "https://u.y.qq.com/cgi-bin/musics.fcg",
}

# 默认QIMEI36（设备标识符）
DEFAULT_QIMEI36 = "6c9d3cd110abca9b16311cee10001e717614"

# 公共参数
COMMON_PARAMS = {
    "ct": "11",
    "cv": API_CONFIG["version"],
    "v": API_CONFIG["version_code"],
    "tmeAppID": "qqmusic",
    "format": "json",
    "inCharset": "utf-8",
    "outCharset": "utf-8",
    "uid": "3931641530",
    "QIMEI36": DEFAULT_QIMEI36,
}

# 请求头
DEFAULT_HEADERS = {
    "User-Agent": "Mozilla/5.0 (Windows NT 11.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Safari/537.36 Edg/116.0.1938.54",
    "Referer": "https://y.qq.com/",
    "Content-Type": "application/json",
}


def build_request_data(module: str, method: str, params: dict) -> dict:
    """
    构建请求数据
    
    Args:
        module: API 模块名
        method: API 方法名
        params: 请求参数字典
        
    Returns:
        完整的请求数据字典
    """
    return {
        "comm": COMMON_PARAMS.copy(),
        f"{module}.{method}": {
            "module": module,
            "method": method,
            "param": params,
        }
    }


def make_api_request(module: str, method: str, params: dict, use_sign: bool = False) -> dict:
    """
    发起 API 请求
    
    Args:
        module: API 模块名
        method: API 方法名
        params: 请求参数字典
        use_sign: 是否使用签名（默认不使用，QIMEI已足够）
        
    Returns:
        响应数据字典
    """
    import requests
    
    # 构建请求数据（不带签名）
    data = build_request_data_without_sign(module, method, params)
    
    # 选择端点（不使用签名端点）
    endpoint = API_CONFIG["endpoint"]
    
    response = requests.post(
        endpoint,
        json=data,
        headers=DEFAULT_HEADERS,
        timeout=10
    )
    
    # 解析响应
    response.raise_for_status()
    result = response.json()
    
    # 提取数据
    key = f"{module}.{method}"
    if key in result:
        # 检查业务状态码
        module_result = result[key]
        if module_result.get("code") == 0:
            # 返回 data 字段（包含 tracks）
            return module_result.get("data", module_result)
        else:
            # 返回完整结果供调试
            return module_result
    return result


def build_request_data_without_sign(module: str, method: str, params: dict) -> dict:
    """
    构建不带签名的请求数据（使用QIMEI）
    
    Args:
        module: API 模块名（如 music.trackInfo.UniformRuleCtrl）
        method: API 方法名（如 CgiGetTrackInfo）
        params: 请求参数字典
        
    Returns:
        完整的请求数据字典
    """
    # 关键：使用完整的module名作为key
    return {
        "comm": COMMON_PARAMS.copy(),
        f"{module}": {
            "module": module,
            "method": method,
            "param": params,
        }
    }


if __name__ == "__main__":
    # 测试签名
    test_data = {
        "comm": {"ct": "11", "cv": "13020508"},
        "music.srfDissInfo.DissInfo": {
            "disstid": "9595891286",
            "song_begin": 0,
            "song_num": 10,
        }
    }
    print("测试签名:", sign(test_data))
