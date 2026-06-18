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
      <nav class="settings-tabs" aria-label="settings" role="tablist">
        ${tabButton("model", "模型", state.settingsTab)}
        ${tabButton("tools", "工具", state.settingsTab)}
        ${tabButton("stats", "统计", state.settingsTab)}
      </nav>
      <div class="settings-content">
        ${renderActiveSettingsTab(state)}
      </div>
      <div class="inline-status" aria-live="polite">${escapeHtml(state.settingsStatus || state.toolStatus || state.statsStatus || "")}</div>
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
  const selected = active === id;
  return `<button class="settings-tab ${selected ? "active" : ""}" data-settings-tab="${id}" type="button" role="tab" aria-selected="${selected ? "true" : "false"}"><span>${SETTINGS_TAB_ICONS[id]}</span>${label}</button>`;
}

function renderActiveSettingsTab(state) {
  if (state.settingsTab === "tools") return renderToolsTab(state);
  if (state.settingsTab === "stats") return renderStatsTab(state);
  return renderModelTab(state);
}

function renderModelTab(state) {
  const settings = state.settings;
  const draft = state.settingsDraft ?? {};
  const statusClass = settings?.hasApiKey ? "ok" : "warn";
  return `
    <form class="settings-form" data-role="settings-form">
      <div class="settings-status ${statusClass}">
        <strong>${settings?.hasApiKey ? "API Key 已配置" : "API Key 未配置"}</strong>
        <span title="${escapeAttr(settings?.settingsPath || "")}">${escapeHtml(settings?.settingsPath || "")}</span>
      </div>

      <section class="settings-card">
        <h3>Provider</h3>
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
      </section>

      <section class="settings-card">
        <h3>生成参数</h3>
        <div class="settings-grid">
          <label>
            <span>Temperature</span>
            <input name="temperature" type="number" min="0" max="2" step="0.1" value="${Number(draft.temperature ?? 0.7)}" />
            <span class="field-hint">0 更稳定，2 更发散</span>
          </label>
          <label>
            <span>上下文</span>
            <input name="maxContextMessages" type="number" min="4" max="64" step="1" value="${Number(draft.maxContextMessages ?? 18)}" />
            <span class="field-hint">保留最近 N 条消息</span>
          </label>
        </div>
      </section>

      <section class="settings-card">
        <h3>人设</h3>
        <label>
          <span>System Prompt</span>
          <textarea name="systemPrompt" rows="4">${escapeHtml(draft.systemPrompt || "")}</textarea>
        </label>
      </section>

      <section class="settings-card">
        <h3>系统自动检查</h3>
        <p class="card-hint">定期调用 get_system_status，让 iPet 感知 CPU、内存和高占用进程。</p>
        <label class="checkbox-row">
          <input name="autoSystemCheckEnabled" type="checkbox" ${draft.autoSystemCheckEnabled ? "checked" : ""} />
          <span>自动调用系统状态工具</span>
        </label>
        <div class="settings-grid">
          <label>
            <span>检查间隔（分钟）</span>
            <input name="autoSystemCheckIntervalMinutes" type="number" min="1" max="120" step="1" value="${Number(draft.autoSystemCheckIntervalMinutes ?? 10)}" ${draft.autoSystemCheckEnabled ? "" : "disabled"} />
          </label>
        </div>
        <div class="tool-format auto-system-status-card">
          <strong>检查状态</strong>
          <span data-role="auto-system-status">${escapeHtml(autoSystemStatusText(state, draft))}</span>
        </div>
      </section>

      <button class="text-button primary" type="submit">保存设置</button>
    </form>

    <section class="settings-card">
      <h3>窗口行为</h3>
      <p class="card-hint">即时操作，无需保存设置。</p>
      <div class="settings-grid window-toggles">
        <button class="text-button" data-action="top" type="button">${state.alwaysOnTop ? "取消置顶" : "窗口置顶"}</button>
        <button class="text-button" data-action="passthrough" type="button">鼠标穿透 10 秒</button>
      </div>
    </section>
  `;
}

