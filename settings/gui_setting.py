import json, os, sys
from functools import lru_cache

class GuiSetting:
    def __init__(self):
        self.file_path = os.path.join(os.path.dirname(__file__), "gui_setting.json")
        if not os.path.exists(self.file_path):
            self.create_default_setting()
        
        self.settings = None
        self.night_mode = False
        self.card_size = 1.0
        self.cancel_button_size = 1.0
        self.setting_size = 1.0
        
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
            "card": {
                "background": "#FFFFFF",
                "background_hover": "#d0ebf0",
                "border": "#76e8fd",
                "font_color": "#000000"
            },
            "cancel_button": {
                "background": "#FFFFFF",
                "background_hover": "#f5d5d0",
                "border": "#d6533e",
                "font_color": "#000000"
            },
            "setting": {
                "background": "#FFFFFF",
                "font_color": "#000000"
            },
            "card_night_mode": {
                "background": "#444444",
                "background_hover": "#3dabbf",
                "border": "#76c6fd",
                "font_color": "#ffffff"
            },
            "cancel_button_night_mode": {
                "background": "#444444",
                "background_hover": "#d6533e",
                "border": "#d6533e",
                "font_color": "#ffffff"
            },
            "setting_night_mode": {
                "size": 1.0,
                "background": "#444444",
                "font_color": "#ffffff"
            }
        }
        with open(self.file_path, "w", encoding="utf-8") as f:
            json.dump(default_setting, f, ensure_ascii=False, indent=4)
            print("创建默认GUI设置成功")