import { icon } from "../ui/icons.js";
import { escapeHtml } from "../utils/markdown.js";

/**
 * WindowChrome — the custom titlebar shared by the Talk Workspace and Control
 * Center views (the Companion Capsule has no chrome, ref-plan §5.1).
 *
 * Layout follows the platform: macOS keeps the window controls on the left
 * (traffic-light style, handled in CSS via `:root[data-platform="macos"]`),
 * Windows/Linux keep them on the right. The Control Center entry point lives
 * here per ref-plan §12.1 — it toggles between "open control" (in talk) and
 * "done / back to talk" (in control).
 */

export function renderWindowChrome(ctx) {
  const { state } = ctx;
  const inControl = state.viewMode === "control";
  const state_ = talkActivityText(state);

  return `
    <header class="titlebar" data-tauri-drag-region>
      <div class="brand" data-tauri-drag-region>
        <span class="brand-mark" aria-hidden="true"></span>
        <span data-tauri-drag-region>${inControl ? "控制中心" : "iPet"}</span>
      </div>
      <div class="chrome-activity" data-role="chrome-activity" aria-live="polite">${escapeHtml(state_)}</div>
      <div class="window-actions">
        <button class="window-button" data-chrome="control" title="${inControl ? "返回对话" : "控制中心"}" aria-label="${inControl ? "返回对话" : "控制中心"}">${icon(inControl ? "compact" : "settings", { label: inControl ? "返回对话" : "控制中心" })}</button>
        <button class="window-button ${state.alwaysOnTop ? "active" : ""}" data-chrome="pin" title="${state.alwaysOnTop ? "取消置顶" : "窗口置顶"}" aria-label="${state.alwaysOnTop ? "取消置顶" : "窗口置顶"}" aria-pressed="${state.alwaysOnTop ? "true" : "false"}">${icon("pin", { label: state.alwaysOnTop ? "取消置顶" : "窗口置顶" })}</button>
        <button class="window-button" data-chrome="compact" title="收起为小窗" aria-label="收起为小窗">${icon("compact", { label: "收起为小窗" })}</button>
        <button class="window-button" data-chrome="minimize" title="最小化" aria-label="最小化">${icon("minimize", { label: "最小化" })}</button>
        <button class="window-button danger" data-chrome="close" title="关闭" aria-label="关闭">${icon("close", { label: "关闭" })}</button>
      </div>
    </header>
  `;
}

export function bindWindowChrome(ctx) {
  const { handlers, appWindow } = ctx;
  const titlebar = document.querySelector(".titlebar");
  if (titlebar) {
    // Native `data-tauri-drag-region` handles most dragging; this JS fallback
    // mirrors the pre-refactor behavior so window dragging never regresses.
    titlebar.addEventListener("mousedown", (event) => {
      if (event.button !== 0 || event.target.closest("button")) return;
      appWindow.startDragging();
    });
  }
  document.querySelectorAll("[data-chrome]").forEach((button) => {
    button.addEventListener("click", () => {
      const action = button.dataset.chrome;
      if (action === "control") handlers.onToggleControl();
      else if (action === "pin") handlers.onToggleTop?.();
      else if (action === "compact") handlers.onCompact();
      else if (action === "minimize") handlers.onMinimize();
      else if (action === "close") handlers.onClose();
    });
  });
}

/** One-line summary of what iPet is doing right now, for the chrome + talk header. */
export function talkActivityText(state) {
  if (state.chatBusy) return state.toolActivity || state.chatStatus || "思考中";
  if (state.chatStatus === "已停止") return "已停止";
  if (state.settings?.hasApiKey) return "正在待命";
  return "等待 API Key";
}
