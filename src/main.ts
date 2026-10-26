// 应用主逻辑：状态、事件、三栏联动、框选补标
import "./styles.css";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import { api } from "./api";
import {
  focusFinding,
  invalidateCache,
  pageIndexForOffset,
  renderPreview,
  selectionOffsets,
  selectionRect,
} from "./components/preview";
import {
  formatBytes,
  formatNumber,
  renderCategoryFilter,
  renderFileList,
  renderFindingList,
} from "./components/panels";
import { listen } from "@tauri-apps/api/event";
import { openExportModal, openRulesModal, openSettingsModal } from "./components/modals";
import type {
  AppPaths,
  CategoryInfo,
  DocPayload,
  DocSummary,
  EnhanceStatus,
  Finding,
  OcrProgress,
  OcrStatus,
  Rule,
  Settings,
  StrategyConfig,
  ViewMode,
} from "./types";
import {
  confirmDialog,
  copyText,
  setBusy,
  setBusyProgress,
  toast,
  updateBusyText,
} from "./ui";

interface AppState {
  docs: DocSummary[];
  currentId: string | null;
  doc: DocPayload | null;
  findings: Finding[];
  rules: Rule[];
  categories: CategoryInfo[];
  settings: Settings | null;
  ocr: OcrStatus | null;
  enhance: EnhanceStatus | null;
  paths: AppPaths | null;
  selected: Set<string>;
  mode: ViewMode;
  pageIndex: number;
  activeId: string | null;
  editingId: string | null;
  filter: string;
  selection: { start: number; end: number; text: string } | null;
}

const state: AppState = {
  docs: [],
  currentId: null,
  doc: null,
  findings: [],
  rules: [],
  categories: [],
  settings: null,
  ocr: null,
  enhance: null,
  paths: null,
  selected: new Set(),
  mode: "original",
  pageIndex: 0,
  activeId: null,
  editingId: null,
  filter: "all",
  selection: null,
};

const $ = <T extends HTMLElement = HTMLElement>(sel: string): T =>
  document.querySelector(sel) as T;
const $$ = <T extends HTMLElement = HTMLElement>(sel: string): NodeListOf<T> =>
  document.querySelectorAll(sel);

// ---------------------------------------------------------------------------
// 初始化
// ---------------------------------------------------------------------------
async function init(): Promise<void> {
  bindStaticEvents();
  // OCR 页级进度：后端在逐页识别时推送，前端显示“第 n / N 页”
  void listen<OcrProgress>("ocr-progress", (event) => onOcrProgress(event.payload)).catch(() => {});
  try {
    const [settings, categories, rules, paths, ocr, enhance] = await Promise.all([
      api.getSettings(),
      api.listCategories(),
      api.listRules(),
      api.appPaths(),
      api.ocrStatus(),
      api.enhanceStatus(),
    ]);
    state.settings = settings;
    state.categories = categories;
    state.rules = rules;
    state.paths = paths;
    state.ocr = ocr;
    state.enhance = enhance;

    const slider = $<HTMLInputElement>("#threshold");
    slider.value = String(settings.threshold);
    $("#threshold-value").textContent = settings.threshold.toFixed(2);
    renderOcrChip();
    renderEnhanceToggle();
    renderAll();
  } catch (e) {
    toast(`初始化失败：${String(e)}`, "err");
  }

  // 拖拽导入（Tauri 提供真实本地路径）
  try {
    await getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "drop" && event.payload.paths.length > 0) {
        void importPaths(event.payload.paths);
      }
    });
  } catch {
    /* 浏览器调试环境忽略 */
  }
}

function onOcrProgress(p: OcrProgress): void {
  if (!p || !p.total) return;
  const file = p.file_name || "扫描件";
  if (p.phase === "loading") {
    setBusyProgress(0, p.total, `${file}：正在加载 OCR 模型…`);
  } else if (p.phase === "page") {
    setBusyProgress(p.current, p.total, `${file}：已识别 ${p.current} / ${p.total} 页`);
  } else if (p.phase === "done") {
    setBusyProgress(p.total, p.total, `${file}：OCR 完成（${p.total} 页）`);
  } else if (p.phase === "error") {
    setBusyProgress(0, p.total, `${file}：OCR 不可用，已跳过`);
  }
}

