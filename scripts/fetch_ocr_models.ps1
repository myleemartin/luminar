<#
.SYNOPSIS
  下载 PP-OCR 检测/识别模型到本地模型目录（Windows）。
.DESCRIPTION
  默认 PP-OCRv3；可用环境变量 OCR_VERSION=v4 切换。
  只在首次安装时联网，程序运行时完全离线。
.PARAMETER Dest
  目标目录。省略时安装到 %APPDATA%\cn.luminar.app\models。
#>
param(
    [string]$Dest = ""
)

$ErrorActionPreference = "Stop"

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
if ([string]::IsNullOrWhiteSpace($Dest)) {
    $Dest = Join-Path $env:APPDATA "cn.luminar.app\models"
}
New-Item -ItemType Directory -Force -Path $Dest | Out-Null

$version = if ($env:OCR_VERSION) { $env:OCR_VERSION } else { "v3" }
switch ($version) {
    { $_ -in @("v3", "3", "PP-OCRv3") } { $dir = "PP-OCRv3"; $tag = "ch_PP-OCRv3"; break }
    { $_ -in @("v4", "4", "PP-OCRv4") } { $dir = "PP-OCRv4"; $tag = "ch_PP-OCRv4"; break }
    default { throw "OCR_VERSION 只支持 v3 / v4" }
}

$mirror = if ($env:HF_ENDPOINT) { $env:HF_ENDPOINT } else { "https://hf-mirror.com" }
$base = "$mirror/SWHL/RapidOCR/resolve/main/$dir"

Write-Host "==> 模型目录：$Dest"

function Fetch-File([string]$Url, [string]$OutFile) {
    if (Test-Path $OutFile) {
        Write-Host "    已存在，跳过：$(Split-Path $OutFile -Leaf)"
        return
    }
    Write-Host "    下载 $(Split-Path $OutFile -Leaf) …"
    Invoke-WebRequest -Uri $Url -OutFile "$OutFile.part" -UseBasicParsing -TimeoutSec 300
    Move-Item "$OutFile.part" $OutFile -Force
}

Fetch-File "$base/${tag}_det_infer.onnx" (Join-Path $Dest "det.onnx")
Fetch-File "$base/${tag}_rec_infer.onnx" (Join-Path $Dest "rec.onnx")
Copy-Item (Join-Path $root "assets\ppocr_keys_v1.txt") (Join-Path $Dest "keys.txt") -Force

Write-Host "==> 完成："
Get-ChildItem $Dest | Format-Table Name, Length
Write-Host "启动应用后，在「设置 → OCR」中确认状态为「模型已就位」。"
