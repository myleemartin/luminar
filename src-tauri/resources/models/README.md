# 内置模型（OCR + 增强脱敏）

把 `det.onnx`、`rec.onnx`、`keys.txt` 放在本目录，打包时会一并放入安装包，
用户安装后无需再手动下载模型（程序会优先使用用户配置目录，其次使用这里）。

- macOS：`Contents/Resources/resources/models/`
- Windows：`<安装目录>\resources\models\`

构建完整安装包时，可先执行：

```bash
./scripts/fetch_ocr_models.sh src-tauri/resources/models
```

模型文件随仓库提交（约 13 MB），打包时会一并带入安装包；
如需更新或切换 PP-OCRv4，可执行上面的命令（已存在时会跳过）。

## 子目录

- `ner/`：可选的本地增强识别模型（中文 NER，PER/ORG/LOC），约 12 MB。
  安装包内置；「增强脱敏」开关打开后才加载，来源与许可见 `ner/README.md`。
