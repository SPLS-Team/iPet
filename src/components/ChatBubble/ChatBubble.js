import { escapeHtml, renderMarkdown } from "../../markdown.js";
import { icon } from "../../icons.js";

export function renderChat(container, state, handlers) {
  const messages = state.messages
    .map((message, idx) => renderMessage(message, idx === state.messages.length - 1))
    .join("");

  const thinkingText = state.thinkingStartedAt ? `思考 ${formatElapsed(state.thinkingElapsedMs)}` : "";
  const showToolChip = Boolean(state.chatBusy && state.toolActivity);

  container.innerHTML = `
    <section class="chat-panel">
      <div class="message-list" data-role="messages">
        ${messages || renderEmptyState()}
      </div>
      <form class="chat-form composer-card" data-role="form" aria-label="发送消息">
        <textarea
          name="prompt"
          rows="2"
          placeholder="输入消息，Enter 发送，Shift+Enter 换行"
          ${state.chatBusy ? "disabled" : ""}
        ></textarea>
        <button class="icon-button primary" type="submit" ${state.chatBusy ? "disabled" : ""} title="发送" aria-label="发送">
          ${icon("send", { label: "发送" })}
        </button>
      </form>
      <div class="inline-status chat-status" aria-live="polite">
        <span class="status-text" data-role="chat-status-text">${escapeHtml(state.chatStatus || "")}</span>
        <span class="status-chips">
          <span class="tool-chip" data-role="tool-chip" ${showToolChip ? "" : "hidden"} title="${escapeAttr(state.toolActivity || "")}">${escapeHtml(state.toolActivity || "")}</span>
          <span class="thinking-clock" data-role="thinking-clock" ${state.thinkingStartedAt ? "" : "hidden"}>${escapeHtml(thinkingText)}</span>
        </span>
      </div>
    </section>
  `;

  // Remember what we drew so updateChatStreaming() can decide whether the
  // fast-path patch is still valid (no new messages, no role swap).
  container.dataset.renderedCount = String(state.messages.length);

  const form = container.querySelector('[data-role="form"]');
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    submitPrompt(form);
  });

  // Enter sends, Shift+Enter (or any modifier) inserts a newline. IME
  // composition still uses Enter to commit candidates; isComposing skips that.
  const textarea = form.elements.prompt;
  textarea.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    if (event.shiftKey || event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.isComposing || event.keyCode === 229) return;
    event.preventDefault();
    submitPrompt(form);
  });

  function submitPrompt(formEl) {
    const input = formEl.elements.prompt;
    const value = input.value.trim();
    if (!value) return;
    input.value = "";
    handlers.onSend(value);
  }

  const messageList = container.querySelector('[data-role="messages"]');
  messageList.scrollTop = messageList.scrollHeight;
}

/// Streaming fast path: re-render only the last assistant bubble's content
/// instead of rebuilding the entire message list. Returns true if the patch
/// applied; false if the caller should fall back to a full renderChat() (e.g.
/// the chat panel isn't mounted, the message count changed, or the last
/// message isn't from the assistant).
export function updateChatStreaming(container, state) {
  if (!container) return false;
  const messageList = container.querySelector('[data-role="messages"]');
  if (!messageList) return false;

  const expectedCount = Number(container.dataset.renderedCount || 0);
  if (expectedCount !== state.messages.length) return false;

  const last = state.messages[state.messages.length - 1];
  if (!last || last.role !== "assistant") return false;

  const lastBubble = messageList.querySelector(".message-row:last-child .message-text");
  if (!lastBubble) return false;

  lastBubble.innerHTML = renderMarkdown(last.content);
  messageList.scrollTop = messageList.scrollHeight;
  return true;
}

function renderEmptyState() {
  return `
    <div class="empty-state">
      <strong>iPet 在这里</strong>
      <span>问它问题，或让它检查系统状态。</span>
    </div>
  `;
}

function renderMessage(message, _isLast) {
  const role = message.role === "user" ? "user" : "assistant";
  const label = role === "user" ? "你" : "iPet";
  const avatar = role === "user" ? "你" : "iP";
  const content = role === "assistant" ? renderMarkdown(message.content) : escapeHtml(message.content);

  return `
    <div class="message-row message-row-${role}" data-role="message" data-message-role="${role}">
      ${role === "assistant" ? `<div class="message-avatar">${avatar}</div>` : ""}
      <div class="message message-${role}">
        <div class="message-role">${label}</div>
        <div class="message-text ${role === "assistant" ? "markdown-body" : ""}">
          ${content}
        </div>
      </div>
      ${role === "user" ? `<div class="message-avatar message-avatar-user">${avatar}</div>` : ""}
    </div>
  `;
}

function escapeAttr(value) {
  return escapeHtml(value).replaceAll("`", "&#096;");
}

function formatElapsed(ms) {
  const totalSeconds = Math.max(0, Math.floor((Number(ms) || 0) / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}
