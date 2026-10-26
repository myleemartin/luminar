# 星星脱敏 · AI 助手部署与使用说明

> 这份文档是写给 AI 助手（Claude Code、Codex、Cursor、OpenClaw、DSH 等）看的。
> 你的任务不是“把项目 clone 下来就结束”，而是：**评估环境 → 下载正确包 → 安装/解压 → 在对话框里一步一步告诉用户怎么打开和使用**。
>
> 项目地址：`https://github.com/myleemartin/luminar`
> 如果该地址打不开、仓库为空或没有 Release，说明项目尚未正式发布；不要改用第三方下载站，
> 直接告诉用户“仓库/Release 还没有发布”，请用户提供安装包或 Release 链接。

## 0. 先记住三件事

1. **不要要求普通用户安装 Python、PaddleOCR、PaddlePaddle、Node.js、Rust 或 LibreOffice。**
   发布包已经内置 OCR 模型、增强脱敏模型、pdfium 和运行库。
2. **Windows 有两个包**：
   - `*_portable.zip`：约 30 MB，体积小，要求目标电脑已有 WebView2（Windows 11 自带，正常更新的 Windows 10 一般也有）；
   - `*_full_offline.zip`：约 326 MB（解压约 720 MB），内置 WebView2 固定运行时，
     **完全离线、不依赖目标电脑装过 WebView2**。
   无法判断环境时，优先让用户下载 full offline。
3. **Mac 用 Universal DMG**：同时支持 Apple Silicon（M1/M2/M3/M4）和 Intel Mac。
   不要给用户 `aarch64` 单架构包，除非确认是 Apple Silicon 且想省下载体积。

## 1. 环境评估（先做，不要直接下载）

### 1.1 判断系统与架构

macOS：

```bash
uname -s          # Darwin
uname -m          # arm64 或 x86_64
sw_vers -productVersion
```

Windows（PowerShell）：

```powershell
$env:PROCESSOR_ARCHITECTURE          # AMD64
[System.Environment]::OSVersion.Version
Get-PSDrive C | Select-Object Used,Free
```

Linux：本项目**不提供 Linux 桌面安装包**。如果用户是 Linux，明确说明需要源码构建且依赖 WebKitGTK，
不要假装有现成包。

### 1.2 检查 Windows WebView2

PowerShell：

```powershell
$paths = @(
  "$env:ProgramFiles(x86)\Microsoft\EdgeWebView\Application",
  "$env:ProgramFiles\Microsoft\EdgeWebView\Application",
  "$env:LOCALAPPDATA\Microsoft\EdgeWebView\Application"
)
$paths | ForEach-Object { if (Test-Path $_) { Get-ChildItem $_ } }
```

只要任意一个目录下有版本子目录（里面通常有 `msedgewebview2.exe`），系统就已有 WebView2：

- **有 WebView2** → 可以下载 `*_portable.zip`；
- **没有 / 不确定 / 目标电脑不能联网** → 下载 `*_full_offline.zip`。

### 1.3 检查磁盘与权限

- 标准包解压后约 50 MB；完整离线包解压后约 720 MB，预留 **1.2 GB** 空间更稳。
- macOS Universal 同时支持 Intel（macOS 10.15+）和 Apple Silicon（macOS 11+）；
  Windows 包是 x64（Windows 10/11，Windows on ARM 走 x64 模拟）。
- Windows 安装/解压到用户有写权限的目录（桌面、`D:\Luminar`、`%LOCALAPPDATA%\Programs`），
  不要放 `C:\Program Files` 根下再让普通用户写配置。
- macOS 解压 DMG 后建议拖到「应用程序」。

## 2. 下载项目（按评估结果选包）

优先从 GitHub Releases 下载**预编译包**，不要为了使用去源码构建。

推荐文件名：

| 系统 | 文件 | 适用 |
| --- | --- | --- |
| macOS | `Luminar_0.11.0_universal.dmg` | Apple Silicon + Intel |
| Windows（标准） | `Luminar_0.11.0_windows_x64_portable.zip` | 已装 WebView2 |
| Windows（完整离线） | `Luminar_0.11.0_windows_x64_full_offline.zip` | 未装 WebView2 / 内网 / 不确定（约 326 MB） |