function renderEnhanceToggle(): void {
  const el = $<HTMLInputElement>("#enhance-toggle");
  if (!state.settings || !state.enhance) {
    el.disabled = true;
    return;
  }
  el.checked = state.settings.enhance_model;
  // 模型缺失时禁止“开启”，但允许把已开启的开关关掉
  el.disabled = !state.enhance.ready && !state.settings.enhance_model;
  const label = el.closest("label");
  if (label) {
    label.title = `${state.enhance.message}（${state.enhance.model_dir}）`;
    label.classList.toggle("opacity-50", !state.enhance.ready);
  }
}

function renderOcrChip(): void {
  const el = $("#ocr-chip");
  if (!state.ocr) {
    el.textContent = "OCR 未配置";
    return;
  }
  el.textContent = state.ocr.ready
    ? `OCR 就绪 · ${state.ocr.engine_label.replace(/（.*?）/g, "")}`
    : "OCR 未就绪 · 点此配置";
  el.title = state.ocr.message;
  el.className = `cursor-pointer rounded-full px-2.5 py-1 text-[11px] ${
    state.ocr.ready ? "bg-emerald-50 text-emerald-700" : "bg-amber-50 text-amber-700"
  }`;
}

// ---------------------------------------------------------------------------
// 渲染
// ---------------------------------------------------------------------------
function renderAll(): void {
  renderFiles();
  renderFindings();
  renderPreviewPane(false);
  renderStats();
}

function renderFiles(): void {
  const list = $("#file-list");
  const keepScroll = list.scrollTop;
  renderFileList(list, $("#file-empty"), state.docs, state.currentId, state.selected);
  list.scrollTop = keepScroll;
  $("#doc-count").textContent = `${state.docs.length} 份`;
}

function renderFindings(): void {
  const list = $("#finding-list");
  const keepScroll = list.scrollTop;
  const findingCount = state.findings.length;
  const enabled = state.findings.filter((f) => f.enabled).length;
  $("#finding-count").textContent = String(findingCount);
  $("#enabled-count").textContent = `已启用 ${enabled} 条`;
  $("#finding-empty").classList.toggle("hidden", findingCount > 0);
  renderCategoryFilter($("#category-filter"), state.findings, state.filter);
  renderFindingList(
    list,
    state.findings,
    state.categories,
    state.activeId,
    state.editingId,
    state.filter,
  );
  list.scrollTop = keepScroll;
  const allOn = findingCount > 0 && enabled === findingCount;
  $<HTMLInputElement>("#chk-all").checked = allOn;
  $<HTMLButtonElement>("#btn-export").disabled = !state.doc;
}

function renderPreviewPane(resetScroll = false): void {
  const doc = state.doc;
  const root = $("#doc-text");
  $("#preview-empty").classList.toggle("hidden", !!doc);
  $("#btn-mark").toggleAttribute("disabled", !doc);
  if (!doc) {
    root.innerHTML = "";
    return;
  }
  renderPreview(root, {
    doc,
    findings: state.findings,
    pageIndex: state.pageIndex,
    mode: state.mode,
    activeId: state.activeId,
    resetScroll,
  });
  const warnBar = $("#doc-warning");
  if (doc.warnings.length > 0) {
    warnBar.textContent = `⚠ ${doc.warnings.slice(0, 3).join("；")}${
      doc.warnings.length > 3 ? `（另有 ${doc.warnings.length - 3} 条提示）` : ""
    }`;
    warnBar.classList.remove("hidden");
    warnBar.title = doc.warnings.join("\n");
  } else {
    warnBar.classList.add("hidden");
    warnBar.textContent = "";
  }
  $("#page-indicator").textContent = `${state.pageIndex + 1} / ${doc.page_count}`;
  $("#doc-title").textContent = doc.file_name;
  $("#doc-title").title = doc.file_path;
  $$("#mode-switch [data-mode]").forEach((b) => {
    b.classList.toggle("active", b.dataset.mode === state.mode);
  });
}

