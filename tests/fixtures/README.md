# 测试样例说明

本目录下的 Word / PDF 全部为**虚构内容**，仅用于自动化测试：

- 人名、身份证号、手机号、银行卡号、案号、地址均为编造；
- 与任何真实当事人、案件、律所无关；
- 请勿在此目录放入真实案卷。

| 文件 | 用途 |
| --- | --- |
| `民事起诉状.docx` | Word 解析、保留排版导出 |
| `民事起诉状.pdf` | 文字版 PDF 字符坐标、涂黑导出 |
| `民事起诉状-扫描件.pdf` | 扫描件 OCR 全流程 |

样例在开发期由作者的本地脚本生成，该脚本依赖 PyMuPDF（AGPL-3.0），**不包含在本仓库中**；
仓库内仅保留生成后的测试数据。如需新增样例，请使用虚构信息，并确保不引入新的许可证义务。

测试命令：

```bash
cd src-tauri
cargo test --test pipeline_test -- --nocapture
TUOMINXING_MODELS=../models cargo test --test pipeline_test scanned -- --nocapture
```
