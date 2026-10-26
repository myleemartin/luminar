# 更新日志

本项目遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [0.11.0] - 2026-10

### 更名与品牌

- 软件正式更名为 **星星脱敏（Luminar）**，版本号升为 0.11.0；
- 应用标识符改为 `cn.luminar.app`；项目首发即使用新标识符，不保留旧配置目录；
- 界面、合规日志、安装包名称统一为「星星脱敏」。

### 修复

- **修复导入/重新解析卡死**：持有配置锁时再次读取配置导致 `std::sync::Mutex` 重入死锁，
  界面会一直停留在“正在解析”。已修复并新增 20 秒超时回归测试。
- 修复取消勾选时点击事件误触发“定位”的问题；
- 修复导出弹窗在文档选择变化时 PDF 选项状态不同步的问题。

### 稳定性与安全加固

- 导出前校验源文件 SHA-256、PDF 页面数；源文件被替换或页面错位时**拒绝导出**，避免漏脱敏；
- 脱敏 PDF 改为**逐页渲染**，百页卷宗不再把整本页图同时驻留内存；
- 某页缺少坐标但又有命中时直接失败，不再输出“看起来已脱敏”的页面；
- 混合型 PDF（正文有文字层 + 夹扫描页）只对无文字页做 OCR；
- 跨段落人工标记自动拆分为每段一条，避免 Word 导出重复替换；
- 预览不再因勾选/取消标记而跳回顶部，文件与结果列表保留滚动位置；
- 外部 OCR 命令增加 3 分钟超时并强制终止，避免卡死；
- `rules.json`/`settings.json` 损坏时先备份再回退，用户主动清空的规则不会被重置；
- Windows 配置写入兼容 `rename` 不能覆盖已存在文件的行为；
- 导出重名极端情况下改用时间戳兜底，绝不覆盖已有文件；
- 弹窗 Esc 只关闭最上层；提示/确认框改为纯文本渲染，避免文档内容注入 HTML；
- 修复 Tailwind 把运行时拼接的分类高亮 class（`cat-*`）purge 掉、导致标记颜色丢失的问题。

### 改进

- OCR 引擎改为**惰性加载**：只有遇到扫描件才加载模型，纯文字文书导入不再等待；
- 导入过程输出 `[导入]` 日志，长任务 12 秒后提示“仍在处理”；
- 手写 `rules.json` 支持省略 `id`、`category`、`confidence`、`strategy` 等字段，自动补默认值；
- Word 导出增加一致性校验：源文件在导入后被修改时回退为重新生成，避免错位漏脱敏。

### Windows

- NSIS 安装包配置完善（简体中文、当前用户安装、内置 WebView2 引导程序）；
- 新增 `scripts/fetch_pdfium.ps1` 与 `scripts/fetch_ocr_models.ps1`；
- `open_path` 兼容 `explorer.exe` 的成功返回码差异；
- 新增 Windows 原生构建矩阵（GitHub Actions）与 PowerShell 依赖脚本。

### 开箱即用的 OCR

- PP-OCRv3 的 `det.onnx`、`rec.onnx` 与字典直接随仓库、随安装包分发（约 13 MB），
  任何电脑装好即可识别扫描件，不需要本地已有模型，也不需要安装 PaddlePaddle；
- 模型目录优先级：环境变量 → 用户配置目录 → 安装包内置，三者都缺失时才提示安装。

### 增强脱敏（可选）

- 新增内置中文 NER 小模型（`hfl/minirbt-h288` 微调，INT8 约 12 MB），
  在顶部「增强脱敏」开关打开后补充识别姓名、单位、律所、法院、完整住址；
- 纯 Rust `tract` + ONNX，本地 CPU 推理，不联网、不依赖 Python/PyTorch；
- 默认关闭，关闭时不加载模型；模型结果标记为「模型识别」，可逐条取消；
- 规则优先：模型只补漏，不和规则抢已有命中；
- 启动时会把安装包内置的 `models/ner` 补到老用户配置目录，避免升级后开关不可用。

### macOS 兼容性修复

- pdfium 固定到 4.30.1（x64 最低 10.13、arm64 最低 11），避免最新版悄悄把最低系统提到 macOS 13；
- `pdfium-render` 固定到 0.8.32 并 vendored 以兼容该 pdfium；
- 重新构建 Universal App，Intel 10.15 / Apple Silicon 11 均可加载 PDF 引擎；
- 34 个单元测试 + 5 个 PDF/集成测试全部通过。

### 全环境自包含

