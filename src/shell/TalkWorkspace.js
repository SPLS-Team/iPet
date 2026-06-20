import { escapeHtml } from "../utils/markdown.js";
import { talkActivityText } from "./WindowChrome.js";

/**
 * TalkWorkspace — the default expanded view (ref-plan §3.2, §5.2). It owns the
 * talk header (a small pet avatar + current state) and the conversation canvas
 * mount point. The actual message list, composer and status strip are still
 * rendered by the legacy `renderChat` into `#panel` for Phase 1 — they get
 * rebuilt as ChatView in Phase 2.
 */

export function renderTalkWorkspace(ctx) {
  const { state } = ctx;
  return `
    <section class="talk-header">
      <div class="talk-avatar" aria-hidden="true">
        <span class="talk-avatar-mark"></span>
      </div>
      <div class="talk-meta">
        <strong>iPet</strong>
        <span class="talk-state" data-role="talk-state" aria-live="polite">${escapeHtml(talkActivityText(state))}</span>
      </div>
      <div class="session-switcher" role="group" aria-label="会话">
        <select class="session-select" data-role="session-select" title="切换会话" aria-label="切换会话">
          ${(state.sessions ?? [])
            .map(
              (s) =>
                `<option value="${s.id}" ${s.id === state.currentSessionId ? "selected" : ""}>${escapeHtml(s.title)}</option>`,
            )
            .join("")}
        </select>
        <button class="session-new" type="button" data-role="session-new" title="新建会话" aria-label="新建会话">+</button>
      </div>
    </section>
    <section id="panel" class="panel" data-role="talk-panel"></section>
  `;
}

export function bindTalkWorkspace(ctx) {
  const { state, handlers } = ctx;
  const select = document.querySelector('[data-role="session-select"]');
  if (select) {
    select.addEventListener("change", () => {
      handlers.onSwitchSession?.(Number(select.value));
    });
  }
  const newBtn = document.querySelector('[data-role="session-new"]');
  if (newBtn) {
    newBtn.addEventListener("click", () => handlers.onNewSession?.());
  }
}
