import { escapeHtml } from "../../markdown.js";

const DEFAULT_SCHEMA = `{
  "type": "object",
  "properties": {
    "query": {
      "type": "string",
      "description": "请求参数"
    }
  },
  "required": ["query"]
}`;

const SETTINGS_TAB_ICONS = {
  model: "◉",
  tools: "◇",
  stats: "▥",
};

export function renderSettings(container, state, handlers) {
  container.innerHTML = `
    <section class="settings-panel">
      <nav class="settings-tabs" aria-label="settings">
        ${tabButton("model", "模型", state.settingsTab)}
        ${tabButton("tools", "工具", state.settingsTab)}
        ${tabButton("stats", "统计", state.settingsTab)}
      </nav>
      <div class="settings-content">
        ${renderActiveSettingsTab(state)}
      </div>
      <div class="inline-status">${escapeHtml(state.settingsStatus || state.toolStatus || state.statsStatus || "")}</div>
    </section>
  `;

  container.querySelectorAll("[data-settings-tab]").forEach((button) => {
    button.addEventListener("click", () => handlers.onSettingsTab(button.dataset.settingsTab));
  });

  if (state.settingsTab === "model") bindModelTab(container, state, handlers);
  if (state.settingsTab === "tools") bindToolsTab(container, handlers);
  if (state.settingsTab === "stats") bindStatsTab(container, handlers);
}

function tabButton(id, label, active) {
  return `<button class="settings-tab ${active === id ? "active" : ""}" data-settings-tab="${id}" type="button"><span>${SETTINGS_TAB_ICONS[id]}</span>${label}</button>`;
}

function renderActiveSettingsTab(state) {
  if (state.settingsTab === "tools") return renderToolsTab(state);
  if (state.settingsTab === "stats") return renderStatsTab(state);
  return renderModelTab(state);
}

function renderModelTab(state) {
  const settings = state.settings;
  const draft = state.settingsDraft ?? {};
  return `
    <div class="settings-status ${settings?.hasApiKey ? "ok" : "warn"}">
      <strong>${settings?.hasApiKey ? "API Key 已配置" : "API Key 未配置"}</strong>
      <span>${escapeHtml(settings?.settingsPath || "")}</span>
    </div>
    <form class="settings-form" data-role="settings-form">
      <label>
        <span>API Key</span>
        <input name="apiKey" type="password" autocomplete="off" placeholder="${settings?.hasApiKey ? "留空则保持原值" : "sk-..."}" />
      </label>
      <label class="checkbox-row">
        <input name="clearApiKey" type="checkbox" ${draft.clearApiKey ? "checked" : ""} />
        <span>清除已保存的 API Key</span>
      </label>
      <label>
        <span>Base URL</span>
        <input name="baseUrl" value="${escapeAttr(draft.baseUrl || "")}" />
      </label>
      <label>
        <span>模型</span>
        <input name="model" value="${escapeAttr(draft.model || "")}" />
      </label>
      <div class="settings-grid">
        <label>
          <span>Temperature</span>
          <input name="temperature" type="number" min="0" max="2" step="0.1" value="${Number(draft.temperature ?? 0.7)}" />
        </label>
        <label>
          <span>上下文</span>
          <input name="maxContextMessages" type="number" min="4" max="64" step="1" value="${Number(draft.maxContextMessages ?? 18)}" />
        </label>
      </div>
      <label>
        <span>人设</span>
        <textarea name="systemPrompt" rows="4">${escapeHtml(draft.systemPrompt || "")}</textarea>
      </label>
      <div class="settings-section">
        <h3>系统自动检查</h3>
        <label class="checkbox-row">
          <input name="autoSystemCheckEnabled" type="checkbox" ${draft.autoSystemCheckEnabled ? "checked" : ""} />
          <span>自动调用系统状态工具</span>
        </label>
        <div class="settings-grid">
          <label>
            <span>检查间隔（分钟）</span>
            <input name="autoSystemCheckIntervalMinutes" type="number" min="1" max="120" step="1" value="${Number(draft.autoSystemCheckIntervalMinutes ?? 10)}" />
          </label>
        </div>
        <div class="tool-format auto-system-status-card">
          <strong>检查状态</strong>
          <span data-role="auto-system-status">${escapeHtml(autoSystemStatusText(state, draft))}</span>
        </div>
      </div>
      <button class="text-button primary" type="submit">保存设置</button>
    </form>
    <div class="settings-grid window-toggles">
      <button class="text-button" data-action="top">${state.alwaysOnTop ? "取消置顶" : "窗口置顶"}</button>
      <button class="text-button" data-action="passthrough">鼠标穿透 10 秒</button>
    </div>
  `;
}

function autoSystemStatusText(state, draft) {
  if (state.autoSystemStatus) return state.autoSystemStatus;
  return draft.autoSystemCheckEnabled
    ? "已启用，保存后会按间隔自动调用 get_system_status。"
    : "未启用。开启后 iPet 会定期检查 CPU、内存和进程状态。";
}

