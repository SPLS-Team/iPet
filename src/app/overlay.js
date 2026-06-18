import { escapeHtml } from "../utils/markdown.js";

export function createOverlayController(state) {
  function renderOverlay() {
    const overlay = document.querySelector("#overlay");
    if (!overlay) return;
    if (!state.dialog && !state.toast) {
      overlay.innerHTML = "";
      return;
    }

    let html = "";
    if (state.dialog) {
      const d = state.dialog;
      const confirmClass = d.danger ? "text-button danger" : "text-button primary";
      html += `
        <div class="scrim" data-role="scrim">
          <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="dialog-title">
            <h3 class="dialog-title" id="dialog-title">${escapeHtml(d.title)}</h3>
            ${d.body ? `<p class="dialog-body">${d.body}</p>` : ""}
            <div class="dialog-actions">
              <button class="text-button" type="button" data-role="dialog-cancel">${escapeHtml(d.cancelLabel || "取消")}</button>
              <button class="${confirmClass}" type="button" data-role="dialog-confirm">${escapeHtml(d.confirmLabel || "确认")}</button>
            </div>
          </div>
        </div>
      `;
    }
    if (state.toast) {
      html += `<div class="toast" data-tone="${state.toast.tone || "default"}" role="status">${escapeHtml(state.toast.message)}</div>`;
    }
    overlay.innerHTML = html;

    const scrim = overlay.querySelector('[data-role="scrim"]');
    if (!scrim) return;

    const cancel = overlay.querySelector('[data-role="dialog-cancel"]');
    const confirm = overlay.querySelector('[data-role="dialog-confirm"]');
    cancel?.addEventListener("click", () => closeDialog(false));
    confirm?.addEventListener("click", () => closeDialog(true));
    scrim.addEventListener("mousedown", (event) => {
      if (event.target === scrim && !state.dialog?.danger) closeDialog(false);
    });

    const dialogEl = scrim.querySelector(".dialog");
    if (!dialogEl) return;
    confirm?.focus();
    scrim.addEventListener("keydown", (event) => {
      if (event.key !== "Tab") return;
      const focusable = dialogEl.querySelectorAll(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    });
  }

  function confirmDialog(config) {
    return new Promise((resolve) => {
      state.dialog = { ...config, _resolve: resolve };
      renderOverlay();
    });
  }

  function closeDialog(confirmed) {
    const dialog = state.dialog;
    if (!dialog) return;
    state.dialog = null;
    renderOverlay();
    dialog._resolve?.(confirmed);
  }

  function showToast(message, tone = "default") {
    state.toast = { message, tone };
    if (state.toastTimer) window.clearTimeout(state.toastTimer);
    state.toastTimer = window.setTimeout(() => {
      state.toast = null;
      state.toastTimer = null;
      renderOverlay();
    }, 3000);
    renderOverlay();
  }

  return {
    renderOverlay,
    confirmDialog,
    closeDialog,
    showToast,
  };
}
