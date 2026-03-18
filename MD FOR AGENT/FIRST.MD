# DiscoAS 代码风格指南 (FIRST.MD)

本文档是 DiscoAS 项目的代码风格总纲，所有 agent 在编写或修改代码时应遵循此规范。

---

## 1. 核心原则

### 1.1 代码可读性优先

- 优先选择清晰的命名，避免过度简化
- 适当的注释说明**为什么**而不是**做了什么**
- 保持函数短小，单一职责

### 1.2 不要过度工程

- 不创建短期内用不到的抽象
- 不添加未来可能需要的配置选项
- 避免过度设计，保持简单

### 1.3 遵循项目现有风格

- 新代码应与现有代码风格一致
- 修改文件时，先阅读该文件的风格

---

## 2. 命名规范

### 2.1 文件命名

| 类型 | 规范 | 示例 |
|------|------|------|
| Python 模块 | 小写下划线 | `setting_gui.py`, `music_setting.py` |
| 包目录 | 小写下划线 | `settings/`, `platforms/` |

### 2.2 类命名

| 类型 | 规范 | 示例 |
|------|------|------|
| 类名 | 大驼峰 | `DiscoverApp`, `DiscoverOverlay`, `SettingsWindow` |
| Qt 组件 | 同上 | `SongCardWidget`, `SplashScreen` |

### 2.3 函数/方法命名

| 类型 | 规范 | 示例 |
|------|------|------|
| 普通方法 | 小写下划线 | `def load_settings()`, `def run_gui()` |
| Qt 信号/槽 | 小写下划线 | `def songs_loaded(self, list)` |
| 私有方法 | 单下划线前缀 | `def _setup_ui(self)` |

### 2.4 变量命名

| 类型 | 规范 | 示例 |
|------|------|------|
| 普通变量 | 小写下划线 | `songs_list`, `image_path` |
| Qt 组件 | 小写下划线 | `self.btn_save`, `self.table_pl` |
| 全局变量 | 双下划线前缀 | `_tray_icon`, `_main_window` |
| 常量 | 全大写下划线 | `MAX_SONGS = 100` |

---

## 3. 导入规范

### 3.1 导入顺序

```python
# 1. 标准库
import sys
import os
import time

# 2. 第三方库
from PyQt6.QtWidgets import QApplication
from PyQt6.QtCore import Qt

# 3. 项目内部模块
from main import DiscoverApp
from settings.setting_gui import SettingsWindow

# 4. 相对导入
from . import module
```

### 3.2 避免循环导入

- 使用延迟导入（在函数内部导入）
- 或使用 `import module` 而非 `from module import something`

---

## 4. 代码结构

### 4.1 类结构

```python
class ExampleClass:
    """类的简短描述"""

    def __init__(self, param1, param2):
        """初始化方法"""
        self.param1 = param1
        self.param2 = param2
        self._internal_state = None
        self._setup_ui()

    def _setup_ui(self):
        """设置 UI 组件"""
        pass

    def public_method(self):
        """公开方法的文档字符串"""
        pass

    def _private_method(self):
        """私有方法的文档字符串"""
        pass
```

### 4.2 文档字符串

- 类和公开方法使用英文或中文 Docstring
- 简短描述即可，无需过度详细

---

## 5. PyQt6 特定规范

### 5.1 信号与槽

```python
# 定义信号
class MyWidget(QWidget):
    my_signal = pyqtSignal(str)

# 连接信号
self.my_signal.connect(self.handle_signal)

# 发射信号
self.my_signal.emit("data")
```

### 5.2 Qt 命名约定

| 组件 | 命名建议 | 示例 |
|------|---------|------|
| 主窗口 | `self.window` | `self.main_window` |
| 按钮 | `self.btn_*` | `self.btn_save`, `self.btn_cancel` |
| 标签 | `self.lbl_*` | `self.lbl_title` |
| 输入框 | `self.input_*` | `self.input_name` |
| 表格 | `self.table_*` | `self.table_data` |

