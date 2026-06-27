import { escapeHtml } from "../utils/markdown.js";
import { icon } from "../ui/icons.js";
import { formatNum } from "./shared.js";

/**
 * UsageView — token usage overview (ref-plan §6.3) + desktop app-usage ("screen
 * time"). Rebuilds the old stats tab as a dashboard: overview metrics,
 * prompt/completion breakdown, daily trend, by-model, and recent requests, then
 * a foreground-usage section fed by the backend sampler. No chart library —
 * just CSS bars + tabular nums. Empty data shows an explanatory empty state.
 */
export function renderUsageView(container, state, handlers) {
  const stats = state.stats;

  if (!stats || (!stats.totalTokens && !stats.requests)) {
    container.innerHTML = `
      <div class="usage-page">
        <div class="empty-state usage-empty">
          <strong>暂无用量数据</strong>
          <span>开始对话后，这里会显示 token 用量、请求和工具调用统计。</span>
        </div>
        ${renderAppUsageSection(state, handlers)}
        ${renderPomodoroSection(state)}
      </div>
    `;
    bindAppUsage(container, handlers);
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
    <div class="usage-page">
      <div class="metric-grid usage-metrics">
        <div class="metric metric-primary"><span>总 Token</span><strong>${formatNum(total)}</strong></div>
        <div class="metric"><span>请求</span><strong>${formatNum(requests)}</strong></div>
        <div class="metric"><span>Completion 占比</span><strong>${prompt + completion > 0 ? `${completionPct}%` : "—"}</strong></div>
        <div class="metric"><span>工具调用</span><strong>${formatNum(toolCalls)}</strong></div>
      </div>
      <div class="breakdown usage-breakdown">
        <div class="breakdown-label">
          <span>Prompt ${formatNum(prompt)} · ${promptPct}%</span>
          <span>Completion ${formatNum(completion)} · ${completionPct}%</span>
        </div>
        <div class="breakdown-bar" title="Prompt vs Completion">
          <span class="seg-prompt" style="width:${promptPct}%"></span>
          <span class="seg-completion" style="width:${completionPct}%"></span>
        </div>
      </div>

      <section class="trend-card">
        <div class="stats-head">
          <div>
            <h3>趋势</h3>
            <div class="stats-refreshed">${formatRefreshed(state.lastStatsRefreshAt)}</div>
          </div>
          <button class="text-button" data-action="refresh-stats" type="button">${icon("refresh")} 刷新</button>
        </div>
        <div class="trend-chart" aria-label="按日 token 用量趋势">${dayBars || '<div class="empty-state">暂无按日数据</div>'}</div>
      </section>

      <div class="usage-tables">
        <section class="stats-section">
          <h3>按模型</h3>
          <div class="table-wrap">
            <table>
              <thead><tr><th>模型</th><th>Prompt</th><th>Completion</th><th>Total</th><th>请求</th></tr></thead>
              <tbody>${modelRows || '<tr><td colspan="5">暂无数据</td></tr>'}</tbody>
            </table>
          </div>
        </section>
        <section class="stats-section">
          <h3>最近请求</h3>
          <div class="table-wrap">
            <table>
              <thead><tr><th>模型</th><th>Prompt</th><th>Completion</th><th>Total</th><th>工具</th></tr></thead>
              <tbody>${recentRows || '<tr><td colspan="5">暂无数据</td></tr>'}</tbody>
            </table>
          </div>
        </section>
      </div>

      ${renderAppUsageSection(state, handlers)}
      ${renderPomodoroSection(state)}
    </div>
  `;

  container.querySelector('[data-action="refresh-stats"]')?.addEventListener("click", handlers.onRefreshStats);
  bindAppUsage(container, handlers);
}

/* --- App usage section (desktop "screen time") ---------------------------- */

function renderAppUsageSection(state, handlers) {
  const range = state.appUsageRange || "today";
  const usage = state.appUsage;
  const rangeOptions = ["today", "7d", "30d"]
    .map(
      (r) =>
        `<button class="text-button ${r === range ? "primary" : ""}" data-app-range="${r}" type="button">${rangeLabel(r)}</button>`,
    )
    .join("");

  let body;
  if (!usage) {
    body = `<div class="empty-state">正在加载应用使用时长…</div>`;
  } else if (!usage.totalSeconds || !usage.byApp?.length) {
    body = `<div class="empty-state">暂无使用时长数据。开启「系统 → 应用使用时长」后，iPet 会在后台每 15 秒采样前台窗口并累计。</div>`;
  } else {
    const max = Math.max(1, ...usage.byApp.map((a) => Number(a.seconds ?? 0)));
    const rows = usage.byApp
      .map(
        (a) => `
        <div class="app-usage-row" title="${escapeHtml(a.appName)} · ${formatDuration(a.seconds)}">
          <span class="app-usage-name">${escapeHtml(a.appName)}</span>
          <span class="app-usage-bar"><span class="app-usage-bar-fill" style="width:${Math.round((Number(a.seconds ?? 0) / max) * 100)}%"></span></span>
          <span class="app-usage-dur" num>${formatDuration(a.seconds)}</span>
        </div>
      `,
      )
      .join("");
    body = `
      <div class="metric-grid usage-metrics">
        <div class="metric metric-primary"><span>前台总时长</span><strong>${formatDuration(usage.totalSeconds)}</strong></div>
        <div class="metric"><span>应用数</span><strong>${formatNum(usage.byApp.length)}</strong></div>
      </div>
      <div class="app-usage-list">${rows}</div>
    `;
  }

  return `
    <section class="stats-section app-usage-card">
      <div class="stats-head">
        <div>
          <h3>应用使用时长</h3>
          <div class="stats-refreshed">${escapeHtml(state.appUsageStatus || "桌面版屏幕使用时间")}</div>
        </div>
        <div class="app-usage-controls">
          ${rangeOptions}
          <button class="text-button" data-action="refresh-app-usage" type="button">${icon("refresh")} 刷新</button>
        </div>
      </div>
      ${body}
    </section>
  `;
}

function bindAppUsage(container, handlers) {
  container.querySelectorAll("[data-app-range]")?.forEach((btn) => {
    btn.addEventListener("click", () => handlers.onSetAppUsageRange?.(btn.dataset.appRange));
  });
  container.querySelector('[data-action="refresh-app-usage"]')?.addEventListener("click", () => {
    handlers.onRefreshAppUsage?.();
  });
}

/* --- Pomodoro history section --------------------------------------------- */

function renderPomodoroSection(state) {
  const stats = state.pomodoroStats;
  const range = rangeLabel(state.appUsageRange || "today");

  let body;
  if (!stats) {
    body = `<div class="empty-state">正在加载番茄钟记录…</div>`;
  } else if (!stats.totalWork && !stats.totalBreak) {
    body = `<div class="empty-state">暂无番茄钟记录。完成一个专注时段后会在此统计。</div>`;
  } else {
    const max = Math.max(1, ...stats.byDay.map((d) => Number(d.workCount ?? 0)));
    const bars =
      stats.byDay
        .map(
          (d) => `
        <div class="trend-bar" title="${escapeHtml(d.day)} · ${formatNum(d.workCount)} 个番茄">
          <span class="trend-bar-fill" style="height:${Math.round((Number(d.workCount ?? 0) / max) * 100)}%"></span>
          <span class="trend-bar-label">${escapeHtml(shortDay(d.day))}</span>
        </div>
      `,
        )
        .join("") || '<div class="empty-state">本区间无记录</div>';
    body = `
      <div class="metric-grid usage-metrics">
        <div class="metric metric-primary"><span>完成番茄</span><strong>${formatNum(stats.totalWork)}</strong></div>
        <div class="metric"><span>完成休息</span><strong>${formatNum(stats.totalBreak)}</strong></div>
      </div>
      <div class="trend-chart" aria-label="按日番茄完成趋势">${bars}</div>
    `;
  }

  return `
    <section class="stats-section app-usage-card">
      <div class="stats-head">
        <div>
          <h3>番茄钟记录</h3>
          <div class="stats-refreshed">${escapeHtml(range)} 统计</div>
        </div>
      </div>
      ${body}
    </section>
  `;
}

function rangeLabel(range) {
  if (range === "7d") return "近 7 天";
  if (range === "30d") return "近 30 天";
  return "今天";
}

/** seconds -> "1h 23m" / "12m" / "45s" */
function formatDuration(seconds) {
  const s = Math.max(0, Math.floor(Number(seconds) || 0));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  const rem = m % 60;
  return rem ? `${h}h ${rem}m` : `${h}h`;
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
