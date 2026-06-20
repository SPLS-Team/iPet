import { escapeHtml } from "../utils/markdown.js";
import { escapeAttr } from "../views/shared.js";

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
            ${d.input != null ? `<input class="dialog-input" type="text" value="${escapeAttr(d.input)}" data-role="dialog-input" ${d.inputPlaceholder ? `placeholder="${escapeAttr(d.inputPlaceholder)}"` : ""} />` : ""}
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
    const input = overlay.querySelector('[data-role="dialog-input"]');
    cancel?.addEventListener("click", () => closeDialog(false));
    confirm?.addEventListener("click", () => closeDialog(true));
    // Enter in the input confirms; Escape cancels — matches native prompt UX.
    input?.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        closeDialog(true);
      } else if (event.key === "Escape") {
        event.preventDefault();
        closeDialog(false);
      }
    });
    scrim.addEventListener("mousedown", (event) => {
      if (event.target === scrim && !state.dialog?.danger) closeDialog(false);
    });

    const dialogEl = scrim.querySelector(".dialog");
    if (!dialogEl) return;
    // Focus the input for an input dialog, else the confirm button.
    if (input) {
      input.focus();
      input.select();
    } else {
      confirm?.focus();
    }
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

  /// Input dialog: resolves to the trimmed string on confirm, or null on
  /// cancel. Same dialog chrome as confirmDialog (focus trap, scrim) plus a
  /// text field — replaces window.prompt so we keep the custom UI system.
  function promptDialog(config) {
    return new Promise((resolve) => {
      state.dialog = {
        ...config,
        input: config.value ?? "",
        inputPlaceholder: config.placeholder ?? "",
        _resolve: (confirmed) => {
          if (!confirmed) return resolve(null);
          const inputEl = document.querySelector('[data-role="dialog-input"]');
          resolve((inputEl?.value ?? "").trim());
        },
      };
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
    promptDialog,
    closeDialog,
    showToast,
  };
}
