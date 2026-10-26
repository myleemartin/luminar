# 第三方组件与来源说明（THIRD PARTY NOTICES）

星星脱敏（Luminar）是独立的 Rust 实现。除下列明确列出的第三方组件、模型与数据外，
项目代码由本项目作者编写；**没有复制任何第三方项目的源代码**。

完整许可证文本见 [`licenses/`](licenses/) 目录。

---

## 一、运行时依赖（Rust）

| 组件 | 版本 | 许可证 | 用途 |
| --- | --- | --- | --- |
| [Tauri](https://tauri.app/)（tauri / tauri-build / tauri-plugin-dialog） | 2.x | MIT OR Apache-2.0 | 桌面框架、窗口、文件对话框 |
| [serde](https://serde.rs/) / serde_json | 1.x | MIT OR Apache-2.0 | 序列化 |
| [thiserror](https://github.com/dtolnay/thiserror) / anyhow | 2.x / 1.x | MIT OR Apache-2.0 | 错误处理 |
| [regex](https://github.com/rust-lang/regex) | 1.x | MIT OR Apache-2.0 | 识别规则正则引擎 |
| [lopdf](https://github.com/J-F-Liu/lopdf) | 0.46 | MIT | PDF 结构解析、文本兜底、PDF 组装 |
| [pdfium-render](https://github.com/ajrcarey/pdfium-render) | 0.8.32（vendored） | MIT OR Apache-2.0 | pdfium 的 Rust 封装（渲染、字符坐标）；固定旧版以兼容 pdfium 4.30.1 |
| [docx-rs](https://github.com/bokuweb/docx-rs) | 0.4 | MIT | 生成 Word 文档 |
| [roxmltree](https://github.com/RazrFalcon/roxmltree) | 0.21 | MIT OR Apache-2.0 | Word XML 只读解析 |
| [zip](https://github.com/zip-rs/zip2) | 2.x | MIT | docx 解包/重打包 |
| [tract-onnx](https://github.com/sonos/tract) | 0.23 | MIT OR Apache-2.0 | PP-OCR ONNX 模型纯 Rust 推理 |
| [image](https://github.com/image-rs/image) | 0.25 | MIT OR Apache-2.0 | 页面渲染图处理、JPEG 编码 |
| [encoding_rs](https://github.com/hsivonen/encoding_rs) | 0.8 | Apache-2.0 OR MIT | GB18030/UTF-16 文本解码 |
| [sha2](https://github.com/RustCrypto/hashes) | 0.10 | MIT OR Apache-2.0 | 源文件 SHA-256 指纹 |
| [chrono](https://github.com/chronotope/chrono) / uuid | 0.4 / 1.x | MIT OR Apache-2.0 | 时间戳与 ID |
| [walkdir](https://github.com/BurntSushi/walkdir) | 2.x | MIT OR Unlicense | 目录遍历 |
| [log](https://github.com/rust-lang/log) / once_cell | 0.4 / 1.x | MIT OR Apache-2.0 | 日志与懒初始化 |

间接依赖（wry、tao、window-vibrancy、tokio、rayon 等）随 Cargo 生态引入，
许可证以 `cargo metadata` / `Cargo.lock` 为准，均为 MIT / Apache-2.0 / BSD 类宽松许可。

## 二、运行时依赖（前端）

| 组件 | 版本 | 许可证 | 用途 |
| --- | --- | --- | --- |
| [@tauri-apps/api](https://github.com/tauri-apps/tauri) / @tauri-apps/plugin-dialog | 2.x | MIT OR Apache-2.0 | 前端 IPC 与文件对话框 |
| [Tailwind CSS](https://tailwindcss.com/) | 3.4 | MIT | 界面样式 |
| [Vite](https://vite.dev/) | 6.x | MIT | 前端构建 |
| [TypeScript](https://www.typescriptlang.org/) | 5.x | Apache-2.0 | 类型系统 |
| [PostCSS](https://postcss.org/) / Autoprefixer | 8.x / 10.x | MIT | CSS 处理 |
| [jsdom](https://github.com/jsdom/jsdom)（仅开发/测试） | 30.x | MIT | 前端冒烟测试 |
| esbuild / Rollup（Vite 间接依赖） | — | MIT | 打包 |

## 三、运行时二进制与模型

### 1. pdfium

- 项目：<https://pdfium.googlesource.com/pdfium/>（Google / The PDFium Authors）
- 许可证：BSD-3-Clause
- 版权声明：`Copyright 2014 The PDFium Authors. All rights reserved.`
- 分发方式：动态库（`libpdfium.dylib` / `pdfium.dll` / `libpdfium.so`），
  macOS 固定使用 pypdfium2 **4.30.1**（x64 最低 10.13、arm64 最低 11），
  构建时从 wheel 中提取并用 `lipo` 合并为 Universal dylib。
  `scripts/fetch_pdfium.sh` / `scripts/fetch_pdfium.ps1` 只下载二进制，不修改其代码。

### 2. PaddleOCR / PP-OCR 模型与字典

- 项目：<https://github.com/PaddlePaddle/PaddleOCR>
- 许可证：Apache-2.0
- 版权声明：`Copyright (c) 2020 PaddlePaddle Authors`
- 使用内容：
  - `assets/ppocr_keys_v1.txt`：PP-OCR 中文字典，随仓库分发（Apache-2.0）；
  - `det.onnx` / `rec.onnx`：PP-OCR 检测（DB）与识别（CRNN+CTC）模型，
    已随仓库 `src-tauri/resources/models/` 分发，并内置进安装包（约 13 MB）；
    也可用 `scripts/fetch_ocr_models.sh` 更新或切换版本。
- 说明：模型结构与前后处理算法参考 PaddleOCR，具体 Rust 实现为本项目重写。

### 3. 本地增强识别模型（中文 NER）

- 基座模型：`hfl/minirbt-h288`（6 层、288 维轻量 BERT）
- 微调模型：`IcyKallen/bert-mini-finetuned-ner-chinese`
  <https://huggingface.co/IcyKallen/bert-mini-finetuned-ner-chinese>
- 许可证：Apache-2.0
- 使用内容：
  - `src-tauri/resources/models/ner/ner.onnx`：INT8 动态量化后的中文 NER 模型（约 12 MB）；
  - `src-tauri/resources/models/ner/vocab.txt`：BERT Chinese 词表；
  - 标签体系：PER / ORG / LOC / MISC（模型卡评估集 F1 约 0.87）。
- 量化方式：本项目用 ONNX Runtime 做动态量化，用 `tract` 在本地 CPU 推理；
  未复制上游代码，也未使用其训练数据。
- 该模型为**可选增强**，默认关闭；关闭时不加载、不影响纯规则识别。

### 3.1 cfb（OLE 复合文档读取）

- 项目：<https://github.com/mdsteele/rust-cfb>
- 许可证：MIT
- 作者：Matthew D. Steele
- 用途：读取 Word 97-2003 `.doc` 的 OLE 复合文档流（`WordDocument`、`0Table/1Table`）；
  只读取，不修改原文件。

### 3.2 Microsoft Edge WebView2 固定运行时（仅 Windows 完整离线包）

- 组件：Microsoft Edge WebView2 Fixed Version Runtime x64，版本 156.0.4314.42
- 来源：Microsoft Edge WebView2 官方固定版分发包（`Microsoft.WebView2.FixedVersionRuntime.156.0.4314.42.x64.cab`）
- 许可证：Microsoft 软件许可条款；仓库副本在 `licenses/WebView2-Runtime-LICENSE.txt`，
  打包时同时放到 `resources/WebView2Runtime/LICENSE.txt`
- 分发方式：仅放入 `*_full_offline.zip`，保持原样不修改；标准便携包不含此运行时。
- 用途：让没有预装 WebView2 的 Windows 10 / LTSC / 内网电脑也能启动界面。

### 4. LibreOffice（可选外部工具，不随包分发）

- 项目：<https://www.libreoffice.org/>
- 许可证：MPL-2.0
- 用途：仅在解析旧版 `.doc` / `.wps` 时，调用用户本机已安装的 LibreOffice 做无头转换；
  软件本身不打包、不修改 LibreOffice。未安装时用户可另存为 `.docx`。

### 5. RapidOCR（ONNX 模型分发）

- 项目：<https://github.com/RapidAI/RapidOCR>
- 许可证：Apache-2.0
- 使用内容：提供 PP-OCR 的 ONNX 转换模型下载地址（Hugging Face `SWHL/RapidOCR`），
  本项目默认从该仓库获取模型文件；未复制其 Python 代码。

## 四、算法与设计思路参考（未复制代码）

| 来源 | 许可证 | 参考内容 |
| --- | --- | --- |
| [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) | Apache-2.0 | DB 文本检测、CRNN + CTC 识别、DB 后处理（连通域/外扩）、CTC 解码流程 |
| [RapidOCR](https://github.com/RapidAI/RapidOCR) | Apache-2.0 | ONNX 模型的输入尺寸、归一化参数与字典组织方式 |
| [Microsoft Presidio](https://github.com/microsoft/presidio) | MIT | “识别器分层 + 置信度 + 冲突消解 + 人工复核”的产品思路（概念参考） |
| [pdfplumber](https://github.com/jsvine/pdfplumber) | MIT | PDF 文本抽取的产品思路（概念参考） |
| [Tauri 官方示例](https://github.com/tauri-apps/tauri) | MIT OR Apache-2.0 | 工程脚手架与 Tauri 2 API 用法 |

> 上述参考仅限算法/产品思路，星星脱敏的解析、识别、脱敏、导出、界面代码均为独立实现。

## 五、开发期工具（不随发布物分发）

| 工具 | 许可证 | 用途 |
| --- | --- | --- |
| [python-docx](https://github.com/python-openxml/python-docx) | MIT | 生成测试用 Word 样例 |
| [PyMuPDF](https://github.com/pymupdf/PyMuPDF) | AGPL-3.0 | 开发期生成测试 PDF 样例；相关脚本不包含在本仓库中，仓库内 PDF 仅为虚构测试数据 |

## 六、字体与图标

- 界面只引用**系统字体名**（PingFang SC / Songti SC / Microsoft YaHei 等），
  **不分发、不嵌入任何字体文件**，字体由用户操作系统提供。
- 应用图标由项目自带脚本 `scripts/make_icons.py` 生成，无第三方素材。
- OCR 字典的字符集版权归 PaddleOCR 项目所有（Apache-2.0）。

---

如发现遗漏或错误，请提交 Issue，我们会在下一个版本更正。
