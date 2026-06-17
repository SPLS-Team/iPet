import { escapeHtml, renderMarkdown } from "../../markdown.js";

export function renderChat(container, state, handlers) {
  const messages = state.messages
    .map(renderMessage)
    .join("");

  const thinkingText = state.thinkingStartedAt ? `思考 ${formatElapsed(state.thinkingElapsedMs)}` : "";

  container.innerHTML = `
    <section class="chat-panel">
      <div class="message-list" data-role="messages">
        ${messages || '<div class="empty-state">输入一句话开始对话</div>'}
      </div>
      <form class="chat-form" data-role="form">
        <textarea
          name="prompt"
          rows="2"
          placeholder="问 iPet..."
          ${state.chatBusy ? "disabled" : ""}
        ></textarea>
        <button class="icon-button primary" type="submit" ${state.chatBusy ? "disabled" : ""} title="发送">
          <span>发送</span>
        </button>
      </form>
      <div class="inline-status chat-status">
        <span data-role="chat-status-text">${escapeHtml(state.chatStatus || "")}</span>
        <span class="thinking-clock" data-role="thinking-clock" ${state.thinkingStartedAt ? "" : "hidden"}>${escapeHtml(thinkingText)}</span>
      </div>
    </section>
  `;

  container.querySelector('[data-role="form"]').addEventListener("submit", (event) => {
    event.preventDefault();
    const input = event.currentTarget.elements.prompt;
    const value = input.value.trim();
    if (!value) return;
    input.value = "";
    handlers.onSend(value);
  });

  const messageList = container.querySelector('[data-role="messages"]');
  messageList.scrollTop = messageList.scrollHeight;
}

function renderMessage(message) {
  const role = message.role === "user" ? "user" : "assistant";
  const label = role === "user" ? "你" : "iPet";
  const avatar = role === "user" ? "你" : "iP";
  const content = role === "assistant" ? renderMarkdown(message.content) : escapeHtml(message.content);

  return `
    <div class="message-row message-row-${role}">
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

function formatElapsed(ms) {
  const totalSeconds = Math.max(0, Math.floor((Number(ms) || 0) / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}
