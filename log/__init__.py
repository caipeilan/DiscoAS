"""
日志模块 - 用于记录程序运行日志
"""
import os
import sys
import logging
from datetime import datetime

# 获取日志目录
LOG_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'log')

# 确保日志目录存在
os.makedirs(LOG_DIR, exist_ok=True)

# 日志文件名（按日期）
LOG_FILE = os.path.join(LOG_DIR, f"discoas_{datetime.now().strftime('%Y%m%d')}.log")


def setup_logger(name='DiscoAS', level=logging.INFO):
    """
    设置日志记录器
    同时输出到控制台和文件
    """
    logger = logging.getLogger(name)
    logger.setLevel(level)
    
    # 避免重复添加 handler
    if logger.handlers:
        return logger
    
    # 日志格式
    formatter = logging.Formatter(
        '%(asctime)s - %(name)s - %(levelname)s - %(message)s',
        datefmt='%Y-%m-%d %H:%M:%S'
    )
    
    # 文件 Handler
    file_handler = logging.FileHandler(LOG_FILE, encoding='utf-8')
    file_handler.setLevel(level)
    file_handler.setFormatter(formatter)
    
    # 控制台 Handler
    console_handler = logging.StreamHandler(sys.stdout)
    console_handler.setLevel(level)
    console_handler.setFormatter(formatter)
    
    logger.addHandler(file_handler)
    logger.addHandler(console_handler)
    
    return logger


# 创建默认日志记录器
logger = setup_logger()


def _custom_print(*args, **kwargs):
    """
    自定义 print 函数，同时输出到控制台和日志文件
    """
    # 获取原始的 print 参数
    sep = kwargs.get('sep', ' ')
    end = kwargs.get('end', '\n')
    file = kwargs.get('file', sys.stdout)
    
    # 格式化消息
    message = sep.join(str(arg) for arg in args)
    
    # 同时输出到控制台（原始行为）和日志文件
    # 1. 输出到日志文件
    logger.info(message)
    
    # 2. 输出到控制台（保持原始行为）
    # 使用原始的 sys.stdout 写入
    sys.stdout.write(message + end)
    sys.stdout.flush()


# 全局替换 print 函数
# 注意：这必须在任何其他模块导入之前执行
import builtins
builtins.print = _custom_print
