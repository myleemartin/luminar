#!/usr/bin/env bash
# 下载 pdfium 动态库到 src-tauri/resources/，用于 PDF 渲染与脱敏 PDF 导出。
#
# 说明：pdfium 只在「构建/安装」阶段下载，运行时完全离线。
#  - macOS：同时下载 arm64 + x86_64 wheel，再用 lipo 合并成 Universal dylib，
#    保证 Universal App 在 Intel / Apple Silicon 上都能用；
#  - Linux：下载对应架构 .so；
#  - Windows 请运行 scripts/fetch_pdfium.ps1。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/src-tauri/resources"
mkdir -p "$DEST"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
# 不要用最新版：5.x 的 macOS dylib 最低要求 macOS 13。
# 4.30.1 的 x64 最低 10.13、arm64 最低 11，兼容 Tauri 的 10.15 部署目标。
PYPDFIUM_VERSION="${PYPDFIUM_VERSION:-4.30.1}"

pick_url() {
  python3 - "$1" "$PYPDFIUM_VERSION" <<'PY'
import json, re, sys, urllib.request
want, version = sys.argv[1], sys.argv[2]
data = json.load(urllib.request.urlopen(f"https://pypi.org/pypi/pypdfium2/{version}/json", timeout=30))
prefs = {
  "mac-arm64": r"py3-none-macosx_\d+_\d+_arm64\.whl$",
  "mac-x64": r"py3-none-macosx_\d+_\d+_x86_64\.whl$",
  "linux-arm64": r"py3-none-manylinux\d*_\d*_aarch64\.whl$",
  "linux-x64": r"py3-none-manylinux\d*_\d*_x86_64\.whl$",
}
pat = re.compile(prefs[want])
def ver(filename):
    m = re.match(r"pypdfium2-([0-9]+(?:\.[0-9]+)*)-", filename)
    return tuple(int(x) for x in m.group(1).split(".")) if m else ()
cand = [u for u in data["urls"] if pat.search(u["filename"])]
if not cand:
    sys.exit(f"没有找到匹配的 pypdfium2 wheel：{want}")
cand.sort(key=lambda u: ver(u["filename"]))
print(cand[-1]["url"])
print(cand[-1]["filename"], file=sys.stderr)
PY
}

fetch_wheel() {
  local url="$1" dir="$2"
  mkdir -p "$dir"
  echo "==> 下载：$url" >&2
  curl -fL --retry 3 --connect-timeout 20 -o "$dir/pypdfium2.whl" "$url" >&2
  (cd "$dir" && unzip -qo pypdfium2.whl)
}

case "$(uname -s)" in
  Darwin)
    echo "==> pypdfium2 $PYPDFIUM_VERSION（最低 macOS 10.13/11）"
    fetch_wheel "$(pick_url mac-arm64)" "$TMP/arm64"
    fetch_wheel "$(pick_url mac-x64)" "$TMP/x64"
    ARM_DYLIB="$(find "$TMP/arm64" -name libpdfium.dylib | head -1)"
    X64_DYLIB="$(find "$TMP/x64" -name libpdfium.dylib | head -1)"
    if [ -z "$ARM_DYLIB" ] || [ -z "$X64_DYLIB" ]; then
      echo "wheel 中缺少 libpdfium.dylib" >&2
      exit 1
    fi
    lipo -create "$ARM_DYLIB" "$X64_DYLIB" -output "$DEST/libpdfium.dylib"
    echo "==> 完成：Universal pdfium"
    lipo -info "$DEST/libpdfium.dylib"
    ;;
  Linux)
    if [ "$(uname -m)" = "aarch64" ] || [ "$(uname -m)" = "arm64" ]; then
      fetch_wheel "$(pick_url linux-arm64)" "$TMP/linux"
    else
      fetch_wheel "$(pick_url linux-x64)" "$TMP/linux"
    fi
    SO="$(find "$TMP/linux" -name libpdfium.so | head -1)"
    if [ -z "$SO" ]; then echo "wheel 中缺少 libpdfium.so" >&2; exit 1; fi
    cp "$SO" "$DEST/libpdfium.so"
    echo "==> 完成：$(ls -lh "$DEST/libpdfium.so" | awk '{print $5}')"
    ;;
  *)
    echo "此脚本仅支持 macOS/Linux；Windows 请运行 scripts/fetch_pdfium.ps1" >&2
    exit 1
    ;;
esac
echo "    可用 TUOMINXING_PDFIUM 环境变量覆盖路径。"