### 5.3 窗口标志

```python
# 无边框窗口
self.setWindowFlags(
    Qt.WindowType.FramelessWindowHint |
    Qt.WindowType.WindowStaysOnTopHint
)

# 透明背景
self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
```

---

## 6. 错误处理

### 6.1 原则

- 仅在系统边界（用户输入、外部 API）进行验证
- 内部函数信任调用者传递正确的参数
- 不添加过度防御性代码

### 6.2 异常处理

```python
try:
    result = risky_operation()
except SpecificException as e:
    # 处理特定异常
    print(f"操作失败: {e}")
except Exception:
    # 捕获所有其他异常
    print("发生未知错误")
    raise  # 重新抛出，让上层处理
```

---

## 7. 日志与调试

### 7.1 打印语句

项目使用自定义的 `print` 函数，自动同时输出到控制台和日志文件：

```python
print("信息日志")
print(f"调试信息: {variable}")
print(f"错误: {e}")
```

### 7.2 日志文件位置

- 路径：`user_data/log/`
- 格式：`app_YYYYMMDD.log`

---

## 8. 配置管理

### 8.1 设置存储

- 位置：`user_data/settings/`
- 格式：JSON 文件
- 编码：UTF-8

### 8.2 设置类

使用专用的设置类管理配置：

- `music_setting.py` - 音乐相关设置
- `gui_setting.py` - GUI 外观设置

---

## 9. 国际化 (i18n)

### 9.1 翻译函数

使用统一的翻译函数：

```python
from settings.i18n import gettext as _

# 在代码中使用
label.setText(_("hello_world"))
```

### 9.2 翻译文件

- 位置：`settings/i18n/`
- 格式：JSON 文件
- 文件：`zh_CN.json`, `en_US.json`

---

## 10. 打包注意事项

### 10.1 路径处理

```python
import sys
import os

if getattr(sys, 'frozen', False):
    # 打包后的环境
    base_dir = os.path.dirname(sys.executable)
else:
    # 开发环境
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
```

### 10.2 资源文件

- 开发时使用项目根目录的 `src/`
- 打包后使用 `sys._MEIPASS` 下的资源

---

## 11. 其他规范

### 11.1 行长度

- 推荐不超过 120 字符
- 超过时合理换行

### 11.2 空行

- 类之间空 2 行
- 方法之间空 1 行
- 逻辑相关的代码块之间空 1 行

### 11.3 缩进

- 使用 4 空格
- 不使用 Tab

---

## 12. 修改现有代码

当修改现有文件时：

1. **先阅读**：理解现有代码的风格和逻辑
2. **保持一致**：新代码与现有代码风格一致
3. **最小改动**：只修改必要的部分，不过度重构
4. **添加注释**：如果修改逻辑复杂，添加说明

---

## 13. 文档更新规范

### 13.1 及时更新文档

**重要**：在操作者（用户）确认模型完成任务后，应及时更新相关文档：

- 如果任务涉及新功能 → 更新 [PRD.md](PRD.md)
- 如果任务涉及界面/流程变化 → 更新 [APP_FLOW.md](APP_FLOW.md)
- 如果任务涉及视觉/样式变化 → 更新 [UI.md](UI.md)
- 如果任务有进度进展 → 更新 [PROGRESS.md](PROGRESS.md)
- 如果任务涉及代码风格变化 → 更新本文档

### 13.2 更新时机

- 任务完成并经用户确认后，立即更新文档
- 不要等到后续再补
- 确保后续的 agent 能通过文档了解最新状态

### 13.3 文档位置

所有文档位于 `MD FOR AGENT/` 文件夹中。

---

## 14. 相关文档

- [PRD.md](PRD.md) - 项目需求文档
- [APP_FLOW.md](APP_FLOW.md) - 软件页面导航
- [UI.md](UI.md) - 视觉设计规范
- [PROGRESS.md](PROGRESS.md) - 当前进度
