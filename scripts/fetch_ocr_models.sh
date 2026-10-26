#!/usr/bin/env bash
# 下载 PaddleOCR（PP-OCR）检测/识别模型到本地模型目录。
# 默认 PP-OCRv3（体积小、速度快，适配本项目的固定输入预处理）；
# 可用 OCR_VERSION=v4 切换到 PP-OCRv4。
# 只在「首次安装」时联网，程序运行时完全离线。
#
# 用法：
#   ./scripts/fetch_ocr_models.sh                 # 安装到系统应用配置目录
#   ./scripts/fetch_ocr_models.sh ./models        # 安装到项目内 models/（配合 TUOMINXING_MODELS 使用）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "${1:-}" != "" ]]; then
  DEST="$1"
else
  case "$(uname -s)" in
    Darwin) DEST="$HOME/Library/Application Support/cn.luminar.app/models" ;;
    Linux)  DEST="${XDG_CONFIG_HOME:-$HOME/.config}/cn.luminar.app/models" ;;
    MINGW*|MSYS*|CYGWIN*) DEST="${APPDATA:-$HOME/AppData/Roaming}/cn.luminar.app/models" ;;
    *) DEST="$ROOT/models" ;;
  esac
fi

MIRROR="${HF_ENDPOINT:-https://hf-mirror.com}"
VERSION="${OCR_VERSION:-v3}"
case "$VERSION" in
  v3|3|PP-OCRv3) DIR="PP-OCRv3"; TAG="ch_PP-OCRv3" ;;
  v4|4|PP-OCRv4) DIR="PP-OCRv4"; TAG="ch_PP-OCRv4" ;;
  *) echo "OCR_VERSION 只支持 v3 / v4"; exit 1 ;;
esac
BASE="$MIRROR/SWHL/RapidOCR/resolve/main/$DIR"

mkdir -p "$DEST"

echo "==> 模型目录：$DEST"

fetch() {
  local url="$1" out="$2"
  if [[ -f "$out" ]]; then
    echo "    已存在，跳过：$(basename "$out")"
    return 0
  fi
  echo "    下载 $(basename "$out") …"
  curl -fL --retry 3 --connect-timeout 20 -o "$out.part" "$url"
  mv "$out.part" "$out"
}

fetch "$BASE/${TAG}_det_infer.onnx" "$DEST/det.onnx"
fetch "$BASE/${TAG}_rec_infer.onnx" "$DEST/rec.onnx"

# 字典随仓库分发（PaddleOCR Apache-2.0），无需联网
cp "$ROOT/assets/ppocr_keys_v1.txt" "$DEST/keys.txt"

echo "==> 完成："
ls -lh "$DEST"
echo
echo "启动应用后，在「设置 → OCR」中确认状态为「模型已就位」。"
