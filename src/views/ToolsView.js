import { escapeHtml } from "../utils/markdown.js";
import { icon } from "../ui/icons.js";
import { escapeAttr, DEFAULT_SCHEMA } from "./shared.js";

/**
 * ToolsView — the Tool Library (ref-plan §6.2, Phase 4). A searchable list of
 * tools (builtin / HTTP / local) with per-tool enable + inspector, plus an
 * "Add Tool" flow that picks a type (import / HTTP / local) before showing one
 * form — never three forms stacked at once. Local tools keep their risk callout
 * and confirm-on-save.
 */
export function renderToolsView(container, state, handlers) {
  const tools = state.tools ?? [];
  const q = (state.toolSearch || "").trim().toLowerCase();
  const visible = q
    ? tools.filter((t) =>
        [t.name, t.displayName, t.description].some((v) => String(v || "").toLowerCase().includes(q)),
      )
    : tools;

  container.innerHTML = `
    <div class="tool-page">
      ${renderToolSummary(tools)}

      <section class="tool-toolbar" aria-label="工具筛选">
        <div class="tool-toolbar-copy">
          <h3>筛选与管理</h3>
          <p>按名称或描述快速定位工具，再查看启用状态和调用 schema。</p>
        </div>
        <label class="tool-search">
          <span>搜索工具</span>
          <div class="field-with-icon">
            ${icon("search")}
            <input name="toolSearch" type="search" placeholder="按名称或描述过滤" value="${escapeAttr(state.toolSearch || "")}" />
          </div>
        </label>
      </section>

      <section class="tool-list-section">
        <div class="section-heading-row">
          <h3 class="section-title">已安装工具</h3>
          <span class="section-count">${visible.length}/${tools.length}</span>
        </div>
        <div class="tool-list-window" role="region" aria-label="已安装工具列表">
          <div class="tool-list">
            ${visible.map(renderToolCard).join("") || renderToolEmpty(q)}
          </div>
        </div>
      </section>

      <section class="section-block">
        <h3 class="section-title">添加工具</h3>
        <div class="segmented" role="tablist" aria-label="添加工具类型">
          ${composerTab("import", "导入", state)}
          ${composerTab("http", "HTTP", state)}
          ${composerTab("local", "本地", state)}
        </div>
        ${renderComposerForm(state)}
      </section>

      <details class="tool-reference">
        <summary>工具格式参考</summary>
        <div class="tool-format">
          <strong>调用约定</strong>
          <span>name 必须是函数名格式；parameters 是 JSON Schema object；HTTP：GET 转 query，其他作为 JSON body；Local：stdin 读一行 JSON，stdout 返回结果，非零退出/超时为错误；Builtin 由工具包和 Rust crate 提供。</span>
        </div>
      </details>
      <div class="inline-status" aria-live="polite">${escapeHtml(state.toolStatus || "")}</div>
    </div>
  `;

  bindTools(container, state, handlers);
}

function renderToolSummary(tools) {
  const enabled = tools.filter((t) => t.enabled).length;
  const builtin = tools.filter((t) => t.builtIn).length;
  const http = tools.filter((t) => t.http).length;
  const local = tools.filter((t) => t.local).length;
  return `
    <div class="tool-summary">
      <div class="metric-mini metric-mini-accent"><span>已启用</span><strong>${enabled}</strong></div>
      <div class="metric-mini"><span>内置</span><strong>${builtin}</strong></div>
      <div class="metric-mini"><span>HTTP</span><strong>${http}</strong></div>
      <div class="metric-mini"><span>本地</span><strong>${local}</strong></div>
    </div>
  `;
}

function renderToolEmpty(query) {
  return query
    ? '<div class="empty-state"><strong>未找到工具</strong><span>调整搜索关键词，或添加一个新工具。</span></div>'
    : '<div class="empty-state"><strong>暂无工具</strong><span>导入工具包，或添加 HTTP / 本地工具。</span></div>';
}

function toolKindLabel(tool) {
  if (tool.http) return "HTTP";
  if (tool.local) return "本地";
  return escapeHtml(tool.kind);
}

function renderToolRuntimeBadge(tool) {
  if (tool.http) {
    return `<span>${escapeHtml(tool.http.method)} ${escapeHtml(tool.http.url)}</span>`;
  }
  if (tool.local) {
    const cmd = [tool.local.command, ...(tool.local.args ?? [])].join(" ");
    return `<span class="inline-code">${escapeHtml(cmd)}</span>`;
  }
  return "";
}

