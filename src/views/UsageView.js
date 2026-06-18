import { escapeHtml } from "../utils/markdown.js";
import { icon } from "../ui/icons.js";
import { formatNum } from "./shared.js";

/**
 * UsageView — token usage overview (ref-plan §6.3). Rebuilds the old stats tab
 * as a dashboard: overview metrics, prompt/completion breakdown, daily trend,
 * by-model, and recent requests. No chart library — just CSS bars + tabular
 * nums. Empty data shows an explanatory empty state.
 */
export function renderUsageView(container, state, handlers) {
  const stats = state.stats;

  if (!stats || (!stats.totalTokens && !stats.requests)) {
    container.innerHTML = `
      <div class="empty-state">
        <strong>暂无用量数据</strong>
        <span>开始对话后，这里会显示 token 用量、请求和工具调用统计。</span>
      </div>
    `;
    return;
  }

  const total = Number(stats.totalTokens ?? 0);
  const prompt = Number(stats.promptTokens ?? 0);
  const completion = Number(stats.completionTokens ?? 0);
  const requests = Number(stats.requests ?? 0);
  const toolCalls = Number(stats.toolCalls ?? 0);
  const completionPct = prompt + completion > 0 ? Math.round((completion / (prompt + completion)) * 100) : 0;
  const promptPct = 100 - completionPct;

  const maxDayTotal = Math.max(1, ...(stats.byDay ?? []).map((d) => Number(d.totalTokens ?? 0)));
  const dayBars = (stats.byDay ?? [])
    .map(
      (d) => `
      <div class="trend-bar" title="${escapeHtml(d.label)} · ${formatNum(d.totalTokens)} tokens">
        <span class="trend-bar-fill" style="height:${Math.round((Number(d.totalTokens ?? 0) / maxDayTotal) * 100)}%"></span>
        <span class="trend-bar-label">${escapeHtml(shortDay(d.label))}</span>
      </div>
    `,
    )
    .join("");

  const modelRows = (stats.byModel ?? []).map(renderBucketRow).join("");
  const recentRows = (stats.recent ?? [])
    .map(
      (row) => `
      <tr>
        <td>${escapeHtml(row.model)}</td>
        <td num>${formatNum(row.promptTokens)}</td>
        <td num>${formatNum(row.completionTokens)}</td>
        <td num>${formatNum(row.totalTokens)}</td>
        <td num>${formatNum(row.toolCalls)}</td>
      </tr>
    `,
    )
    .join("");

  container.innerHTML = `
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
      <h3>趋势</h3>
      <div class="stats-refreshed">${formatRefreshed(state.lastStatsRefreshAt)}</div>
    </div>
    <div class="trend-chart">${dayBars || '<div class="empty-state">暂无按日数据</div>'}</div>
    <button class="text-button" data-action="refresh-stats" type="button">${icon("refresh")} 刷新统计</button>

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

  container.querySelector('[data-action="refresh-stats"]')?.addEventListener("click", handlers.onRefreshStats);
}

function renderBucketRow(bucket) {
  return `
    <tr>
      <td>${escapeHtml(bucket.label)}</td>
      <td num>${formatNum(bucket.promptTokens)}</td>
      <td num>${formatNum(bucket.completionTokens)}</td>
      <td num>${formatNum(bucket.totalTokens)}</td>
      <td num>${formatNum(bucket.requests)}</td>
    </tr>
  `;
}

function shortDay(label) {
  // "2026-06-17" -> "06-17"; fall back to the raw label if it doesn't parse.
  const match = String(label || "").match(/\d{4}-(\d{2}-\d{2})/);
  return match ? match[1] : String(label || "");
}

function formatRefreshed(date) {
  if (!date) return "尚未刷新";
  const h = String(date.getHours()).padStart(2, "0");
  const m = String(date.getMinutes()).padStart(2, "0");
  return `${h}:${m} 更新`;
}
