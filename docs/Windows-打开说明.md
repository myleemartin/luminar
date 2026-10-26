# 星星脱敏 Windows 版打开说明

1. 把压缩包完整解压到任意位置（例如 `D:\Luminar` 或桌面）。
2. **Windows 版没有桌面图标，必须双击 EXE 程序才能运行**：
   双击文件夹里的 **「Luminar.exe」**。
3. 不需要安装、不需要管理员权限，这是无需安装的绿色版。
4. 不要只把 `Luminar.exe` 单独拷走：OCR、增强脱敏和 PDF 渲染依赖同目录的 `resources` 文件夹。
5. 如果 Windows 提示“已保护你的电脑”或“未知发布者”，点「更多信息」→「仍要运行」。
6. 完整离线版首次启动会花几秒到几十秒设置内置 WebView2 的权限，请耐心等待；不要放在网络共享文件夹里运行。
   本软件未购买代码签名证书，这是正常提示。

> 如果文件夹里还有「点击这个启动程序.txt」，那只是给 Windows 用户的说明文字，不是程序本身；
> 真正要双击运行的是 **「Luminar.exe」**。

## 系统要求

- Windows 10 / Windows 11，64 位。
- 需要 Microsoft Edge WebView2 运行时。Windows 11 自带；Windows 10 一般已随 Edge 安装。
  如果启动时提示缺少 WebView2，请改用 **完整离线版**（`*_full_offline.zip`，已内置 WebView2），
  或安装微软官方「WebView2 Runtime」。
- `.doc`（Word 97-2003）内置原生解析，大多数情况不需要 LibreOffice；
  只有 Word 6/95、加密或结构异常的文档才回退到本机 LibreOffice。
- `.wps` 的二进制版本若原生解析失败，会尝试 LibreOffice；都没有时请另存为 `.docx`。

## 支持的格式

`.docx` / `.docm` / `.doc` / `.wps` / PDF（文字版、扫描件）/ `.rtf` / `.ofd` / `.html` / `.htm` / `.txt` / `.md`

其中 `.doc` 使用内置 Word 97-2003 解析，大多数情况不需要额外安装；`.wps` 的二进制版本会先尝试原生解析，再回退 LibreOffice。

## 意见反馈

- 请通过 GitHub Issues 反馈：<https://github.com/myleemartin/luminar/issues>sinoyouth@qq.com