function autoSystemStatusText(state, draft) {
  if (state.autoSystemStatus) return state.autoSystemStatus;
  return draft.autoSystemCheckEnabled
    ? "已启用，保存后会按间隔自动调用 get_system_status。"
    : "未启用。开启后 iPet 会定期检查 CPU、内存和进程状态。";
}

/* ----------------------------------------------------------------------- */
/* Tools tab                                                               */
/* ----------------------------------------------------------------------- */

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

function renderToolSummary(tools) {
  const enabled = tools.filter((t) => t.enabled).length;
  const builtin = tools.filter((t) => t.builtIn).length;
  const http = tools.filter((t) => t.http).length;
  const local = tools.filter((t) => t.local).length;
  return `
    <div class="tool-summary">
      <div class="metric-mini"><span>已启用</span><strong>${enabled}</strong></div>
      <div class="metric-mini"><span>内置</span><strong>${builtin}</strong></div>
      <div class="metric-mini"><span>HTTP</span><strong>${http}</strong></div>
      <div class="metric-mini"><span>本地</span><strong>${local}</strong></div>
    </div>
  `;
}

function renderToolCard(tool) {
  const kindLabel = toolKindLabel(tool);
  const kindBadgeClass = tool.local ? "badge-local" : tool.builtIn ? "badge-builtin" : "";
  return `
    <article class="tool-card" data-kind="${escapeAttr(tool.kind)}">
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
        <span class="${tool.builtIn ? "badge-builtin" : ""}">${tool.builtIn ? "内置" : "自定义"}</span>
        <span class="${kindBadgeClass}">${kindLabel}</span>
        ${renderToolRuntimeBadge(tool)}
      </div>
      ${tool.local ? `<div class="risk-callout">本地工具会在你的电脑上运行程序，请确认你信任该工具。</div>` : ""}
      <details>
        <summary>工具 schema</summary>
        <pre>${escapeHtml(JSON.stringify(tool.parameters, null, 2))}</pre>
      </details>
      <div class="tool-actions">
        ${tool.builtIn ? "" : `<button class="text-button danger" data-delete-tool="${escapeAttr(tool.name)}" type="button">删除工具</button>`}
      </div>
    </article>
  `;
}

function renderToolsTab(state) {
  const tools = state.tools ?? [];
  const cards = tools.map(renderToolCard).join("");

  return `
    <div class="tool-layout">
      <div>
        ${renderToolSummary(tools)}
        <div class="tool-list">
          ${cards || '<div class="empty-state">暂无工具</div>'}
        </div>
      </div>
      <div>
        <form class="tool-form" data-role="import-form">
          <h3>导入工具包</h3>
          <p class="hint">
            指定一个目录或 <code>tool.json</code> 文件路径，从 schema v1
            工具包导入（结构参见 <code>docs/TOOL_PACKAGE.md</code>）。
          </p>
          <label>
            <span>路径</span>
            <input name="packagePath" placeholder="C:/.../my-tool" />
          </label>
          <button class="action-button" type="submit">导入</button>
        </form>
        <details class="tool-collapsible" open>
          <summary class="tool-collapsible-summary">添加 HTTP 工具</summary>
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
            <button class="text-button primary" type="submit">添加或更新工具</button>
          </form>
        </details>
        <details class="tool-collapsible">
          <summary class="tool-collapsible-summary">添加本地工具</summary>
          <div class="risk-callout">本地工具可以启动你电脑上的可执行文件或脚本。请只添加你信任的工具。</div>
          <form class="tool-form" data-role="local-tool-form">
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
            <button class="text-button primary" type="submit">添加或更新本地工具</button>
          </form>
        </details>
      </div>
    </div>
  `;
}

/* ----------------------------------------------------------------------- */
/* Stats tab                                                               */
/* ----------------------------------------------------------------------- */

