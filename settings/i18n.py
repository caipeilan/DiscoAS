"""
i18n - Internationalization module for DiscoAS

Supports:
- zh_CN (Simplified Chinese)
- en_US (English)
"""

import os
import json
import sys

# Language configurations
LANGUAGES = {
    "zh_CN": "简体中文",
    "en_US": "English",
}

# Default language
DEFAULT_LANGUAGE = "zh_CN"

# Global state
_current_language = DEFAULT_LANGUAGE
_translations = {}


def get_i18n_dir():
    """Get the i18n directory path"""
    exe_name = os.path.basename(sys.executable).lower()
    is_packaged = exe_name not in ('python.exe', 'pythonw.exe', 'python')
    print(f"[i18n] executable={sys.executable}, is_packaged={is_packaged}")

    if is_packaged:
        # 打包环境：从 exe 所在目录找 settings/i18n
        exe_dir = os.path.dirname(sys.executable)
        path = os.path.join(exe_dir, "settings", "i18n")
        print(f"[i18n] packaged path: {path}, exists={os.path.exists(path)}")
        return path
    else:
        path = os.path.join(os.path.dirname(__file__), "i18n")
        print(f"[i18n] dev path: {path}, exists={os.path.exists(path)}")
        return path


def load_translations(lang_code):
    """Load translations for a specific language"""
    lang_file = os.path.join(get_i18n_dir(), f"{lang_code}.json")
    print(f"[i18n] load_translations({lang_code}): file={lang_file}, exists={os.path.exists(lang_file)}")
    if os.path.exists(lang_file):
        with open(lang_file, "r", encoding="utf-8") as f:
            data = json.load(f)
            print(f"[i18n] loaded {len(data)} keys")
            return data
    print(f"[i18n] load_translations FAILED - returning empty dict")
    return {}


def set_language(lang_code):
    """Set the current language"""
    global _current_language, _translations
    
    if lang_code in LANGUAGES:
        _current_language = lang_code
        _translations = load_translations(lang_code)
        return True
    return False


def get_language():
    """Get the current language code"""
    return _current_language


def get_language_name(lang_code):
    """Get the display name for a language code"""
    return LANGUAGES.get(lang_code, lang_code)


def get_available_languages():
    """Get all available languages"""
    return LANGUAGES.copy()


def t(key, default=None):
    """
    Translate a key to the current language
    
    Args:
        key: The translation key
        default: Default value if key not found
    
    Returns:
        Translated string
    """
    if not _translations:
        # Load translations if not loaded
        set_language(_current_language)

    result = _translations.get(key, default or key)
    print(f"[i18n] t('{key}') -> '{result}' (found={key in _translations}, total_keys={len(_translations)})")
    return result


def init_language(lang_code=None):
    """
    Initialize language from config or system default
    
    Args:
        lang_code: Language code to use (optional)
    """
    global _current_language
    
    # 1. 首先尝试从 gui_setting 读取保存的语言
    try:
        from .gui_setting import get_global_gui_setting
        gui_setting = get_global_gui_setting()
        saved_lang = getattr(gui_setting, 'language', None)
        print(f"[i18n] init_language step1: saved_lang={saved_lang}")
        if saved_lang and saved_lang in LANGUAGES:
            set_language(saved_lang)
            return
    except Exception as e:
        print(f"[i18n] init_language step1 FAILED: {e}")
    
    # 2. 如果没有保存的语言，使用传入的参数
    if lang_code and lang_code in LANGUAGES:
        set_language(lang_code)
    else:
        # 3. 尝试检测系统语言
        import locale
        try:
            system_lang = locale.getdefaultlocale()[0]
            if system_lang:
                if system_lang.startswith("zh"):
                    set_language("zh_CN")
                elif system_lang.startswith("en"):
                    set_language("en_US")
                else:
                    set_language(DEFAULT_LANGUAGE)
            else:
                set_language(DEFAULT_LANGUAGE)
        except Exception:
            set_language(DEFAULT_LANGUAGE)


# Initialize with default language
init_language()
