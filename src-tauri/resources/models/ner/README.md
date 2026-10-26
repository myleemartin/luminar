# 增强识别模型（可选，安装包已内置）

- 模型：`ner.onnx`（INT8 动态量化，约 12 MB）
- 词表：`vocab.txt`（BERT Chinese 21128 词）
- 任务：中文命名实体识别（PER / ORG / LOC）
- 用途：在正则规则之外，用本地小模型补充识别当事人姓名、单位/律所/法院、住址

## 来源与许可

- 基座：`hfl/minirbt-h288`（6 层、288 维，轻量 BERT）
- 微调模型：`IcyKallen/bert-mini-finetuned-ner-chinese`
  （Apache-2.0，评估集 F1 约 0.87）
- 本项目用 ONNX Runtime 做 INT8 动态量化，并由 `tract` 在本地 CPU 推理；
  不联网、不依赖 Python/PyTorch。

## 实现说明

本目录文件随仓库提交，并内置进安装包。程序「增强识别」开关打开后：
1. 按 BERT 中文分词切分文本；
2. 每 510 个 token 分块推理（块间重叠 48 token）；
3. 解码 BIO 标签，映射为「当事人姓名 / 单位名称 / 律所名称 / 法院名称 / 住址」；
4. 与正则规则结果合并，冲突时规则优先。
