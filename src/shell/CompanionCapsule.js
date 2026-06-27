/**
 * CompanionCapsule — the always-on-desktop shrunken form. Previously a 148x166
 * pet puck; now a small translucent "glass pill" that floats on the desktop
 * showing one line of status (and, when active, the pomodoro countdown). Fully
 * draggable; a click that wasn't a drag expands into the Talk Workspace.
 *
 * The pill text is computed from state so it stays live without a full re-render
 * — main.js patches `[data-role="capsule-pill-text"]` directly on each tick.
 */

export function renderCompanionCapsule(_ctx) {
  return `
    <div class="capsule" data-capsule data-tauri-drag-region>
      <div class="capsule-pill" data-tauri-drag-region>
        <span class="capsule-pill-text" data-role="capsule-pill-text" data-tauri-drag-region></span>
      </div>
    </div>
  `;
}

/** The one-line text shown in the glass pill. Pomodoro countdown takes
 *  precedence when a focus/break session is running; otherwise fall back to the
 *  chat/system status. Exported so main.js can refresh it without re-rendering. */
export function capsulePillText(state) {
  const p = state.pomodoro;
  if (p && p.phase !== "idle" && p.running) {
    const label = p.phase === "work" ? "专注" : "休息";
    return `🍅 ${formatClock(p.remainingSec)} · ${label}`;
  }
  if (p && p.phase !== "idle" && !p.running) {
    const label = p.phase === "work" ? "专注" : "休息";
    return `🍅 ${formatClock(p.remainingSec)} · ${label}（暂停）`;
  }
  return capsuleStatusText(state);
}

/** Kept for callers that want the non-pomodoro status line. */
export function capsuleStatusText(state) {
  if (state.chatBusy) return state.toolActivity || state.chatStatus || "思考中";
  if (state.chatStatus === "已停止") return "已停止";
  if (state.autoSystemCheckBusy) return "检查系统…";
  if (!state.settings?.hasApiKey) return "API Key 未配置";
  return "点击展开";
}

function formatClock(totalSec) {
  const s = Math.max(0, Math.floor(Number(totalSec) || 0));
  const m = Math.floor(s / 60);
  const sec = s % 60;
  return `${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}
