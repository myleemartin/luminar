# 参与贡献

感谢你愿意一起把「星星脱敏」做得更好。这个项目面向一线律师，**准确性、可解释性和数据安全**优先于功能数量。

## 可以贡献什么

- 新的法律行业识别规则（仲裁案号、执行案号、公证编号、执业证号等）
- 识别/脱敏的误报、漏报案例（请用虚构数据）
- Windows / macOS 的兼容性问题与打包改进
- 文档、教程、规则示例
- 性能优化（大文档、批量导入、OCR 速度）

## 开发环境

```bash
npm install
./scripts/fetch_pdfium.sh          # Windows: pwsh scripts/fetch_pdfium.ps1
./scripts/fetch_ocr_models.sh      # 可选，扫描件 OCR
npm run app:dev
```

依赖版本与命令见 [README](README.md)；分层结构见 [docs/架构设计.md](docs/架构设计.md)。

## 代码规范

- Rust：`cargo fmt` + `cargo clippy --all-targets`，注释用中文；新增模块请在 `lib.rs` 注册并在架构文档补一句说明。
- TypeScript：`npx tsc --noEmit` 必须通过；不要引入运行时网络请求。
- 提交前至少运行：

```bash
cd src-tauri && cargo test
npm run build
npm run test:ui          # 改前端时
```

## 提交信息

建议使用 Conventional Commits（中文描述即可）：

```text
fix(import): 修复配置锁重入导致导入卡死
feat(rule): 新增仲裁案号规则
docs(readme): 补充 Windows 打包说明
```

## 新增一条内置规则

1. 在 `src-tauri/src/detect/builtin_rules.rs` 增加 `Rule`（参考现有条目）；
2. 补一个单元测试：正例 + 反例（`src-tauri/src/detect/mod.rs` 的 tests）；
3. 在 `docs/规则编写教程.md` 的内置分类表补一行；
4. 确认不会与已有规则大面积重叠（必要时调 `priority`）。

## 数据与隐私

- **禁止**在 Issue、PR、测试样例中提交真实案卷、真实身份证号、真实手机号；
- 测试样例一律使用虚构信息，并在文件名或 README 中注明；
- 日志、截图请先脱敏。

## 许可证与贡献授权

本项目采用 [PolyForm Noncommercial 1.0.0](LICENSE)（源码公开、禁止商业使用）。
提交 PR 即表示你同意：

1. 你的贡献按本项目当前许可证分发；
2. 你授予项目维护者一项**非独占、永久、全球范围、免费、可再许可（包括商业许可）**的权利，
   以便未来进行商业授权或调整许可证；
3. 你确认贡献为原创，或已获得第三方素材的必要授权并在 PR 中注明来源与许可证。

如果你不同意上述条款，请不要提交 PR，欢迎仅通过 Issue 提出建议。

## 行为准则

参与讨论即表示同意遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。

## 安全问题

请勿在公开 Issue 中披露安全漏洞，按 [SECURITY.md](SECURITY.md) 私下报告。