function renderToolsTab(state) {
  const tools = state.tools ?? [];
  const cards = tools
    .map(
      (tool) => `
      <article class="tool-card">
        <div class="tool-head">
          <div>
            <strong>${escapeHtml(tool.displayName || tool.name)}</strong>
            <code>${escapeHtml(tool.name)}</code>
          </div>
          <label class="switch-row">
            <input type="checkbox" data-tool-enabled="${escapeAttr(tool.name)}" ${tool.enabled ? "checked" : ""} />
            <span>${tool.enabled ? "已启用" : "停用"}</span>
          </label>
        </div>
        <p>${escapeHtml(tool.description)}</p>
        <div class="tool-meta">
          <span>${tool.builtIn ? "内置" : "自定义"}</span>
          <span>${escapeHtml(tool.kind)}</span>
        </div>
        <details>
          <summary>工具 schema</summary>
          <pre>${escapeHtml(JSON.stringify(tool.parameters, null, 2))}</pre>
        </details>
        ${tool.builtIn ? "" : `<button class="text-button danger" data-delete-tool="${escapeAttr(tool.name)}" type="button">删除工具</button>`}
      </article>
    `,
    )
    .join("");

  return `
    <div class="tool-layout">
      <div class="tool-list">
        ${cards || '<div class="empty-state">暂无工具</div>'}
      </div>
      <form class="tool-form" data-role="tool-form">
        <h3>添加 HTTP 工具</h3>
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
        <button class="text-button primary" type="submit">添加或更新工具</button>
      </form>
    </div>
  `;
}

function renderStatsTab(state) {
  const stats = state.stats;
  const dayRows = (stats?.byDay ?? []).map(renderBucketRow).join("");
  const modelRows = (stats?.byModel ?? []).map(renderBucketRow).join("");
  const recentRows = (stats?.recent ?? [])
    .map(
      (row) => `
      <tr>
        <td>${escapeHtml(row.model)}</td>
        <td>${row.promptTokens}</td>
        <td>${row.completionTokens}</td>
        <td>${row.totalTokens}</td>
        <td>${row.toolCalls}</td>
      </tr>
    `,
    )
    .join("");

  return `
    <div class="metric-grid">
      <div class="metric"><span>总 Token</span><strong>${stats?.totalTokens ?? 0}</strong></div>
      <div class="metric"><span>Prompt</span><strong>${stats?.promptTokens ?? 0}</strong></div>
      <div class="metric"><span>Completion</span><strong>${stats?.completionTokens ?? 0}</strong></div>
      <div class="metric"><span>请求</span><strong>${stats?.requests ?? 0}</strong></div>
      <div class="metric"><span>工具调用</span><strong>${stats?.toolCalls ?? 0}</strong></div>
    </div>
    <button class="text-button" data-action="refresh-stats" type="button">刷新统计</button>
    <div class="stats-section">
      <h3>按日期</h3>
      <div class="table-wrap">
        <table>
          <thead><tr><th>日期</th><th>Prompt</th><th>Completion</th><th>Total</th><th>请求</th></tr></thead>
          <tbody>${dayRows || '<tr><td colspan="5">暂无数据</td></tr>'}</tbody>
        </table>
      </div>
    </div>
    <div class="stats-section">
      <h3>按模型</h3>
      <div class="table-wrap">
        <table>
          <thead><tr><th>模型</th><th>Prompt</th><th>Completion</th><th>Total</th><th>请求</th></tr></thead>
          <tbody>${modelRows || '<tr><td colspan="5">暂无数据</td></tr>'}</tbody>
        </table>
      </div>
    </div>
    <div class="stats-section">
      <h3>最近请求</h3>
      <div class="table-wrap">
        <table>
          <thead><tr><th>模型</th><th>Prompt</th><th>Completion</th><th>Total</th><th>工具</th></tr></thead>
          <tbody>${recentRows || '<tr><td colspan="5">暂无数据</td></tr>'}</tbody>
        </table>
      </div>
    </div>
  `;
}

function bindModelTab(container, _state, handlers) {
  container.querySelector('[data-role="settings-form"]').addEventListener("submit", handlers.onSaveSettings);
  container.querySelector('[data-action="top"]').addEventListener("click", handlers.onToggleTop);
  container
    .querySelector('[data-action="passthrough"]')
    .addEventListener("click", handlers.onTemporaryPassthrough);
}

function bindToolsTab(container, handlers) {
  container.querySelectorAll("[data-tool-enabled]").forEach((input) => {
    input.addEventListener("change", () => {
      handlers.onSetToolEnabled(input.dataset.toolEnabled, input.checked);
    });
  });
  container.querySelectorAll("[data-delete-tool]").forEach((button) => {
    button.addEventListener("click", () => handlers.onDeleteTool(button.dataset.deleteTool));
  });
  container.querySelector('[data-role="tool-form"]').addEventListener("submit", (event) => {
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
}

function bindStatsTab(container, handlers) {
  container.querySelector('[data-action="refresh-stats"]').addEventListener("click", handlers.onRefreshStats);
}

function renderBucketRow(bucket) {
  return `
    <tr>
      <td>${escapeHtml(bucket.label)}</td>
      <td>${bucket.promptTokens}</td>
      <td>${bucket.completionTokens}</td>
      <td>${bucket.totalTokens}</td>
      <td>${bucket.requests}</td>
    </tr>
  `;
}

function escapeAttr(value) {
  return escapeHtml(value).replaceAll("`", "&#096;");
}