function renderToolCard(tool) {
  const kindLabel = toolKindLabel(tool);
  const kindBadgeClass = tool.local ? "badge-local" : tool.builtIn ? "badge-builtin" : "";
  const enabledText = tool.enabled ? "已启用" : "停用";
  return `
    <article class="tool-card" data-kind="${escapeAttr(tool.kind)}">
      <div class="tool-head">
        <div class="tool-title">
          <strong>${escapeHtml(tool.displayName || tool.name)}</strong>
          <code>${escapeHtml(tool.name)}</code>
        </div>
        <label class="tool-toggle">
          <input type="checkbox" data-tool-enabled="${escapeAttr(tool.name)}" ${tool.enabled ? "checked" : ""} />
          <span>${enabledText}</span>
        </label>
      </div>
      <p>${escapeHtml(tool.description)}</p>
      <div class="tool-meta">
        <span class="${tool.builtIn ? "badge-builtin" : ""}">${tool.builtIn ? "内置" : "自定义"}</span>
        <span class="${kindBadgeClass}">${kindLabel}</span>
        ${renderToolRuntimeBadge(tool)}
      </div>
      ${tool.local ? `<div class="risk-callout">${icon("warning", { size: 16 })}<span>本地工具会在你的电脑上运行程序，请确认你信任该工具。</span></div>` : ""}
      <details class="tool-schema">
        <summary>工具 schema</summary>
        <pre>${escapeHtml(JSON.stringify(tool.parameters, null, 2))}</pre>
      </details>
      <div class="tool-actions">
        ${tool.builtIn ? "" : `<button class="text-button danger" data-delete-tool="${escapeAttr(tool.name)}" type="button">${icon("trash")} 删除工具</button>`}
      </div>
    </article>
  `;
}

function composerTab(mode, label, state) {
  const active = (state.toolComposerMode || "import") === mode;
  return `<button class="segmented-tab ${active ? "active" : ""}" type="button" data-composer-mode="${mode}" role="tab" aria-selected="${active ? "true" : "false"}" tabindex="${active ? "0" : "-1"}">${label}</button>`;
}

function renderComposerForm(state) {
  const mode = state.toolComposerMode || "import";
  if (mode === "import") return renderImportForm();
  if (mode === "local") return renderLocalForm();
  return renderHttpForm();
}

function renderImportForm() {
  return `
    <form class="tool-form" data-role="import-form">
      <p class="hint">
        指定一个目录或 <code>tool.json</code> 文件路径，从 schema v1
        工具包导入（结构参见 <code>docs/TOOL_PACKAGE.md</code>）。
      </p>
      <label>
        <span>路径</span>
        <input name="packagePath" placeholder="C:/.../my-tool" />
      </label>
      <button class="action-button" type="submit">${icon("upload")} 导入</button>
    </form>
  `;
}

function renderHttpForm() {
  return `
    <form class="tool-form" data-role="tool-form">
      <label>
        <span>工具名称</span>
        <input name="name" placeholder="search_docs" />
      </label>
      <label>
        <span>显示名</span>
        <input name="displayName" placeholder="搜索文档" />
      </label>
      <label>
        <span>描述</span>
        <textarea name="description" rows="3" placeholder="说明模型何时应该调用这个工具"></textarea>
      </label>
      <div class="settings-grid">
        <label>
          <span>Method</span>
          <select name="method">
            <option>POST</option>
            <option>GET</option>
            <option>PUT</option>
            <option>PATCH</option>
          </select>
        </label>
        <label>
          <span>启用</span>
          <select name="enabled">
            <option value="true">是</option>
            <option value="false">否</option>
          </select>
        </label>
      </div>
      <label>
        <span>URL</span>
        <input name="url" placeholder="https://example.com/tool" />
      </label>
      <label>
        <span>Headers JSON</span>
        <textarea name="headers" rows="2" placeholder='[{"key":"Authorization","value":"Bearer ..."}]'>[]</textarea>
      </label>
      <label>
        <span>Parameters JSON Schema</span>
        <textarea name="parameters" rows="8">${escapeHtml(DEFAULT_SCHEMA)}</textarea>
      </label>
      <div class="tool-format">
        <strong>工具格式</strong>
        <span>name 必须是函数名格式；parameters 必须是 JSON Schema object；HTTP 工具会把模型参数作为 JSON body 发送，GET 会转为 query。</span>
      </div>
      <button class="text-button primary" type="submit">${icon("check")} 添加或更新工具</button>
    </form>
  `;
}

function renderLocalForm() {
  return `
    <form class="tool-form" data-role="local-tool-form">
      <div class="risk-callout">${icon("warning", { size: 16 })} 本地工具可以启动你电脑上的可执行文件或脚本。请只添加你信任的工具。</div>
      <label>
        <span>工具名称</span>
        <input name="name" placeholder="my_local_tool" />
      </label>
      <label>
        <span>显示名</span>
        <input name="displayName" placeholder="本地工具" />
      </label>
      <label>
        <span>描述</span>
        <textarea name="description" rows="3" placeholder="说明模型何时应该调用这个工具"></textarea>
      </label>
      <label>
        <span>命令 (command)</span>
        <input name="command" placeholder="python / node / C:/tools/my_tool.exe" />
      </label>
      <label>
        <span>参数 (args，空格分隔，可选)</span>
        <input name="args" placeholder="script.py --flag" />
      </label>
      <label>
        <span>工作目录 (cwd，可选)</span>
        <input name="cwd" placeholder="C:/.../tool-dir" />
      </label>
      <div class="settings-grid">
        <label>
          <span>超时(秒)</span>
          <input name="timeoutSecs" type="number" min="1" max="300" value="30" />
        </label>
        <label>
          <span>启用</span>
          <select name="enabled">
            <option value="true">是</option>
            <option value="false">否</option>
          </select>
        </label>
      </div>
      <label>
        <span>Parameters JSON Schema</span>
        <textarea name="parameters" rows="6">${escapeHtml(DEFAULT_SCHEMA)}</textarea>
      </label>
      <div class="tool-format">
        <strong>stdio 协议</strong>
        <span>子进程从 stdin 读取一行 JSON（模型参数对象），把结果写到 stdout。非零退出码或超时会被当作错误。</span>
      </div>
      <button class="text-button primary" type="submit">${icon("check")} 添加或更新本地工具</button>
    </form>
  `;
}