function renderStats(): void {
  const doc = state.doc;
  if (!doc) {
    $("#stat-chars").textContent = "—";
    $("#stat-findings").textContent = "—";
    $("#stat-total").textContent = "—";
    $("#stat-filemeta").textContent = "";
    return;
  }
  const enabled = state.findings.filter((f) => f.enabled).length;
  $("#stat-chars").textContent = `共 ${formatNumber(doc.char_count)} 字`;
  $("#stat-findings").textContent = `识别 ${state.findings.length} 条 · 生效 ${enabled} 条`;
  if (doc.warnings.length) {
    $("#stat-total").textContent = `⚠ ${doc.warnings.length} 条提示`;
    $("#stat-total").title = doc.warnings.join("\n");
    $("#stat-total").className = "cursor-help text-amber-600";
  } else {
    $("#stat-total").textContent = doc.ocr_used ? "已 OCR" : "文本层";
    $("#stat-total").className = "";
    $("#stat-total").removeAttribute("title");
  }
  $("#stat-filemeta").textContent = `${doc.kind_label} · ${doc.page_count} 页 · ${formatBytes(
    doc.file_size,
  )} · SHA256 ${doc.sha256.slice(0, 12)}…`;
}

// ---------------------------------------------------------------------------
// 文档操作
// ---------------------------------------------------------------------------
async function importFiles(): Promise<void> {
  const picked = await openDialog({
    multiple: true,
    title: "选择要脱敏的文书",
    filters: [
        {
          name: "文书文件",
          extensions: [
            "docx",
            "docm",
            "doc",
            "wps",
            "pdf",
            "rtf",
            "ofd",
            "html",
            "htm",
            "xhtml",
            "txt",
            "md",
          ],
        },
      ],
  });
  if (!picked) return;
  const paths = Array.isArray(picked) ? picked : [picked];
  await importPaths(paths);
}

async function importPaths(paths: string[]): Promise<void> {
  if (paths.length === 0) return;
  const hasPdf = paths.some((p) => /\.pdf$/i.test(p));
  setBusy(
    hasPdf
      ? `正在解析 ${paths.length} 份文书（扫描件会自动 OCR，可能需要数秒/页）…`
      : `正在解析 ${paths.length} 份文书…`,
  );
  const slowHint = window.setTimeout(
    () => updateBusyText("仍在处理…大文件或扫描件 OCR 需要更长时间，请勿关闭窗口"),
    12000,
  );
  try {
    const out = await api.importDocuments(paths);
    for (const d of out.docs) {
      const idx = state.docs.findIndex((x) => x.id === d.id);
      if (idx >= 0) state.docs[idx] = d;
      else state.docs.push(d);
    }
    out.errors.forEach((e) => toast(e, "err"));
    if (out.docs.length === 0) {
      toast("没有成功导入的文书", "warn");
      return;
    }
    // 清理已被替换/移除的选中项，避免批量导出时引用不存在的文档
    for (const id of Array.from(state.selected)) {
      if (!state.docs.some((d) => d.id === id)) state.selected.delete(id);
    }
    const warnings = out.docs.flatMap((d) => d.warnings.map((w) => `${d.file_name}：${w}`));
    if (warnings.length) toast(warnings[0], "warn");
    if (!state.currentId || out.docs.length > 0) {
      await selectDoc(out.docs[out.docs.length - 1].id);
    }
    toast(`已导入 ${out.docs.length} 份文书`);
  } catch (e) {
    toast(`导入失败：${String(e)}`, "err");
  } finally {
    window.clearTimeout(slowHint);
    setBusy(null);
  }
}

let selectSeq = 0;

async function selectDoc(id: string): Promise<void> {
  const seq = ++selectSeq;
  setBusy("正在载入文书…");
  try {
    const doc = await api.getDocument(id);
    if (seq !== selectSeq) return; // 用户已切换到别的文档，丢弃旧结果
    state.currentId = id;
    state.doc = doc;
    state.findings = doc.findings;
    state.pageIndex = 0;
    state.activeId = null;
    state.editingId = null;
    state.filter = "all";
    state.selection = null;
    invalidateCache();
    hidePopover();
    renderAll();
    $("#preview-scroll").scrollTop = 0;
  } catch (e) {
    toast(`载入失败：${String(e)}`, "err");
  } finally {
    setBusy(null);
  }
}

function applyFindings(list: Finding[]): void {
  state.findings = list;
  if (state.doc) state.doc.findings = list;
  const summary = state.docs.find((d) => d.id === state.currentId);
  if (summary) {
    summary.finding_count = list.length;
    summary.enabled_count = list.filter((f) => f.enabled).length;
    summary.detected = true;
  }
  renderFiles();
  renderFindings();
  renderPreviewPane();
  renderStats();
}

