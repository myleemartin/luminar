// 规则管理 / 设置 / 导出 三个弹窗
import { api } from "../api";
import type {
  AppPaths,
  CategoryInfo,
  DocSummary,
  EnhanceStatus,
  ExportOutcome,
  OcrStatus,
  Rule,
  Settings,
} from "../types";
import { confirmDialog, escHtml, formatBytes, openModal, setBusy, toast } from "../ui";

// ---------------------------------------------------------------------------
// 规则管理
// ---------------------------------------------------------------------------

export interface RulesDeps {
  categories: CategoryInfo[];
  rules: Rule[];
  onChanged: (rules: Rule[]) => void;
}

export function openRulesModal(deps: RulesDeps): void {
  let rules = [...deps.rules];
  let draft: Rule = rules[0] ? clone(rules[0]) : newRule();
  let isNew = rules.length === 0;
  let search = "";

  const handle = openModal(
    "规则管理 · 自定义识别与脱敏策略",
    `<div class="grid h-[640px] grid-cols-[290px_minmax(0,1fr)]">
       <div class="flex min-h-0 flex-col border-r border-ink-900/10">
         <div class="p-3">
           <input id="rule-search" class="input w-full" placeholder="搜索规则名称 / 分类 / 表达式" />
         </div>
         <div id="rule-list" class="min-h-0 flex-1 overflow-y-auto px-2 pb-2"></div>
         <div class="flex gap-2 border-t border-ink-900/10 p-2">
           <button id="rule-new" class="btn-primary flex-1 justify-center">新增规则</button>
           <button id="rule-reset" class="btn-ghost">恢复内置</button>
         </div>
       </div>
       <div class="flex min-h-0 flex-col">
         <div id="rule-form" class="min-h-0 flex-1 overflow-y-auto p-4"></div>
         <div id="rule-foot" class="flex items-center gap-2 border-t border-ink-900/10 bg-paper/40 px-4 py-3"></div>
       </div>
     </div>`,
    "",
    "max-w-[1080px]",
  );

  const listEl = handle.body.querySelector<HTMLElement>("#rule-list")!;
  const formEl = handle.body.querySelector<HTMLElement>("#rule-form")!;
  const footEl = handle.body.querySelector<HTMLElement>("#rule-foot")!;

  const renderList = () => {
    const kw = search.trim().toLowerCase();
    const filtered = rules.filter(
      (r) =>
        !kw ||
        r.name.toLowerCase().includes(kw) ||
        r.category_label.toLowerCase().includes(kw) ||
        r.pattern.toLowerCase().includes(kw),
    );
    listEl.innerHTML = filtered
      .map((r) => {
        const active = !isNew && draft.id === r.id;
        const type = r.kind === "regex" ? "正则" : r.kind === "keyword" ? "关键词" : "智能";
        return `<button class="mb-1 flex w-full items-start gap-2 rounded-lg px-2.5 py-2 text-left transition ${
          active ? "bg-ink-800 text-paper" : "hover:bg-ink-50"
        }" data-rule="${r.id}">
          <span class="mt-1 h-1.5 w-1.5 shrink-0 rounded-full ${r.enabled ? "bg-emerald-400" : "bg-ink-300"}"></span>
          <span class="min-w-0 flex-1">
            <span class="block truncate text-[12px] font-medium">${escHtml(r.name)}</span>
            <span class="mt-0.5 flex items-center gap-1 text-[10px] ${
              active ? "text-ink-200" : "text-ink-400"
            }">
              <span>${escHtml(r.category_label)}</span><span>·</span><span>${type}</span>
              ${r.builtin ? `<span class="ml-auto">内置</span>` : `<span class="ml-auto text-bronze-400">自定义</span>`}
            </span>
          </span>
        </button>`;
      })
      .join("");

    listEl.querySelectorAll<HTMLElement>("[data-rule]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const rule = rules.find((r) => r.id === btn.dataset.rule);
        if (!rule) return;
        draft = clone(rule);
        isNew = false;
        renderList();
        renderForm();
      });
    });
  };

  const renderForm = () => {
    formEl.innerHTML = formHtml(draft, deps.categories);
    footEl.innerHTML = `
      <span class="text-[10px] text-ink-400">${
        draft.builtin ? "内置规则可修改策略与置信度，也可删除" : "自定义规则"
      }</span>
      <span class="ml-auto"></span>
      ${
        isNew
          ? ""
          : `<button class="btn-ghost !text-bronze-500 hover:!border-bronze-500/40" id="rule-delete">删除</button>`
      }
      <button class="btn-primary" id="rule-save">保存规则</button>`;

    bindForm();
  };

  const bindForm = () => {
    // 文本类字段：直接改 draft，不重绘，避免丢焦点
    formEl.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>("[data-field]").forEach((el) => {
      const field = el.dataset.field as keyof Rule;
      if (el.tagName === "SELECT" || (el as HTMLInputElement).type === "checkbox") return;
      el.addEventListener("input", () => {
        if (field === "keywords") {
          draft.keywords = el.value
            .split("\n")
            .map((s) => s.trim())
            .filter(Boolean);
        } else if (field === "capture_group" || field === "priority") {
          (draft as unknown as Record<string, unknown>)[field as string] = Number(el.value) || 0;
        } else if (field === "confidence") {
          draft.confidence = Number(el.value);
          const label = formEl.querySelector<HTMLElement>("#confidence-label");
          if (label) label.textContent = Number(el.value).toFixed(2);
        } else {
          (draft as unknown as Record<string, unknown>)[field as string] = el.value;
        }
        if (field === "name" || field === "category_label") renderList();
      });
    });

    // 策略字段
    formEl.querySelectorAll<HTMLInputElement | HTMLSelectElement>("[data-strategy]").forEach((el) => {
      const field = el.dataset.strategy as keyof typeof draft.strategy;
      el.addEventListener("input", () => {
        const target = draft.strategy as unknown as Record<string, unknown>;
        if (field === "keep_prefix" || field === "keep_suffix") {
          target[field as string] = Number(el.value) || 0;
        } else {
          target[field as string] = el.value;
        }
      });
    });

    formEl.querySelector<HTMLSelectElement>("[data-field='category']")?.addEventListener("change", (e) => {
      const id = (e.target as HTMLSelectElement).value;
      draft.category = id;
      draft.category_label =
        deps.categories.find((c) => c.id === id)?.label ?? draft.category_label;
      renderForm();
    });

    formEl.querySelector<HTMLSelectElement>("[data-field='kind']")?.addEventListener("change", (e) => {
      draft.kind = (e.target as HTMLSelectElement).value as Rule["kind"];
      renderForm();
    });

    formEl.querySelector<HTMLSelectElement>("[data-strategy='kind']")?.addEventListener("change", (e) => {
      draft.strategy.kind = (e.target as HTMLSelectElement).value as Rule["strategy"]["kind"];
      renderForm();
    });

    formEl.querySelector<HTMLInputElement>("[data-field='enabled']")?.addEventListener("change", (e) => {
      draft.enabled = (e.target as HTMLInputElement).checked;
      renderList();
    });

    formEl.querySelector<HTMLInputElement>("[data-field='case_insensitive']")?.addEventListener("change", (e) => {
      draft.case_insensitive = (e.target as HTMLInputElement).checked;
    });

    formEl.querySelector<HTMLInputElement>("[data-field='require_boundary']")?.addEventListener("change", (e) => {
      draft.require_boundary = (e.target as HTMLInputElement).checked;
    });

    formEl.querySelector<HTMLSelectElement>("[data-field='validator']")?.addEventListener("change", (e) => {
      draft.validator = (e.target as HTMLSelectElement).value as Rule["validator"];
    });

    const testBtn = formEl.querySelector<HTMLElement>("#btn-test");
    testBtn?.addEventListener("click", async () => {
      const text = formEl.querySelector<HTMLTextAreaElement>("#test-text")?.value ?? "";
      const out = formEl.querySelector<HTMLElement>("#test-result");
      if (!out) return;
      if (!text.trim()) {
        out.innerHTML = `<span class="text-ink-400">请先粘贴一段测试文字</span>`;
        return;
      }
      try {
        const res = await api.testRule(draft, text);
        out.innerHTML = res.count
          ? `<span class="text-emerald-600">命中 ${res.count} 处：</span>` +
            res.matches
              .slice(0, 12)
              .map((m) => `<code class="mr-1 rounded bg-ink-50 px-1 py-0.5">${escHtml(m)}</code>`)
              .join("")
          : `<span class="text-ink-400">没有命中，可调整表达式后重试</span>`;
      } catch (e) {
        out.innerHTML = `<span class="text-bronze-500">${escHtml(String(e))}</span>`;
      }
    });
  };

  handle.body.querySelector("#rule-search")?.addEventListener("input", (e) => {
    search = (e.target as HTMLInputElement).value;
    renderList();
  });

  handle.body.querySelector("#rule-new")?.addEventListener("click", () => {
    draft = newRule();
    isNew = true;
    renderList();
    renderForm();
  });

  handle.body.querySelector("#rule-reset")?.addEventListener("click", async () => {
    const ok = await confirmDialog(
      "恢复内置规则",
      "将把全部规则恢复为出厂预设，自定义规则会被删除。确定继续吗？",
      "恢复",
      true,
    );
    if (!ok) return;
    try {
      rules = await api.resetRules();
      deps.onChanged(rules);
      draft = clone(rules[0]);
      isNew = false;
      renderList();
      renderForm();
      toast("已恢复内置规则");
    } catch (e) {
      toast(String(e), "err");
    }
  });

  footEl.addEventListener("click", async (e) => {
    const target = e.target as HTMLElement;
    if (target.id === "rule-save") {
      if (!draft.name.trim()) {
        toast("请填写规则名称", "warn");
        return;
      }
      try {
        rules = await api.saveRule(draft);
        deps.onChanged(rules);
        const saved = rules.find((r) => r.id === draft.id);
        if (saved) draft = clone(saved);
        isNew = false;
        renderList();
        renderForm();
        toast("规则已保存，重新识别后生效");
      } catch (err) {
        toast(String(err), "err");
      }
    }
    if (target.id === "rule-delete") {
      const ok = await confirmDialog(
        "删除规则",
        `确定删除「${draft.name}」？删除后可用「恢复内置」找回内置规则。`,
        "删除",
        true,
      );
      if (!ok) return;
      try {
        rules = await api.deleteRule(draft.id);
        deps.onChanged(rules);
        draft = rules[0] ? clone(rules[0]) : newRule();
        isNew = rules.length === 0;
        renderList();
        renderForm();
        toast("规则已删除");
      } catch (err) {
        toast(String(err), "err");
      }
    }
  });

  renderList();
  renderForm();
}

