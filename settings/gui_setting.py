import json, os, sys
from functools import lru_cache

# 导入统一的路径管理模块
from .user_data_path import get_gui_setting_path, init_user_data_dirs

# 全局设置实例，用于跨模块访问
_global_gui_setting = None

def get_global_gui_setting():
    """获取全局 GuiSetting 实例"""
    global _global_gui_setting
    if _global_gui_setting is None:
        _global_gui_setting = GuiSetting()
        _global_gui_setting.load()
    return _global_gui_setting

def reload_global_gui_setting():
    """重新加载全局设置"""
    global _global_gui_setting
    if _global_gui_setting is not None:
        _global_gui_setting.load()
    return _global_gui_setting

class GuiSetting:
    def __init__(self):
        # 初始化用户数据目录
        init_user_data_dirs()
        self.file_path = get_gui_setting_path()
        if not os.path.exists(self.file_path):
            self.create_default_setting()
        
        self.settings = None
        self.night_mode = False
        self.card_size = 1.0
        self.cancel_button_size = 1.0
        self.setting_size = 1.0
        
        # 语言设置
        self.language = "zh_CN"
        
        # 日间模式配置
        self.card = {}
        self.cancel_button = {}
        self.setting = {}
        
        # 夜间模式配置 (原代码缺失部分)
        self.card_night_mode = {}
        self.cancel_button_night_mode = {}
        self.setting_night_mode = {}

    def load(self):
        # 移除 lru_cache 以便实时重载，或者在保存时手动清除缓存
        # 这里为了简单，直接读取
        if os.path.exists(self.file_path):
            with open(self.file_path, "r", encoding="utf-8") as f:
                self.settings = json.load(f)
        else:
            self.settings = {}

        self.night_mode = self.settings.get("night_mode", False)
        self.card_size = self.settings.get("card_size", 1.0)
        self.cancel_button_size = self.settings.get("cancel_button_size", 1.0)
        self.setting_size = self.settings.get("setting_size", 1.0)
        
        # 加载语言设置
        self.language = self.settings.get("language", "zh_CN")
        
        self.card = self.settings.get("card", {})
        self.cancel_button = self.settings.get("cancel_button", {})
        self.setting = self.settings.get("setting", {})
        
        # 修复：显式加载夜间模式配置
        self.card_night_mode = self.settings.get("card_night_mode", {})
        self.cancel_button_night_mode = self.settings.get("cancel_button_night_mode", {})
        self.setting_night_mode = self.settings.get("setting_night_mode", {})

    def get(self, ParameterName):
        # 使得可以通过 get("card_night_mode") 访问属性
        if hasattr(self, ParameterName):
            return getattr(self, ParameterName)
        return self.settings.get(ParameterName, None)

    def set(self, ParameterName, ParameterValue):
        setattr(self, ParameterName, ParameterValue)

    def save(self):
        settings = {
            "night_mode": self.night_mode,
            "card_size": self.card_size,
            "cancel_button_size": self.cancel_button_size,
            "setting_size": self.setting_size,
            "language": self.language,
            "card": self.card,
            "cancel_button": self.cancel_button,
            "setting": self.setting,
            # 修复：保存夜间模式配置
            "card_night_mode": self.card_night_mode,
            "cancel_button_night_mode": self.cancel_button_night_mode,
            "setting_night_mode": self.setting_night_mode
        }
        with open(self.file_path, "w", encoding="utf-8") as f:
            json.dump(settings, f, ensure_ascii=False, indent=4)
            print("保存GUI设置成功")

    def create_default_setting(self):
        default_setting = {
            "night_mode": False,
            "card_size": 1.0,
            "cancel_button_size": 1.0,
            "setting_size": 1.0,
            "language": "zh_CN",
            "card": {
                "background": "#FFFFFF",
                "background_hover": "#e3f3f6",
                "border": "#76d2fd",
                "font_color": "#000000"
            },
            "cancel_button": {
                "background": "#fecbc1",
                "background_hover": "#fd8b76",
                "border": "#fc6044",
                "font_color": "#000000"
            },
            "setting": {
                "background": "#FFFFFF",
                "background_hover": "#d0ebf0",
                "border": "#76e8fd",
                "font_color": "#000000"
            },
            "card_night_mode": {
                "background": "#565656",
                "background_hover": "#3d75bf",
                "border": "#76d2fd",
                "font_color": "#ffffff"
            },
            "cancel_button_night_mode": {
                "background": "#400601",
                "background_hover": "#bd0316",
                "border": "#fc6044",
                "font_color": "#ffffff"
            },
            "setting_night_mode": {
                "size": 1.0,
                "background": "#565656",
                "background_hover": "#3dabbf",
                "border": "#76c6fd",
                "font_color": "#ffffff"
            }
        }
        with open(self.file_path, "w", encoding="utf-8") as f:
            json.dump(default_setting, f, ensure_ascii=False, indent=4)
            print("创建默认GUI设置成功")