function bindTools(container, state, handlers) {
  container.querySelectorAll("[data-tool-enabled]").forEach((input) => {
    input.addEventListener("change", () => {
      handlers.onSetToolEnabled(input.dataset.toolEnabled, input.checked);
    });
  });
  container.querySelectorAll("[data-delete-tool]").forEach((button) => {
    button.addEventListener("click", () => handlers.onDeleteTool(button.dataset.deleteTool));
  });

  container.querySelectorAll("[data-composer-mode]").forEach((button) => {
    button.addEventListener("click", () => handlers.onSetComposerMode(button.dataset.composerMode));
  });
  bindSegmentedKeyboard(container);

  const search = container.querySelector('[name="toolSearch"]');
  if (search) {
    search.addEventListener("input", () => {
      state.toolSearch = search.value;
      // Only the list reacts to search — re-render the view, not the whole app.
      const list = container.querySelector(".tool-list");
      if (!list) return;
      const q = search.value.trim().toLowerCase();
      const visible = q
        ? (state.tools ?? []).filter((t) =>
            [t.name, t.displayName, t.description].some((v) => String(v || "").toLowerCase().includes(q)),
          )
        : state.tools ?? [];
      const count = container.querySelector(".tool-list-section .section-count");
      if (count) count.textContent = `${visible.length}/${(state.tools ?? []).length}`;
      list.innerHTML = visible.map(renderToolCard).join("") || renderToolEmpty(q);
      // Re-bind the per-card controls that were just replaced.
      list.querySelectorAll("[data-tool-enabled]").forEach((input) => {
        input.addEventListener("change", () => handlers.onSetToolEnabled(input.dataset.toolEnabled, input.checked));
      });
      list.querySelectorAll("[data-delete-tool]").forEach((button) => {
        button.addEventListener("click", () => handlers.onDeleteTool(button.dataset.deleteTool));
      });
    });
  }

  container.querySelector('[data-role="tool-form"]')?.addEventListener("submit", (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    handlers.onSaveTool({
      name: form.elements.name.value.trim(),
      displayName: form.elements.displayName.value.trim(),
      description: form.elements.description.value.trim(),
      kind: "http",
      enabled: form.elements.enabled.value === "true",
      parametersRaw: form.elements.parameters.value,
      http: {
        method: form.elements.method.value,
        url: form.elements.url.value.trim(),
        headersRaw: form.elements.headers.value,
      },
    });
  });

  container.querySelector('[data-role="local-tool-form"]')?.addEventListener("submit", (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    const argsRaw = form.elements.args.value.trim();
    handlers.onSaveTool({
      name: form.elements.name.value.trim(),
      displayName: form.elements.displayName.value.trim(),
      description: form.elements.description.value.trim(),
      kind: "local",
      enabled: form.elements.enabled.value === "true",
      parametersRaw: form.elements.parameters.value,
      local: {
        command: form.elements.command.value.trim(),
        args: argsRaw ? argsRaw.split(/\s+/) : [],
        cwd: form.elements.cwd.value.trim() || null,
        timeoutSecs: Number.parseInt(form.elements.timeoutSecs.value, 10) || 30,
      },
    });
  });

  container.querySelector('[data-role="import-form"]')?.addEventListener("submit", (event) => {
    event.preventDefault();
    const path = event.currentTarget.elements.packagePath.value.trim();
    if (!path) return;
    handlers.onImportTool(path);
  });
}

function bindSegmentedKeyboard(container) {
  container.querySelectorAll('.segmented[role="tablist"]').forEach((tablist) => {
    tablist.addEventListener("keydown", (event) => {
      const keys = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"];
      if (!keys.includes(event.key)) return;

      const tabs = Array.from(tablist.querySelectorAll('[role="tab"]'));
      if (!tabs.length) return;

      event.preventDefault();
      const current = Math.max(
        0,
        tabs.indexOf(document.activeElement),
        tabs.findIndex((tab) => tab.getAttribute("aria-selected") === "true"),
      );
      const delta = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
      let next = current;
      if (event.key === "Home") next = 0;
      else if (event.key === "End") next = tabs.length - 1;
      else next = (current + delta + tabs.length) % tabs.length;

      tabs[next].click();
      const refocus = globalThis.requestAnimationFrame || ((callback) => setTimeout(callback, 0));
      refocus(() => {
        const mode = tabs[next].dataset.composerMode;
        container.querySelector(`[data-composer-mode="${mode}"]`)?.focus();
      });
    });
  });
}