function formHtml(r: Rule, categories: CategoryInfo[]): string {
  const catOptions = categories
    .map((c) => `<option value="${c.id}" ${c.id === r.category ? "selected" : ""}>${escHtml(c.label)}</option>`)
    .join("");

  const kindOptions = (
    [
      ["regex", "正则表达式"],
      ["keyword", "关键词列表"],
      ["ner", "内置智能识别（不可编辑表达式）"],
    ] as const
  )
    .map(
      ([v, label]) =>
        `<option value="${v}" ${v === r.kind ? "selected" : ""} ${v === "ner" && !r.builtin ? "disabled" : ""}>${label}</option>`,
    )
    .join("");

  const strategyOptions = (
    [
      ["mask", "掩码遮蔽（如 138****5678）"],
      ["placeholder", "占位标签（如 【手机号】）"],
      ["delete", "直接删除"],
    ] as const
  )
    .map(([v, l]) => `<option value="${v}" ${v === r.strategy.kind ? "selected" : ""}>${l}</option>`)
    .join("");

  const validatorOptions = (
    [
      ["none", "不做校验"],
      ["id_card", "身份证校验位"],
      ["luhn", "银行卡 Luhn"],
      ["uscc", "统一社会信用代码"],
    ] as const
  )
    .map(([v, l]) => `<option value="${v}" ${v === r.validator ? "selected" : ""}>${l}</option>`)
    .join("");

  const matchFields =
    r.kind === "regex"
      ? `<div>
           <label class="field-label">正则表达式</label>
           <textarea data-field="pattern" rows="3" spellcheck="false"
             class="input w-full font-mono text-[11px] leading-relaxed">${escHtml(r.pattern)}</textarea>
           <p class="field-hint">正则表达式语法：支持 \\d、[\\u4e00-\\u9fa5]、分组捕获；不支持后行断言。使用捕获组时可在下方指定取第几组。</p>
         </div>
         <div class="grid grid-cols-2 gap-3">
           <div>
             <label class="field-label">捕获组（0 = 整段）</label>
             <input data-field="capture_group" type="number" min="0" value="${r.capture_group}" class="input w-full" />
           </div>
           <div>
             <label class="field-label">规则优先级</label>
             <input data-field="priority" type="number" value="${r.priority}" class="input w-full" />
           </div>
         </div>`
      : r.kind === "keyword"
        ? `<div>
             <label class="field-label">关键词列表（每行一个）</label>
             <textarea data-field="keywords" rows="5" class="input w-full font-mono text-[11px] leading-relaxed">${escHtml(
               r.keywords.join("\n"),
             )}</textarea>
             <p class="field-hint">关键词按包含匹配，命中后整体脱敏；长词优先，避免「张三」抢先匹配「张三丰」。</p>
           </div>`
        : `<div class="rounded-lg bg-ink-50 px-3 py-2 text-[11px] leading-relaxed text-ink-500">
             该规则由内置人名识别引擎执行（姓氏表 + 上下文线索），不依赖外部模型，完全离线。
             可调整下方「基础置信度」与脱敏方式。
           </div>`;

  const maskFields =
    r.strategy.kind === "mask"
      ? `<div class="grid grid-cols-3 gap-3">
           <div><label class="field-label">掩码字符</label>
             <input data-strategy="mask_char" maxlength="1" value="${escHtml(r.strategy.mask_char)}" class="input w-full text-center" /></div>
           <div><label class="field-label">保留前几位</label>
             <input data-strategy="keep_prefix" type="number" min="0" value="${r.strategy.keep_prefix}" class="input w-full" /></div>
           <div><label class="field-label">保留后几位</label>
             <input data-strategy="keep_suffix" type="number" min="0" value="${r.strategy.keep_suffix}" class="input w-full" /></div>
         </div>`
      : r.strategy.kind === "placeholder"
        ? `<div>
             <label class="field-label">占位模板</label>
             <input data-strategy="placeholder" value="${escHtml(r.strategy.placeholder)}" class="input w-full font-mono" />
             <p class="field-hint">支持 <code>{type}</code>（分类名）与 <code>{index}</code>（同类序号），如 <code>【{type}-{index}】</code>。</p>
           </div>`
        : `<p class="text-[11px] text-ink-400">直接删除命中的文字，不保留任何痕迹。</p>`;

  return `<div class="space-y-3">
    <div class="grid grid-cols-2 gap-3">
      <div>
        <label class="field-label">规则名称</label>
        <input data-field="name" value="${escHtml(r.name)}" class="input w-full" placeholder="如：手机号（含港澳台）" />
      </div>
      <div>
        <label class="field-label">识别分类</label>
        <select data-field="category" class="input w-full">${catOptions}</select>
      </div>
    </div>
    ${
      r.category === "custom"
        ? `<div>
             <label class="field-label">自定义分类名称（显示在识别清单与占位标签里）</label>
             <input data-field="category_label" value="${escHtml(r.category_label)}" class="input w-full" placeholder="如：项目代号" />
           </div>`
        : ""
    }

    <div class="grid grid-cols-2 gap-3">
      <div>
        <label class="field-label">规则类型</label>
        <select data-field="kind" class="input w-full">${kindOptions}</select>
      </div>
      <div>
        <label class="field-label">二次校验</label>
        <select data-field="validator" class="input w-full">${validatorOptions}</select>
      </div>
    </div>

    ${matchFields}

    <div class="rounded-xl border border-ink-900/10 p-3">
      <div class="mb-2 flex items-center gap-2">
        <span class="text-[11px] font-medium text-ink-700">脱敏方式</span>
        <label class="ml-auto flex items-center gap-1.5 text-[11px] text-ink-500">
          <input type="checkbox" data-field="enabled" class="accent-ink-700" ${r.enabled ? "checked" : ""} /> 启用本规则
        </label>
        <label class="flex items-center gap-1.5 text-[11px] text-ink-500">
          <input type="checkbox" data-field="case_insensitive" class="accent-ink-700" ${r.case_insensitive ? "checked" : ""} /> 忽略大小写
        </label>
        <label class="flex items-center gap-1.5 text-[11px] text-ink-500">
          <input type="checkbox" data-field="require_boundary" class="accent-ink-700" ${r.require_boundary ? "checked" : ""} /> 边界校验
        </label>
      </div>
      <select data-strategy="kind" class="input mb-2 w-full">${strategyOptions}</select>
      ${maskFields}
    </div>

    <div class="grid grid-cols-[1fr_auto] items-end gap-3">
      <div>
        <label class="field-label">基础置信度</label>
        <input data-field="confidence" type="range" min="0.3" max="1" step="0.01" value="${r.confidence}" class="w-full accent-ink-700" />
      </div>
      <span id="confidence-label" class="pb-1 font-mono text-xs text-ink-600">${r.confidence.toFixed(2)}</span>
    </div>

    <div>
      <label class="field-label">规则说明（便于团队协作）</label>
      <textarea data-field="description" rows="2" class="input w-full">${escHtml(r.description)}</textarea>
    </div>

    <div class="rounded-xl border border-dashed border-ink-900/15 p-3">
      <div class="mb-2 flex items-center gap-2">
        <span class="text-[11px] font-medium text-ink-700">规则测试</span>
        <button id="btn-test" class="ml-auto btn-ghost !py-1">测试匹配</button>
      </div>
      <textarea id="test-text" rows="3" class="input w-full text-[11px]"
        placeholder="粘贴一段脱敏前的示例文字，例如：原告张三，身份证号 420102198801011234……"></textarea>
      <div id="test-result" class="mt-2 min-h-[18px] text-[11px]"></div>
    </div>
  </div>`;
}

