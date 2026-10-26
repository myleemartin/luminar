# 星星脱敏（Luminar）

<img src="assets/logo.svg" width="76" height="76" align="right" alt="星星脱敏 logo" />

[![Version](https://img.shields.io/badge/version-0.11.0-1b3b36.svg)](CHANGELOG.md)
[![License](https://img.shields.io/badge/license-PolyForm%20Noncommercial%201.0.0-c9784f.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows-4a7269.svg)](#)
[![GitHub](https://img.shields.io/badge/GitHub-myleemartin%2Fluminar-181717.svg)](https://github.com/myleemartin/luminar)
[![Rust](https://img.shields.io/badge/Rust-1.77%2B-000.svg)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-24c8db.svg)](https://tauri.app/)

> 面向律师行业的**纯本地离线**文书脱敏桌面软件。批量导入合同、判决书、起诉状、证据卷宗，
> 自动识别身份证号、手机号、住址、银行卡、姓名、案号等敏感信息，人工复核后导出脱敏文本与
> 合规留痕日志。**不联网、不上传**；内置轻量 OCR 与可选的中文 NER 小模型，
> 脱敏后的文本可直接交给本地 DeepSeek 使用。

- 版本：**0.11.0**
- 开箱即用：安装包内置 pdfium、PP-OCR 模型（约 13 MB）与增强脱敏模型（约 12 MB）
- 技术栈：Rust + Tauri v2 + TypeScript + TailwindCSS
- 支持平台：macOS（Apple Silicon / Intel）、Windows 10/11
- 支持格式：`.docx` / `.docm`、`.doc`（内置 Word 97-2003 解析）、`.wps`（OOXML 直读，二进制版回退 LibreOffice）、
  文字版/扫描件 PDF、`.rtf`、`.ofd`、`.html` / `.htm`、`.txt` / `.md`
- 许可：**PolyForm Noncommercial 1.0.0**（源码公开，免费非商业使用；商用需授权）

## 下载

> 让 AI 助手帮你部署？把仓库交给它，并让它先读 [`AGENTS.md`](AGENTS.md)：里面有环境判断、
> Windows WebView2 检查、下载选包和“必须一步步告诉用户怎么打开”的对话要求。

- 已构建安装包见 **[Releases v0.11.0](https://github.com/myleemartin/luminar/releases/tag/v0.11.0)**：
  - macOS 通用版：[`Luminar_0.11.0_universal.dmg`](https://github.com/myleemartin/luminar/releases/download/v0.11.0/Luminar_0.11.0_universal.dmg)
  - Windows 标准免安装：[`Luminar_0.11.0_windows_x64_portable.zip`](https://github.com/myleemartin/luminar/releases/download/v0.11.0/Luminar_0.11.0_windows_x64_portable.zip)
  - Windows 完整离线：[`Luminar_0.11.0_windows_x64_full_offline.zip`](https://github.com/myleemartin/luminar/releases/download/v0.11.0/Luminar_0.11.0_windows_x64_full_offline.zip)
- 支持系统：macOS 10.15+（Intel）/ macOS 11+（Apple Silicon）；Windows 10/11 x64。
- 本机构建产物同名，放在 `release/`，校验和见同名 `.sha256`。
- 也可以按[第五节](#五编译打包)从源码构建；
- 官方 release 安装包内置 pdfium、OCR 与增强脱敏模型，装好即用（未签名版本首次打开需手动放行）。

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  星星脱敏     本地离线 · 数据不出机      OCR 就绪      阈值 0.75  规则  设置 │
├──────────────┬───────────────────────────────────────────────┬───────────────┤
│ 案件文档      │  民事起诉状.docx   ‹ 1/12 ›  [原文|脱敏|对照]  │ 识别结果  18  │
│ ＋ 批量导入   │ ┌───────────────────────────────────────────┐ │ ● 身份证号 96%│
│              │ │ 原告：【当事人姓名】，男，1988年1月1日出生， │ │   420102…     │
│ ▸ 起诉状.docx │ │ 住【住址】，公民身份号码：******************，│ │ ● 手机号   94%│
│ ▸ 判决书.pdf  │ │ 联系电话：138****5678。                    │ │   1381234…    │
│ ▸ 证据卷.pdf  │ │                                           │ │ ● 当事人姓名  │
│              │ │ 委托诉讼代理人：【代理人】，【律所名称】律师。│ │   张三        │
│ 批量识别 导出 │ └───────────────────────────────────────────┘ │ 应用脱敏并导出 │
└──────────────┴───────────────────────────────────────────────┴───────────────┘
```

---

## 运行时依赖（是否真的开箱即用）

| 依赖 | macOS Universal | Windows 标准包 | Windows 完整离线包 |
| --- | --- | --- | --- |
| 界面内核 | 系统 WebKit（macOS 自带） | 系统 WebView2 | **内置 WebView2 固定运行时** |
| PDF | 内置 pdfium（arm64+x64） | 内置 `pdfium.dll` | 内置 `pdfium.dll` |
| OCR / 增强模型 | 内置 | 内置 | 内置 |
| Python / PaddleOCR | 不需要 | 不需要 | 不需要 |
| Node / Rust | 运行不需要 | 运行不需要 | 运行不需要 |

普通 Windows 10/11 用标准包即可；不确定、内网、LTSC 或提示缺少 WebView2 时用完整离线包。
核心功能不依赖 LibreOffice；只有二进制 `.wps`、加密 Word、Word 6/95 才回退本机 LibreOffice，
这类文件建议另存为 `.docx`。

## 一、为什么做这个工具

律师处理卷宗时，大量材料需要对外交付、归档或送入本地大模型做摘要/检索，但其中包含
身份证号、手机号、住址、银行卡、当事人姓名、案号等信息。Python 方案（Presidio +
pdfplumber）在批量处理时启动慢、内存高、分发困难，还需要用户自己装 Python 环境。

本项目用 Rust 重写整条流水线：

| 维度 | Python 方案 | 星星脱敏（Rust） |
| --- | --- | --- |
| 分发 | 需要 Python + 依赖 | 单机安装包，双击即用 |
| 启动 | 数秒 | 毫秒级 |
| 内存 | 数百 MB 起 | 数万字文档约 10–20 MB |
| 识别 | 依赖 Presidio（含模型/网络） | 自研正则 + 可插拔 NER，零网络 |
| 合规 | 需另行拼装日志 | 导出即生成 SHA-256 留痕日志 |

设计原则：**机器识别只是第一遍，律师复核才是最终决定权**。所有自动标记都可以取消、修改、
补充，导出前所见即所得。

---

## 二、功能

### 1. 文件导入与解析

- 批量导入（文件对话框多选，或直接把文件拖进窗口），左栏统一列表、支持多选与批量操作
- 自动区分文字版 PDF / 扫描件 PDF：无文字层时自动渲染 + 本地 OCR；
  扫描件识别时显示**逐页进度条**（已识别 n / N 页），长卷宗不会“黑屏干等”
- 长文档按页预览（PDF/OFD 用真实页；TXT/Word/RTF/HTML 按 3000 字左右虚拟分页）
- 编码自动识别：UTF-8 / UTF-16 / GB18030（兼容 GBK、GB2312）
- 常见法律文书格式：
  - `.docx` / `.docm`：保留原段落排版；`.docm` 导出时剥离 VBA 宏；
  - `.doc`：内置 Word 97-2003 二进制解析（OLE + piece table），大多数文档不装 LibreOffice 也能读；
    解析失败才回退本机 LibreOffice / macOS `textutil`；
  - `.wps`：OOXML 版直接读；二进制版先尝试原生解析，再回退 LibreOffice；
  - `.rtf`：内置 RTF 控制字解析，支持 `\uN` 与 GBK 转义；
  - `.ofd`：国产版式文档，按 `TextCode` 坐标重组文字；
  - `.html` / `.htm` / `.xhtml`：去脚本/样式后提取正文
- 单个文件解析失败不会中断整批导入

### 2. 自动识别 + 自定义规则

内置 13 类法律行业规则（首次启动写入本地 `rules.json`，可改可删可恢复）：

| 分类 | 说明 | 默认脱敏方式 |
| --- | --- | --- |
| 身份证号 | 18 位 / 15 位，含校验位校验 | 掩码 |
| 手机号 | 大陆 11 位，含 +86 前缀 | 掩码（保留前 3 后 4） |
| 固定电话 | 区号 + 号码 + 分机 | 掩码 |
| 银行卡号 | 13–19 位，Luhn 校验 + 上下文加分 | 掩码 |
| 电子邮箱 | 常规邮箱 | 掩码 |
| 住址 | 省市区 + 路街号 + 楼栋室 | 占位标签 |
| 当事人姓名 | 内置姓氏表 + 上下文线索识别 | 占位标签 |
| 代理人 | 「委托诉讼代理人：张三」等 | 占位标签 |
| 律所名称 | 「XX（武汉）律师事务所」 | 占位标签 |
| 案号 | （2024）鄂0102民初1234号 | 占位标签 |
| 法院名称 | 各级人民法院 | 占位标签 |
| 法院编号 | 法院代码 / 法院代字 | 占位标签 |
| 统一社会信用代码 | 18 位，GB 32100 校验 | 掩码 |

- 规则类型：**正则表达式** / **关键词列表** / 内置人名智能识别
- 每条规则独立配置：分类、置信度、二次校验、优先级、边界校验、脱敏方式
- 脱敏方式：**掩码遮蔽**（可保留前后 N 位）、**占位标签**（`【{type}】`，支持 `{index}`）、**直接删除**
- 规则面板内置「测试匹配」，可粘贴样例文字边调边测
- 全部规则持久化到本地 JSON，重启不丢失；一键恢复内置规则

### 3. 增强脱敏（可选，本地小模型）

除了正则规则，程序内置一个 **12 MB 的中文 NER 小模型**（`hfl/minirbt-h288` 微调，INT8 量化），
在顶部开关「增强脱敏」打开后启用：

| 能力 | 说明 |
| --- | --- |
| 补充识别 | 当事人姓名、单位名称、律所名称、法院名称、完整住址 |
| 运行方式 | 纯 Rust `tract` + ONNX，本地 CPU，不需要 GPU、Python、PyTorch |
| 速度 | 约 90 ms / 500 字（M1 Pro CPU）；首次启用加载约 0.15 s，之后复用缓存 |
| 结果标记 | 来源显示「模型识别」，可逐条取消、改分类、改脱敏方式 |
| 与规则的关系 | 规则优先；模型只补规则漏掉的实体，冲突时不会覆盖规则结果 |

**默认关闭**：不开启时导入/识别行为与之前完全一致，不加载模型、不增加耗时。
适合“人名、单位名较多，规则容易漏”的卷宗；如果发现误标，关闭开关即可恢复纯规则结果。

### 4. 交互式人工复核

- 正文高亮所有命中，按分类着色；右侧清单展示原文片段、分类、置信度、来源、规则名
- **取消误报**：取消勾选即不参与脱敏，正文实时变为灰色删除线
- **手动补标**：鼠标框选正文任意文字 → 浮层选择分类与脱敏方式 → 确认，即成为人工标记（置信度 100%）
- **逐条编辑**：修改分类、切换脱敏方式、调整掩码保留位数，立即生效
- **原文 / 脱敏 / 对照** 三种视图实时切换；对照视图左右分栏
- 同名传播：已确认的姓名在全文再次出现时自动补标（可在设置关闭）

### 5. 导出与合规留痕

- **默认只导出原格式**：PDF → PDF，Word → Word，TXT → TXT；
  RTF / OFD / 旧版 Office 统一导出为可编辑 Word。默认勾选「仅导出原格式」，
  不会因为界面上还显示其他格式就多生成 TXT/DOCX/PDF；取消勾选后才进入手动多选。
- **默认导出到桌面**：打开导出窗口时自动填当前用户桌面，点「选择…」可改到任意目录。
- **TXT**：UTF-8（带 BOM），Windows 记事本/Word 打开不乱码
- **DOCX**：源文件是 Word 时**保留原排版**（段落属性、表格、图片、页眉页脚、样式表均不改动，
  只定点替换受影响的文本节点）；源文件是 PDF/TXT 时生成干净的新文档
- **脱敏 PDF**：页面渲染为图片后，用黑色实心块覆盖敏感区域，**不残留任何可提取的原文字符**；
  扫描件同样支持
- **合规日志**：每次导出同步生成 `.json` + `.txt` 双份，包含文件名、源路径、文件指纹（SHA-256）、
  处理时间、工具版本、导出格式、每条记录的页码 / 原文片段 / 脱敏后 / 方式 / 置信度 / 来源
- 输出目录由用户自选；同名文件自动追加序号，**绝不覆盖**已有文件

### 6. 配置

- 置信度阈值滑块（0.30–0.99）：控制识别严格程度，松开即重新识别
- 增强脱敏开关（本地小模型，默认关闭）
- OCR DPI、导出 PDF DPI、OCR 引擎选择、默认掩码字符、占位模板
- 所有配置与规则保存在系统应用配置目录，界面内可一键打开

---

## 三、目录结构

```text
星星脱敏/
├── index.html                  # 三栏界面骨架
├── src/                        # 前端（TypeScript）
│   ├── main.ts                 # 状态、事件、三栏联动、框选补标
│   ├── api.ts                  # 全部后端调用集中封装（便于审计“无网络请求”）
│   ├── types.ts                # 与 Rust serde 结构一一对应
│   ├── ui.ts                   # 弹窗 / Toast / 忙碌遮罩
│   ├── styles.css              # Tailwind + 组件样式 + 高亮配色
│   └── components/
│       ├── preview.ts          # 正文渲染、高亮、选区坐标换算、对照视图
│       ├── panels.ts           # 文件列表、识别结果清单
│       └── modals.ts           # 规则管理、设置、导出
├── src-tauri/                  # 后端（Rust）
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   ├── resources/              # 运行时资源（pdfium + OCR/NER 模型）
│   ├── examples/pipeline.rs    # 命令行流水线验证工具
│   ├── examples/ner_probe.rs   # 增强脱敏效果/耗时探针
│   ├── tests/pipeline_test.rs  # 端到端集成测试
│   └── src/
│       ├── main.rs / lib.rs    # 入口、插件、pdfium 定位
│       ├── commands.rs         # Tauri 命令层（唯一前后端接口）
│       ├── state.rs            # 文档会话、OCR/NER 缓存
│       ├── models.rs           # 领域模型与 DTO
│       ├── config/mod.rs       # settings.json / rules.json 持久化
│       ├── parser/             # 解析层
│       │   ├── mod.rs          # 按扩展名分发
│       │   ├── pdf.rs          # pdfium 字符级坐标 + lopdf 兜底 + 页面渲染
│       │   ├── docx.rs         # zip + roxmltree，记录 <w:t> 字节区间
│       │   ├── txt.rs          # 编码探测、虚拟分页
│       │   └── ocr/            # OCR 层（可插拔引擎）
│       │       ├── engine.rs   # OcrEngine trait
│       │       ├── paddle.rs   # PP-OCR ONNX 本地推理（tract，纯 Rust）
│       │       └── external.rs # 调用本机已有 OCR 命令的兜底实现
│       ├── detect/             # 识别层
│       │   ├── mod.rs          # 候选生成、阈值、冲突消解、同名传播
│       │   ├── builtin_rules.rs# 法律行业内置规则
│       │   ├── ner.rs          # 人名识别（姓氏表 + 上下文线索）
│       │   ├── model_ner.rs    # 可选增强脱敏（轻量中文 NER，本地 CPU）
│       │   └── validator.rs    # 身份证 / Luhn / 统一社会信用代码校验
│       ├── redact/mod.rs       # 脱敏层（三种策略、字符级替换）
│       └── export/             # 导出层
│           ├── mod.rs          # 流程编排、文件命名
│           ├── txt.rs
│           ├── docx.rs         # 保留排版的 XML 定点改写
│           ├── pdf.rs          # 涂黑 + lopdf 组装图片型 PDF
│           └── audit.rs        # 合规留痕日志
├── scripts/
│   ├── fetch_pdfium.{sh,ps1}      # 构建前下载 pdfium（运行时离线）
│   ├── fetch_ocr_models.{sh,ps1}  # 首次安装下载 PP-OCR 模型
│   ├── export_ner_model.py         # 开发期导出/量化增强识别模型（不随包运行）
│   ├── make_icons.py              # 生成应用图标（无第三方依赖）
│   └── ui_smoke.mjs               # 前端冒烟测试
├── assets/ppocr_keys_v1.txt       # OCR 字典（PaddleOCR Apache-2.0）
├── docs/
│   ├── 规则编写教程.md
│   ├── 架构设计.md
│   └── 参考来源.md                 # 算法 / 数据 / 产品思路来源说明
├── licenses/                      # Apache-2.0 / MIT / BSD-3-Clause 全文
├── LICENSE                        # PolyForm Noncommercial 1.0.0
├── COMMERCIAL.md                  # 商业授权说明
├── THIRD_PARTY_NOTICES.md         # 第三方组件与来源清单
├── CONTRIBUTING.md / SECURITY.md / CODE_OF_CONDUCT.md / CHANGELOG.md
└── .github/workflows/{build,release}.yml
```

---

## 四、环境搭建

### 4.1 通用依赖

| 依赖 | 版本 | 说明 |
| --- | --- | --- |
| Rust | ≥ 1.77（推荐 stable） | <https://rustup.rs> |
| Node.js | ≥ 20 | 仅构建前端，运行时不需要 |
| npm | ≥ 10 | 或 pnpm / yarn |
| 系统编译工具 | — | macOS：Xcode Command Line Tools；Windows：MSVC Build Tools + WebView2（Win10/11 自带） |

macOS 安装编译工具：

```bash
xcode-select --install
```

Windows 需要安装 **Microsoft C++ 生成工具**（勾选“使用 C++ 的桌面开发”）以及
[WebView2 运行时](https://developer.microsoft.com/microsoft-edge/webview2/)（Win11 自带）。

### 4.2 安装依赖

```bash
git clone <your-repo-url> luminar
cd luminar
npm install
```

### 4.3 安装 pdfium（PDF 渲染 / 脱敏 PDF 必需）

```bash
# macOS / Linux
./scripts/fetch_pdfium.sh
# 输出：src-tauri/resources/libpdfium.dylib（或 .so）

# Windows（PowerShell）
pwsh scripts/fetch_pdfium.ps1
# 输出：src-tauri/resources/pdfium.dll
```

也可用环境变量指定任意路径：

```bash
export TUOMINXING_PDFIUM=/opt/pdfium/lib/libpdfium.dylib
```

> 缺少 pdfium 时程序仍可运行：PDF 退化为文本抽取，扫描件无法 OCR，脱敏 PDF 无法导出，
> 但 TXT / Word 导出不受影响。

### 4.4 模型文件（扫描件 / 增强脱敏）

> 模型已随仓库提交在 `src-tauri/resources/models/`：
> `det.onnx` + `rec.onnx` + `keys.txt`（OCR，约 13 MB）与 `ner/ner.onnx` + `ner/vocab.txt`
> （增强脱敏，约 12 MB），并内置进 Release 安装包，一般无需再安装；
> 本节仅用于更新 OCR 模型或切换 v4。老版本用户升级后，首次启动会把安装包里的
> `models/ner` 自动补到配置目录（约 12 MB），无需手工拷贝。

```bash
./scripts/fetch_ocr_models.sh
# 默认安装到：
#   macOS   ~/Library/Application Support/cn.luminar.app/models/
#   Windows %APPDATA%\cn.luminar.app\models\
```

脚本会下载 PP-OCR 的 `det.onnx`（检测）与 `rec.onnx`（识别，默认 v3，`OCR_VERSION=v4` 可切换），
并复制仓库自带的 `assets/ppocr_keys_v1.txt` 为 `keys.txt`。模型只在安装时下载，**程序运行时不联网**。

Windows 原生 PowerShell：

```powershell
pwsh scripts/fetch_ocr_models.ps1
# 或安装到指定目录：
pwsh scripts/fetch_ocr_models.ps1 -Dest .\models
```

开发时也可放到项目内，避免污染用户目录：

```bash
./scripts/fetch_ocr_models.sh ./models
export TUOMINXING_MODELS="$PWD/models"
```

### 4.5 开发运行

```bash
npm run app:dev        # 等价于 tauri dev，自动编译 Rust 并打开窗口
```

只调前端（浏览器里看不到 Tauri 命令，会提示初始化失败）：

```bash
npm run dev
```

---

## 五、编译打包

### 5.1 macOS（Apple Silicon）

```bash
npm run build                          # 前端产物 -> dist/
npm run tauri build -- --target aarch64-apple-darwin
```

产物：

- `src-tauri/target/aarch64-apple-darwin/release/bundle/macos/星星脱敏.app`
- `src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/Luminar_0.11.0_aarch64.dmg`
- Universal：`src-tauri/target/universal-apple-darwin/release/bundle/dmg/Luminar_0.11.0_universal.dmg`

Intel Mac 把 target 换成 `x86_64-apple-darwin`；也可打通用包：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

**分发给其他律师前**建议做签名与公证（否则首次打开会提示“无法验证开发者”）：

```bash
codesign --deep --force --options runtime \
  --sign "Developer ID Application: 你的名字 (TEAMID)" \
  "src-tauri/target/aarch64-apple-darwin/release/bundle/macos/星星脱敏.app"

xcrun notarytool submit "…/Luminar_0.11.0_universal.dmg" \
  --apple-id "你的 Apple ID" --team-id TEAMID --password "应用专用密码" --wait
xcrun stapler staple "…/Luminar_0.11.0_universal.dmg"
```

### 5.2 Windows（原生构建，推荐）

**Windows 版没有桌面图标，必须双击 `Luminar.exe` 才能运行。**
Mac 版则是：双击 DMG → 桌面出现「星星脱敏」磁盘图标 → 双击磁盘图标 → 双击 `星星脱敏.app`。
两份图文步骤见 [`docs/Windows-打开说明.md`](docs/Windows-打开说明.md) 与
[`docs/Mac-打开说明.md`](docs/Mac-打开说明.md)。

> 为了方便大家使用，本软件做成了**无需安装**的形式：解压后直接双击 **「Luminar.exe」** 即可运行，不需要安装、不需要管理员权限。

在 Windows 机器上（或 GitHub Actions 的 `windows-latest`）：

```powershell
npm install
.\scripts\fetch_pdfium.ps1   # 或手动放置 pdfium.dll（见 4.3）
npm run tauri build
```

产物：

- `src-tauri\target\release\Luminar.exe`（主程序）
- `src-tauri\target\release\bundle\nsis\Luminar_0.11.0_x64-setup.exe`（安装包，中文界面）

前置条件：

- **Rust MSVC 工具链**：`rustup default stable-msvc`；
- **Visual Studio Build Tools**：勾选“使用 C++ 的桌面开发”；
- **WebView2**：Win11 自带；Win10 如缺失，NSIS 安装包内置的 bootstrapper 会自动下载
  （需要一次联网）。如果目标电脑完全不能联网或没有 WebView2，请使用
  `*_full_offline.zip`（内置 WebView2 固定运行时）或把固定运行时放进
  `resources/WebView2Runtime/`。

如需安装包内置 OCR 模型：

```powershell
.\scripts\fetch_ocr_models.ps1 -Dest .\src-tauri\resources\models
```

产物与分发说明：

- `src-tauri\target\release\Luminar.exe`：主程序（需同目录 resources/）；
- `src-tauri\target\release\bundle\nsis\Luminar_0.11.0_x64-setup.exe`：NSIS 安装包，
  简体中文、**仅当前用户安装**，无需管理员权限，适合律所内网统一分发；
- 未签名的安装包会触发 SmartScreen“更多信息 → 仍要运行”，正式分发建议购买代码签名证书；

### 5.3 Windows 免安装包（可从 macOS 交叉构建）

Tauri 的 NSIS 安装器只能在 Windows 上生成；macOS 可以用 `cargo-xwin` + LLVM MinGW
交叉编译出 x64 exe，再和 `resources/` 打包成免安装 zip：

```bash
# 1) 准备 Windows MSVC 头文件/库（cargo-xwin 自动下载）
cargo install cargo-xwin --locked
rustup target add x86_64-pc-windows-msvc

# 2) 准备 clang-cl / llvm-lib / lld-link（可用 llvm-mingw 发行包）
export PATH="/path/to/llvm-mingw/bin:$PATH"
export CC_x86_64_pc_windows_msvc=clang-cl
export AR_x86_64_pc_windows_msvc=llvm-lib
export XWIN_ACCEPT_LICENSE=1

# 3) 只编译 exe，不生成 NSIS
npm run tauri build -- --target x86_64-pc-windows-msvc --runner cargo-xwin --no-bundle

# 4) 组装免安装目录：exe + pdfium.dll + models/ + README
```

产物目录结构（资源必须和 exe 同级，不能只拷 exe）：

```text
星星脱敏/
├── Luminar.exe
├── 打开说明.txt
├── 点击这个启动程序.txt
├── LICENSE / THIRD_PARTY_NOTICES.md
└── resources/
    ├── pdfium.dll
    └── models/            # det.onnx / rec.onnx / keys.txt / ner/
```

本次已按上述方式生成 `release/Luminar_0.11.0_windows_x64_portable.zip`（约 30 MB），
解压后直接双击 **「Luminar.exe」** 即可。免安装版没有开始菜单/桌面快捷方式；
需要 NSIS 安装器的，用 Windows 原生构建（当前 Release 由维护者本地构建）。
- 配置目录：`%APPDATA%\cn.luminar.app\`（规则 `rules.json`、设置 `settings.json`、模型 `models\`）。

> **跨平台编译说明**：Tauri 依赖平台 WebView（macOS WebKit / Windows WebView2），
> 不能在 macOS 上直接产出 Windows exe。请用 Windows 机器构建；当前 Release 由维护者本地构建。
> GitHub Actions 工作流已随源码提交，可在 Actions 页面手动触发；推送 `v*` 标签会创建 Release 草稿。

### 5.4 自动构建

`.github/workflows/build.yml` 已配置两套矩阵：`macos-14`（编译 arm64 + x64，
产出 Universal 通用包）与 `windows-latest`；推送 main 或手动触发即可构建。
`.github/workflows/release.yml` 在推送 `v*` 标签时构建并创建 Release 草稿。

### 5.5 常见打包问题

| 现象 | 原因与处理 |
| --- | --- |
| macOS 打 DMG 报 `Finder 遇到一个错误 (-1712)` | 无图形会话/自动化权限时 AppleScript 超时。改用 `npm run tauri build -- --bundles app` 只产出 `.app`，DMG 可稍后在桌面环境或 CI 中生成 |
| 首次打开提示“无法验证开发者” | 未签名/未公证。右键 → 打开；或 `xattr -dr com.apple.quarantine "星星脱敏.app"` |
| Windows 启动白屏 | 缺少 WebView2：改用完整离线包 `*_full_offline.zip`，或安装微软官方 Evergreen Runtime |
| 提示 pdfium 架构不匹配 | Apple Silicon 需 arm64 的 `libpdfium.dylib`，Intel 需 x64 版本；重新执行 `scripts/fetch_pdfium.sh` |
| 扫描件提示缺少 OCR 模型 | 执行 `scripts/fetch_ocr_models.sh`，或在「设置 → 打开模型目录」放入 `det.onnx`/`rec.onnx`/`keys.txt` |
| 扫描件识别时卡在某一页 | 进度条会显示当前页；若长时间不动，可能是该页版式复杂或 OCR 外部命令卡住，可先取消导入并拆分该页 |
| 「增强脱敏」开关灰色不可用 | 确认 `models/ner/ner.onnx` 与 `vocab.txt` 存在；安装包用户可重新安装，源码用户确认模型已提交 |

---

## 六、使用流程

1. **导入**：左栏「批量导入文书」，或多选文件拖入窗口；导入即自动识别。
2. **复核**：
   - 右侧清单逐条查看，取消误报的勾选；
   - 发现漏标：在正文中用鼠标选中文字 → 浮层选择分类/方式 → 「确认标记」；
   - 点「定位」跳到原文位置；「编辑」可改分类与脱敏方式。
3. **预览**：中间工具栏切「原文 / 脱敏 / 对照」，拖动阈值滑块调整识别严格程度。
4. **导出**：右栏底部「应用脱敏并导出」→ 选择文书、格式（TXT / Word / PDF）、输出目录。
5. **留痕**：导出目录中会多出 `*_脱敏日志_时间戳.json` 与 `.txt`，随卷归档。

> 批量导出：左栏勾选多份文书后点「批量导出」，格式与目录对整批生效。

---

## 七、规则编写教程（摘要）

完整版见 [`docs/规则编写教程.md`](docs/规则编写教程.md)。

内置规则在首次启动时写入：

```text
macOS   ~/Library/Application Support/cn.luminar.app/rules.json
Windows %APPDATA%\cn.luminar.app\rules.json
```

规则示例（正则 + 占位标签）：

```json
{
  "id": "custom_project_code",
  "name": "项目代号",
  "category": "custom",
  "category_label": "项目代号",
  "kind": "regex",
  "pattern": "星河计划[-—]?[0-9]{2,6}",
  "capture_group": 0,
  "confidence": 0.95,
  "strategy": {
    "kind": "placeholder",
    "mask_char": "*",
    "keep_prefix": 0,
    "keep_suffix": 0,
    "placeholder": "【{type}】"
  },
  "validator": "none",
  "priority": 30,
  "enabled": true,
  "require_boundary": false,
  "case_insensitive": false,
  "builtin": false,
  "description": "所内项目代号，对外交付时统一替换"
}
```

要点：

- 正则语法为 [Rust `regex`](https://docs.rs/regex/latest/regex/#syntax)：**不支持后行断言**，
  需要“前面不是数字”这类约束时，使用捕获组 + `capture_group`，或勾选「边界校验」。
- `capture_group`：0 表示整段命中；填 1 表示取第一个括号内容（如「代理人：(.{2,4})」只脱敏姓名）。
- 校验器：`id_card` / `luhn` / `uscc` 会按校验位微调置信度，校验失败只降权不丢结果。
- 占位模板支持 `{type}`（分类名）与 `{index}`（同类序号），如 `【{type}-{index}】`。
- 关键词规则按「包含」匹配，长词优先；适合所名、客户简称、项目代号、内部编号。
- 改完规则后点「重新识别」或直接拖阈值，即可对已导入文档重新跑一遍。

---

## 八、性能与内存

在 M1 Pro（macOS，纯 CPU）上的实测：

| 场景 | 耗时 |
| --- | --- |
| 解析 500 字 Word（含 XML 扫描） | 1–3 ms |
| 解析 1 页文字版 PDF（pdfium 字符坐标） | 30–50 ms |
| 识别 500 字（13 类规则 + NER） | 2 ms |
| 扫描件 OCR（200 DPI，1 页 20 行） | 2–4 s（界面显示逐页进度） |
| 增强脱敏模型（500 字，CPU） | 约 90 ms |
| 导出 TXT + DOCX + PDF（1 页） | 250–300 ms |

- 全文常驻内存以 `String` + 字符数组保存，10 万字约 1–2 MB；批量 20 份文档约 30–60 MB。
- 规则一次性编译缓存，识别为线性时间扫描，无回溯风险。
- OCR 模型只在首次使用时加载并缓存，后续页面复用同一会话；
- 增强脱敏模型只在开关打开后加载，首次约 0.15 s，之后复用缓存；纯规则模式不加载。
- 解析/识别/导出都在 `spawn_blocking` 中执行，界面不卡顿。
- OCR 引擎**惰性加载**：只有遇到扫描件才读模型，纯文字文书导入不受影响；
  首次加载模型约 1–3 秒，之后复用缓存。
- 扫描件按页 OCR，约 2–4 秒/页，100 页大约需要 4–7 分钟，属正常耗时；导入期间请勿关闭窗口，
  界面会提示“仍在处理”。模型随安装包内置，不需要本机预装 PaddleOCR/Python。

---

## 九、已知限制

1. **脱敏 PDF 是图片型**（不可检索、不可复制）。这是为了彻底消除文字层残留导致的复制泄露；
   如需可检索的脱敏 PDF，请在导出后对成品再做一次 OCR。
2. PDF 文字层顺序异常（少见）时，抽取出的文本可能与视觉顺序不一致；此时可用 TXT/Word 导出
   或对源文件重新导出为标准 PDF。
3. Word 导出保留段落级排版（pPr、表格、图片、页眉页脚），但**被脱敏段落的行内格式**
   （同一段内的局部加粗/字号）会统一为第一个 run 的格式。
4. 人名识别以「姓氏表 + 上下文线索」的可解释规则为主；对非常见姓氏、无上下文的人名
   可能漏标。打开「增强脱敏」后会用 12 MB 的本地 NER 小模型补充，但模型评估集 F1 约 0.87，
   仍可能漏标或把单位/地名标得过宽，必须人工复核。
5. 脱敏 PDF 的遮盖范围按字符框计算，个别特殊字体（艺术字、竖排）可能覆盖不全，导出后请抽查。
6. `.doc` 已支持原生 Word 97-2003 解析，但 Word 6/95、加密文档或结构异常时仍需
   LibreOffice（macOS 的 `.doc` 可退化到 `textutil`）；`.wps` 的二进制版本视 LibreOffice 兼容性而定。
7. RTF / HTML / OFD 只提取**文字**，不保留原始版式；脱敏 Word 是重新生成的，
   PDF 导出仅对 PDF 源启用。扫描图片型 OFD 没有文字层，需先转 PDF/图片再 OCR。

---

## 十、离线与安全声明

- 代码中**没有任何 HTTP 客户端**，不请求任何接口；OCR、增强脱敏模型与 pdfium 都是本地文件。
- 不写入用户目录以外的位置：仅应用配置目录（规则/设置/模型）与用户主动选择的导出目录。
- 不修改原始文书；导出文件一律新建，重名自动追加序号。
- 日志包含源文件 SHA-256，可用于证明“导出件与哪份原件对应”，便于卷宗留痕。
- 本工具只做脱敏；不连接任何在线大模型，内置的只是 12 MB 的 NER 小模型与 13 MB 的 OCR 模型。
  脱敏后的文本请自行交给本地 DeepSeek 等外部流程。

## 十一、意见反馈

- 请通过 [GitHub Issues](https://github.com/myleemartin/luminar/issues) 反馈。
- 软件内「意见反馈」栏保留原有联系方式，不在 GitHub 仓库公开展示。

软件底部也常驻同一组联系方式，点击官网或邮箱即可复制；反馈时请附上软件版本、
操作系统、问题现象或脱敏效果截图（请先隐去敏感信息）。功能建议、规则误报/漏报、
安装问题都欢迎直接联系。

> 免责：脱敏结果需由执业律师复核后使用。任何自动化工具都无法保证 100% 召回，
> 对外交付前请抽查正文与导出件。

---

## 十一、开发与测试

```bash
# Rust 单元测试（识别、校验、脱敏）
cd src-tauri && cargo test --lib

# 端到端集成测试（需要 tests/fixtures 下的样例文书）
cd src-tauri && cargo test --test pipeline_test -- --nocapture

# 扫描件 OCR 集成测试（需模型）
cd src-tauri && TUOMINXING_MODELS=../models cargo test --test pipeline_test scanned -- --nocapture

# 命令行流水线：解析 → 识别 → 脱敏 → 导出
cd src-tauri && cargo run --example pipeline -- ../tests/fixtures/民事起诉状.docx /tmp/out
```

前端 UI 冒烟测试（jsdom 加载真实构建产物，mock Tauri IPC，验证导入 → 高亮 → 取消 → 脱敏预览 → 筛选）：

```bash
npm install          # 已包含 jsdom
npm run test:ui
```

测试样例（`tests/fixtures/`）为虚构数据，已随仓库提供，无需重新生成。

---

## 十二、参与贡献

- 贡献指南：[CONTRIBUTING.md](CONTRIBUTING.md)
- 行为准则：[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- 安全问题：[SECURITY.md](SECURITY.md)（请勿公开披露）
- 更新记录：[CHANGELOG.md](CHANGELOG.md)

优先方向：更多法律行业规则（仲裁/执行案号、公证编号、执业证号）、更高精度的本地 NER、
批量导出并行化、超大文档流式处理、Windows/macOS 安装包签名与自动更新。

提交前请确保 `cargo test`、`npx tsc --noEmit`、`npm run build` 全部通过。

---

## 十三、第三方组件与参考来源

本项目是独立的 Rust 实现，**没有复制任何第三方项目的源代码**。主要依赖与参考：

| 名称 | 许可证 | 用途 |
| --- | --- | --- |
| [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) / PP-OCR | Apache-2.0 | DB 检测、CRNN+CTC 识别算法思路；`ppocr_keys_v1.txt` 字典 |
| [RapidOCR](https://github.com/RapidAI/RapidOCR) | Apache-2.0 | PP-OCR 的 ONNX 模型分发（安装时下载） |
| [pdfium](https://pdfium.googlesource.com/pdfium/) | BSD-3-Clause | PDF 渲染、字符坐标、扫描件栅格化 |
| [Tauri](https://tauri.app/) | MIT / Apache-2.0 | 桌面框架 |
| [lopdf](https://github.com/J-F-Liu/lopdf) / [pdfium-render](https://github.com/ajrcarey/pdfium-render) | MIT / MIT OR Apache-2.0 | PDF 结构解析与 pdfium 绑定 |
| [docx-rs](https://github.com/bokuweb/docx-rs) / roxmltree | MIT / MIT OR Apache-2.0 | Word 生成与解析 |
| [tract](https://github.com/sonos/tract) | MIT / Apache-2.0 | ONNX 纯 Rust 推理 |
| Microsoft Presidio / pdfplumber | MIT | 识别器分层、PDF 文本抽取的产品思路（概念参考） |

完整的依赖清单、版权声明与许可证全文见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)；
算法与数据的逐项参考说明见 [docs/参考来源.md](docs/参考来源.md)。

---

## 十四、许可证

本项目采用 **[PolyForm Noncommercial License 1.0.0](LICENSE)**：

- ✅ 个人学习、研究、公益等非营利用途：免费（著作权人自身商用不受限）；
- ❌ 任何商业用途（销售、SaaS、收费服务、商业公司生产经营等）：需事先取得书面授权，
  见 [COMMERCIAL.md](COMMERCIAL.md)；
- 修改、非商业分发须保留许可证、`THIRD_PARTY_NOTICES.md` 与版权声明。

> PolyForm Noncommercial 属于 **source-available（源码公开）** 许可，而非 OSI 定义的
> “open source”。我们选择它的原因：源码完全公开、便于审计与二次开发，但不允许他人
> 拿去做商业发行。

第三方组件（PaddleOCR、pdfium、Tauri 等）仍按其各自许可证分发，详见
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 与 [`licenses/`](licenses/)。

---

*脱敏结果需由执业律师复核后使用。任何自动化工具都无法保证 100% 召回，对外交付前请抽查。*
