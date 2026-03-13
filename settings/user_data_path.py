"""
用户数据路径管理模块

统一管理所有用户数据的存储路径，支持打包后的环境
"""

import os
import sys


def get_app_root():
    """
    获取应用根目录
    打包后使用 exe 所在目录，未打包使用脚本所在目录
    """
    if getattr(sys, 'frozen', False):
        # 打包后的环境 - 使用 exe 所在目录
        return os.path.dirname(sys.executable)
    else:
        # 开发环境
        return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def get_resource_dir():
    """
    获取资源目录（src, settings, platforms, log）
    打包后在 sys._MEIPASS，开发环境在脚本目录
    """
    if getattr(sys, 'frozen', False):
        # 打包后的环境 - 资源在 sys._MEIPASS
        return sys._MEIPASS
    else:
        # 开发环境
        return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def get_user_data_dir():
    """
    获取用户数据根目录
    """
    return os.path.join(get_app_root(), "user_data")


def get_settings_dir():
    """获取设置目录"""
    return os.path.join(get_user_data_dir(), "settings")


def get_platform_dir(platform_name):
    """
    获取指定平台的数据目录
    
    Args:
        platform_name: 平台名称 (NeteaseCloudMusic, QQMusic, Spotify)
    
    Returns:
        平台数据目录路径
    """
    return os.path.join(get_user_data_dir(), platform_name)


def get_playlist_dir(platform_name):
    """获取指定平台歌单目录"""
    return os.path.join(get_platform_dir(platform_name), "playlist")


def get_album_dir(platform_name):
    """获取指定平台专辑目录"""
    return os.path.join(get_platform_dir(platform_name), "album")


def ensure_dir(path):
    """
    确保目录存在，不存在则创建
    
    Args:
        path: 目录路径
    """
    os.makedirs(path, exist_ok=True)


def get_music_setting_path():
    """获取音乐设置文件路径"""
    path = get_settings_dir()
    ensure_dir(path)
    return os.path.join(path, "music_setting.json")


def get_gui_setting_path():
    """获取GUI设置文件路径"""
    path = get_settings_dir()
    ensure_dir(path)
    return os.path.join(path, "gui_setting.json")


# 初始化确保目录存在
def init_user_data_dirs():
    """初始化所有用户数据目录"""
    ensure_dir(get_settings_dir())
    ensure_dir(get_playlist_dir("NeteaseCloudMusic"))
    ensure_dir(get_album_dir("NeteaseCloudMusic"))
    ensure_dir(get_playlist_dir("QQMusic"))
    ensure_dir(get_album_dir("QQMusic"))
    print(f"用户数据目录: {get_user_data_dir()}")


if __name__ == "__main__":
    # 测试路径
    print("=== 用户数据路径测试 ===")
    print(f"应用根目录: {get_app_root()}")
    print(f"用户数据目录: {get_user_data_dir()}")
    print(f"设置目录: {get_settings_dir()}")
    print(f"网易云歌单目录: {get_playlist_dir('NeteaseCloudMusic')}")
    print(f"QQ音乐专辑目录: {get_album_dir('QQMusic')}")
    print(f"音乐设置文件: {get_music_setting_path()}")
    print(f"GUI设置文件: {get_gui_setting_path()}")