步骤：

1. 打开 `https://github.com/myleemartin/luminar/releases`
2. 选最新 Release，下载对应文件；
3. 同时下载同名 `.sha256`，校验：
   - macOS：`shasum -a 256 <文件>`
   - Windows：`Get-FileHash <文件> -Algorithm SHA256`
4. 校验值不一致就不要运行，重新下载。

如果 Release 不存在，但用户要求从源码构建，见第 6 节；否则停止并告知用户。

## 3. 安装 / 打开（必须按步骤说给用户听）

> ⚠️ 这一节是硬要求：**你必须在对话框里把下面步骤讲清楚**，不能只说“下载好了”。
> 好的说法示例：
> “我已经把安装包下载到 `~/Downloads/Luminar_0.11.0_universal.dmg`。
> 现在请你：1）双击这个 DMG；2）桌面上会出现「星星脱敏」磁盘图标；3）双击磁盘图标；
> 4）在窗口里双击「星星脱敏.app」。打开后回我一句‘已打开’，我再带你导入第一份文书。”
> 不要只回复“完成”“已部署”。

### 3.1 macOS

1. 双击下载的 `Luminar_0.11.0_universal.dmg`；
2. 桌面上会出现名为 **「星星脱敏」** 的磁盘图标（不是直接运行）；
3. 双击这个磁盘图标，打开 DMG 窗口；
4. 双击窗口里的 **「星星脱敏.app」**；
5. 第一次打开如果提示“无法验证开发者”：
   - 在「星星脱敏.app」上右键 →「打开」→ 再点「打开」；
   - 或到「系统设置 → 隐私与安全性」点「仍要打开」。
6. 建议把 App 拖到「Applications（应用程序）」。
7. 看到三栏主界面 = 启动成功。

### 3.2 Windows（标准包和完整离线包步骤一样）