function newRule(): Rule {
  return {
    id: `custom_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`,
    name: "新建规则",
    category: "custom",
    category_label: "自定义",
    kind: "regex",
    pattern: "",
    keywords: [],
    capture_group: 0,
    confidence: 0.9,
    strategy: {
      kind: "placeholder",
      mask_char: "*",
      keep_prefix: 0,
      keep_suffix: 0,
      placeholder: "【{type}】",
    },
    validator: "none",
    priority: 10,
    enabled: true,
    require_boundary: false,
    case_insensitive: false,
    builtin: false,
    description: "",
  };
}

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

// ---------------------------------------------------------------------------
// 设置
// ---------------------------------------------------------------------------

export interface SettingsDeps {
  settings: Settings;
  paths: AppPaths;
  ocr: OcrStatus;
  enhance: EnhanceStatus;
  onSaved: (s: Settings) => void;
}

export function openSettingsModal(deps: SettingsDeps): void {
  const s = deps.settings;
  const handle = openModal(
    "设置",
    `<div class="space-y-4 p-5">
      <section class="rounded-xl border border-ink-900/10 p-4">
        <h3 class="mb-3 text-[12px] font-semibold text-ink-800">识别严格程度</h3>
        <label class="field-label">置信度阈值：低于该值的自动识别结果不展示</label>
        <div class="flex items-center gap-3">
          <input id="set-threshold" type="range" min="0.3" max="0.99" step="0.01" value="${s.threshold}" class="flex-1 accent-ink-700" />
          <span id="set-threshold-label" class="w-10 text-right font-mono text-xs">${s.threshold.toFixed(2)}</span>
        </div>
        <label class="mt-3 flex items-center gap-2 text-[11px] text-ink-600">
          <input id="set-propagate" type="checkbox" class="accent-ink-700" ${s.propagate_names ? "checked" : ""} />
          同一姓名在全文其他位置出现时一并标记（推荐）
        </label>
      </section>

      <section class="rounded-xl border border-ink-900/10 p-4">
        <h3 class="mb-1 text-[12px] font-semibold text-ink-800">增强脱敏（本地小模型，可选）</h3>
        <label class="mt-2 flex items-start gap-2 text-[11px] leading-relaxed text-ink-600 ${
          deps.enhance.ready ? "" : "opacity-60"
        }">
          <input id="set-enhance" type="checkbox" class="mt-0.5 accent-ink-700" ${
            s.enhance_model ? "checked" : ""
          } ${deps.enhance.ready || s.enhance_model ? "" : "disabled"} />
          <span>
            在正则规则之外，用本地小模型补充识别<b>当事人姓名、单位/律所/法院、住址</b>。
            首次启用需加载约 0.2–1 秒，长文档会略慢；不联网、CPU 可跑。
          </span>
        </label>
        <p class="mt-2 rounded-lg bg-ink-50 px-3 py-2 text-[11px] leading-relaxed ${
          deps.enhance.ready ? "text-ink-500" : "text-bronze-500"
        }">${escHtml(deps.enhance.message)}</p>
      </section>

      <section class="rounded-xl border border-ink-900/10 p-4">
        <h3 class="mb-1 text-[12px] font-semibold text-ink-800">文字识别（OCR，扫描件）</h3>
        <p id="ocr-status" class="mb-3 rounded-lg bg-ink-50 px-3 py-2 text-[11px] leading-relaxed ${
          deps.ocr.ready ? "text-ink-600" : "text-bronze-500"
        }">${escHtml(deps.ocr.message)}</p>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="field-label">文字识别引擎</label>
            <select id="set-ocr-engine" class="input w-full">
              <option value="paddle_onnx" ${s.ocr_engine === "paddle_onnx" ? "selected" : ""}>PaddleOCR 本地模型（推荐，离线）</option>
              <option value="external" ${s.ocr_engine === "external" ? "selected" : ""}>本机 OCR 命令（外部进程）</option>
              <option value="off" ${s.ocr_engine === "off" ? "selected" : ""}>关闭文字识别</option>
            </select>
          </div>
          <div>
            <label class="field-label">文字识别清晰度（DPI）</label>
            <input id="set-ocr-dpi" type="number" min="100" max="400" value="${s.ocr_dpi}" class="input w-full" />
          </div>
        </div>
        <label class="field-label mt-3">外部文字识别命令（{image} 会被替换为页面图片路径）</label>
        <input id="set-ocr-cmd" value="${escHtml(s.external_ocr_command)}" class="input w-full font-mono text-[11px]" placeholder="如：~/.local/bin/ocr_rapid.sh {image}" />
        <p class="field-hint">模型目录：<code>${escHtml(deps.ocr.models_dir)}</code></p>
        <button id="open-models" class="btn-ghost mt-2">打开模型目录</button>
      </section>

      <section class="rounded-xl border border-ink-900/10 p-4">
        <h3 class="mb-3 text-[12px] font-semibold text-ink-800">导出</h3>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="field-label">导出 PDF 渲染 DPI</label>
            <input id="set-pdf-dpi" type="number" min="96" max="400" value="${s.pdf_export_dpi}" class="input w-full" />
            <p class="field-hint">越高越清晰，文件也越大；建议 150。</p>
          </div>
          <div>
            <label class="field-label">默认掩码字符</label>
            <input id="set-mask-char" maxlength="1" value="${escHtml(s.default_mask_char)}" class="input w-full text-center" />
          </div>
        </div>
        <label class="field-label mt-3">默认占位模板</label>
        <input id="set-placeholder" value="${escHtml(s.placeholder_template)}" class="input w-full font-mono" />
      </section>

      <section class="rounded-xl border border-ink-900/10 bg-paper/60 p-4 text-[11px] leading-relaxed text-ink-500">
        <div class="mb-1 font-medium text-ink-700">数据存放位置</div>
        <div class="break-all font-mono text-[10px]">${escHtml(deps.paths.config_dir)}</div>
        <div class="mt-2 flex gap-2">
          <button id="open-config" class="btn-ghost">打开配置目录</button>
          <button id="open-rules-file" class="btn-ghost">打开规则文件</button>
        </div>
        <p class="mt-2 text-[10px] text-ink-400">本程序不联网：所有解析、识别、文字识别、导出均在本机完成。</p>
        <p class="mt-2 text-[10px] text-ink-400">星星脱敏 v0.11.0 · 源码公开、免费非商业使用（PolyForm Noncommercial 1.0.0）</p>
      </section>
    </div>`,
    `<button class="btn-ghost" data-close>取消</button>
     <button class="btn-primary" id="settings-save">保存设置</button>`,
    "max-w-[680px]",
  );

  handle.body.querySelector("#set-threshold")?.addEventListener("input", (e) => {
    const v = Number((e.target as HTMLInputElement).value);
    const label = handle.body.querySelector("#set-threshold-label");
    if (label) label.textContent = v.toFixed(2);
  });

  handle.body.querySelector("#open-models")?.addEventListener("click", () => {
    api.openPath(deps.ocr.models_dir).catch((e) => toast(String(e), "err"));
  });
  handle.body.querySelector("#open-config")?.addEventListener("click", () => {
    api.openPath(deps.paths.config_dir).catch((e) => toast(String(e), "err"));
  });
  handle.body.querySelector("#open-rules-file")?.addEventListener("click", () => {
    api.openPath(deps.paths.rules_file).catch((e) => toast(String(e), "err"));
  });

  handle.back.querySelector("#settings-save")?.addEventListener("click", async () => {
    const next: Settings = {
      ...s,
      threshold: Number(handle.body.querySelector<HTMLInputElement>("#set-threshold")?.value ?? s.threshold),
      propagate_names: !!handle.body.querySelector<HTMLInputElement>("#set-propagate")?.checked,
      enhance_model: !!handle.body.querySelector<HTMLInputElement>("#set-enhance")?.checked,
      ocr_engine: handle.body.querySelector<HTMLSelectElement>("#set-ocr-engine")?.value ?? s.ocr_engine,
      ocr_dpi: Number(handle.body.querySelector<HTMLInputElement>("#set-ocr-dpi")?.value ?? s.ocr_dpi),
      external_ocr_command: handle.body.querySelector<HTMLInputElement>("#set-ocr-cmd")?.value ?? "",
      pdf_export_dpi: Number(handle.body.querySelector<HTMLInputElement>("#set-pdf-dpi")?.value ?? s.pdf_export_dpi),
      default_mask_char: handle.body.querySelector<HTMLInputElement>("#set-mask-char")?.value ?? "*",
      placeholder_template:
        handle.body.querySelector<HTMLInputElement>("#set-placeholder")?.value ?? "{{type}}",
    };
    try {
      const saved = await api.saveSettings(next);
      deps.onSaved(saved);
      handle.close();
      toast("设置已保存");
    } catch (e) {
      toast(String(e), "err");
    }
  });
}

