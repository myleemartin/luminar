#!/usr/bin/env python3
"""开发期脚本：把中文 NER 模型导出为固定输入 1×512 的 ONNX，并做 INT8 动态量化。

上游模型（Apache-2.0）：
  - hfl/minirbt-h288
  - IcyKallen/bert-mini-finetuned-ner-chinese

运行需要（仅开发机，不随软件分发）：
  pip install torch transformers onnx onnxruntime

用法：
  python scripts/export_ner_model.py
输出：
  src-tauri/resources/models/ner/ner.onnx
  src-tauri/resources/models/ner/vocab.txt
"""
from pathlib import Path

import torch
from transformers import AutoModelForTokenClassification, AutoTokenizer
from onnxruntime.quantization import QuantType, quantize_dynamic

MODEL_ID = "IcyKallen/bert-mini-finetuned-ner-chinese"
OUT = Path(__file__).resolve().parent.parent / "src-tauri/resources/models/ner"
TMP = OUT / "_float.onnx"


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    model = AutoModelForTokenClassification.from_pretrained(MODEL_ID)
    tokenizer = AutoTokenizer.from_pretrained(MODEL_ID)
    model.eval()

    ids = torch.ones((1, 512), dtype=torch.long)
    mask = torch.ones((1, 512), dtype=torch.long)
    token_type = torch.zeros((1, 512), dtype=torch.long)
    with torch.no_grad():
        model(input_ids=ids, attention_mask=mask, token_type_ids=token_type)

    torch.onnx.export(
        model,
        (ids, mask, token_type),
        TMP,
        input_names=["input_ids", "attention_mask", "token_type_ids"],
        output_names=["logits"],
        opset_version=14,
        do_constant_folding=True,
    )
    quantize_dynamic(TMP, OUT / "ner.onnx", weight_type=QuantType.QInt8)
    TMP.unlink(missing_ok=True)

    # transformers 的 tokenizer.json 是权威词表；导出为 Rust 侧逐行读取的 vocab.txt
    tok = tokenizer.backend_tokenizer
    items = tok.get_vocab()
    tokens = [None] * len(items)
    for token, idx in items.items():
        tokens[idx] = token
    (OUT / "vocab.txt").write_text("\n".join(tokens) + "\n", encoding="utf-8")

    print("已生成：", OUT / "ner.onnx")
    print("已生成：", OUT / "vocab.txt")


if __name__ == "__main__":
    main()
