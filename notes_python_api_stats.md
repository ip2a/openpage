# OpenPage Python API 完整统计 (修正版)

## 文件结构

- python/openpage/__init__.py - 公开 API 导出
- python/openpage/_compat.py - 兼容层，所有 Python 包装类
- python/openpage/py.typed - PEP 561 类型标记
- python/examples/ - 示例代码
- python/tests/test_openpage.py - 测试代码

## __init__.py 导出的公开 API (共 17 个)

- **Browser** (class, 17 个公开成员)
- **ChromiumOptions** (class, 17 个公开成员)
- **ChromiumPage** (class, 38 个公开成员)
- **DownloadMission** (class, 10 个公开成员)
- **Element** (class, 13 个公开成员)
- **Listener** (class, 10 个公开成员)
- **ListenerFailInfo** (class, 3 个公开成员)
- **ListenerPacket** (class, 8 个公开成员)
- **ListenerRequest** (class, 5 个公开成员)
- **ListenerRequestExtraInfo** (class, 1 个公开成员)
- **ListenerResponse** (class, 8 个公开成员)
- **ListenerResponseExtraInfo** (class, 3 个公开成员)
- **Page** (class, 29 个公开成员)
- **SessionElement** (class, 22 个公开成员)
- **SessionOptions** (class, 4 个公开成员)
- **SessionPage** (class, 16 个公开成员)
- **WebPage** (class, 36 个公开成员)

## _compat.py 中的内部辅助类 (未在 __all__ 中导出，共 16 个)

- **BrowserSetter** (class, 1 个公开成员)
- **BrowserStates** (class, 4 个公开成员)
- **BrowserWait** (class, 3 个公开成员)
- **ChromiumPageSetter** (class, 13 个公开成员)
- **ElementStates** (class, 10 个公开成员)
- **ElementWait** (class, 10 个公开成员)
- **InterceptedRequest** (class, 11 个公开成员)
- **Interceptor** (class, 4 个公开成员)
- **LoadModeSetter** (class, 3 个公开成员)
- **PageSetter** (class, 13 个公开成员)
- **PageStates** (class, 7 个公开成员)
- **PageWait** (class, 14 个公开成员)
- **WebPageSetter** (class, 13 个公开成员)
- **WebPageStates** (class, 7 个公开成员)
- **WebPageWait** (class, 14 个公开成员)
- **WindowSetter** (class, 8 个公开成员)

## 底层 Rust 模块 openpage_rs 中的类 (共 16 个)

- **Browser** (class, 28 个公开成员)
- **DownloadMission** (class, 10 个公开成员)
- **Element** (class, 31 个公开成员)
- **InterceptedRequest** (class, 11 个公开成员)
- **Interceptor** (class, 4 个公开成员)
- **Listener** (class, 9 个公开成员)
- **ListenerFailInfo** (class, 3 个公开成员)
- **ListenerPacket** (class, 8 个公开成员)
- **ListenerRequest** (class, 5 个公开成员)
- **ListenerRequestExtraInfo** (class, 1 个公开成员)
- **ListenerResponse** (class, 8 个公开成员)
- **ListenerResponseExtraInfo** (class, 3 个公开成员)
- **Page** (class, 64 个公开成员)
- **SessionElement** (class, 20 个公开成员)
- **SessionPage** (class, 23 个公开成员)
- **WebPage** (class, 83 个公开成员)
- **openpage_rs** (module)
