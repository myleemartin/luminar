// 左栏文件列表 + 右栏识别结果渲染（纯渲染，事件由 main.ts 统一委托）
import type { CategoryInfo, DocSummary, Finding } from "../types";

export function esc(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

export function kindShort(kind: string): string {
  switch (kind) {
    case "pdf_text":
      return "PDF";
    case "pdf_scan":
      return "扫描件";
    case "docx":
      return "WORD";
    case "rtf":
      return "RTF";
    case "html":
      return "HTML";
    case "ofd":
      return "OFD";
    case "doc_legacy":
      return "DOC/WPS";
    default:
      return "TXT";
  }
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export function formatNumber(n: number): string {
  return n.toLocaleString("zh-CN");
}

export function sourceLabel(s: Finding["source"]): string {
  return (
    {
      regex: "规则识别",
      keyword: "关键词",
      ner: "人名识别",
      propagated: "同名传播",
      manual: "人工标记",
      model: "模型识别",
    } as Record<string, string>
  )[s] ?? s;
}

export function modeLabel(kind: string): string {
  return ({ mask: "掩码", placeholder: "占位", delete: "删除" } as Record<string, string>)[kind] ?? kind;
}

export function renderFileList(
  ul: HTMLElement,
  empty: HTMLElement,
  docs: DocSummary[],
  currentId: string | null,
  selected: Set<string>,
): void {
  empty.classList.toggle("hidden", docs.length > 0);
  ul.innerHTML = docs
    .map((d) => {
      const active = d.id === currentId ? "active" : "";
      const checked = selected.has(d.id) ? "checked" : "";
      const warn = d.warnings.length > 0 ? "⚠" : "";
      return `
      <li class="file-item ${active}" data-id="${d.id}">
        <input type="checkbox" class="doc-check mt-0.5 h-3 w-3 accent-ink-700" data-check="${d.id}" ${checked} />
        <div class="min-w-0 flex-1">
          <div class="truncate text-[12.5px] font-medium leading-5" title="${esc(d.file_name)}">${esc(
            d.file_name,
          )}</div>
          <div class="file-meta mt-0.5 flex items-center gap-1.5 text-[10px] text-ink-400">
            <span class="rounded bg-ink-900/5 px-1 py-0.5">${kindShort(d.kind)}</span>
            <span>${d.page_count} 页</span>
            <span>${formatNumber(d.char_count)} 字</span>
            ${
              d.finding_count > 0
                ? `<span class="ml-auto rounded-full bg-bronze-500/10 px-1.5 py-0.5 text-bronze-500">${d.enabled_count}/${d.finding_count}</span>`
                : `<span class="ml-auto text-ink-300">${warn || "待识别"}</span>`
            }
            ${d.warnings.length > 0 ? `<span class="text-amber-500" title="${esc(d.warnings.join("；"))}">⚠</span>` : ""}
          </div>
        </div>
      </li>`;
    })
    .join("");
}

export function renderCategoryFilter(
  root: HTMLElement,
  findings: Finding[],
  active: string,
): void {
  const counts = new Map<string, { label: string; n: number }>();
  for (const f of findings) {
    const item = counts.get(f.category) ?? { label: f.category_label, n: 0 };
    item.n += 1;
    counts.set(f.category, item);
  }
  const chips: string[] = [
    `<button class="chip ${active === "all" ? "active" : ""}" data-cat="all">全部 ${findings.length}</button>`,
  ];
  for (const [id, { label, n }] of counts) {
    chips.push(
      `<button class="chip ${active === id ? "active" : ""}" data-cat="${esc(id)}">${esc(label)} ${n}</button>`,
    );
  }
  root.innerHTML = chips.join("");
}

export function renderFindingList(
  root: HTMLElement,
  findings: Finding[],
  categories: CategoryInfo[],
  activeId: string | null,
  editingId: string | null,
  filter: string,
): void {
  const list = findings
    .filter((f) => filter === "all" || f.category === filter)
    .sort((a, b) => a.start - b.start);

  root.innerHTML = list
    .map((f) => {
      const editing = editingId === f.id;
      const editor = editing ? buildEditor(f, categories) : "";
      return `
      <div class="finding-card ${activeId === f.id ? "active" : ""} ${f.enabled ? "" : "off"}" data-id="${f.id}">
        <div class="flex items-start gap-2">
          <input type="checkbox" class="mt-1 h-3 w-3 accent-ink-700" data-action="toggle" data-id="${f.id}" ${
            f.enabled ? "checked" : ""
          } />
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1.5">
              <span class="inline-block h-2 w-2 rounded-full cat-${esc(f.category)}"></span>
              <span class="text-[11px] font-medium text-ink-700">${esc(f.category_label)}</span>
              <span class="ml-auto font-mono text-[10px] ${
                f.source === "manual" ? "text-bronze-500" : "text-ink-400"
              }">${f.source === "manual" ? "人工" : (f.confidence * 100).toFixed(0) + "%"}</span>
            </div>
            <p class="mt-1 break-all font-mono text-[11px] leading-relaxed text-ink-700">${esc(
              f.text.length > 80 ? f.text.slice(0, 80) + "…" : f.text,
            )}</p>
            <div class="mt-1 flex items-center gap-2 text-[10px] text-ink-400">
              <span>${sourceLabel(f.source)}</span>
              <span class="truncate" title="${esc(f.rule_name)}">${esc(f.rule_name)}</span>
              <span class="ml-auto shrink-0">${modeLabel(f.strategy.kind)}</span>
            </div>
          </div>
        </div>
        ${editor}
        <div class="mt-1 flex items-center justify-end gap-0.5">
          <button class="rounded px-1.5 py-0.5 text-[10px] text-ink-400 hover:bg-ink-50 hover:text-ink-700" data-action="locate">定位</button>
          <button class="rounded px-1.5 py-0.5 text-[10px] text-ink-400 hover:bg-ink-50 hover:text-ink-700" data-action="edit">编辑</button>
          <button class="rounded px-1.5 py-0.5 text-[10px] text-bronze-500/80 hover:bg-bronze-50 hover:text-bronze-600" data-action="delete">删除</button>
        </div>
      </div>`;
    })
    .join("");
}

function buildEditor(f: Finding, categories: CategoryInfo[]): string {
  const catOptions = categories
    .map(
      (c) =>
        `<option value="${esc(c.id)}" ${c.id === f.category ? "selected" : ""}>${esc(c.label)}</option>`,
    )
    .join("");
  const strOptions = (
    [
      ["mask", "掩码遮蔽"],
      ["placeholder", "占位标签"],
      ["delete", "直接删除"],
    ] as const
  )
    .map(
      ([v, label]) =>
        `<option value="${v}" ${v === f.strategy.kind ? "selected" : ""}>${label}</option>`,
    )
    .join("");

  const maskFields =
    f.strategy.kind === "mask"
      ? `<div class="mt-1 flex items-center gap-1 text-[10px] text-ink-500">
           保留前 <input class="input w-12 px-1 py-0.5 text-center" type="number" min="0" data-field="keep_prefix" value="${f.strategy.keep_prefix}" />
           后 <input class="input w-12 px-1 py-0.5 text-center" type="number" min="0" data-field="keep_suffix" value="${f.strategy.keep_suffix}" />
           位
         </div>`
      : "";

  return `<div class="mt-2 rounded-lg bg-ink-50/70 p-2" data-editor>
      <div class="grid grid-cols-2 gap-1.5">
        <select class="input w-full py-1 text-[11px]" data-field="category">${catOptions}</select>
        <select class="input w-full py-1 text-[11px]" data-field="strategy">${strOptions}</select>
      </div>
      ${maskFields}
      <p class="mt-1 text-[10px] text-ink-400">修改后立即生效，无需保存</p>
    </div>`;
}