async function redetect(): Promise<void> {
  if (!state.currentId) return;
  setBusy("正在重新识别…");
  try {
    const list = await api.detectDocument(state.currentId);
    applyFindings(list);
    toast(`识别完成，共 ${list.length} 条`);
  } catch (e) {
    toast(`识别失败：${String(e)}`, "err");
  } finally {
    setBusy(null);
  }
}

async function redetectAll(showToast = true): Promise<void> {
  if (state.docs.length === 0) {
    if (showToast) toast("请先导入文书", "warn");
    return;
  }
  setBusy("正在批量识别…");
  try {
    const summaries = await api.redetectAll();
    state.docs = state.docs.map((d) => summaries.find((s) => s.id === d.id) ?? d);
    if (state.currentId) await refreshCurrentDoc();
    else renderAll();
    if (showToast) {
      const total = summaries.reduce((n, s) => n + s.finding_count, 0);
      toast(`批量识别完成，共 ${total} 条命中`);
    }
  } catch (e) {
    toast(`批量识别失败：${String(e)}`, "err");
  } finally {
    setBusy(null);
  }
}

async function refreshCurrentDoc(): Promise<void> {
  if (!state.currentId) return;
  try {
    const doc = await api.getDocument(state.currentId);
    state.doc = doc;
    state.findings = doc.findings;
    if (state.pageIndex >= doc.page_count) state.pageIndex = Math.max(0, doc.page_count - 1);
    invalidateCache();
    renderAll();
  } catch (e) {
    toast(`刷新文档失败：${String(e)}`, "err");
  }
}

// ---------------------------------------------------------------------------
// 人工复核
// ---------------------------------------------------------------------------
async function updateFinding(next: Finding): Promise<void> {
  if (!state.currentId) return;
  try {
    const list = await api.updateFinding(state.currentId, next);
    applyFindings(list);
  } catch (e) {
    toast(String(e), "err");
  }
}

async function toggleFinding(id: string, enabled: boolean): Promise<void> {
  const f = state.findings.find((x) => x.id === id);
  if (!f) return;
  await updateFinding({ ...f, enabled });
}

async function deleteFinding(id: string): Promise<void> {
  if (!state.currentId) return;
  if (state.activeId === id) state.activeId = null;
  if (state.editingId === id) state.editingId = null;
  try {
    const list = await api.removeFinding(state.currentId, id);
    applyFindings(list);
    toast("已移除该条标记");
  } catch (e) {
    toast(String(e), "err");
  }
}

function locateFinding(f: Finding): void {
  if (!state.doc) return;
  state.activeId = f.id;
  const idx = pageIndexForOffset(state.doc, f.start);
  state.pageIndex = idx;
  renderPreviewPane(false);
  renderFindings();
  window.setTimeout(() => focusFinding($("#doc-text"), f.id), 60);
}

// ---------------------------------------------------------------------------
// 框选补标
// ---------------------------------------------------------------------------
function showPopover(): void {
  const sel = state.selection;
  const rect = selectionRect();
  if (!sel || !rect) return;
  const pop = $("#selection-popover");
  pop.classList.remove("hidden");
  $("#popover-snippet").textContent = sel.text.slice(0, 24);
  const select = $<HTMLSelectElement>("#popover-category");
  select.replaceChildren(
    ...state.categories.map((c) => {
      const opt = document.createElement("option");
      opt.value = c.id;
      opt.textContent = c.label;
      return opt;
    }),
  );
  const guess = guessCategory(sel.text);
  if (guess) select.value = guess;
  const width = 280;
  const left = Math.min(window.innerWidth - width - 16, Math.max(16, rect.left + rect.width / 2 - width / 2));
  const top = Math.max(16, Math.min(window.innerHeight - 260, rect.bottom + 8));
  pop.style.left = `${left}px`;
  pop.style.top = `${top}px`;
}

function hidePopover(): void {
  $("#selection-popover").classList.add("hidden");
}

function guessCategory(text: string): string | null {
  if (/^\d{17}[\dXx]$/.test(text)) return "id_card";
  if (/^1[3-9]\d{9}$/.test(text)) return "phone";
  if (/^\d{16,19}$/.test(text)) return "bank_card";
  if (/@/.test(text)) return "email";
  if (/[（(]\d{4}[）)]/.test(text)) return "case_number";
  if (/[省市区县].*(路|街|号|室)/.test(text)) return "address";
  if (/律师事务所/.test(text)) return "law_firm";
  if (/^[\u4e00-\u9fa5]{2,4}$/.test(text)) return "person_name";
  return null;
}

