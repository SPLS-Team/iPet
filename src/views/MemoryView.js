import { escapeHtml } from "../utils/markdown.js";
import { icon } from "../ui/icons.js";

/**
 * MemoryView — long-term memory management (ref-plan §memory, Tier 1).
 *
 * Memories persist cross-session and are surfaced to the model two ways:
 *   1. A recent slice is injected into the system prompt every turn (backend).
 *   2. The model calls memory_save / memory_search tools on demand.
 * This view is the human-readable surface: users see what the model remembered,
 * edit content/category, and delete anything they don't want kept. Transparency
 * matters — there's no opaque "black-box" memory the user can't inspect.
 */
export function renderMemoryView(container, state, handlers) {
  const memories = state.memories ?? [];

  container.innerHTML = `
    <div class="settings-page memory-page">
      <section class="tool-toolbar memory-toolbar" aria-label="记忆库">
        <div class="tool-toolbar-copy">
          <h3>长期记忆</h3>
          <p>跨会话持久的事实与偏好。模型在对话中主动保存，并注入系统提示词辅助后续回答。</p>
        </div>
        <button class="text-button" data-role="refresh-memories" type="button">${icon("refresh", { size: 16 })}<span>刷新</span></button>
      </section>

      <div class="section-heading-row">
        <h3 class="section-title">已保存记忆</h3>
        <span class="section-count">${memories.length}</span>
      </div>

      ${memories.length === 0 ? renderEmpty() : `<div class="memory-list">${memories.map(renderMemoryCard).join("")}</div>`}

      <div class="inline-status" aria-live="polite">${escapeHtml(state.memoryStatus || "")}</div>
    </div>
  `;

  bindMemory(container, state, handlers);
}

function renderEmpty() {
  return `
    <div class="empty-state memory-empty">
      <strong>还没有记忆</strong>
      <span>模型在对话中觉得值得长期记住的用户偏好或事实时，会调用记忆工具保存。你也可以等待模型主动记忆。</span>
    </div>
  `;
}

function renderMemoryCard(memory) {
  const used = memory.useCount > 0
    ? `被检索 ${memory.useCount} 次`
    : "未被检索";
  const updated = memory.updatedAt || memory.createdAt || "";
  return `
    <article class="memory-card" data-id="${memory.id}">
      <div class="memory-head">
        <div class="memory-title">
          <code>${escapeHtml(memory.key)}</code>
          <span class="badge badge-builtin">${escapeHtml(memory.category || "general")}</span>
        </div>
        <div class="memory-actions">
          <button class="text-button" type="button" data-memory-edit="${memory.id}">${icon("edit", { size: 16 })}<span>编辑</span></button>
          <button class="text-button danger" type="button" data-memory-delete="${memory.id}">${icon("trash", { size: 16 })}<span>删除</span></button>
        </div>
      </div>
      <p class="memory-content" data-role="memory-content-${memory.id}">${escapeHtml(memory.content)}</p>
      <div class="memory-meta">
        <span>${escapeHtml(used)}</span>
        ${updated ? `<span>更新于 ${escapeHtml(updated.slice(0, 16).replace("T", " "))}</span>` : ""}
      </div>
    </article>
  `;
}

function bindMemory(container, state, handlers) {
  const refresh = container.querySelector('[data-role="refresh-memories"]');
  if (refresh) refresh.addEventListener("click", () => handlers.onRefreshMemories?.());

  container.querySelectorAll("[data-memory-delete]").forEach((button) => {
    button.addEventListener("click", () => {
      const id = Number(button.dataset.memoryDelete);
      const memory = (state.memories ?? []).find((m) => m.id === id);
      handlers.onDeleteMemory?.(id, memory?.key);
    });
  });

  container.querySelectorAll("[data-memory-edit]").forEach((button) => {
    button.addEventListener("click", () => {
      const id = Number(button.dataset.memoryEdit);
      handlers.onEditMemory?.(id);
    });
  });
}