function renderStatsTab(state) {
  const stats = state.stats;
  const total = Number(stats?.totalTokens ?? 0);
  const prompt = Number(stats?.promptTokens ?? 0);
  const completion = Number(stats?.completionTokens ?? 0);
  const requests = Number(stats?.requests ?? 0);
  const toolCalls = Number(stats?.toolCalls ?? 0);
  const completionPct = prompt + completion > 0 ? Math.round((completion / (prompt + completion)) * 100) : 0;
  const promptPct = 100 - completionPct;

  const dayRows = (stats?.byDay ?? []).map(renderBucketRow).join("");
  const modelRows = (stats?.byModel ?? []).map(renderBucketRow).join("");
  const recentRows = (stats?.recent ?? [])
    .map(
      (row) => `
      <tr>
        <td>${escapeHtml(row.model)}</td>
        <td num>${row.promptTokens}</td>
        <td num>${row.completionTokens}</td>
        <td num>${row.totalTokens}</td>
        <td num>${row.toolCalls}</td>
      </tr>
    `,
    )
    .join("");

  return `
    <div class="metric-grid">
      <div class="metric"><span>总 Token</span><strong>${formatNum(total)}</strong></div>
      <div class="metric"><span>请求</span><strong>${formatNum(requests)}</strong></div>
      <div class="metric"><span>Completion 占比</span><strong>${prompt + completion > 0 ? `${completionPct}%` : "—"}</strong></div>
      <div class="metric"><span>工具调用</span><strong>${formatNum(toolCalls)}</strong></div>
    </div>
    <div class="breakdown">
      <div class="breakdown-label">
        <span>Prompt ${promptPct}%</span>
        <span>Completion ${completionPct}%</span>
      </div>
      <div class="breakdown-bar" title="Prompt vs Completion">
        <span class="seg-prompt" style="width:${promptPct}%"></span>
        <span class="seg-completion" style="width:${completionPct}%"></span>
      </div>
    </div>
    <div class="stats-head">
      <h3>明细</h3>
      <div class="stats-refreshed">${formatRefreshed(state.lastStatsRefreshAt)}</div>
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

function formatNum(value) {
  return Number(value || 0).toLocaleString("en-US");
}

function formatRefreshed(date) {
  if (!date) return "尚未刷新";
  const h = String(date.getHours()).padStart(2, "0");
  const m = String(date.getMinutes()).padStart(2, "0");
  return `${h}:${m} 更新`;
}

function bindModelTab(container, _state, handlers) {
  container.querySelector('[data-role="settings-form"]').addEventListener("submit", handlers.onSaveSettings);
  container.querySelector('[data-action="top"]').addEventListener("click", handlers.onToggleTop);
  container
    .querySelector('[data-action="passthrough"]')
    .addEventListener("click", handlers.onTemporaryPassthrough);

  // Disable the interval field when the toggle is off (ui-plan.md §9.8).
  const toggle = container.querySelector('[name="autoSystemCheckEnabled"]');
  const interval = container.querySelector('[name="autoSystemCheckIntervalMinutes"]');
  if (toggle && interval) {
    toggle.addEventListener("change", () => {
      interval.disabled = !toggle.checked;
    });
  }
}

function bindToolsTab(container, handlers) {
  container.querySelectorAll("[data-tool-enabled]").forEach((input) => {
    input.addEventListener("change", () => {
      handlers.onSetToolEnabled(input.dataset.toolEnabled, input.checked);
    });
  });
  container.querySelectorAll("[data-delete-tool]").forEach((button) => {
    button.addEventListener("click", () => {
      handlers.onDeleteTool(button.dataset.deleteTool);
    });
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
  const localForm = container.querySelector('[data-role="local-tool-form"]');
  if (localForm) {
    localForm.addEventListener("submit", (event) => {
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
  }

  const importForm = container.querySelector('[data-role="import-form"]');
  if (importForm) {
    importForm.addEventListener("submit", (event) => {
      event.preventDefault();
      const path = event.currentTarget.elements.packagePath.value.trim();
      if (!path) return;
      handlers.onImportTool(path);
    });
  }
}

function bindStatsTab(container, handlers) {
  container.querySelector('[data-action="refresh-stats"]').addEventListener("click", handlers.onRefreshStats);
}

function renderBucketRow(bucket) {
  return `
    <tr>
      <td>${escapeHtml(bucket.label)}</td>
      <td num>${bucket.promptTokens}</td>
      <td num>${bucket.completionTokens}</td>
      <td num>${bucket.totalTokens}</td>
      <td num>${bucket.requests}</td>
    </tr>
  `;
}

function escapeAttr(value) {
  return escapeHtml(value).replaceAll("`", "&#096;");
}