function defaultStrategy(category: string): StrategyConfig {
  const base: StrategyConfig = {
    kind: "placeholder",
    mask_char: "*",
    keep_prefix: 0,
    keep_suffix: 0,
    placeholder: "【{type}】",
  };
  if (["id_card", "bank_card", "phone", "landline", "email", "credit_code"].includes(category)) {
    return { ...base, kind: "mask", keep_suffix: category === "phone" ? 4 : 0 };
  }
  return base;
}

async function confirmManualFinding(): Promise<void> {
  const sel = state.selection;
  if (!sel || !state.currentId) return;
  const catId = $<HTMLSelectElement>("#popover-category").value;
  const cat = state.categories.find((c) => c.id === catId);
  const kind = $<HTMLSelectElement>("#popover-strategy").value as StrategyConfig["kind"];
  const strategy = { ...defaultStrategy(catId), kind };
  setBusy("正在添加标记…");
  try {
    const list = await api.addManualFinding({
      docId: state.currentId,
      start: sel.start,
      end: sel.end,
      category: catId,
      categoryLabel: cat?.label ?? "敏感信息",
      strategy,
    });
    applyFindings(list);
    hidePopover();
    window.getSelection()?.removeAllRanges();
    toast(`已标记「${sel.text.slice(0, 12)}」`);
  } catch (e) {
    toast(`标记失败：${String(e)}`, "err");
  } finally {
    setBusy(null);
  }
}

// ---------------------------------------------------------------------------
// 导出
// ---------------------------------------------------------------------------
async function openExport(): Promise<void> {
  if (!state.settings) return;
  if (state.docs.length === 0) {
    toast("请先导入文书", "warn");
    return;
  }
  const ids =
    state.selected.size > 0
      ? Array.from(state.selected)
      : state.currentId
        ? [state.currentId]
        : state.docs.map((d) => d.id);
  // 默认导出到桌面；取不到时退回上次目录，用户仍可手动改
  const defaultDir = await api
    .defaultExportDir()
    .catch(() => state.settings?.last_output_dir ?? "");
  openExportModal({
    docs: state.docs,
    initialIds: ids,
    settings: state.settings,
    defaultDir,
    pickDir: async () => {
      const dir = await openDialog({ directory: true, multiple: false, title: "选择导出目录" });
      return typeof dir === "string" ? dir : null;
    },
    onFinished: () => {
      void refreshCurrentDoc();
      void api.getSettings().then((s) => (state.settings = s));
    },
  });
}

