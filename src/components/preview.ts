// 正文预览：分段渲染 + 高亮 + 选区坐标换算 + 脱敏效果实时预览
import type { DocPayload, Finding, PageRange, StrategyConfig, ViewMode } from "../types";

export interface PreviewOptions {
  doc: DocPayload;
  findings: Finding[];
  pageIndex: number;
  mode: ViewMode;
  activeId: string | null;
  /** 是否回到顶部：翻页/切换文档/切换视图时传 true，复核勾选时传 false */
  resetScroll?: boolean;
}

/** 全文按 Unicode 码点缓存，避免每次渲染都重新拆分 */
let charCache: { id: string; chars: string[] } | null = null;

export function charsOf(doc: DocPayload): string[] {
  if (charCache && charCache.id === doc.id) return charCache.chars;
  const chars = Array.from(doc.text);
  charCache = { id: doc.id, chars };
  return chars;
}

export function invalidateCache(): void {
  charCache = null;
}

export function pageAt(doc: DocPayload, index: number): PageRange {
  return doc.pages[index] ?? { page: 1, start: 0, end: doc.text.length, label: "第 1 页" };
}

/** 前端侧的策略预览（导出以后端逻辑为准） */
export function applyStrategy(original: string, s: StrategyConfig, label: string): string {
  if (s.kind === "delete") return "";
  if (s.kind === "placeholder") {
    const tpl = s.placeholder || "【{type}】";
    return tpl.replaceAll("{type}", label).replaceAll("{index}", "1");
  }
  const chars = Array.from(original);
  const n = chars.length;
  const pre = Math.min(Math.max(s.keep_prefix, 0), n);
  const suf = Math.min(Math.max(s.keep_suffix, 0), n - pre);
  const mc = Array.from(s.mask_char || "*")[0] ?? "*";
  return chars.slice(0, pre).join("") + mc.repeat(n - pre - suf) + chars.slice(n - suf).join("");
}

/** 渲染预览区 */
export function renderPreview(root: HTMLElement, opts: PreviewOptions): void {
  const page = pageAt(opts.doc, opts.pageIndex);
  root.innerHTML = "";
  const scroller = root.parentElement;
  if (opts.resetScroll && scroller) scroller.scrollTop = 0;

  if (opts.mode === "compare") {
    const wrap = document.createElement("div");
    wrap.className = "compare-wrap";
    wrap.append(
      buildComparePane("原文（敏感信息高亮）", opts, page, false),
      buildComparePane("脱敏后（已替换）", opts, page, true),
    );
    root.append(wrap);
    return;
  }

  const pane = buildPane(opts, page, opts.mode === "redacted");
  pane.classList.add("paper-pane");
  root.append(pane);
}

function buildComparePane(
  label: string,
  opts: PreviewOptions,
  page: PageRange,
  redacted: boolean,
): HTMLElement {
  const section = document.createElement("section");
  section.className = "compare-pane";
  const head = document.createElement("span");
  head.className = "pane-label";
  head.textContent = label;
  const body = buildPane(opts, page, redacted);
  body.classList.add("pane-body");
  section.append(head, body);
  return section;
}

function buildPane(opts: PreviewOptions, page: PageRange, redacted: boolean): HTMLElement {
  const pane = document.createElement("div");
  pane.className = "doc-pane";
  pane.dataset.base = String(page.start);
  // 脱敏视图里的文字已被替换，不能再用它反推原文坐标（框选补标只在原文/对照左栏生效）
  pane.dataset.redacted = redacted ? "1" : "0";

  const chars = charsOf(opts.doc);
  const list = opts.findings
    .filter((f) => f.end > page.start && f.start < page.end)
    .sort((a, b) => a.start - b.start);

  let cursor = page.start;
  for (const f of list) {
    const s = Math.max(f.start, page.start);
    const e = Math.min(f.end, page.end);
    if (e <= cursor) continue;
    if (s > cursor) {
      pane.append(document.createTextNode(chars.slice(cursor, s).join("")));
    }
    const text = chars.slice(s, e).join("");
    if (redacted && !f.enabled) {
      pane.append(document.createTextNode(text));
      cursor = e;
      continue;
    }
    const el = document.createElement(redacted ? "span" : "mark");
    el.className = redacted
      ? "redacted"
      : ["hit", `cat-${f.category}`, f.enabled ? "" : "disabled", f.source === "manual" ? "manual" : "", opts.activeId === f.id ? "active" : ""]
          .filter(Boolean)
          .join(" ");
    el.dataset.id = f.id;
    el.dataset.start = String(f.start);
    el.dataset.end = String(f.end);
    el.title = `${f.category_label} · 置信度 ${(f.confidence * 100).toFixed(0)}%${
      f.enabled ? "" : "（已取消勾选）"
    }`;
    el.textContent = redacted ? applyStrategy(text, f.strategy, f.category_label) : text;
    pane.append(el);
    cursor = e;
  }
  if (cursor < page.end) {
    pane.append(document.createTextNode(chars.slice(cursor, page.end).join("")));
  }
  return pane;
}

/** 选区 → 全文码点区间 */
export function selectionOffsets(root: HTMLElement): { start: number; end: number; text: string } | null {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0 || sel.isCollapsed) return null;
  const range = sel.getRangeAt(0);
  const node = range.commonAncestorContainer;
  const el = node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  const pane = el?.closest(".doc-pane") as HTMLElement | null;
  if (!pane || !root.contains(pane) || pane.dataset.redacted === "1") return null;

  const base = Number(pane.dataset.base ?? "0");
  const pre = document.createRange();
  pre.selectNodeContents(pane);
  pre.setEnd(range.startContainer, range.startOffset);
  const start = base + cpLen(pre.toString());
  pre.setEnd(range.endContainer, range.endOffset);
  const end = base + cpLen(pre.toString());
  if (end <= start) return null;
  return { start, end, text: sel.toString() };
}

export function selectionRect(): DOMRect | null {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const rect = sel.getRangeAt(0).getBoundingClientRect();
  return rect.width === 0 && rect.height === 0 ? null : rect;
}

export function cpLen(s: string): number {
  return Array.from(s).length;
}

/** 找到偏移量所在页码 */
export function pageIndexForOffset(doc: DocPayload, offset: number): number {
  for (let i = 0; i < doc.pages.length; i++) {
    if (offset >= doc.pages[i].start && offset <= doc.pages[i].end) return i;
  }
  return 0;
}

/** 定位到某条命中并闪烁提示 */
export function focusFinding(root: HTMLElement, findingId: string): void {
  const el = root.querySelector<HTMLElement>(`[data-id="${CSS.escape(findingId)}"]`);
  if (!el) return;
  el.scrollIntoView({ block: "center", behavior: "smooth" });
  el.classList.add("animate-flash");
  window.setTimeout(() => el.classList.remove("animate-flash"), 1500);
}