// ---------------------------------------------------------------------------
// 导出
// ---------------------------------------------------------------------------

export interface ExportDeps {
  docs: DocSummary[];
  initialIds: string[];
  settings: Settings;
  /// 默认输出目录（桌面）；用户仍可手动选择
  defaultDir: string;
  pickDir: () => Promise<string | null>;
  onFinished: () => void;
}

export function openExportModal(deps: ExportDeps): void {
  const selected = new Set(deps.initialIds);
  let outDir = deps.defaultDir || deps.settings.last_output_dir || "";

  const pickedAllPdf = () => {
    const picked = deps.docs.filter((d) => selected.has(d.id));
    return picked.length > 0 && picked.every((d) => d.kind.startsWith("pdf"));
  };

  const handle = openModal(
    "导出脱敏文书",
    `<div class="grid grid-cols-[280px_minmax(0,1fr)]">
      <div class="border-r border-ink-900/10 p-4">
        <div class="mb-2 text-[11px] font-medium text-ink-600">选择文书</div>
        <div id="export-docs" class="max-h-[300px] space-y-1 overflow-y-auto">
          ${deps.docs
            .map(
              (d) => `<label class="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 hover:bg-ink-50">
                <input type="checkbox" class="export-doc accent-ink-700" value="${d.id}" ${selected.has(d.id) ? "checked" : ""} />
                <span class="min-w-0 flex-1 truncate text-[11px]">${escHtml(d.file_name)}</span>
                <span class="text-[10px] text-ink-400">${d.finding_count}</span>
              </label>`,
            )
            .join("")}
        </div>
      </div>
      <div class="space-y-4 p-4">
        <div>
          <div class="mb-2 flex items-center justify-between gap-3">
            <span class="text-[11px] font-medium text-ink-600">导出格式</span>
            <label class="flex cursor-pointer items-center gap-1.5 text-[11px] text-ink-600">
              <input id="export-native" type="checkbox" class="accent-ink-700" checked />
              仅导出原格式（推荐）
            </label>
          </div>
          <div id="export-format-grid" class="hidden grid-cols-3 gap-2">
            <label class="flex cursor-pointer items-center gap-2 rounded-lg border border-ink-900/10 px-2.5 py-2 text-[11px]">
              <input type="checkbox" class="export-format accent-ink-700" value="txt" /> TXT 纯文本
            </label>
            <label class="flex cursor-pointer items-center gap-2 rounded-lg border border-ink-900/10 px-2.5 py-2 text-[11px]">
              <input type="checkbox" class="export-format accent-ink-700" value="docx" /> Word
            </label>
            <label class="flex cursor-pointer items-center gap-2 rounded-lg border border-ink-900/10 px-2.5 py-2 text-[11px]">
              <input id="export-pdf" type="checkbox" class="export-format accent-ink-700" value="pdf" ${pickedAllPdf() ? "" : "disabled"} /> 脱敏 PDF
            </label>
          </div>
          <p id="export-native-hint" class="field-hint">
            默认只导出原格式：PDF → PDF，Word → Word，TXT → TXT；RTF / OFD / 旧版 Office 转 Word。
          </p>
          <p id="export-format-hint" class="field-hint hidden">
            手动模式下，所选格式会应用到所有文书；PDF 只能处理 PDF 源文件。
          </p>
        </div>

        <div>
          <div class="mb-2 text-[11px] font-medium text-ink-600">输出目录</div>
          <div class="flex gap-2">
            <input id="export-dir" readonly value="${escHtml(outDir)}" placeholder="请选择本地文件夹"
              class="input min-w-0 flex-1 text-[11px]" />
            <button id="pick-dir" class="btn-ghost shrink-0">选择…</button>
          </div>
        </div>

        <label class="flex items-center gap-2 text-[11px] text-ink-600">
          <input id="export-audit" type="checkbox" checked class="accent-ink-700" />
          同时生成合规脱敏日志（JSON + TXT，含原文片段与脱敏方式，用于卷宗留痕）
        </label>

        <div class="rounded-lg bg-ink-50 px-3 py-2 text-[10px] leading-relaxed text-ink-500">
          同名文件不会覆盖：自动追加序号。原始文书始终保持不变。
        </div>
      </div>
    </div>`,
    `<span id="export-hint" class="mr-auto text-[10px] text-ink-400"></span>
     <button class="btn-ghost" data-close>取消</button>
     <button class="btn-primary" id="export-start">开始导出</button>`,
    "max-w-[820px]",
  );

  const dirInput = handle.body.querySelector<HTMLInputElement>("#export-dir")!;
  const startBtn = handle.back.querySelector<HTMLButtonElement>("#export-start")!;
  const pdfBox = handle.body.querySelector<HTMLInputElement>("#export-pdf")!;
  const nativeBox = handle.body.querySelector<HTMLInputElement>("#export-native")!;
  const formatGrid = handle.body.querySelector<HTMLElement>("#export-format-grid")!;
  const nativeHint = handle.body.querySelector<HTMLElement>("#export-native-hint")!;
  const formatHint = handle.body.querySelector<HTMLElement>("#export-format-hint")!;
  const syncPdf = () => {
    const ids = Array.from(
      handle.body.querySelectorAll<HTMLInputElement>(".export-doc:checked"),
    ).map((el) => el.value);
    const picked = deps.docs.filter((d) => ids.includes(d.id));
    const ok = picked.length > 0 && picked.every((d) => d.kind.startsWith("pdf"));
    pdfBox.disabled = !ok;
    if (!ok) pdfBox.checked = false;
  };
  const syncNative = () => {
    const native = nativeBox.checked;
    formatGrid.classList.toggle("hidden", native);
    formatGrid.classList.toggle("grid", !native);
    nativeHint.classList.toggle("hidden", !native);
    formatHint.classList.toggle("hidden", native);
    if (!native) syncPdf();
  };
  nativeBox.addEventListener("change", syncNative);
  handle.body.querySelectorAll<HTMLInputElement>(".export-doc").forEach((cb) => {
    cb.addEventListener("change", syncPdf);
  });
  syncNative();

  handle.body.querySelector("#pick-dir")?.addEventListener("click", async () => {
    const dir = await deps.pickDir();
    if (dir) {
      outDir = dir;
      dirInput.value = dir;
    }
  });

  startBtn.addEventListener("click", async () => {
    const ids = Array.from(
      handle.body.querySelectorAll<HTMLInputElement>(".export-doc:checked"),
    ).map((el) => el.value);
    const nativeOnly = nativeBox.checked;
    const formats = Array.from(
      handle.body.querySelectorAll<HTMLInputElement>(".export-format:checked:not(:disabled)"),
    ).map((el) => el.value);
    if (ids.length === 0) {
      toast("请选择至少一份文书", "warn");
      return;
    }
    if (!outDir.trim()) {
      toast("请选择输出目录", "warn");
      return;
    }
    if (!nativeOnly && formats.length === 0) {
      toast("请选择至少一种格式，或勾选「仅导出原格式」", "warn");
      return;
    }
    const audit = handle.body.querySelector<HTMLInputElement>("#export-audit")?.checked ?? true;
    startBtn.disabled = true;
    setBusy("正在生成脱敏文书…");
    try {
      const results = await api.exportDocuments({
        doc_ids: ids,
        out_dir: outDir,
        formats,
        native_only: nativeOnly,
        write_audit: audit,
      });
      setBusy(null);
      renderResults(handle.body, results, outDir);
      handle.back.querySelector("#export-start")?.remove();
      deps.onFinished();
    } catch (e) {
      setBusy(null);
      startBtn.disabled = false;
      toast(String(e), "err");
    }
  });
}