// ---------------------------------------------------------------------------
// 事件绑定
// ---------------------------------------------------------------------------
function bindStaticEvents(): void {
  // 页脚联系方式：点击复制，避免 WebView 直接跳转外链
  $$<HTMLButtonElement>("[data-copy]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const text = btn.dataset.copy ?? "";
      const label = btn.dataset.copyLabel ?? "内容";
      if (!text) return;
      const ok = await copyText(text);
      toast(ok ? `${label}已复制：${text}` : `复制失败，请手动复制：${text}`, ok ? "ok" : "warn");
    });
  });

  $("#btn-import").addEventListener("click", () => void importFiles());
  $("#btn-batch-detect").addEventListener("click", () => void redetectAll());
  $("#btn-batch-export").addEventListener("click", openExport);
  $("#btn-redetect").addEventListener("click", () => void redetect());
  $("#btn-export").addEventListener("click", openExport);
  $("#btn-rules").addEventListener("click", () =>
    openRulesModal({
      categories: state.categories,
      rules: state.rules,
      onChanged: (rules) => {
        state.rules = rules;
        void api.listCategories().then((c) => {
          state.categories = c;
          renderFindings();
        });
        void redetectAll(false);
      },
    }),
  );
  $("#btn-settings").addEventListener("click", () => {
    if (!state.settings || !state.paths || !state.ocr || !state.enhance) {
      toast("设置尚未加载完成，请稍候", "warn");
      return;
    }
    openSettingsModal({
      settings: state.settings!,
      paths: state.paths!,
      ocr: state.ocr!,
      enhance: state.enhance!,
      onSaved: async (s) => {
        state.settings = s;
        $<HTMLInputElement>("#threshold").value = String(s.threshold);
        $("#threshold-value").textContent = s.threshold.toFixed(2);
        state.ocr = await api.ocrStatus();
        state.enhance = await api.enhanceStatus();
        renderOcrChip();
        renderEnhanceToggle();
        // 刚装好 OCR 模型时，对当前扫描件自动重新解析一次
        if (state.doc?.needs_ocr && state.ocr.ready && state.currentId) {
          setBusy("OCR 已就绪，正在重新解析扫描件…");
          try {
            await api.reparseDocument(state.currentId);
            // 重新解析会替换全文，必须重新拉取文档并清空字符缓存
            await refreshCurrentDoc();
            toast("扫描件已重新 OCR");
          } catch (e) {
            toast(String(e), "err");
          } finally {
            setBusy(null);
          }
        }
        void redetectAll(false);
      },
    });
  });
  $("#ocr-chip").addEventListener("click", () => $("#btn-settings").click());

  // 文件列表
  $("#file-list").addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    const checkId = target.dataset.check;
    if (checkId) {
      if ((target as HTMLInputElement).checked) state.selected.add(checkId);
      else state.selected.delete(checkId);
      renderFiles();
      return;
    }
    const li = target.closest<HTMLElement>(".file-item");
    if (li?.dataset.id) void selectDoc(li.dataset.id);
  });

  // 分页
  $("#btn-prev-page").addEventListener("click", () => {
    if (!state.doc) return;
    state.pageIndex = Math.max(0, state.pageIndex - 1);
    renderPreviewPane(true);
  });
  $("#btn-next-page").addEventListener("click", () => {
    if (!state.doc) return;
    state.pageIndex = Math.min(state.doc.page_count - 1, state.pageIndex + 1);
    renderPreviewPane(true);
  });

  // 视图模式
  $("#mode-switch").addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLElement>("[data-mode]");
    if (!btn) return;
    state.mode = btn.dataset.mode as ViewMode;
    renderPreviewPane(true);
  });

  // 阈值
  const slider = $<HTMLInputElement>("#threshold");
  slider.addEventListener("input", () => {
    $("#threshold-value").textContent = Number(slider.value).toFixed(2);
  });
  slider.addEventListener("change", async () => {
    if (!state.settings) return;
    state.settings.threshold = Number(slider.value);
    try {
      await api.saveSettings(state.settings);
      await redetectAll();
    } catch (e) {
      toast(String(e), "err");
    }
  });

  // 增强识别开关
  $("#enhance-toggle").addEventListener("change", async (e) => {
    const el = e.target as HTMLInputElement;
    if (!state.settings) return;
    const enabled = el.checked;
    state.settings.enhance_model = enabled;
    try {
      await api.saveSettings(state.settings);
      toast(
        enabled ? "增强脱敏已开启，正在重新识别…" : "增强脱敏已关闭，正在恢复规则识别…",
        "ok",
      );
      await redetectAll(false);
      renderEnhanceToggle();
    } catch (err) {
      el.checked = !enabled;
      state.settings.enhance_model = !enabled;
      toast(`切换增强脱敏失败：${String(err)}`, "err");
    }
  });

  // 全选
  $("#chk-all").addEventListener("change", async (e) => {
    if (!state.currentId) return;
    const enabled = (e.target as HTMLInputElement).checked;
    try {
      const list = await api.setAllEnabled(state.currentId, enabled);
      applyFindings(list);
    } catch (err) {
      toast(String(err), "err");
    }
  });

  // 分类筛选
  $("#category-filter").addEventListener("click", (e) => {
    const chip = (e.target as HTMLElement).closest<HTMLElement>("[data-cat]");
    if (!chip) return;
    state.filter = chip.dataset.cat ?? "all";
    renderFindings();
  });

  // 识别结果卡片
  bindFindingListEvents();

  // 正文：点击高亮 / 选区
  const preview = $("#preview-scroll");
  preview.addEventListener("click", (e) => {
    const hit = (e.target as HTMLElement).closest<HTMLElement>("[data-id]");
    if (!hit) return;
    const id = hit.dataset.id!;
    state.activeId = id;
    state.editingId = null;
    renderFindings();
    renderPreviewPane(false);
    const card = $(`#finding-list [data-id="${CSS.escape(id)}"]`);
    card?.scrollIntoView({ block: "nearest" });
  });

  preview.addEventListener("mouseup", () => {
    window.setTimeout(() => {
      const sel = selectionOffsets($("#doc-text"));
      if (!sel) {
        hidePopover();
        return;
      }
      if (sel.end - sel.start > 4000) {
        toast("选区过长（超过 4000 字），请缩小范围", "warn");
        return;
      }
      state.selection = sel;
      showPopover();
    }, 0);
  });

  preview.addEventListener("scroll", hidePopover);

  $("#btn-mark").addEventListener("click", () => {
    const sel = selectionOffsets($("#doc-text"));
    if (!sel) {
      toast("请先在正文中选中要标记的文字", "warn");
      return;
    }
    state.selection = sel;
    showPopover();
  });

  $("#popover-cancel").addEventListener("click", () => {
    hidePopover();
    window.getSelection()?.removeAllRanges();
  });
  $("#popover-confirm").addEventListener("click", () => void confirmManualFinding());

  document.addEventListener("mousedown", (e) => {
    const pop = $("#selection-popover");
    if (pop.classList.contains("hidden")) return;
    const t = e.target as HTMLElement;
    if (!pop.contains(t) && !t.closest("#doc-text") && !t.closest("#btn-mark")) hidePopover();
  });

  // 快捷键
  document.addEventListener("keydown", (e) => {
    const meta = e.metaKey || e.ctrlKey;
    if (meta && e.key.toLowerCase() === "o") {
      e.preventDefault();
      void importFiles();
    }
    if (meta && e.key.toLowerCase() === "e") {
      e.preventDefault();
      if (state.doc) void openExport();
    }
  });
}

