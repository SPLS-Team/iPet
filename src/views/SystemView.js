import { escapeHtml } from "../utils/markdown.js";
import { icon } from "../ui/icons.js";
import { card, escapeAttr } from "./shared.js";

/**
 * SystemView — live system status + auto-check automation + window behavior +
 * diagnostics (ref-plan §6.4). Pulls auto-check and window controls out of the
 * old Model tab so they sit where users actually look for them. Auto-check
 * fields save as a partial override merged with the settings draft.
 */
export function renderSystemView(container, state, handlers) {
  const draft = state.settingsDraft ?? {};
  const snap = state.systemSnapshot;
  const pomo = {
    workMinutes: 25,
    breakMinutes: 5,
    longBreakMinutes: 15,
    longBreakEvery: 4,
    autoStartBreak: true,
    autoStartWork: false,
    completedWorkCount: 0,
    ...(state.pomodoro ?? {}),
  };

  container.innerHTML = `
    <div class="settings-page system-page">
      <form class="settings-form" data-role="system-form">
        ${card(
          "实时状态",
          snap
            ? `
              <div class="metric-grid">
                <div class="metric"><span>CPU</span><strong>${formatPercent(snap.cpuUsage)}</strong></div>
                <div class="metric"><span>内存</span><strong>${formatPercent(snap.memory?.usagePercent)}</strong></div>
                <div class="metric"><span>进程数</span><strong>${escapeHtml(String(snap.processCount ?? "—"))}</strong></div>
                <div class="metric"><span>高占用</span><strong>${escapeHtml(snap.processes?.[0]?.name || "—")}</strong></div>
              </div>
              <div class="form-actions">
                <button class="text-button" type="button" data-action="run-check">${icon("refresh")} 立即检查</button>
              </div>
            `
            : `
              <p class="card-hint">暂未采样。点击“立即检查”获取 CPU、内存和高占用进程。</p>
              <div class="form-actions">
                <button class="text-button" type="button" data-action="run-check">${icon("refresh")} 立即检查</button>
              </div>
            `,
        )}

        ${card(
          "自动化",
          `
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
            <div class="form-actions">
              <button class="text-button primary" type="submit">${icon("check")} 保存</button>
            </div>
          `,
        )}

        ${card(
          "通知",
          `
            <p class="card-hint">通过系统通知中心提醒你。可分别开关「回答完成」与「系统负载告警」。</p>
            <label class="checkbox-row">
              <input name="notifyOnReply" type="checkbox" ${draft.notifyOnReply ? "checked" : ""} />
              <span>回答完成时通知</span>
            </label>
            <label class="checkbox-row">
              <input name="notifyOnSystemAlert" type="checkbox" ${draft.notifyOnSystemAlert ? "checked" : ""} />
              <span>系统负载告警时通知（CPU 或内存 ≥ 85%）</span>
            </label>
            <div class="form-actions">
              <button class="text-button primary" type="submit">${icon("check")} 保存</button>
            </div>
          `,
        )}

        ${card(
          "应用使用时长",
          `
            <p class="card-hint">后台每 15 秒采样前台窗口，按应用累计使用时长（桌面版"屏幕使用时间"），仅本机存储。支持 Windows / macOS / Linux(X11)，纯 Wayland 暂不支持。</p>
            <label class="checkbox-row">
              <input name="trackAppUsage" type="checkbox" ${draft.trackAppUsage ? "checked" : ""} />
              <span>启用应用使用时长统计</span>
            </label>
            <div class="settings-grid">
              <label>
                <span>空闲阈值（分钟，超过则不计入）</span>
                <input name="appUsageIdleMinutes" type="number" min="0" max="240" step="1" value="${Number(draft.appUsageIdleMinutes ?? 5)}" ${draft.trackAppUsage ? "" : "disabled"} />
              </label>
            </div>
            <div class="form-actions">
              <button class="text-button primary" type="submit">${icon("check")} 保存</button>
            </div>
          `,
        )}
      </form>

      ${card(
        "窗口行为",
        `
          <p class="card-hint">即时操作，无需保存。</p>
          <div class="settings-grid window-toggles">
            <button class="text-button" data-action="top" type="button">${icon("pin")} ${state.alwaysOnTop ? "取消置顶" : "窗口置顶"}</button>
            <button class="text-button" data-action="passthrough" type="button">${icon("eyeOff")} 鼠标穿透 10 秒</button>
            <button class="text-button" data-action="compact" type="button">${icon("compact")} 收起为小窗</button>
          </div>
        `,
      )}

      ${card(
        "番茄钟",
        `
          <form data-role="pomodoro-form">
            <p class="card-hint">默认 25 分钟专注 / 5 分钟休息循环，每 ${escapeHtml(String(pomo.longBreakEvery))} 个番茄一次长休息。时段切换时通过系统通知提醒。</p>
            <div class="settings-grid">
              <label>
                <span>专注（分钟）</span>
                <input name="workMinutes" type="number" min="1" max="90" step="1" value="${Number(pomo.workMinutes)}" />
              </label>
              <label>
                <span>休息（分钟）</span>
                <input name="breakMinutes" type="number" min="1" max="60" step="1" value="${Number(pomo.breakMinutes)}" />
              </label>
              <label>
                <span>长休息（分钟）</span>
                <input name="longBreakMinutes" type="number" min="1" max="60" step="1" value="${Number(pomo.longBreakMinutes)}" />
              </label>
              <label>
                <span>长休息间隔（个）</span>
                <input name="longBreakEvery" type="number" min="2" max="12" step="1" value="${Number(pomo.longBreakEvery)}" />
              </label>
            </div>
            <label class="checkbox-row">
              <input name="autoStartBreak" type="checkbox" ${pomo.autoStartBreak ? "checked" : ""} />
              <span>专注结束后自动开始休息</span>
            </label>
            <label class="checkbox-row">
              <input name="autoStartWork" type="checkbox" ${pomo.autoStartWork ? "checked" : ""} />
              <span>休息结束后自动开始下一个专注</span>
            </label>
            <div class="tool-format">
              <strong>已完成番茄</strong>
              <span>${Number(pomo.completedWorkCount)} 个（本次会话）</span>
            </div>
            <div class="form-actions">
              <button class="text-button primary" type="submit">${icon("check")} 保存</button>
            </div>
          </form>
        `,
      )}

      ${card(
        "诊断",
        `
          <p class="card-hint">配置与日志路径，便于排错。</p>
          <div class="settings-grid">
            <label>
              <span>配置路径</span>
              <input readonly value="${escapeAttr(state.settings?.settingsPath || "")}" />
            </label>
          </div>
        `,
      )}
    </div>
  `;

  const form = container.querySelector('[data-role="system-form"]');

  // Disable the interval field when the toggle is off (ui-plan.md §9.8).
  const toggle = form.querySelector('[name="autoSystemCheckEnabled"]');
  const interval = form.querySelector('[name="autoSystemCheckIntervalMinutes"]');
  if (toggle && interval) {
    toggle.addEventListener("change", () => {
      interval.disabled = !toggle.checked;
    });
  }

  // Same disable-when-off coupling for the app-usage idle threshold.
  const usageToggle = form.querySelector('[name="trackAppUsage"]');
  const usageIdle = form.querySelector('[name="appUsageIdleMinutes"]');
  if (usageToggle && usageIdle) {
    usageToggle.addEventListener("change", () => {
      usageIdle.disabled = !usageToggle.checked;
    });
  }

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    handlers.onSaveSettings({
      autoSystemCheckEnabled: form.elements.autoSystemCheckEnabled.checked,
      autoSystemCheckIntervalMinutes: Number(form.elements.autoSystemCheckIntervalMinutes.value),
      notifyOnReply: form.elements.notifyOnReply.checked,
      notifyOnSystemAlert: form.elements.notifyOnSystemAlert.checked,
      trackAppUsage: form.elements.trackAppUsage.checked,
      appUsageIdleMinutes: Number(form.elements.appUsageIdleMinutes.value),
    });
  });

  container.querySelector('[data-action="run-check"]')?.addEventListener("click", () => {
    handlers.onRunSystemCheck?.();
  });
  container.querySelector('[data-action="top"]')?.addEventListener("click", handlers.onToggleTop);
  container
    .querySelector('[data-action="passthrough"]')
    ?.addEventListener("click", handlers.onTemporaryPassthrough);
  container.querySelector('[data-action="compact"]')?.addEventListener("click", handlers.onGoCapsule);

  // Pomodoro durations live in their own form (separate from the LLM settings
  // form above) and persist via the `ipet:pomodoro` preference, not save_llm_settings.
  const pomoForm = container.querySelector('[data-role="pomodoro-form"]');
  pomoForm?.addEventListener("submit", (event) => {
    event.preventDefault();
    handlers.onSavePomodoro?.({
      workMinutes: Number(pomoForm.elements.workMinutes.value),
      breakMinutes: Number(pomoForm.elements.breakMinutes.value),
      longBreakMinutes: Number(pomoForm.elements.longBreakMinutes.value),
      longBreakEvery: Number(pomoForm.elements.longBreakEvery.value),
      autoStartBreak: pomoForm.elements.autoStartBreak.checked,
      autoStartWork: pomoForm.elements.autoStartWork.checked,
    });
  });
}

function autoSystemStatusText(state, draft) {
  if (state.autoSystemStatus) return state.autoSystemStatus;
  return draft.autoSystemCheckEnabled
    ? "已启用，保存后会按间隔自动调用 get_system_status。"
    : "未启用。开启后 iPet 会定期检查 CPU、内存和进程状态。";
}

function formatPercent(value) {
  return `${(Number(value) || 0).toFixed(1)}%`;
}
