// 前端冒烟测试：用 jsdom 加载真实构建产物，mock Tauri IPC，验证三栏联动。
// 运行：npm run build && node scripts/ui_smoke.mjs
import { readFileSync, readdirSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { JSDOM } from "jsdom";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const html = readFileSync(resolve(root, "dist/index.html"), "utf8");
const dom = new JSDOM(html, { url: "http://localhost/", pretendToBeVisual: true });

// 暴露浏览器全局
const { window } = dom;
globalThis.window = window;
for (const key of [
  "document",
  "HTMLElement",
  "HTMLInputElement",
  "HTMLSelectElement",
  "Element",
  "Node",
  "Event",
  "MouseEvent",
  "KeyboardEvent",
  "CustomEvent",
  "Range",
  "DOMParser",
  "MutationObserver",
  "CSS",
  "getComputedStyle",
]) {
  try {
    Object.defineProperty(globalThis, key, { value: window[key], writable: true, configurable: true });
  } catch {
    /* ignore */
  }
}
globalThis.requestAnimationFrame = (cb) => setTimeout(() => cb(Date.now()), 0);
globalThis.fetch = () => Promise.resolve({ ok: true, text: () => Promise.resolve("") });
window.CSS = window.CSS || {};
window.CSS.escape = (s) => String(s).replace(/[^a-zA-Z0-9_-]/g, (c) => `\\${c}`);
window.HTMLElement.prototype.scrollIntoView = function () {};
window.Element.prototype.scrollIntoView = function () {};
window.Range.prototype.getBoundingClientRect = function () {
  return { left: 20, top: 20, width: 100, height: 20, bottom: 40, right: 120, x: 20, y: 20 };
};

// ---------------- mock 数据 ----------------
const text = [
  "民事起诉状",
  "原告：张三，男，1988年1月1日出生，汉族，住湖北省武汉市江岸区建设大道123号，公民身份号码：420102198801011234，联系电话：13812345678。",
  "被告：李四，男，1985年5月20日出生，汉族，住湖北省武汉市武昌区中南路8号。",
  "（2024）鄂0102民初1234号",
].join("\n");

const cpIndex = (needle, from = 0) => {
  const chars = Array.from(text);
  const target = Array.from(needle);
  for (let i = from; i <= chars.length - target.length; i++) {
    if (target.every((c, j) => chars[i + j] === c)) return i;
  }
  return -1;
};

const strategy = (kind, extra = {}) => ({
  kind,
  mask_char: "*",
  keep_prefix: 0,
  keep_suffix: 0,
  placeholder: "【{type}】",
  ...extra,
});

const mkFinding = (id, needle, category, label, conf, kind, extra = {}) => ({
  id,
  start: cpIndex(needle),
  end: cpIndex(needle) + Array.from(needle).length,
  text: needle,
  category,
  category_label: label,
  confidence: conf,
  source: "regex",
  rule_id: "r1",
  rule_name: "测试规则",
  enabled: true,
  strategy: strategy(kind, extra),
});

let findings = [
  mkFinding("f1", "张三", "person_name", "当事人姓名", 0.86, "placeholder"),
  mkFinding("f2", "420102198801011234", "id_card", "身份证号", 0.96, "mask"),
  mkFinding("f3", "13812345678", "phone", "手机号", 0.94, "mask", { keep_prefix: 3, keep_suffix: 4 }),
  mkFinding("f4", "湖北省武汉市江岸区建设大道123号", "address", "住址", 0.8, "placeholder"),
  mkFinding("f5", "李四", "person_name", "当事人姓名", 0.86, "placeholder"),
];

const summary = {
  id: "doc-1",
  file_name: "民事起诉状.docx",
  file_path: "/fake/民事起诉状.docx",
  kind: "docx",
  kind_label: "Word 文档",
  page_count: 1,
  char_count: Array.from(text).length,
  file_size: 12345,
  needs_ocr: false,
  ocr_used: false,
  finding_count: findings.length,
  enabled_count: findings.length,
  detected: true,
  warnings: [],
};

const payload = () => ({
  id: "doc-1",
  file_name: summary.file_name,
  file_path: summary.file_path,
  kind: "docx",
  kind_label: "Word 文档",
  page_count: 1,
  char_count: summary.char_count,
  file_size: 12345,
  sha256: "abc123",
  pages: [{ page: 1, start: 0, end: Array.from(text).length, label: "第 1 页" }],
  text,
  findings,
  needs_ocr: false,
  ocr_used: false,
  warnings: [],
});

const settings = {
  threshold: 0.75,
  ocr_dpi: 200,
  ocr_engine: "paddle_onnx",
  external_ocr_command: "",
  pdf_export_dpi: 150,
  default_mask_char: "*",
  placeholder_template: "【{type}】",
  propagate_names: true,
  enhance_model: false,
  last_output_dir: "",
};

const categories = [
  { id: "person_name", label: "当事人姓名" },
  { id: "id_card", label: "身份证号" },
  { id: "phone", label: "手机号" },
  { id: "address", label: "住址" },
  { id: "custom", label: "自定义" },
];

const paths = {
  config_dir: "/fake/config",
  rules_file: "/fake/config/rules.json",
  settings_file: "/fake/config/settings.json",
  models_dir: "/fake/config/models",
};

const ocr = {
  engine: "paddle_onnx",
  engine_label: "PaddleOCR 本地模型（PP-OCR）",
  ready: true,
  message: "模型已就位",
  models_dir: "/fake/config/models",
};

const calls = [];
const handlers = {
  get_settings: () => settings,
  save_settings: (a) => a.settings,
  list_categories: () => categories,
  list_rules: () => [],
  app_paths: () => paths,
  ocr_status: () => ocr,
  default_export_dir: () => "/fake/Desktop",
  enhance_status: () => ({
    ready: true,
    message: "本地增强模型已就绪（约 12 MB，CPU 推理）",
    model_dir: "/fake/models/ner",
  }),
  import_documents: () => ({ docs: [summary], errors: [] }),
  get_document: () => payload(),
  update_finding: (a) => {
    findings = findings.map((f) => (f.id === a.finding.id ? a.finding : f));
    return findings;
  },
  set_all_enabled: (a) => {
    findings = findings.map((f) => ({ ...f, enabled: a.enabled }));
    return findings;
  },
  remove_finding: (a) => {
    findings = findings.filter((f) => f.id !== a.findingId);
    return findings;
  },
  redetect_all: () => [summary],
  "plugin:dialog|open": () => ["/fake/民事起诉状.docx"],
  "plugin:event|listen": () => 7,
  "plugin:event|unlisten": () => null,
};

window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
window.__TAURI_INTERNALS__ = {
  metadata: {
    currentWindow: { label: "main" },
    currentWebview: { label: "main", windowLabel: "main" },
  },
  transformCallback: () => 1,
  convertFileSrc: (p) => p,
  invoke: (cmd, args) => {
    calls.push(cmd);
    const h = handlers[cmd];
    if (!h) return Promise.resolve(null);
    try {
      return Promise.resolve(h(args || {}));
    } catch (e) {
      return Promise.reject(e);
    }
  },
};

// ---------------- 加载构建产物 ----------------
const asset = readdirSync(resolve(root, "dist/assets")).find((f) => /^index-.*\.js$/.test(f));
if (!asset) throw new Error("找不到构建产物，请先 npm run build");
await import(pathToFileURL(resolve(root, "dist/assets", asset)).href);

const $ = (sel) => window.document.querySelector(sel);
const $$ = (sel) => Array.from(window.document.querySelectorAll(sel));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const assert = (cond, msg) => {
  if (!cond) throw new Error("断言失败：" + msg);
  console.log("  ✓ " + msg);
};

await sleep(150);
console.log("初始化：");
assert($("#threshold-value").textContent === "0.75", "阈值显示为 0.75");
assert($("#ocr-chip").textContent.includes("OCR 就绪"), "OCR 状态已渲染");

console.log("导入流程：");
$("#btn-import").click();
await sleep(400);
assert($$("#file-list .file-item").length === 1, "左栏出现 1 个文件");
assert($("#doc-title").textContent.includes("民事起诉状"), "中间标题显示文件名");
assert(($("#doc-text").textContent || "").includes("原告：张三"), "正文已渲染");
assert($$("#doc-text .hit").length === 5, "正文 5 处高亮");
assert($$("#finding-list .finding-card").length === 5, "右栏 5 条结果");

console.log("取消误报：");
const firstCheck = $('#finding-list [data-action="toggle"]');
firstCheck.click();
await sleep(250);
assert(firstCheck.checked === false, "复选框已取消");
assert($("#enabled-count").textContent.includes("4"), "启用计数变为 4");

console.log("脱敏预览：");
$('#mode-switch [data-mode="redacted"]').click();
await sleep(100);
const redacted = $("#doc-text").textContent;
assert(redacted.includes("【当事人姓名】"), "姓名替换为占位标签");
assert(redacted.includes("138****5678"), "手机号掩码正确");
assert(!redacted.includes("420102198801011234"), "身份证号不再出现");

console.log("对照视图：");
$('#mode-switch [data-mode="compare"]').click();
await sleep(100);
assert($$(".compare-pane").length === 2, "左右两栏");

console.log("分类筛选：");
$('#category-filter [data-cat="phone"]').click();
await sleep(100);
assert($$("#finding-list .finding-card").length === 1, "筛选后仅 1 条");

console.log("弹窗：");
$("#btn-export").click();
await sleep(180);
assert($$(".modal").length === 1 && $(".modal").textContent.includes("导出格式"), "导出弹窗正常");
assert($("#export-dir").value.includes("Desktop"), "导出目录默认桌面");
assert($("#export-native").checked, "默认仅导出原格式");
assert($("#export-format-grid").classList.contains("hidden"), "原格式模式下不显示额外格式");
$(".modal [data-close]").click();
await sleep(60);
$("#btn-rules").click();
await sleep(120);
assert($(".modal").textContent.includes("规则管理"), "规则弹窗正常");
$(".modal [data-close]").click();
await sleep(60);
if (!document.getElementById("busy-progress")) throw new Error("缺少 OCR 进度条节点");
if (!document.getElementById("busy-progress-bar")) throw new Error("缺少 OCR 进度条填充");
console.log("  ✓ OCR 进度条节点已就绪");

const footer = document.querySelector("footer");
if (!footer) throw new Error("缺少意见反馈页脚");
const footerText = footer.textContent || "";
for (const expected of ["意见反馈", "李晨星律师", "湖北观筑律师事务所", "江发路12号", "lichenxing.cn", "sinoyouth@qq.com"]) {
  if (!footerText.includes(expected)) throw new Error(`页脚缺少：${expected}`);
}
if (footer.querySelectorAll("[data-copy]").length !== 2) throw new Error("页脚复制按钮数量不对");
console.log("  ✓ 意见反馈联系方式完整");

const enhanceToggle = $("#enhance-toggle");
if (!enhanceToggle) throw new Error("缺少增强识别开关");
if (enhanceToggle.disabled) throw new Error("增强识别开关不应被禁用（mock 模型已就绪）");
console.log("  ✓ 增强脱敏开关已就绪");

$("#btn-settings").click();
await sleep(120);
assert($(".modal").textContent.includes("OCR"), "设置弹窗正常");
$(".modal [data-close]").click();
await sleep(60);

console.log("手动标记提示：");
window.getSelection().removeAllRanges();
$("#btn-mark").click();
await sleep(100);
assert($$("#toast-root > div").length >= 1, "未选中时给出提示");

console.log("\n全部前端冒烟测试通过。");
process.exit(0);