function renderResults(container: HTMLElement, results: ExportOutcome[], outDir: string): void {
  const ok = results.filter((r) => !r.error);
  const failed = results.filter((r) => r.error);
  container.innerHTML = `<div class="p-5">
    <div class="mb-3 flex items-center gap-2">
      <span class="text-[13px] font-medium text-ink-800">导出完成</span>
      <span class="rounded-full bg-emerald-50 px-2 py-0.5 text-[10px] text-emerald-700">成功 ${ok.length}</span>
      ${failed.length ? `<span class="rounded-full bg-bronze-50 px-2 py-0.5 text-[10px] text-bronze-600">失败 ${failed.length}</span>` : ""}
      <button class="btn-ghost ml-auto" id="res-open-dir">打开导出目录</button>
    </div>
    <div class="space-y-2">
      ${results
        .map(
          (r) => `<div class="rounded-lg border border-ink-900/10 p-3">
            <div class="flex items-center gap-2">
              <span class="truncate text-[12px] font-medium">${escHtml(r.file_name || r.doc_id)}</span>
              <span class="ml-auto text-[10px] text-ink-400">${r.total} 条脱敏</span>
            </div>
            ${
              r.error
                ? `<p class="mt-1 text-[11px] text-bronze-500">${escHtml(r.error)}</p>`
                : `<div class="mt-2 space-y-1">${r.files
                    .map(
                      (f) => `<div class="flex items-center gap-2 text-[11px]">
                        <span class="w-20 shrink-0 text-ink-400">${escHtml(f.label)}</span>
                        <span class="min-w-0 flex-1 truncate font-mono text-[10px] text-ink-600" title="${escHtml(f.path)}">${escHtml(f.path)}</span>
                        <span class="shrink-0 text-[10px] text-ink-400">${formatBytes(f.bytes)}</span>
                        <button class="shrink-0 text-[10px] text-ink-500 hover:text-ink-800" data-open="${escHtml(f.path)}">打开</button>
                      </div>`,
                    )
                    .join("")}</div>`
            }
            ${r.warnings.length ? `<p class="mt-1 text-[10px] text-amber-600">${r.warnings.map(escHtml).join("；")}</p>` : ""}
          </div>`,
        )
        .join("")}
    </div>
  </div>`;

  container.querySelector("#res-open-dir")?.addEventListener("click", () => {
    api.openPath(outDir).catch((e) => toast(String(e), "err"));
  });
  container.querySelectorAll<HTMLElement>("[data-open]").forEach((btn) => {
    btn.addEventListener("click", () => {
      api.openPath(btn.dataset.open!).catch((e) => toast(String(e), "err"));
    });
  });
}
