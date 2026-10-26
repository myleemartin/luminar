<#
.SYNOPSIS
  下载 pdfium.dll 到 src-tauri/resources/（Windows）。
.DESCRIPTION
  从 PyPI 的 pypdfium2 官方 wheel 中提取 pdfium.dll，避免依赖 GitHub 直连。
  只在构建/安装阶段联网，程序运行时不联网。
#>
$ErrorActionPreference = "Stop"

$dest = Join-Path $PSScriptRoot "..\src-tauri\resources"
$dest = [System.IO.Path]::GetFullPath($dest)
New-Item -ItemType Directory -Force -Path $dest | Out-Null

Write-Host "==> 目标目录：$dest"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("pdfium_" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
    Write-Host "==> 查询 pypdfium2 最新版本…"
    $meta = Invoke-RestMethod -Uri "https://pypi.org/pypi/pypdfium2/json" -TimeoutSec 30
    $wheel = $meta.urls | Where-Object { $_.filename -like "*win_amd64.whl" } | Select-Object -First 1
    if (-not $wheel) { throw "未找到 Windows x64 的 pypdfium2 wheel" }

    $zip = Join-Path $tmp "pypdfium2.zip"
    Write-Host "==> 下载：$($wheel.url)"
    Invoke-WebRequest -Uri $wheel.url -OutFile $zip -TimeoutSec 120

    $unzip = Join-Path $tmp "extract"
    Expand-Archive -Path $zip -DestinationPath $unzip -Force

    $dll = Get-ChildItem -Path $unzip -Recurse -Filter "pdfium.dll" | Select-Object -First 1
    if (-not $dll) { throw "wheel 中没有 pdfium.dll" }
    Copy-Item $dll.FullName (Join-Path $dest "pdfium.dll") -Force
    Write-Host "==> 完成：$dest\pdfium.dll"
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
