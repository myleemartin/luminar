<#
.SYNOPSIS
  在 Windows 构建机/CI 上生成标准便携包与完整离线包。
.DESCRIPTION
  前置：已运行 npm run tauri build，且 src-tauri/resources 下已有 pdfium.dll 与 models/。
  产物：
    release\Luminar_<版本>_windows_x64_portable.zip
    release\Luminar_<版本>_windows_x64_full_offline.zip
#>
param(
  [string]$Version = "0.11.0",
  [string]$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
)
$ErrorActionPreference = "Stop"

function Write-Utf8Bom([string]$Path, [string]$Text) {
  [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($true))
}

$exe = Get-ChildItem (Join-Path $Root "src-tauri\target\release\*.exe") -File |
  Where-Object { $_.Name -notmatch "setup|uninstall" } |
  Select-Object -First 1
if (-not $exe) { throw "找不到 src-tauri\target\release\*.exe，请先运行 npm run tauri build" }

$docDir = Join-Path $Root "docs"
$readme = Join-Path $Root "README.md"
$resourceDir = Join-Path $Root "src-tauri\resources"
$releaseDir = Join-Path $Root "release"
New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null

function New-Stage([string]$Name) {
  $stage = Join-Path $releaseDir "$Name\Luminar"
  Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force -Path (Join-Path $stage "resources\models") | Out-Null
  Copy-Item $exe.FullName (Join-Path $stage "Luminar.exe")
  Copy-Item (Join-Path $resourceDir "pdfium.dll") (Join-Path $stage "resources\pdfium.dll")
  Copy-Item (Join-Path $resourceDir "models\*") (Join-Path $stage "resources\models") -Recurse
  Copy-Item -Path @($readme, (Join-Path $Root "LICENSE"), (Join-Path $Root "THIRD_PARTY_NOTICES.md")) -Destination $stage
  return $stage
}

function Add-Docs([string]$Stage, [bool]$Full) {
  $windowsDoc = Get-Content (Join-Path $docDir "Windows-打开说明.md") -Raw -Encoding UTF8
  if ($Full) {
    $windowsDoc = "# 星星脱敏（Luminar）Windows 完整离线版说明`r`n`r`n" +
      "> 这个版本已经内置 WebView2 运行时，**不需要目标电脑安装 WebView2**，也不需要联网。`r`n" +
      "> 解压后直接双击「Luminar.exe」即可。首次启动会花几秒到几十秒设置内置 WebView2 权限，请耐心等待。`r`n`r`n" +
      ($windowsDoc -replace "^# 星星脱敏 Windows 版打开说明\r?\n", "")
  }
  Write-Utf8Bom (Join-Path $Stage "打开说明.txt") $windowsDoc
  $short = Get-Content (Join-Path $docDir "点击这个启动程序.md") -Raw -Encoding UTF8
  Write-Utf8Bom (Join-Path $Stage "点击这个启动程序.txt") $short
}

function Compress-Stage([string]$Stage, [string]$ZipName) {
  $zip = Join-Path $releaseDir $ZipName
  Remove-Item $zip -Force -ErrorAction SilentlyContinue
  Compress-Archive -Path $Stage -DestinationPath $zip -CompressionLevel Optimal
  Write-Host "==> $zip"
}

# 标准便携包
$standard = New-Stage "windows"
Add-Docs $standard $false
Compress-Stage $standard "星星脱敏_${Version}_windows_x64_portable.zip"

# 完整离线包（内置 WebView2 固定运行时）
$full = New-Stage "windows_full"
Add-Docs $full $true
$runtime = Join-Path $full "resources\WebView2Runtime"
New-Item -ItemType Directory -Force -Path $runtime | Out-Null
$cabUrl = "https://msedge.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/074c0930-36ad-4756-9d04-dfe0e503bd94/Microsoft.WebView2.FixedVersionRuntime.156.0.4314.42.x64.cab"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("wv2_" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
  $cab = Join-Path $tmp "wv2.cab"
  Write-Host "==> 下载 WebView2 固定运行时…"
  Invoke-WebRequest -Uri $cabUrl -OutFile $cab -TimeoutSec 600
  $extract = Join-Path $tmp "extract"
  New-Item -ItemType Directory -Force -Path $extract | Out-Null
  & expand.exe $cab -F:* $extract | Out-Null
  $inner = Get-ChildItem $extract -Directory | Where-Object { $_.Name -like "Microsoft.WebView2.FixedVersionRuntime.*" } | Select-Object -First 1
  if (-not $inner) { throw "WebView2 cab 解压目录异常" }
  Copy-Item (Join-Path $inner.FullName "*") $runtime -Recurse
  Copy-Item (Join-Path $Root "licenses\WebView2-Runtime-LICENSE.txt") (Join-Path $runtime "LICENSE.txt")
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
Compress-Stage $full "星星脱敏_${Version}_windows_x64_full_offline.zip"
