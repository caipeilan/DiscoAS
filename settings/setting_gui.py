import sys
import os
import importlib
import datetime
from PyQt6.QtWidgets import (QApplication, QMainWindow, QWidget, QVBoxLayout, 
                             QHBoxLayout, QStackedWidget, QLabel, QSpinBox, 
                             QCheckBox, QLineEdit, QTableWidget, QTableWidgetItem, 
                             QPushButton, QHeaderView, QGroupBox, 
                             QFormLayout, QScrollArea, QComboBox, QMessageBox, 
                             QDoubleSpinBox, QButtonGroup, QSlider, QFileDialog,QFrame)
from PyQt6.QtGui import QColor, QAction, QFont, QIcon, QPixmap
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
        self.setWindowTitle("编辑你的DiscoAS！")
        
        # 设置窗口图标
        icon_path = os.path.join(os.path.dirname(__file__), "../src", "Icon.ico")
        if os.path.exists(icon_path):
            self.setWindowIcon(QIcon(icon_path))
        
        self.resize(1200, 750)
        
        # 中央部件 - 使用水平布局
        central = QWidget()
        self.setCentralWidget(central)
        main_layout = QHBoxLayout(central)
        main_layout.setContentsMargins(0, 0, 0, 0)
        main_layout.setSpacing(0)
        
        # ========== 左侧导航栏 ==========
        left_widget = QWidget()
        left_widget.setFixedWidth(200)
        left_layout = QVBoxLayout(left_widget)
        left_layout.setContentsMargins(15, 20, 15, 20)
        left_layout.setSpacing(10)
        
        # Logo
        logo_label = QLabel()
        logo_path = os.path.join(os.path.dirname(__file__), "../src", "DiscoAS.png")
        if os.path.exists(logo_path):
            pixmap = QPixmap(logo_path)
            # 缩放到合适大小
            scaled = pixmap.scaled(120, 120, Qt.AspectRatioMode.KeepAspectRatio, Qt.TransformationMode.SmoothTransformation)
            logo_label.setPixmap(scaled)
            logo_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        left_layout.addWidget(logo_label)
        
        # 导航按钮
        nav_buttons = [
            ("关于", 0),
            ("发现设置", 1),
            ("界面设置", 2),
        ]
        
        self.btn_group = QButtonGroup(self)
        self.nav_buttons = {}
        
        for text, idx in nav_buttons:
            btn = QPushButton(text)
            btn.setCheckable(True)
            btn.setCursor(Qt.CursorShape.PointingHandCursor)
            self.btn_group.addButton(btn, idx)
            self.nav_buttons[idx] = btn
            left_layout.addWidget(btn)
        
        self.btn_group.buttonClicked.connect(self.switch_page)
        
        left_layout.addStretch()
        
        # 底部按钮
        self.btn_apply = QPushButton("应用并保存")
        self.btn_apply.setCursor(Qt.CursorShape.PointingHandCursor)
        self.btn_apply.setMinimumHeight(40)
        self.btn_apply.clicked.connect(self.save_all_settings)
        
        self.btn_close = QPushButton("关闭")
        self.btn_close.setMinimumHeight(40)
        self.btn_close.clicked.connect(self.close)
        
        left_layout.addWidget(self.btn_apply)
        left_layout.addWidget(self.btn_close)
        
        main_layout.addWidget(left_widget)
        
        # 分割线
        line = QFrame()
        line.setFrameShape(QFrame.Shape.VLine)
        main_layout.addWidget(line)
        
        # ========== 右侧内容区 ==========
        self.stack = QStackedWidget()
        main_layout.addWidget(self.stack, 1)
        
        # 创建三个页面
        self.init_about_page()
        self.init_music_page()
        self.init_gui_page()
        
        # 默认显示关于页面
        self.btn_group.button(0).setChecked(True)
        self.stack.setCurrentIndex(0)
        
        # 初始渲染
        self.apply_gui_theme()


    def switch_page(self, button):
        """切换页面"""
        page_id = self.btn_group.id(button)
        self.stack.setCurrentIndex(page_id)


    def init_about_page(self):
        """关于页面"""
        page = QWidget()
        layout = QVBoxLayout(page)
        layout.setContentsMargins(40, 40, 40, 40)
        
        # 标题
        title = QLabel("DiscoAS - 发现一首歌！")
        title_font = QFont()
        title_font.setPointSize(24)
        title_font.setBold(True)
        title.setFont(title_font)
        layout.addWidget(title)
        
        layout.addSpacing(20)
        
        # 简介
        intro = QLabel(
            "一个基于PyQt6的音乐选择工具，\n"
            "通过scheme url链接唤醒本地音乐软件。"
        )
        intro.setWordWrap(True)
        intro_font = QFont()
        intro_font.setPointSize(12)
        intro.setFont(intro_font)
        layout.addWidget(intro)
        
        layout.addSpacing(30)
        
        # 功能列表
        features_title = QLabel("主要功能很简单↓")
        features_title.setFont(QFont("", weight=QFont.Weight.Bold))
        layout.addWidget(features_title)
        
        features = QLabel(
            "• 通过全局快捷键或系统托盘进行“发现”\n"
            "• 全屏浮窗随机展示歌曲卡片\n"
            "• 附带秘密歌曲模式，利好选择困难症\n"
            "• 自定义界面配色和尺寸"
        )
        layout.addWidget(features)
        
        layout.addSpacing(30)
        
        # 使用说明
        usage_title = QLabel("使用方法：")
        usage_title.setFont(QFont("", weight=QFont.Weight.Bold))
        layout.addWidget(usage_title)
        
        usage = QLabel(
            "1. 在「发现设置」中添加你的歌单\n"
            "2. 点击「加载」获取歌曲列表\n"
            "3. 启用一个歌单，应用并保存\n"
            "4. 待程序重启后，发现一首歌！"
        )
        layout.addWidget(usage)
        
        layout.addStretch()
        
        self.stack.addWidget(page)


    def init_music_page(self):
        """发现设置页面"""
        self.music_page = QWidget()
        layout = QVBoxLayout(self.music_page)
        layout.setContentsMargins(20, 20, 20, 20)
        
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        content_widget = QWidget()
        scroll.setWidget(content_widget)
        
        v_layout = QVBoxLayout(content_widget)
        v_layout.setSpacing(15)
        
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
        
        self.chk_mystery = QCheckBox("启用秘密歌曲模式")
        self.chk_mystery.setChecked(self.pa_setting.have_mystery_song)
        form_basic.addRow(self.chk_mystery)
        
        self.spin_mystery_num = QSpinBox()
        self.spin_mystery_num.setRange(0, 50)
        self.spin_mystery_num.setValue(self.pa_setting.num_of_mystery_song)
        self.spin_mystery_num.setButtonSymbols(QSpinBox.ButtonSymbols.NoButtons)
        self.spin_mystery_num.setMinimumWidth(120)
        form_basic.addRow("秘密歌曲数量:", self.spin_mystery_num)
        
        self.chk_overlap = QCheckBox("允许歌曲重复（当歌单歌曲不足时）")
        self.chk_overlap.setChecked(self.pa_setting.overlap)
        form_basic.addRow(self.chk_overlap)
        
        self.chk_refresh = QCheckBox("取消选择后刷新歌曲列表")
        self.chk_refresh.setChecked(self.pa_setting.refreshing_after_cancel)
        form_basic.addRow(self.chk_refresh)
        
        self.edit_shortcut = QLineEdit()
        self.edit_shortcut.setText(self.pa_setting.shortcut_key)
        self.edit_shortcut.setPlaceholderText("例如: Alt+D")
        form_basic.addRow("全局快捷键:", self.edit_shortcut)
        
        v_layout.addWidget(group_basic)

        # --- 秘密歌曲封面设置 ---
        group_mystery_cover = QGroupBox("秘密歌曲封面")
        form_cover = QFormLayout(group_mystery_cover)
        
        hint_label = QLabel("留空使用默认封面，支持URL和本地图片路径")
        hint_label.setWordWrap(True)
        form_cover.addRow(hint_label)

        cover_input_row = QWidget()
        cover_input_layout = QHBoxLayout(cover_input_row)
        cover_input_layout.setContentsMargins(0, 0, 0, 0)

        self.edit_mystery_cover = QLineEdit()
        self.edit_mystery_cover.setText(self.pa_setting.mystery_song_cover)
        self.edit_mystery_cover.setPlaceholderText("图片URL或本地路径")
        cover_input_layout.addWidget(self.edit_mystery_cover)

        btn_browse_cover = QPushButton("浏览...")
        btn_browse_cover.setFixedWidth(80)
        btn_browse_cover.clicked.connect(self._browse_mystery_cover)
        cover_input_layout.addWidget(btn_browse_cover)

        form_cover.addRow("封面来源:", cover_input_row)

        v_layout.addWidget(group_mystery_cover)
        
        # --- 列表区 ---
        group_list = QGroupBox("歌单/专辑管理")
        v_list = QVBoxLayout(group_list)
        
        self.table_pl = QTableWidget()
        self.table_pl.setColumnCount(6)
        self.table_pl.setHorizontalHeaderLabels(["平台", "ID", "类型", "名称/备注", "启用", "操作"])
        self.table_pl.horizontalHeader().setSectionResizeMode(3, QHeaderView.ResizeMode.Stretch)
        self.table_pl.horizontalHeader().setSectionResizeMode(5, QHeaderView.ResizeMode.Fixed)
        self.table_pl.setColumnWidth(0, 200)   # 平台
        self.table_pl.setColumnWidth(1, 120)   # ID
        self.table_pl.setColumnWidth(2, 100)   # 类型
        self.table_pl.setColumnWidth(4, 60)   # 启用
        self.table_pl.setColumnWidth(5, 150) # 操作
        self.table_pl.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table_pl.verticalHeader().setDefaultSectionSize(45)
        v_list.addWidget(self.table_pl)
        
        h_btn = QHBoxLayout()
        btn_add = QPushButton("添加歌单")
        btn_add.clicked.connect(self.add_playlist_row)
        h_btn.addWidget(btn_add)
        h_btn.addStretch()
        v_list.addLayout(h_btn)
        
        v_layout.addWidget(group_list)
        
        layout.addWidget(scroll)
        
        # 加载数据
        self.load_playlist_table()
        
        self.stack.addWidget(self.music_page)


    def init_gui_page(self):
        """界面设置页面"""
        self.gui_page = QWidget()
        layout = QVBoxLayout(self.gui_page)
        layout.setContentsMargins(20, 20, 20, 20)
        
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        content_widget = QWidget()
        scroll.setWidget(content_widget)
        
        main_layout = QVBoxLayout(content_widget)
        main_layout.setSpacing(15)
        
        # --- 全局 ---
        group_global = QGroupBox("全局设置")
        form_global = QFormLayout(group_global)
        
        self.mode_toggle_layout = QHBoxLayout()
        self.btn_day_mode = QPushButton("日间模式")
        self.btn_night_mode = QPushButton("夜间模式")
        
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
        
        # 尺寸设置
        self.slider_card_size = FloatSlider(self.gui_setting.card_size)
        form_global.addRow("卡片尺寸:", self.slider_card_size)
        
        self.slider_cancel_size = FloatSlider(self.gui_setting.cancel_button_size)
        form_global.addRow("取消按钮尺寸:", self.slider_cancel_size)
        
        self.slider_setting_size = FloatSlider(self.gui_setting.setting_size)
        self.slider_setting_size.valueChanged.connect(self.live_update_setting_size)
        form_global.addRow("设置界面尺寸:", self.slider_setting_size)
        
        main_layout.addWidget(group_global)
        
        # --- 颜色配置 ---
        self.color_inputs = {} 
        self.color_previews = {} 

        def create_color_section(layout, mode_name, setting_dict, prefix):
            box = QGroupBox(mode_name)
            box_layout = QFormLayout(box)
            
            self.color_inputs[prefix] = {}
            self.color_previews[prefix] = {} 
            
            fields = [
                ("卡片背景", "card", "background"),
                ("卡片悬停", "card", "background_hover"),
                ("卡片边框", "card", "border"),
                ("卡片文字", "card", "font_color"),
                
                ("取消键背景", "cancel_button", "background"),
                ("取消键悬停", "cancel_button", "background_hover"),
                ("取消键边框", "cancel_button", "border"),
                ("取消键文字", "cancel_button", "font_color"),
                
                ("设置窗背景", "setting", "background"),
                ("设置窗文字", "setting", "font_color"),
            ]
            
            for label, main_key, sub_key in fields:
                sub_dict = setting_dict.get(main_key, {})
                current_color = sub_dict.get(sub_key, "#FFFFFF")
                
                color_input = QLineEdit()
                color_input.setText(current_color)
                color_input.setFixedWidth(100)
                
                color_preview = ColorPreviewWidget(current_color)
                
                from functools import partial
                color_input.textChanged.connect(partial(self.update_color_preview, color_preview))
                
                input_container = QWidget()
                input_layout = QHBoxLayout(input_container)
                input_layout.setContentsMargins(0, 0, 0, 0)
                input_layout.addWidget(color_input)
                input_layout.addWidget(color_preview)
                input_layout.addStretch()
                
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
        
        main_layout.addLayout(h_colors)

        layout.addWidget(scroll)
        
        self.stack.addWidget(self.gui_page)


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
        btn_load.setFixedSize(60, 28)
        btn_load.clicked.connect(lambda: self.load_playlist_data(row))

        btn_del = QPushButton("删除")
        btn_del.setStyleSheet("color: white; background-color: #d6533e; border: none; border-radius: 3px;")
        btn_del.setFixedSize(60, 28)
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
        platform = self.table_pl.cellWidget(row, 0).currentText()
        playlist_id = self.table_pl.cellWidget(row, 1).text()
        typename = self.table_pl.cellWidget(row, 2).currentText()
        
        if not playlist_id:
            QMessageBox.warning(self, "加载失败", "请输入歌单/专辑ID")
            return
            
        try:
            module_path = f"{platform}.get_json"
            sys.path.append(os.path.join(os.path.dirname(__file__), '..', 'platforms'))
            get_json_module = importlib.import_module(module_path)
            PlaylistAlbumJson = getattr(get_json_module, 'PlaylistAlbumJson')
            
            playlist_album = PlaylistAlbumJson(playlist_id, typename)
            playlist_album.save()
            
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
                QMessageBox.warning(self, "操作阻止", "必须至少有一个歌单被启用！")


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
                            QMessageBox.warning(self, "操作阻止", "不能删除唯一被启用的歌单！")
                            return
                    
                    self.table_pl.removeRow(r)
                    return


    def _browse_mystery_cover(self):
        """打开文件选择对话框"""
        file_path, _ = QFileDialog.getOpenFileName(
            self,
            "选择秘密歌曲封面图片",
            "",
            "图片文件 (*.jpg *.jpeg *.png *.gif *.bmp *.webp);;所有文件 (*)"
        )
        if file_path:
            self.edit_mystery_cover.setText(file_path)


    def switch_to_day_mode(self):
        self.gui_setting.night_mode = False
        self.apply_gui_theme()

    def switch_to_night_mode(self):
        self.gui_setting.night_mode = True
        self.apply_gui_theme()


    def update_color_preview(self, color_preview, text):
        color_preview.set_color(text)


    def get_input_color(self, mode_prefix, main_key, sub_key):
        return self.color_inputs[mode_prefix][f"{main_key}_{sub_key}"].text()


    def live_update_setting_size(self, value):
        self.gui_setting.setting_size = value
        self.apply_gui_theme()


    def save_all_settings(self):
        # 0. 保存之前的平台信息
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
        self.pa_setting.mystery_song_cover = self.edit_mystery_cover.text().strip()
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
                "playlist_album_name": remark,
                "playlist_album_remark": remark,
                "update_time": datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
                "enabled": enabled
            }
            new_playlists.append(PlaylistAlbum(p_data))
            
        self.pa_setting.playlist_albums = new_playlists
        self.pa_setting.save()
        
        # 2. 保存 GUI Setting
        self.gui_setting.night_mode = self.btn_night_mode.isChecked()
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
        
        # 3. 检测变化
        platform_changed = (old_enabled_platform != new_enabled_platform)
        playlist_changed = (new_enabled_playlist != old_enabled_playlist)
        
        # 4. 界面反馈
        self.pa_setting.load()
        self.load_playlist_table()
        self.apply_gui_theme()
        
        if platform_changed or playlist_changed:
            QMessageBox.information(self, "歌单已切换", "歌单已切换，程序将重启以应用更改。")
            self.restart_application()
        else:
            self.notify_settings_changed()
            QMessageBox.information(self, "保存成功", "所有设置已保存并应用！")
    

    def restart_application(self):
        import subprocess
        import sys
        
        script_path = os.path.join(os.path.dirname(os.path.dirname(__file__)), "main.py")
        self.close()
        subprocess.Popen([sys.executable, script_path])
        QApplication.instance().quit()


    def notify_settings_changed(self):
        try:
            from gui_setting import reload_global_gui_setting
            reload_global_gui_setting()
            print("已通知 GUI 设置已更新")
        except Exception as e:
            print(f"通知设置更新失败: {e}")
        
        try:
            import sys
            import os
            sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))
            import Discover_gui
            
            if Discover_gui._main_window is not None and Discover_gui._main_window.isVisible():
                if Discover_gui._global_discover_app:
                    Discover_gui._global_discover_app.gui_setting.load()
                print("Discover 浮窗已刷新")
            
            if Discover_gui._global_app and Discover_gui._global_discover_app:
                Discover_gui.reregister_shortcut(Discover_gui._global_app, Discover_gui._global_discover_app)
                print("快捷键已重新注册")
            
            if Discover_gui._global_discover_app:
                Discover_gui._global_discover_app.music_setting.load()
                Discover_gui._global_discover_app._apply_settings()
                Discover_gui._global_discover_app._update_enabled_playlist()
                print("已更新歌单设置")

            Discover_gui._image_cache.clear()
            Discover_gui._cached_songs.clear()
            print("已清空图片缓存和歌曲缓存")

            if Discover_gui._global_discover_app:
                Discover_gui.preload_next_batch(Discover_gui._global_discover_app)
                print("已触发重新预加载")
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

        current_setting_scale = self.gui_setting.setting_size
        base_font_size = 14
        scaled_font_size = int(base_font_size * current_setting_scale)
        scaled_font_size = max(8, scaled_font_size)

        # 左侧导航栏背景
        left_bg = bg
        if is_night:
            left_bg = "#2d2d2d"

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
                margin-top: 10px;
                padding-top: 10px;
                font-weight: bold;
            }}
            QGroupBox::title {{
                subcontrol-origin: margin;
                subcontrol-position: top left;
                padding: 0 5px;
                color: {fg};
            }}
            QLineEdit, QSpinBox, QDoubleSpinBox, QComboBox {{
                background-color: {input_bg};
                color: {input_fg};
                border: 1px solid {input_bd};
                padding: 4px;
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
                padding: 4px;
                border: 1px solid {btn_bd};
            }}
            QPushButton {{
                background-color: {btn_bg};
                color: {btn_fg};
                border: 1px solid {btn_bd};
                border-radius: 4px;
                padding: 5px 15px;
            }}
            QPushButton:hover {{
                background-color: {btn_hover};
            }}
            QPushButton:checked {{
                background-color: {btn_bd};
                color: white;
            }}
            QCheckBox {{
                color: {fg};
            }}
            QScrollArea {{
                border: none;
                background: transparent;
            }}
        """
        self.setStyleSheet(style)
        
        # 设置左侧导航栏特殊样式
        self.centralWidget().layout().itemAt(0).widget().setStyleSheet(f"""
            QWidget {{
                background-color: {left_bg};
            }}
            QPushButton {{
                background-color: transparent;
                border: none;
                text-align: left;
                padding: 12px 15px;
                color: {fg};
                font-size: {scaled_font_size + 1}px;
            }}
            QPushButton:hover {{
                background-color: {btn_hover};
            }}
            QPushButton:checked {{
                background-color: {btn_bd};
                color: white;
                border-radius: 4px;
            }}
        """)


if __name__ == "__main__":
    app = QApplication(sys.argv)
    if hasattr(Qt.ApplicationAttribute, 'AA_EnableHighDpiScaling'):
        app.setAttribute(Qt.ApplicationAttribute.AA_EnableHighDpiScaling, True)
    if hasattr(Qt.ApplicationAttribute, 'AA_UseHighDpiPixmaps'):
        app.setAttribute(Qt.ApplicationAttribute.AA_UseHighDpiPixmaps, True)

    window = SettingsWindow()
    window.show()
    sys.exit(app.exec())
