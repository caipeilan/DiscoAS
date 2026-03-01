import sys
import os
import importlib
import datetime
from PyQt6.QtWidgets import (QApplication, QMainWindow, QWidget, QVBoxLayout, 
                             QHBoxLayout, QTabWidget, QLabel, QSpinBox, 
                             QCheckBox, QLineEdit, QTableWidget, QTableWidgetItem, 
                             QPushButton, QHeaderView, QGroupBox, 
                             QFormLayout, QScrollArea, QComboBox, QMessageBox, 
                             QDoubleSpinBox, QButtonGroup, QSlider)
from PyQt6.QtGui import QColor, QAction, QFont
from PyQt6.QtCore import Qt, pyqtSignal

# 导入你修改后的类
try:
    from music_setting import PASetting, PlaylistAlbum
    from gui_setting import GuiSetting
except ImportError:
    print("错误: 无法导入设置类，请确保 music_setting.py 和 gui_setting.py 在同一目录下。")
    sys.exit(1)

class ColorPreviewWidget(QWidget):
    """用于显示颜色的小方块 (纯展示，无点击事件)"""
    def __init__(self, color_hex, parent=None):
        super().__init__(parent)
        self.setFixedSize(24, 24)
        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground, True)
        self.color_hex = color_hex if color_hex else "#FFFFFF"
        self.update_style()
        
    def update_style(self):
        if not QColor(self.color_hex).isValid():
            display_color = "#FFFFFF"
        else:
            display_color = self.color_hex
            
        self.setStyleSheet(f"""
            background-color: {display_color}; 
            border: 1px solid #888; 
            border-radius: 3px;
        """)
    
    def set_color(self, color_hex):
        self.color_hex = color_hex
        self.update_style()

class FloatSlider(QWidget):
    """
    自定义控件：带有数值显示的浮点数滑动条
    QSlider 本身只支持整数，这里通过倍率转换实现浮点控制
    """
    valueChanged = pyqtSignal(float)

    def __init__(self, initial_value, min_val=0.5, max_val=3.0, parent=None):
        super().__init__(parent)
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        # 滑动条 (内部使用整数，精度 0.01 -> 放大100倍)
        self.slider = QSlider(Qt.Orientation.Horizontal)
        self.slider.setRange(int(min_val * 100), int(max_val * 100))
        self.slider.setValue(int(initial_value * 100))
        self.slider.setTracking(True) # 允许实时触发

        # 数值显示标签
        self.label = QLabel(f"{initial_value:.2f}")
        self.label.setFixedWidth(40) # 固定宽度防止抖动
        self.label.setAlignment(Qt.AlignmentFlag.AlignCenter)

        layout.addWidget(self.slider)
        layout.addWidget(self.label)

        self.slider.valueChanged.connect(self._on_slider_changed)

    def _on_slider_changed(self, int_value):
        float_value = int_value / 100.0
        self.label.setText(f"{float_value:.2f}")
        self.valueChanged.emit(float_value)

    def value(self):
        return self.slider.value() / 100.0

    def setValue(self, value):
        self.slider.setValue(int(value * 100))


class SettingsWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        
        # 1. 加载后端逻辑
        self.pa_setting = PASetting()
        self.pa_setting.load()
        
        self.gui_setting = GuiSetting()
        self.gui_setting.load()

        # 2. 初始化界面
        self.setWindowTitle("编辑")
        self.resize(1050, 750)
        
        self.central_widget = QWidget()
        self.setCentralWidget(self.central_widget)
        self.main_layout = QVBoxLayout(self.central_widget)
        
        # 选项卡
        self.tabs = QTabWidget()
        self.main_layout.addWidget(self.tabs)
        
        self.init_music_tab()
        self.init_gui_tab()
        
        # 底部按钮
        self.bottom_layout = QHBoxLayout()
        
        self.btn_apply = QPushButton("应用并保存 (Apply & Save)")
        self.btn_apply.setCursor(Qt.CursorShape.PointingHandCursor)
        self.btn_apply.setMinimumHeight(40)
        self.btn_apply.clicked.connect(self.save_all_settings)
        
        self.btn_close = QPushButton("关闭 (Close)")
        self.btn_close.setMinimumHeight(40)
        self.btn_close.clicked.connect(self.close)
        
        self.bottom_layout.addStretch()
        self.bottom_layout.addWidget(self.btn_apply)
        self.bottom_layout.addWidget(self.btn_close)
        self.main_layout.addLayout(self.bottom_layout)

        # 初始渲染
        self.apply_gui_theme()

    def init_music_tab(self):
        self.music_tab = QWidget()
        self.tabs.addTab(self.music_tab, "发现设置")
        layout = QVBoxLayout(self.music_tab)
        
        # --- 参数区 ---
        group_basic = QGroupBox("基础参数")
        form_basic = QFormLayout(group_basic)
        form_basic.setFieldGrowthPolicy(QFormLayout.FieldGrowthPolicy.FieldsStayAtSizeHint)
        
        self.spin_discovered = QSpinBox()
        self.spin_discovered.setRange(1, 999)
        self.spin_discovered.setValue(self.pa_setting.number_of_discovered_songs)
        self.spin_discovered.setButtonSymbols(QSpinBox.ButtonSymbols.NoButtons)
        self.spin_discovered.setMinimumWidth(120)
        form_basic.addRow("发现歌曲数量:", self.spin_discovered)
        
        self.chk_mystery = QCheckBox("启用秘密歌曲 (Mystery Song)")
        self.chk_mystery.setChecked(self.pa_setting.have_mystery_song)
        form_basic.addRow(self.chk_mystery)
        
        self.spin_mystery_num = QSpinBox()
        self.spin_mystery_num.setRange(0, 50)
        self.spin_mystery_num.setValue(self.pa_setting.num_of_mystery_song)
        self.spin_mystery_num.setButtonSymbols(QSpinBox.ButtonSymbols.NoButtons)
        self.spin_mystery_num.setMinimumWidth(120)
        form_basic.addRow("秘密歌曲数量:", self.spin_mystery_num)
        
        self.chk_overlap = QCheckBox("允许重复 (Overlap)")
        self.chk_overlap.setChecked(self.pa_setting.overlap)
        form_basic.addRow(self.chk_overlap)
        
        self.chk_refresh = QCheckBox("取消选择后刷新")
        self.chk_refresh.setChecked(self.pa_setting.refreshing_after_cancel)
        form_basic.addRow(self.chk_refresh)
        
        self.edit_shortcut = QLineEdit()
        self.edit_shortcut.setText(self.pa_setting.shortcut_key)
        self.edit_shortcut.setPlaceholderText("例如: Alt+D")
        form_basic.addRow("全局快捷键:", self.edit_shortcut)
        
        layout.addWidget(group_basic)
        
        # --- 列表区 ---
        group_list = QGroupBox("歌单/专辑管理")
        v_list = QVBoxLayout(group_list)
        
        self.table_pl = QTableWidget()
        self.table_pl.setColumnCount(6)
        self.table_pl.setHorizontalHeaderLabels(["平台", "ID", "类型", "名称/备注", "启用", "操作"])
        self.table_pl.horizontalHeader().setSectionResizeMode(3, QHeaderView.ResizeMode.Stretch)
        self.table_pl.horizontalHeader().setSectionResizeMode(5, QHeaderView.ResizeMode.Fixed)
        self.table_pl.setColumnWidth(5, 180)
        self.table_pl.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table_pl.verticalHeader().setDefaultSectionSize(50)  # 设置默认行高
        v_list.addWidget(self.table_pl)
        
        h_btn = QHBoxLayout()
        btn_add = QPushButton("添加歌单")
        btn_add.clicked.connect(self.add_playlist_row)
        h_btn.addWidget(btn_add)
        h_btn.addStretch()
        v_list.addLayout(h_btn)
        
        layout.addWidget(group_list)
        
        # 加载数据
        self.load_playlist_table()

    def load_playlist_table(self):
        self.table_pl.setRowCount(0)
        for pl in self.pa_setting.playlist_albums:
            self.add_playlist_row(data=pl)

    def add_playlist_row(self, data=None):
        row = self.table_pl.rowCount()
        self.table_pl.insertRow(row)
        
        # 1. Platform
        cmb_platform = QComboBox()
        cmb_platform.addItems(["NeteaseCloudMusic", "QQMusic"])
        if data: cmb_platform.setCurrentText(data.name)
        self.table_pl.setCellWidget(row, 0, cmb_platform)
        
        # 2. ID
        edit_id = QLineEdit()
        if data: edit_id.setText(str(data.playlist_album_id))
        self.table_pl.setCellWidget(row, 1, edit_id)
        
        # 3. Type
        cmb_type = QComboBox()
        cmb_type.addItems(["playlist", "album"])
        if data: cmb_type.setCurrentText(data.typename)
        self.table_pl.setCellWidget(row, 2, cmb_type)
        
        # 4. Remark
        edit_remark = QLineEdit()
        if data: 
            val = data.playlist_album_remark if data.playlist_album_remark else data.playlist_album_name
            edit_remark.setText(val)
        self.table_pl.setCellWidget(row, 3, edit_remark)
        
        # 5. Enabled
        chk_widget = QWidget()
        chk_layout = QHBoxLayout(chk_widget)
        chk_layout.setContentsMargins(0,0,0,0)
        chk_layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        chk_enabled = QCheckBox()
        if data and data.enabled:
            chk_enabled.setChecked(True)
        chk_layout.addWidget(chk_enabled)
        self.table_pl.setCellWidget(row, 4, chk_widget)
        chk_enabled.setObjectName("chk_enabled") 
        
        chk_enabled.toggled.connect(self.handle_enabled_checkbox_toggled)
        
        # 6. Delete & Load
        btn_load = QPushButton("加载")
        btn_load.setStyleSheet("color: white; background-color: #3e8ed6; border: none; border-radius: 3px;")
        btn_load.setFixedSize(70, 30)
        btn_load.clicked.connect(lambda: self.load_playlist_data(row))

        btn_del = QPushButton("删除")
        btn_del.setStyleSheet("color: white; background-color: #d6533e; border: none; border-radius: 3px;")
        btn_del.setFixedSize(70, 30)
        btn_del.clicked.connect(self.delete_current_row)
        
        
        
        cell_widget_del = QWidget()
        layout_del = QHBoxLayout(cell_widget_del)
        layout_del.setContentsMargins(0,0,0,0)
        layout_del.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout_del.addWidget(btn_del)
        layout_del.addWidget(btn_load)
        
        self.table_pl.setCellWidget(row, 5, cell_widget_del)

    def load_playlist_data(self, row):
        """加载歌单/专辑数据"""
        # 获取平台、ID和类型
        platform = self.table_pl.cellWidget(row, 0).currentText()
        playlist_id = self.table_pl.cellWidget(row, 1).text()
        typename = self.table_pl.cellWidget(row, 2).currentText()
        
        if not playlist_id:
            QMessageBox.warning(self, "加载失败", "请输入歌单/专辑ID")
            return
            
        try:
            # 动态导入对应平台的get_json模块
            module_path = f"{platform}.get_json"
            sys.path.append(os.path.join(os.path.dirname(__file__), '..', 'platforms'))
            get_json_module = importlib.import_module(module_path)
            PlaylistAlbumJson = getattr(get_json_module, 'PlaylistAlbumJson')
            
            # 获取数据
            playlist_album = PlaylistAlbumJson(playlist_id, typename)
            playlist_album.save()
            
            # 更新界面显示名称
            remark_widget = self.table_pl.cellWidget(row, 3)
            if remark_widget.text() == "":
                remark_widget.setText(playlist_album.get_name())
            
            QMessageBox.information(self, "加载成功", f"已成功加载 {playlist_album.get_name()}")
        except Exception as e:
            QMessageBox.critical(self, "加载失败", f"加载过程中出现错误:\n{str(e)}")

    def handle_enabled_checkbox_toggled(self, checked):
        sender = self.sender()
        if checked:
            for row in range(self.table_pl.rowCount()):
                chk_widget = self.table_pl.cellWidget(row, 4)
                chk = chk_widget.findChild(QCheckBox, "chk_enabled")
                if chk and chk != sender:
                    chk.setChecked(False)
        else:
            enabled_count = 0
            for row in range(self.table_pl.rowCount()):
                chk_widget = self.table_pl.cellWidget(row, 4)
                chk = chk_widget.findChild(QCheckBox, "chk_enabled")
                if chk and chk.isChecked():
                    enabled_count += 1
            
            if enabled_count == 0:
                sender.setChecked(True)
                QMessageBox.warning(self, "操作阻止", "必须至少有一个歌单处于被启用状态！")

    def delete_current_row(self):
        button = self.sender()
        if button:
            for r in range(self.table_pl.rowCount()):
                cell_w = self.table_pl.cellWidget(r, 5)
                if cell_w.isAncestorOf(button):
                    chk_widget = self.table_pl.cellWidget(r, 4)
                    chk = chk_widget.findChild(QCheckBox, "chk_enabled")
                    if chk and chk.isChecked():
                        enabled_count = 0
                        for row in range(self.table_pl.rowCount()):
                            if row != r:
                                other_chk_widget = self.table_pl.cellWidget(row, 4)
                                other_chk = other_chk_widget.findChild(QCheckBox, "chk_enabled")
                                if other_chk and other_chk.isChecked():
                                    enabled_count += 1
                        
                        if enabled_count == 0:
                            QMessageBox.warning(self, "操作阻止", "不能删除唯一被启用的歌单！请先启用其他歌单。")
                            return
                    
                    self.table_pl.removeRow(r)
                    return

    def init_gui_tab(self):
        self.gui_tab = QWidget()
        self.tabs.addTab(self.gui_tab, "界面设置")
        
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        content_widget = QWidget()
        scroll.setWidget(content_widget)
        
        main_layout = QVBoxLayout(self.gui_tab)
        main_layout.addWidget(scroll)
        
        v_layout = QVBoxLayout(content_widget)
        
        # --- 全局 ---
        group_global = QGroupBox("全局设置")
        form_global = QFormLayout(group_global)
        
        self.mode_toggle_layout = QHBoxLayout()
        self.btn_day_mode = QPushButton("🌞 日间模式")
        self.btn_night_mode = QPushButton("🌙 夜间模式")
        
        self.btn_day_mode.setCheckable(True)
        self.btn_night_mode.setCheckable(True)
        
        self.mode_group = QButtonGroup()
        self.mode_group.addButton(self.btn_day_mode, 0)
        self.mode_group.addButton(self.btn_night_mode, 1)
        
        if self.gui_setting.night_mode:
            self.btn_night_mode.setChecked(True)
        else:
            self.btn_day_mode.setChecked(True)
            
        self.btn_day_mode.clicked.connect(self.switch_to_day_mode)
        self.btn_night_mode.clicked.connect(self.switch_to_night_mode)
        
        self.mode_toggle_layout.addWidget(self.btn_day_mode)
        self.mode_toggle_layout.addWidget(self.btn_night_mode)
        form_global.addRow("界面模式:", self.mode_toggle_layout)
        
        # --- 使用自定义的 FloatSlider 替换原来的 SpinBox ---
        
        # 1. 卡片尺寸
        self.slider_card_size = FloatSlider(self.gui_setting.card_size)
        form_global.addRow("卡片尺寸 (Multiplier):", self.slider_card_size)
        
        # 2. 取消按钮尺寸
        self.slider_cancel_size = FloatSlider(self.gui_setting.cancel_button_size)
        form_global.addRow("取消按钮尺寸:", self.slider_cancel_size)
        
        # 3. 设置界面尺寸 (需要实时反馈)
        self.slider_setting_size = FloatSlider(self.gui_setting.setting_size)
        # 链接信号到实时更新函数
        self.slider_setting_size.valueChanged.connect(self.live_update_setting_size)
        form_global.addRow("设置界面尺寸:", self.slider_setting_size)
        
        v_layout.addWidget(group_global)
        
        # --- 颜色配置生成 ---
        self.color_inputs = {} 
        self.color_previews = {} 

        def create_color_section(layout, mode_name, setting_dict, prefix):
            box = QGroupBox(mode_name)
            box_layout = QFormLayout(box)
            
            self.color_inputs[prefix] = {}
            self.color_previews[prefix] = {} 
            
            fields = [
                ("卡片背景 (Card BG)", "card", "background"),
                ("卡片悬停 (Card Hover)", "card", "background_hover"),
                ("卡片边框 (Card Border)", "card", "border"),
                ("卡片文字 (Card Font)", "card", "font_color"),
                
                ("取消键背景 (Cancel BG)", "cancel_button", "background"),
                ("取消键悬停 (Cancel Hover)", "cancel_button", "background_hover"),
                ("取消键边框 (Cancel Border)", "cancel_button", "border"),
                ("取消键文字 (Cancel Font)", "cancel_button", "font_color"),
                
                ("设置窗背景 (Setting BG)", "setting", "background"),
                ("设置窗文字 (Setting Font)", "setting", "font_color"),
            ]
            
            for label, main_key, sub_key in fields:
                sub_dict = setting_dict.get(main_key, {})
                current_color = sub_dict.get(sub_key, "#FFFFFF")
                
                # 创建颜色输入框
                color_input = QLineEdit()
                color_input.setText(current_color)
                color_input.setFixedWidth(100)
                
                # 创建颜色预览小方块
                color_preview = ColorPreviewWidget(current_color)
                
                # 连接输入框的变化 -> 更新小方块
                from functools import partial
                color_input.textChanged.connect(partial(self.update_color_preview, color_preview))
                
                # 容器
                input_container = QWidget()
                input_layout = QHBoxLayout(input_container)
                input_layout.setContentsMargins(0, 0, 0, 0)
                input_layout.addWidget(color_input)
                input_layout.addWidget(color_preview)
                input_layout.addStretch() # 靠左对齐
                
                box_layout.addRow(label, input_container)
                
                self.color_inputs[prefix][f"{main_key}_{sub_key}"] = color_input
                self.color_previews[prefix][f"{main_key}_{sub_key}"] = color_preview
                
            layout.addWidget(box)

        day_data = {
            "card": self.gui_setting.card,
            "cancel_button": self.gui_setting.cancel_button,
            "setting": self.gui_setting.setting
        }
        
        night_data = {
            "card": self.gui_setting.card_night_mode,
            "cancel_button": self.gui_setting.cancel_button_night_mode,
            "setting": self.gui_setting.setting_night_mode
        }
        
        h_colors = QHBoxLayout()
        w_day = QWidget()
        l_day = QVBoxLayout(w_day)
        l_day.setContentsMargins(0,0,0,0)
        create_color_section(l_day, "日间模式配色", day_data, "day")
        
        w_night = QWidget()
        l_night = QVBoxLayout(w_night)
        l_night.setContentsMargins(0,0,0,0)
        create_color_section(l_night, "夜间模式配色", night_data, "night")
        
        h_colors.addWidget(w_day)
        h_colors.addWidget(w_night)
        
        v_layout.addLayout(h_colors)

    def switch_to_day_mode(self):
        self.gui_setting.night_mode = False
        self.apply_gui_theme()

    def switch_to_night_mode(self):
        self.gui_setting.night_mode = True
        self.apply_gui_theme()

    def update_color_preview(self, color_preview, text):
        """当输入框文字改变时，更新小方块颜色"""
        color_preview.set_color(text)

    def get_input_color(self, mode_prefix, main_key, sub_key):
        return self.color_inputs[mode_prefix][f"{main_key}_{sub_key}"].text()

    def live_update_setting_size(self, value):
        """
        实时更新设置界面尺寸
        注：此时并未保存到文件，仅更新内存中的对象以触发 apply_gui_theme
        """
        self.gui_setting.setting_size = value
        self.apply_gui_theme()

    def save_all_settings(self):
        # 0. 保存之前的平台信息（用于检测是否切换平台）
        old_enabled_platform = None
        old_enabled_playlist = None
        for pl in self.pa_setting.playlist_albums:
            if pl.enabled:
                old_enabled_platform = pl.name
                old_enabled_playlist = pl.playlist_album_id
                break
        
        # 1. 保存 Music Setting
        self.pa_setting.number_of_discovered_songs = self.spin_discovered.value()
        self.pa_setting.have_mystery_song = self.chk_mystery.isChecked()
        self.pa_setting.num_of_mystery_song = self.spin_mystery_num.value()
        self.pa_setting.overlap = self.chk_overlap.isChecked()
        self.pa_setting.refreshing_after_cancel = self.chk_refresh.isChecked()
        self.pa_setting.shortcut_key = self.edit_shortcut.text()
        
        new_playlists = []
        new_enabled_platform = None
        new_enabled_playlist = None
        for row in range(self.table_pl.rowCount()):
            name = self.table_pl.cellWidget(row, 0).currentText()
            pid = self.table_pl.cellWidget(row, 1).text()
            typename = self.table_pl.cellWidget(row, 2).currentText()
            remark = self.table_pl.cellWidget(row, 3).text()
            
            chk_widget = self.table_pl.cellWidget(row, 4)
            chk = chk_widget.findChild(QCheckBox, "chk_enabled")
            enabled = chk.isChecked() if chk else False
            
            if enabled:
                new_enabled_platform = name
                new_enabled_playlist = pid
            
            p_data = {
                "name": name,
                "playlist_album_id": pid,
                "typename": typename,
                "playlist_album_name": remark,  # 保存名称
                "playlist_album_remark": remark,
                "update_time": datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S"),  # 添加时间戳
                "enabled": enabled
            }
            new_playlists.append(PlaylistAlbum(p_data))
            
        self.pa_setting.playlist_albums = new_playlists
        self.pa_setting.save()
        
        # 2. 保存 GUI Setting
        self.gui_setting.night_mode = self.btn_night_mode.isChecked()
        # 获取 Slider 的值
        self.gui_setting.card_size = self.slider_card_size.value()
        self.gui_setting.cancel_button_size = self.slider_cancel_size.value()
        self.gui_setting.setting_size = self.slider_setting_size.value()
        
        # 更新 Day Colors
        for main_key in ["card", "cancel_button", "setting"]:
            target_dict = getattr(self.gui_setting, main_key)
            for sub_key in ["background", "background_hover", "border", "font_color"]:
                lookup_key = f"{main_key}_{sub_key}"
                if lookup_key in self.color_inputs["day"]:
                    target_dict[sub_key] = self.get_input_color("day", main_key, sub_key)
        
        # 更新 Night Colors
        map_keys = {
            "card": "card_night_mode",
            "cancel_button": "cancel_button_night_mode", 
            "setting": "setting_night_mode"
        }
        
        for main_key, attr_name in map_keys.items():
            target_dict = getattr(self.gui_setting, attr_name)
            for sub_key in ["background", "background_hover", "border", "font_color"]:
                lookup_key = f"{main_key}_{sub_key}"
                if lookup_key in self.color_inputs["night"]:
                    target_dict[sub_key] = self.get_input_color("night", main_key, sub_key)
        
        self.gui_setting.save()
        
        # 3. 检测平台、歌单是否发生变化
        platform_changed = (old_enabled_platform != new_enabled_platform)
        playlist_changed = (new_enabled_playlist != old_enabled_playlist)
        
        # 4. 界面反馈
        self.pa_setting.load()
        self.load_playlist_table()
        self.apply_gui_theme()
        
        if platform_changed or playlist_changed:
            # 平台、歌单切换了，提示并重启
            QMessageBox.information(self, "歌单已切换", "歌单已切换，程序将重启以应用更改。")
            self.restart_application()
        else:
            # 通知 Discover_gui 重新加载设置
            self.notify_settings_changed()
            QMessageBox.information(self, "保存成功", "所有设置已保存并应用！")
    
    def restart_application(self):
        """重启应用程序"""
        import subprocess
        import sys
        
        # 获取当前运行的脚本路径
        script_path = os.path.join(os.path.dirname(os.path.dirname(__file__)), "main.py")
        
        # 关闭当前设置窗口
        self.close()
        
        # 启动新的应用程序实例
        subprocess.Popen([sys.executable, script_path])
        
        # 退出当前应用程序
        QApplication.instance().quit()

    def notify_settings_changed(self):
        """通知其他模块设置已更改"""
        # 通知全局 DiscoverApp 重新加载 gui_setting
        try:
            from gui_setting import reload_global_gui_setting
            reload_global_gui_setting()
            print("已通知 GUI 设置已更新")
        except Exception as e:
            print(f"通知设置更新失败: {e}")
        
        # 如果 Discover 浮窗正在显示，刷新它
        try:
            import sys
            import os
            # 尝试导入 Discover_gui 中的全局变量
            sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))
            import Discover_gui
            
            # 如果浮窗存在且可见，刷新它
            if Discover_gui._main_window is not None and Discover_gui._main_window.isVisible():
                # 刷新 gui_setting 引用
                if Discover_gui._global_discover_app:
                    Discover_gui._global_discover_app.gui_setting.load()
                print("Discover 浮窗已刷新")
            
            # 重新注册快捷键
            if Discover_gui._global_app and Discover_gui._global_discover_app:
                Discover_gui.reregister_shortcut(Discover_gui._global_app, Discover_gui._global_discover_app)
                print("快捷键已重新注册")
            
            # 重新应用 music_setting 并更新歌单
            if Discover_gui._global_discover_app:
                # 重新加载 music_setting
                Discover_gui._global_discover_app.music_setting.load()
                # 重新应用设置（更新 platform, playlist_type, playlist_id）
                Discover_gui._global_discover_app._apply_settings()
                # 更新启用的歌单数据
                Discover_gui._global_discover_app._update_enabled_playlist()
                print("已更新歌单设置")
        except Exception as e:
            print(f"刷新浮窗失败: {e}")

    def apply_gui_theme(self):
        is_night = self.gui_setting.night_mode or self.btn_night_mode.isChecked()
        
        if is_night:
            s_conf = self.gui_setting.setting_night_mode
            btn_conf = self.gui_setting.cancel_button_night_mode
            input_conf = self.gui_setting.card_night_mode 
        else:
            s_conf = self.gui_setting.setting
            btn_conf = self.gui_setting.cancel_button
            input_conf = self.gui_setting.card 

        bg = s_conf.get("background", "#F0F0F0")
        fg = s_conf.get("font_color", "#000000")
        
        btn_bg = btn_conf.get("background", "#E0E0E0")
        btn_fg = btn_conf.get("font_color", "#000000")
        btn_bd = btn_conf.get("border", "#888888")
        btn_hover = btn_conf.get("background_hover", "#CCCCCC")

        input_bg = input_conf.get("background", "#FFFFFF")
        input_bd = input_conf.get("border", "#AAAAAA")
        input_fg = input_conf.get("font_color", "#000000")

        # 动态计算字体大小
        # 假设基准字体为 14px
        current_setting_scale = self.gui_setting.setting_size
        base_font_size = 14
        scaled_font_size = int(base_font_size * current_setting_scale)
        # 防止字体过小
        scaled_font_size = max(8, scaled_font_size)

        style = f"""
            QMainWindow, QWidget {{
                background-color: {bg};
                color: {fg};
                font-family: "Segoe UI", "Microsoft YaHei", sans-serif;
                font-size: {scaled_font_size}px;
            }}
            QGroupBox {{
                border: 1px solid {input_bd};
                border-radius: 5px;
                margin-top: {int(10 * current_setting_scale)}px;
                padding-top: {int(10 * current_setting_scale)}px;
                font-weight: bold;
            }}
            QGroupBox::title {{
                subcontrol-origin: margin;
                subcontrol-position: top left;
                padding: 0 5px;
                color: {fg};
            }}
            QTabWidget::pane {{
                border: 1px solid {input_bd};
                background: {bg};
            }}
            QTabBar::tab {{
                background: {btn_bg};
                color: {btn_fg};
                padding: {int(8 * current_setting_scale)}px {int(20 * current_setting_scale)}px;
                border: 1px solid {btn_bd};
                border-bottom: none;
                border-top-left-radius: 4px;
                border-top-right-radius: 4px;
            }}
            QTabBar::tab:selected {{
                background: {bg};
                border-bottom: 1px solid {bg};
                font-weight: bold;
            }}
            QLineEdit, QSpinBox, QDoubleSpinBox, QComboBox {{
                background-color: {input_bg};
                color: {input_fg};
                border: 1px solid {input_bd};
                padding: {int(4 * current_setting_scale)}px;
                border-radius: 3px;
            }}
            QTableWidget {{
                gridline-color: {input_bd};
                background-color: {input_bg};
                alternate-background-color: {bg};
            }}
            QHeaderView::section {{
                background-color: {btn_bg};
                color: {btn_fg};
                padding: {int(4 * current_setting_scale)}px;
                border: 1px solid {btn_bd};
            }}
            QPushButton {{
                background-color: {btn_bg};
                color: {btn_fg};
                border: 1px solid {btn_bd};
                border-radius: 4px;
                padding: {int(5 * current_setting_scale)}px {int(15 * current_setting_scale)}px;
            }}
            QPushButton:hover {{
                background-color: {btn_hover};
            }}
        """
        self.setStyleSheet(style)

if __name__ == "__main__":
    app = QApplication(sys.argv)
    if hasattr(Qt.ApplicationAttribute, 'AA_EnableHighDpiScaling'):
        app.setAttribute(Qt.ApplicationAttribute.AA_EnableHighDpiScaling, True)
    if hasattr(Qt.ApplicationAttribute, 'AA_UseHighDpiPixmaps'):
        app.setAttribute(Qt.ApplicationAttribute.AA_UseHighDpiPixmaps, True)

    window = SettingsWindow()
    window.show()
    sys.exit(app.exec())