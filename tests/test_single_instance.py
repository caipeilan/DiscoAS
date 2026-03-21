"""
tests/test_single_instance.py
测试单实例锁：main.acquire_single_instance_lock()
"""
import os
import sys


def test_lock_creates_file(tmp_path, monkeypatch):
    """首次调用应创建锁文件"""
    lock_dir = tmp_path / "DiscoAS"
    monkeypatch.setenv("APPDATA", str(tmp_path))

    # 动态重导入以使用新路径
    if "main" in sys.modules:
        del sys.modules["main"]
    import main

    result = main.acquire_single_instance_lock()
    assert result is True
    assert (lock_dir / "single_instance.lock").exists()


def test_second_instance_blocked(tmp_path, monkeypatch):
    """锁文件含无效 PID 时应清理并重新获得锁"""
    lock_dir = tmp_path / "DiscoAS"
    lock_file = lock_dir / "single_instance.lock"
    lock_dir.mkdir()
    lock_file.write_text("999999")  # 假的无效 PID

    monkeypatch.setenv("APPDATA", str(tmp_path))

    if "main" in sys.modules:
        del sys.modules["main"]
    import main

    result = main.acquire_single_instance_lock()
    assert result is True  # 无效 PID 被清理后获得锁


def test_valid_pid_blocks_second_call(tmp_path, monkeypatch):
    """锁文件中 PID 进程还活着时应阻止第二次启动"""
    lock_dir = tmp_path / "DiscoAS"
    lock_file = lock_dir / "single_instance.lock"
    lock_dir.mkdir()
    lock_file.write_text(str(os.getpid()))  # 当前进程 PID（必然活着）

    monkeypatch.setenv("APPDATA", str(tmp_path))

    if "main" in sys.modules:
        del sys.modules["main"]
    import main

    result = main.acquire_single_instance_lock()
    assert result is False  # 已有实例在运行
