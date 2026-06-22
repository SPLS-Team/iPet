import { escapeHtml, renderMarkdown } from "../../utils/markdown.js";
import { icon } from "../../ui/icons.js";

export function renderChat(container, state, handlers) {
  const messages = state.messages
    .map((message, idx) => renderMessage(message, idx === state.messages.length - 1))
    .join("");

  const thinkingText = state.thinkingStartedAt ? `思考 ${formatElapsed(state.thinkingElapsedMs)}` : "";
  const showToolChip = Boolean(state.chatBusy && state.toolActivity);

  container.innerHTML = `
    <section class="chat-panel">
      <div class="message-list" data-role="messages">
        ${messages || renderEmptyState(handlers)}
      </div>
      <form class="chat-form composer-card" data-role="form" aria-label="发送消息">
        <textarea
          name="prompt"
          rows="2"
          placeholder="输入消息，Enter 发送，Shift+Enter 换行"
          ${state.chatBusy ? "disabled" : ""}
        ></textarea>
        ${state.chatBusy
          ? `<button class="icon-button stop" type="button" data-role="stop" title="停止" aria-label="停止">${icon("close", { label: "停止" })}</button>`
          : `<button class="icon-button primary" type="submit" title="发送" aria-label="发送">${icon("send", { label: "发送" })}</button>`}
      </form>
      <div class="inline-status chat-status" aria-live="polite">
        <span class="status-text" data-role="chat-status-text">${escapeHtml(state.chatStatus || "")}</span>
        <span class="status-chips">
          <span class="tool-chip" data-role="tool-chip" ${showToolChip ? "" : "hidden"} title="${escapeAttr(state.toolActivity || "")}">${escapeHtml(state.toolActivity || "")}</span>
          <span class="thinking-clock" data-role="thinking-clock" ${state.thinkingStartedAt ? "" : "hidden"}>${escapeHtml(thinkingText)}</span>
          ${tokenHint(state)}
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

  // Stop button: locally cancel the active stream (ui-plan §8.5).
  const stopBtn = form.querySelector('[data-role="stop"]');
  if (stopBtn) {
    stopBtn.addEventListener("click", () => handlers.onStop?.());
  }

  function submitPrompt(formEl) {
    const input = formEl.elements.prompt;
    const value = input.value.trim();
    if (!value) return;
    input.value = "";
    handlers.onSend(value);
  }

  const messageList = container.querySelector('[data-role="messages"]');
  messageList.scrollTop = messageList.scrollHeight;

  // Wire empty-state quick chips (ui-plan §8.7).
  const chipActions = [
    () => handlers.onSend?.("请检查一下当前的系统状态。"),
    () => handlers.onGoSettings?.("tools"),
    () => handlers.onGoSettings?.("stats"),
  ];
  container.querySelectorAll("[data-empty-chip]").forEach((button) => {
    button.addEventListener("click", () => {
      const idx = Number(button.dataset.emptyChip);
      chipActions[idx]?.();
    });
  });
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
  // Typed messages (tool-event / system-event / error) are not streaming
  // assistant text — leave them to a full render (ref-plan §12.3).
  if (last.type) return false;

  const lastBubble = messageList.querySelector(".message-row:last-child .message-text");
  if (!lastBubble) return false;

  lastBubble.innerHTML = renderMarkdown(last.content);

  // Patch the reasoning-chain disclosure in place so live-streaming thinking
  // tokens show without a full re-render. Reuse the user's open/closed state.
  const messageEl = messageList.querySelector(".message-row:last-child .message");
  if (!messageEl) return false;
  const existing = messageEl.querySelector('[data-role="reasoning-chain"]');
  const hasReasoning = Boolean(last.reasoning && last.reasoning.trim());
  if (hasReasoning) {
    const isOpen = existing ? existing.open : false;
    const html = `<details class="reasoning-chain" data-role="reasoning-chain" ${isOpen ? "open" : ""}>
        <summary>${icon("search", { size: 12 })}<span>思考链</span></summary>
        <div class="reasoning-body markdown-body">${renderMarkdown(last.reasoning)}</div>
      </details>`;
    if (existing) {
      existing.outerHTML = html;
    } else {
      messageEl.insertAdjacentHTML("afterbegin", html);
    }
  } else if (existing) {
    existing.remove();
  }

  messageList.scrollTop = messageList.scrollHeight;
  return true;
}

function renderEmptyState(handlers) {
  // Quick-start chips wired to real behavior (ui-plan §8.7): no fake buttons —
  // each either sends a real prompt or navigates to a real panel.
  const chips = [
    { label: "检查系统状态", action: () => handlers.onSend?.("请检查一下当前的系统状态。") },
    { label: "查看工具", action: () => handlers.onGoSettings?.("tools") },
    { label: "查看 token 使用", action: () => handlers.onGoSettings?.("stats") },
  ];
  const chipsHtml = handlers.onSend
    ? `<div class="empty-chips">${chips
        .map(
          (chip, idx) =>
            `<button class="empty-chip" type="button" data-empty-chip="${idx}">${escapeHtml(chip.label)}</button>`,
        )
        .join("")}</div>`
    : "";
  return `
    <div class="empty-state" data-role="empty-state">
      <strong>iPet 在这里</strong>
      <span>问它问题，或让它检查系统状态。</span>
      ${chipsHtml}
    </div>
  `;
}

function renderMessage(message, _isLast) {
  // ref-plan §12.3: a message carries an optional `type` (tool-event /
  // system-event / error) in addition to its role. Events render as compact
  // inline timeline cards, not as full chat bubbles, so they don't compete
  // with the conversation.
  const type = message.type;
  if (type === "tool-event") return renderEventMessage(message, "tool");
  if (type === "system-event") return renderEventMessage(message, "system");
  if (type === "error") return renderErrorMessage(message);

  const role = message.role === "user" ? "user" : "assistant";
  const avatar = role === "user" ? "你" : "iP";
  const content = role === "assistant" ? renderMarkdown(message.content) : escapeHtml(message.content);
  // Per ui-plan §8.3: user messages carry no role label (right-align + blue
  // already convey authorship); assistant shows a small "iPet" caption.
  const roleLabel = role === "assistant" ? `<div class="message-role">iPet</div>` : "";
  // Collapsible reasoning/thinking chain. Only render the disclosure when the
  // assistant actually produced reasoning text; collapsed by default so it
  // doesn't crowd the answer, but one click reveals the full chain (rendered
  // as markdown so model-emitted lists/inline-code read naturally).
  const reasoning = role === "assistant" && message.reasoning && message.reasoning.trim()
    ? `<details class="reasoning-chain" data-role="reasoning-chain">
        <summary>${icon("search", { size: 12 })}<span>思考链</span></summary>
        <div class="reasoning-body markdown-body">${renderMarkdown(message.reasoning)}</div>
      </details>`
    : "";

  return `
    <div class="message-row message-row-${role}" data-role="message" data-message-role="${role}">
      ${role === "assistant" ? `<div class="message-avatar">${avatar}</div>` : ""}
      <div class="message message-${role}">
        ${roleLabel}
        ${reasoning}
        <div class="message-text ${role === "assistant" ? "markdown-body" : ""}">
          ${content}
        </div>
      </div>
    </div>
  `;
}

/** A one-line timeline event (tool call or system snapshot), ref-plan §12.3. */
function renderEventMessage(message, kind) {
  const label = kind === "tool" ? "工具" : "系统";
  const icon_ = kind === "tool" ? "tools" : "settings";
  return `
    <div class="message-row message-row-event" data-role="message" data-message-type="${kind}-event">
      <div class="event-card event-${kind}">
        ${icon(icon_, { size: 14, label: label })}
        <span class="event-label">${escapeHtml(label)}</span>
        <span class="event-text">${escapeHtml(message.content)}</span>
      </div>
    </div>
  `;
}

/** Error bubble — distinct red-tinted card so failures aren't lost in chat. */
function renderErrorMessage(message) {
  return `
    <div class="message-row message-row-error" data-role="message" data-message-type="error">
      <div class="message message-error">
        <div class="message-role">错误</div>
        <div class="message-text">${escapeHtml(message.content)}</div>
      </div>
    </div>
  `;
}

function escapeAttr(value) {
  return escapeHtml(value).replaceAll("`", "&#096;");
}

/** Status-strip token hint (ref-plan §5.2) — total tokens used so far. */
function tokenHint(state) {
  const total = Number(state.stats?.totalTokens ?? 0);
  if (!total) return "";
  return `<span class="token-hint" title="累计 token 用量">${formatNum(total)} tokens</span>`;
}

function formatNum(value) {
  return Number(value || 0).toLocaleString("en-US");
}

function formatElapsed(ms) {
  const totalSeconds = Math.max(0, Math.floor((Number(ms) || 0) / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}