- macOS 改为 Universal 包（arm64 + x64），pdfium 也合并为通用 dylib；
- Windows 新增完整离线包，内置 WebView2 固定运行时，未装 WebView2 的 Win10/LTSC 也能启动；
- 标准包仍保留 30 MB 轻量版，供已有 WebView2 的电脑使用；
- 新增 `AGENTS.md`：给 AI 助手的环境评估、选包、下载校验、逐步教用户打开的说明。

### Word 97-2003 原生支持

- 新增纯 Rust `.doc` 解析：`cfb` 读取 OLE 复合文档，解析 FIB、CLX/piece table，
  压缩片段按 GB18030、未压缩片段按 UTF-16LE 提取正文；
- 大多数 `.doc` 不再依赖 LibreOffice；只有 Word 6/95、加密或结构异常文档才回退转换器；
- `.wps` 同样先尝试原生解析，OOXML 版直接按 docx 读；
- 新增合成 Word 97 单元测试与开发期 `examples/doc_probe.rs`。

### Windows 中文程序名

- Windows 版主程序改为 **「星星脱敏.exe」**，压缩包内目录也改为「星星脱敏」；
- 文档明确说明：本软件是无需安装的绿色版，解压后直接双击「星星脱敏.exe」即可运行；
- 新增 `docs/Mac-打开说明.md` 与 `docs/Windows-打开说明.md`；
  Windows 压缩包内放「打开说明.txt」和「点击这个启动程序.txt」，Mac DMG 内放「打开说明.txt」。

### 导出修复

- 默认只导出每份文档的原格式：PDF 只出 PDF、Word 只出 Word、TXT 只出 TXT，
  RTF/OFD/旧版 Office 出 Word；不再因为格式复选框默认勾选而多生成两份；
- 新增「仅导出原格式」开关（默认开），关闭后才手动多选格式；
- 导出目录默认当前用户桌面，保留「选择…」手动修改。

### Windows 与格式扩展

- 新增 Windows x64 免安装包（cargo-xwin 交叉编译，约 30 MB），含 pdfium 与 OCR/NER 模型；
- 新增常见法律文书格式：`.docm`、`.doc` / `.wps`（LibreOffice 转换，`.doc` 在 macOS 可用 textutil 兜底）、
  `.rtf`、`.ofd`、`.html` / `.htm` / `.xhtml`；
- `.docm` 导出时重新生成 docx，剥离 VBA 宏，避免把宏带进交付件；
- RTF 支持 `\uN` / GBK 转义；OFD 按 TextCode 坐标重组文字；HTML 去脚本/样式并解码实体；
- 重复导入先用「路径 + 大小」粗筛再算 SHA-256，大文件首次导入不再白读一遍；
- 文档字符数写入会话缓存，刷新列表不再反复扫描全文；
- 旧版 `.doc/.wps` 转换超时 180 秒，临时目录解析后立即清理；
- Windows x64 免安装包已产出（约 30 MB），含 pdfium 与全部模型；
- 性能：重复导入先按路径+大小粗筛，不再给新文件白算 SHA-256；文档字符数缓存，刷新列表不重复扫描全文。

### OCR 进度

- 扫描件 OCR 增加**逐页进度条**：忙碌遮罩中显示「已识别 n / N 页」和百分比，
  换文件时自动重置；混合型 PDF 只统计真正需要 OCR 的页面；
- 后端通过 `ocr-progress` 事件推送页级进度，导入和重新解析都支持；
- OCR 模型加载阶段也会显示“正在加载 OCR 模型…”，避免误以为卡死。

### 意见反馈

- 软件底部新增常驻「意见反馈」栏（联系方式保留在软件内，不在仓库公开）；
- 官网/邮箱可点击复制，方便用户直接粘贴到邮件或微信；Toast 上移避免遮挡页脚。

### 构建与发布

- 新增 `tauri.macos.conf.json` / `tauri.windows.conf.json` 平台化打包目标（macOS app+dmg、Windows nsis），
  避免跨平台目标互相干扰；
- Release 工作流会自动下载 pdfium 与 PP-OCR 模型并内置进安装包，用户装好即用；
- 新增 `docs/发布清单.md`，列出发布前需替换的仓库地址、邮箱与版本号。

### 许可证与开源材料

- 许可证改为 **PolyForm Noncommercial 1.0.0**（源码公开、免费非商业使用）；
- 新增 `COMMERCIAL.md`、`THIRD_PARTY_NOTICES.md`、`licenses/`、`docs/参考来源.md`；
- 补齐 `CONTRIBUTING.md`、`SECURITY.md`、`CODE_OF_CONDUCT.md`、Issue/PR 模板、Release 工作流。

## [0.1.0] - 2026-10（内部预览）

- 首个可运行版本：批量导入 docx/PDF/TXT、规则识别、人工复核、TXT/Word/脱敏 PDF 导出、
  合规日志、PaddleOCR 本地 OCR。