1. **完整解压** zip 到本地目录，例如 `D:\Luminar` 或桌面；不要直接在压缩包里双击；
2. 进入解压后的 `星星脱敏` 文件夹；
3. **双击 `Luminar.exe`**（Windows 没有桌面图标，必须点 EXE）；
4. 出现“已保护你的电脑 / 未知发布者”时：点「更多信息」→「仍要运行」；
5. 不要只把 `Luminar.exe` 单独拷走，必须保留同目录 `resources\`；
6. 看到三栏主界面 = 启动成功。

完整离线包内还有 `resources\WebView2Runtime\`，**不要删除**；程序会自动使用它。
完整离线包首次启动时要在本机设置 WebView2 运行时权限，可能多等 5–15 秒，**不要以为卡死而反复双击**。
它不能放在网络共享/UNC 路径下运行，必须先解压到本地磁盘。
标准包如果提示缺少 WebView2，改下完整离线包，不要让用户自己找运行库。

## 3.3 依赖到底带了什么

| 依赖 | macOS Universal | Windows 标准包 | Windows 完整离线包 |
| --- | --- | --- | --- |
| WebView / WebKit | 系统 WebKit（macOS 自带） | 系统 WebView2 | **内置 WebView2 固定运行时** |
| PDF 渲染 | 内置 pdfium（arm64+x64） | 内置 pdfium.dll | 内置 pdfium.dll |
| OCR 模型 | 内置 | 内置 | 内置 |
| 增强脱敏模型 | 内置 | 内置 | 内置 |
| Python / PaddleOCR | 不需要 | 不需要 | 不需要 |
| Node / Rust | 运行不需要 | 运行不需要 | 运行不需要 |
| VC++ 运行库 | 系统自带 | Windows 10/11 UCRT | Windows 10/11 UCRT + 运行时自带 VC 库 |
| 字体 | 系统字体 | 系统字体 | 系统字体 |

核心功能不需要 LibreOffice。只有二进制 WPS、加密 Word、Word 6/95 等边缘格式，
才会回退到本机 LibreOffice；这类文件让用户另存为 `.docx` 即可。

## 4. 第一次使用（AI 要带着用户走）

打开软件后，用自然语言告诉用户：

1. 左栏点 **「批量导入文书」**，或把文件直接拖进窗口；
2. 支持的格式：
   `.docx` / `.docm` / `.doc` / `.wps` / `.pdf`（文字版、扫描件）/ `.rtf` / `.ofd` / `.html` / `.htm` / `.txt` / `.md`；
3. 扫描件会自动 OCR，界面显示「已识别 n / N 页」进度；100 页大约几分钟，不要关窗口；
4. 右栏逐条复核：取消误报、点「编辑」改分类/脱敏方式、正文框选可手动补标；
5. 点「应用脱敏并导出」：
   - 默认只导出**原格式**（PDF→PDF、Word→Word、TXT→TXT；RTF/OFD/旧版 Office→Word）；
   - 默认输出到**桌面**，可点「选择…」改目录；
   - 「合规脱敏日志」默认开启，会多出 JSON + TXT 两个日志；不需要可取消；
6. 导出完成后，告诉用户文件在桌面的哪个文件名，不要只说“导出了”。

首次建议让用户先拿一份**不含真实隐私的样例**试跑，确认效果后再批量。

## 5. 常见问题

| 现象 | 处理 |
| --- | --- |
| Windows 双击没反应/闪退 | 先确认是否完整解压、`resources` 是否在同目录；再确认是否用标准包但机器没有 WebView2，改用 full offline |
| Windows 提示缺少 WebView2 | 用 `*_full_offline.zip`，不要只发一个 WebView2 安装链接让用户自己折腾 |
| macOS 提示无法验证开发者 | 右键 App →「打开」；或系统设置 → 隐私与安全性 → 仍要打开 |
| 扫描件一直 0 页/识别慢 | 看进度条；确认模型目录未被杀软删除；100 页 OCR 数分钟属正常 |
| `.doc` 能打开但排版变了 | `.doc` 是原生文字解析，只保证正文；需要原排版请让用户另存 `.docx` |
| `.wps` 失败 | 先确认是否是 OOXML 版；二进制 WPS/加密文档需要本机 LibreOffice，或另存 `.docx` |
| 导出多出两个文件 | 那是 `_脱敏日志.json` / `.txt`，不是文书格式；不要勾选日志即可 |
| 文件窗口按钮是英文 | 系统原生“打开/取消”跟随操作系统显示语言：中文 Windows/macOS 显示中文；英文系统仍显示英文。应用内标题和筛选器已中文化，Mac 已补中文本地化。不要为此改系统语言 |
| 杀软误报 | 未签名程序常见；校验 SHA256 后加入信任，或向用户说明来源 |

## 6. 从源码构建（仅当没有 Release 且用户明确要求）

前置：Node.js 20+、Rust stable、系统构建工具。

```bash
git clone https://github.com/myleemartin/luminar.git
cd luminar
npm ci
./scripts/fetch_pdfium.sh        # Windows: .\scripts\fetch_pdfium.ps1
npm run tauri build
```

模型已经随仓库提交，不需要 `fetch_ocr_models`。构建产物在 `src-tauri/target/release/bundle/`。
普通律师用户不要走源码构建；优先 Release 包。

## 7. 安全与隐私

- 软件不联网、不上传；不要把它说成“云端脱敏”。
- 不要在对话、Issue 或日志里粘贴真实案卷内容、身份证号、手机号。
- 只从本项目 GitHub Releases 下载；不要从网盘/第三方转存。
- 运行前核对 SHA256。
- 脱敏结果仍需律师人工复核，不能承诺 100% 召回。

## 8. 完成标准（AI 自检）

- [ ] 已判断系统、架构、Windows WebView2 状态；
- [ ] 下载了匹配的包并校验 SHA256；
- [ ] 在对话框里逐步告诉用户“双击哪个文件、桌面出现什么、再点哪个”；
- [ ] 用户确认已看到三栏主界面；
- [ ] 已说明导入、复核、导出、桌面默认目录和日志；
- [ ] 没有要求用户安装 Python/PaddleOCR/Node/Rust；
- [ ] 没有上传或索要真实案卷内容。