function bindFindingListEvents(): void {
  const list = $("#finding-list");

  list.addEventListener("change", (e) => {
    const el = e.target as HTMLInputElement | HTMLSelectElement;
    const card = el.closest<HTMLElement>(".finding-card");
    const id = card?.dataset.id;
    if (!id) return;

    if (el.dataset.action === "toggle") {
      void toggleFinding(id, (el as HTMLInputElement).checked);
      return;
    }

    const editor = el.closest<HTMLElement>("[data-editor]");
    if (editor) {
      const f = state.findings.find((x) => x.id === id);
      if (!f) return;
      const next: Finding = JSON.parse(JSON.stringify(f));
      if (el.dataset.field === "category") {
        next.category = el.value;
        next.category_label = state.categories.find((c) => c.id === el.value)?.label ?? next.category_label;
      } else if (el.dataset.field === "strategy") {
        next.strategy.kind = el.value as StrategyConfig["kind"];
      } else if (el.dataset.field === "keep_prefix") {
        next.strategy.keep_prefix = Number(el.value) || 0;
      } else if (el.dataset.field === "keep_suffix") {
        next.strategy.keep_suffix = Number(el.value) || 0;
      }
      void updateFinding(next);
    }
  });

  list.addEventListener("input", (e) => {
    const el = e.target as HTMLInputElement;
    const card = el.closest<HTMLElement>(".finding-card");
    const id = card?.dataset.id;
    if (!id || !el.dataset.field) return;
    if (el.dataset.field !== "keep_prefix" && el.dataset.field !== "keep_suffix") return;
    const f = state.findings.find((x) => x.id === id);
    if (!f) return;
    const next: Finding = JSON.parse(JSON.stringify(f));
    if (el.dataset.field === "keep_prefix") next.strategy.keep_prefix = Number(el.value) || 0;
    if (el.dataset.field === "keep_suffix") next.strategy.keep_suffix = Number(el.value) || 0;
    void updateFinding(next);
  });

  list.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    const card = target.closest<HTMLElement>(".finding-card");
    const id = card?.dataset.id;
    if (!id) return;
    const action = target.dataset.action;
    const f = state.findings.find((x) => x.id === id);
    if (!f) return;
    // 勾选框、编辑器表单的交互不应触发“定位”
    if (action === "toggle" || target.closest("[data-editor]")) return;

    if (action === "locate") {
      locateFinding(f);
    } else if (action === "delete") {
      void confirmDialog("删除标记", `确定删除「${f.text.slice(0, 30)}」这条标记？`, "删除", true).then(
        (ok) => ok && void deleteFinding(id),
      );
    } else if (action === "edit") {
      state.editingId = state.editingId === id ? null : id;
      state.activeId = id;
      renderFindings();
    } else {
      state.activeId = id;
      renderFindings();
      locateFinding(f);
    }
  });
}

// 启动
void init();
