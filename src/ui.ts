// 通用 UI：弹窗、确认框、Toast、忙碌遮罩
// 弹窗栈：Esc 只关闭最上层，避免“确认删除”时把规则面板一起关掉
const modalStack: HTMLElement[] = [];

export interface ModalHandle {
  back: HTMLElement;
  modal: HTMLElement;
  body: HTMLElement;
  close: () => void;
}

export function openModal(
  title: string,
  bodyHtml: string,
  footerHtml: string,
  widthClass = "max-w-3xl",
): ModalHandle {
  const root = document.getElementById("modal-root")!;
  const back = document.createElement("div");
  back.className = "modal-backdrop";
  back.innerHTML = `
    <div class="modal ${widthClass}" role="dialog" aria-modal="true">
      <div class="modal-head">
        <span class="modal-title">${title}</span>
        <button class="ml-auto rounded-md px-2 py-1 text-ink-400 hover:bg-ink-50 hover:text-ink-700" data-close>✕</button>
      </div>
      <div class="modal-body"></div>
      <div class="modal-foot">${footerHtml}</div>
    </div>`;
  const body = back.querySelector<HTMLElement>(".modal-body")!;
  body.innerHTML = bodyHtml;
  root.append(back);
  modalStack.push(back);

  const close = () => {
    const idx = modalStack.indexOf(back);
    if (idx >= 0) modalStack.splice(idx, 1);
    back.remove();
    document.removeEventListener("keydown", onKey);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape" && modalStack[modalStack.length - 1] === back) close();
  };
  document.addEventListener("keydown", onKey);
  back.addEventListener("mousedown", (e) => {
    if (e.target === back) close();
  });
  back.querySelectorAll("[data-close]").forEach((b) => b.addEventListener("click", close));
  return { back, modal: back.querySelector<HTMLElement>(".modal")!, body, close };
}

export function confirmDialog(
  title: string,
  message: string,
  confirmText = "确认",
  danger = false,
): Promise<boolean> {
  return new Promise((resolve) => {
    // 用 textContent 渲染提示语，避免文档里的恶意文件名/片段被当成 HTML
    const handle = openModal(
      title,
      `<p class="px-5 py-4 text-xs leading-relaxed text-ink-600" data-confirm-message></p>`,
      `<button class="btn-ghost" data-no>取消</button>
       <button class="${danger ? "btn-primary !bg-bronze-500 hover:!bg-bronze-600" : "btn-primary"}" data-yes>${confirmText}</button>`,
      "max-w-md",
    );
    const msg = handle.body.querySelector<HTMLElement>("[data-confirm-message]");
    if (msg) msg.textContent = message;
    handle.back.querySelector("[data-no]")?.addEventListener("click", () => {
      handle.close();
      resolve(false);
    });
    handle.back.querySelector("[data-yes]")?.addEventListener("click", () => {
      handle.close();
      resolve(true);
    });
    handle.back.addEventListener("mousedown", (e) => {
      if (e.target === handle.back) resolve(false);
    });
  });
}

export function toast(message: string, type: "ok" | "warn" | "err" = "ok"): void {
  const root = document.getElementById("toast-root")!;
  const el = document.createElement("div");
  const color =
    type === "ok"
      ? "border-ink-800/10 bg-white text-ink-700"
      : type === "warn"
        ? "border-amber-300 bg-amber-50 text-amber-800"
        : "border-bronze-500/30 bg-bronze-50 text-bronze-600";
  el.className = `pointer-events-auto flex max-w-sm items-start gap-2 rounded-lg border px-3 py-2 text-[11px] leading-relaxed shadow-pop ${color}`;
  const icon = document.createElement("span");
  icon.className = "mt-0.5";
  icon.textContent = type === "ok" ? "✓" : type === "warn" ? "!" : "✕";
  const text = document.createElement("span");
  text.className = "flex-1";
  text.textContent = message;
  el.append(icon, text);
  root.append(el);
  window.setTimeout(() => {
    el.style.transition = "opacity .25s";
    el.style.opacity = "0";
    window.setTimeout(() => el.remove(), 260);
  }, 3200);
}

let busyCount = 0;

/** 不改变计数，只更新忙碌提示文字（用于长任务中途提示） */
export function updateBusyText(text: string): void {
  const el = document.getElementById("busy-text");
  if (el) el.textContent = text;
}

export function setBusy(text: string | null): void {
  const el = document.getElementById("busy")!;
  if (text === null) {
    busyCount = Math.max(0, busyCount - 1);
    if (busyCount === 0) {
      el.classList.add("hidden");
      hideBusyProgress();
    }
    return;
  }
  busyCount += 1;
  // 新任务开始时清掉上一轮 OCR 进度，避免旧进度条残留
  hideBusyProgress();
  document.getElementById("busy-text")!.textContent = text;
  el.classList.remove("hidden");
}

/** 更新忙碌遮罩里的 OCR 进度条 */
export function setBusyProgress(current: number, total: number, label: string): void {
  if (busyCount === 0) return;
  const box = document.getElementById("busy-progress");
  const bar = document.getElementById("busy-progress-bar");
  const text = document.getElementById("busy-progress-label");
  const pctEl = document.getElementById("busy-progress-pct");
  if (!box || !bar || !text) return;
  const pct = total > 0 ? Math.min(100, Math.max(0, Math.round((current / total) * 100))) : 0;
  box.classList.remove("hidden");
  bar.style.width = `${pct}%`;
  text.textContent = label;
  text.title = label;
  if (pctEl) pctEl.textContent = `${pct}%`;
}

export function hideBusyProgress(): void {
  const box = document.getElementById("busy-progress");
  const bar = document.getElementById("busy-progress-bar");
  if (!box || !bar) return;
  box.classList.add("hidden");
  bar.style.width = "0%";
  const text = document.getElementById("busy-progress-label");
  const pct = document.getElementById("busy-progress-pct");
  if (text) text.textContent = "";
  if (pct) pct.textContent = "";
}

export function escHtml(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/// 复制文本：优先 Clipboard API，失败时退回隐藏 textarea + execCommand（兼容旧 WebView）
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // 某些 WebView 在非安全上下文下没有 clipboard 权限
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.setAttribute("readonly", "");
    ta.style.position = "fixed";
    ta.style.left = "-9999px";
    document.body.append(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  } catch {
    return false;
  }
}
