# 运行时资源目录

打包时本目录会整体复制到应用资源目录（macOS: `Contents/Resources/resources/`）。

## PDF 渲染：pdfium 动态库

把对应平台的 pdfium 库放到本目录即可启用：

- macOS（Intel / Apple Silicon 通用）：`libpdfium.dylib`
- Windows：`pdfium.dll`
- Linux（可选）：`libpdfium.so`

下载地址（二选一，均为官方构建）：
- https://github.com/bblanchon/pdfium-binaries/releases
- https://github.com/PDFium/pdfium-binaries

也可通过环境变量 `TUOMINXING_PDFIUM=/绝对路径/libpdfium.dylib` 指定。

## OCR 模型（可选）

放到用户配置目录下的 `models/`（程序「设置」里可一键打开该目录），
或直接放到本目录的 `models/` 子目录随安装包内置（程序会优先用用户目录）：

- `det.onnx` 文本检测模型（PP-OCR DB，默认 v3，可选 v4）
- `rec.onnx` 文字识别模型（PP-OCR CRNN）
- `keys.txt` 字典（ppocr_keys_v1.txt）

下载脚本见项目根目录 `scripts/fetch_ocr_models.sh`（Windows 用 `scripts/fetch_ocr_models.ps1`；
默认 v3，`OCR_VERSION=v4` 可切换）。
