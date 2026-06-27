import { escapeHtml } from "../utils/markdown.js";
import { icon } from "../ui/icons.js";
import { talkActivityText } from "./WindowChrome.js";

/**
 * TalkWorkspace — the default expanded view (ref-plan §3.2, §5.2). It owns the
 * talk header (a small pet avatar + current state), a slim pomodoro control
 * bar, and the conversation canvas mount point. The actual message list,
 * composer and status strip are still rendered by the legacy `renderChat` into
 * `#panel` for Phase 1 — they get rebuilt as ChatView in Phase 2.
 */

export function renderTalkWorkspace(ctx) {
  const { state } = ctx;
  const p = { ...DEFAULT_POMODORO, ...(state.pomodoro ?? {}) };
  const sessionTitle = (state.sessions ?? []).find((s) => s.id === state.currentSessionId)?.title || "暂无会话";
  return `
    <section class="talk-dashboard" aria-label="对话工作台">
      <div class="talk-identity-card">
        <div class="talk-avatar" aria-hidden="true">
          <span class="talk-avatar-mark"></span>
        </div>
        <div class="talk-meta">
          <span class="talk-kicker">桌面伙伴</span>
          <strong>iPet</strong>
          <span class="talk-state" data-role="talk-state" aria-live="polite">${escapeHtml(talkActivityText(state))}</span>
        </div>
      </div>

      <div class="talk-session-card">
        <div class="talk-card-head">
          <span>当前会话</span>
          <strong>${escapeHtml(sessionTitle)}</strong>
        </div>
        <div class="session-switcher" role="group" aria-label="会话">
          <select class="session-select" data-role="session-select" title="切换会话" aria-label="切换会话">
            ${(state.sessions ?? [])
              .map(
                (s) =>
                  `<option value="${s.id}" ${s.id === state.currentSessionId ? "selected" : ""}>${escapeHtml(s.title)}</option>`,
              )
              .join("") || '<option value="" disabled>暂无会话</option>'}
          </select>
          <button class="session-icon-btn" type="button" data-role="session-rename" title="重命名当前会话" aria-label="重命名当前会话" ${state.currentSessionId == null ? "disabled" : ""}>${icon("edit", { size: 14 })}</button>
          <button class="session-icon-btn danger" type="button" data-role="session-delete" title="删除当前会话" aria-label="删除当前会话" ${state.currentSessionId == null ? "disabled" : ""}>${icon("trash", { size: 14 })}</button>
          <button class="session-new" type="button" data-role="session-new" title="新建会话" aria-label="新建会话">+</button>
        </div>
      </div>

      <div class="talk-pomodoro" data-role="talk-pomodoro" aria-label="番茄钟">
        <div class="talk-card-head">
          <span>专注计时</span>
          <strong class="pomodoro-time" data-role="pomodoro-time">🍅 ${formatClock(p.remainingSec)}</strong>
        </div>
        <div class="pomodoro-controls">
          <button class="pomodoro-btn" data-role="pomodoro-toggle" type="button" title="${p.running ? "暂停" : "开始"}" aria-label="${p.running ? "暂停番茄钟" : "开始番茄钟"}">${icon(p.running ? "pause" : "play", { size: 14, label: p.running ? "暂停" : "开始" })}</button>
          <div class="pomodoro-labels">
            <span class="pomodoro-phase" data-role="pomodoro-phase">${escapeHtml(phaseLabel(p))}</span>
            <span class="pomodoro-count" title="本次已完成番茄数">已完成 ${p.completedWorkCount}</span>
          </div>
          <div class="pomodoro-actions" aria-label="番茄钟操作">
            <button class="pomodoro-btn" data-role="pomodoro-skip" type="button" title="跳过当前时段" aria-label="跳过当前时段" ${p.phase === "idle" ? "disabled" : ""}>${icon("skip", { size: 14, label: "跳过" })}</button>
            <button class="pomodoro-btn" data-role="pomodoro-reset" type="button" title="重置" aria-label="重置番茄钟" ${p.phase === "idle" ? "disabled" : ""}>${icon("reset", { size: 14, label: "重置" })}</button>
          </div>
        </div>
      </div>
    </section>
    <section id="panel" class="panel" data-role="talk-panel"></section>
  `;
}

export function bindTalkWorkspace(ctx) {
  const { state, handlers } = ctx;
  const select = document.querySelector('[data-role="session-select"]');
  if (select) {
    select.addEventListener("change", () => {
      const value = select.value;
      if (value === "") return; // placeholder option
      handlers.onSwitchSession?.(Number(value));
    });
  }
  const wire = (role, fn) => {
    const btn = document.querySelector(`[data-role="${role}"]`);
    if (btn && !btn.disabled) btn.addEventListener("click", fn);
  };
  wire("session-new", () => handlers.onNewSession?.());
  wire("session-rename", () => handlers.onRenameSession?.(state.currentSessionId));
  wire("session-delete", () => handlers.onDeleteSession?.(state.currentSessionId));
  wire("pomodoro-toggle", () => handlers.onPomodoroToggle?.());
  wire("pomodoro-skip", () => handlers.onPomodoroSkip?.());
  wire("pomodoro-reset", () => handlers.onPomodoroReset?.());
}

/** Label for the current pomodoro phase, shown next to the countdown. */
export function phaseLabel(p) {
  if (!p || p.phase === "idle") return "就绪";
  if (p.phase === "work") return p.running ? "专注中" : "专注·暂停";
  return p.running ? "休息中" : "休息·暂停";
}

const DEFAULT_POMODORO = {
  phase: "idle",
  running: false,
  remainingSec: 25 * 60,
  totalSec: 25 * 60,
  completedWorkCount: 0,
  workMinutes: 25,
  breakMinutes: 5,
  longBreakMinutes: 15,
  longBreakEvery: 4,
};

function formatClock(totalSec) {
  const s = Math.max(0, Math.floor(Number(totalSec) || 0));
  const m = Math.floor(s / 60);
  const sec = s % 60;
  return `${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}
