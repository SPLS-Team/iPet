import { appWindow, invoke, listen } from "./tauriBridge.js";
import { escapeHtml } from "./markdown.js";
import { icon } from "./icons.js";
import { createPetCharacter } from "./components/PetCharacter/PetCharacter.js";
import { renderChat, updateChatStreaming } from "./components/ChatBubble/ChatBubble.js";
import { renderSettings } from "./components/SettingsPanel/SettingsPanel.js";
import "./styles.css";

const state = {
  platform: detectPlatform(),
  activeTab: "chat",
  settingsTab: "model",
  messages: [],
  settings: null,
  settingsDraft: null,
  settingsStatus: "",
  tools: [],
  toolStatus: "",
  stats: null,
  statsStatus: "",
  lastStatsRefreshAt: null,
  chatBusy: false,
  chatStatus: "",
  toolActivity: "",
  stopRequested: false,
  currentRequestId: null,
  alwaysOnTop: true,
  compactMode: false,
  thinkingStartedAt: null,
  thinkingElapsedMs: 0,
  thinkingTimer: null,
  autoSystemCheckTimer: null,
  autoSystemCheckBusy: false,
  autoSystemStatus: "",
  toast: null,
  toastTimer: null,
  dialog: null,
  toolComposerMode: "import",
  theme: "system",
  settingsFieldErrors: {},
  settingsSaveFailed: false,
};

let pet;
let compactDragStart = null;
let compactDragStarted = false;

bootstrap();

/** Lightweight platform detection (ui-plan.md §5.0). No new deps. */
function detectPlatform() {
  const platform = navigator.userAgentData?.platform || navigator.platform || "";
  const value = String(platform).toLowerCase();
  if (value.includes("mac")) return "macos";
  if (value.includes("win")) return "windows";
  if (value.includes("linux")) return "linux";
  return "unknown";
}

const THEME_KEY = "ipet:theme";
const THEME_OPTIONS = ["system", "light", "dark"];

/** Load persisted theme preference and apply it (ui-plan.md §14.1 phase 2). */
async function loadTheme() {
  try {
    const stored = await invoke("get_preference", { key: THEME_KEY });
    if (stored && THEME_OPTIONS.includes(stored)) {
      state.theme = stored;
    }
  } catch {
    /* preference command unavailable in older builds — fall back to system */
  }
  applyTheme();
}

function applyTheme() {
  // "system" → no data-theme (CSS follows prefers-color-scheme).
  if (state.theme === "system") {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = state.theme;
  }
}

async function setTheme(theme) {
  if (!THEME_OPTIONS.includes(theme)) return;
  state.theme = theme;
  applyTheme();
  try {
    await invoke("set_preference", { key: THEME_KEY, value: theme });
  } catch (error) {
    showToast(`主题保存失败：${String(error)}`, "error");
  }
  render();
}

async function bootstrap() {
  document.documentElement.dataset.platform = state.platform;
  await loadTheme();

  document.querySelector("#app").innerHTML = `
    <main class="app-shell">
      <header class="titlebar" data-tauri-drag-region>
        <div class="brand" data-tauri-drag-region>
          <span class="brand-mark" aria-hidden="true"></span>
          <span data-tauri-drag-region>iPet</span>
        </div>
        <div class="window-actions">
          <button class="window-button danger" data-action="close" title="关闭" aria-label="关闭">${icon("close", { label: "关闭" })}</button>
          <button class="window-button" data-action="minimize" title="最小化" aria-label="最小化">${icon("minimize", { label: "最小化" })}</button>
          <button class="window-button" data-action="compact" title="收起为宠物" aria-label="收起为宠物">${icon("compact", { label: "收起为宠物" })}</button>
        </div>
      </header>
      <section class="pet-wrap">
        <div id="pet"></div>
      </section>
      <nav class="tabbar" aria-label="main" role="tablist">
        <button class="tab active" data-tab="chat" role="tab" aria-selected="true">${icon("chat")}<span>聊天</span></button>
        <button class="tab" data-tab="settings" role="tab" aria-selected="false">${icon("settings")}<span>设置</span></button>
      </nav>
      <section id="panel" class="panel"></section>
      <div id="overlay" class="overlay" aria-live="polite"></div>
    </main>
  `;

  pet = createPetCharacter(document.querySelector("#pet"));
  bindShellEvents();
  await bindChatEvents();
  await loadInitialData();
  render();
  scheduleAutoSystemCheck({ runNow: true });
}

// Esc closes the active (non-danger-confirmable) dialog. Danger dialogs also
// close on Esc — canceling is safe; the destructive action only runs on confirm.
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && state.dialog) {
    event.preventDefault();
    closeDialog(false);
  }
});

function bindShellEvents() {
  document.querySelector(".titlebar").addEventListener("mousedown", (event) => {
    if (event.button !== 0 || event.target.closest("button")) return;
    appWindow.startDragging();
  });

  const petWrap = document.querySelector(".pet-wrap");
  petWrap.addEventListener("mousedown", (event) => {
    if (!state.compactMode || event.button !== 0) return;
    compactDragStart = { x: event.clientX, y: event.clientY };
    compactDragStarted = false;
  });
  petWrap.addEventListener(
    "click",
    (event) => {
      if (!state.compactMode) return;
      if (compactDragStarted) {
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      toggleCompact(false);
    },
    true,
  );
  document.addEventListener("mousemove", (event) => {
    if (!state.compactMode || !compactDragStart || compactDragStarted) return;
    const moved = Math.hypot(event.clientX - compactDragStart.x, event.clientY - compactDragStart.y);
    if (moved < 4) return;
    compactDragStarted = true;
    appWindow.startDragging();
  });
  document.addEventListener("mouseup", () => {
    compactDragStart = null;
    window.setTimeout(() => {
      compactDragStarted = false;
    }, 0);
  });

  document.querySelectorAll("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      state.activeTab = button.dataset.tab;
      render();
    });
  });

  document.querySelector('[data-action="compact"]').addEventListener("click", () => {
    toggleCompact(true);
  });
  document.querySelector('[data-action="minimize"]').addEventListener("click", async () => {
    await appWindow.minimize();
  });
  document.querySelector('[data-action="close"]').addEventListener("click", async () => {
    await appWindow.close();
  });
}

async function bindChatEvents() {
  await listen("chat-stream", (event) => {
    const payload = event.payload;
    if (!payload || payload.requestId !== state.currentRequestId) return;

    let shouldRender = false;
    if (payload.kind === "start") {
      beginThinking("思考中");
      state.toolActivity = "";
      pet.setMood("thinking");
    } else if (payload.kind === "tool") {
      state.toolActivity = payload.content || "正在使用工具";
      beginThinking(state.toolActivity);
      pet.setMood("tool");
    } else if (payload.kind === "delta") {
      stopThinking();
      state.toolActivity = "";
      appendAssistantDelta(payload.content);
      state.chatStatus = "";
      pet.setMood("talking");
      // Fast path: just patch the last assistant bubble. Skip the full render
      // (rebuilds every message + re-binds the form) which gets quadratic on
      // long chats during streaming.
      const panel = document.querySelector("#panel");
      const patched = state.activeTab === "chat" && updateChatStreaming(panel, state);
      if (!patched) shouldRender = true;
    } else if (payload.kind === "done") {
      state.chatBusy = false;
      state.chatStatus = "";
      state.toolActivity = "";
      stopThinking();
      pet.setMood("idle");
      refreshStatsSilently();
      shouldRender = true;
    }
    if (shouldRender) render();
  });
}

async function loadInitialData() {
  await Promise.allSettled([loadSettings(), loadMessages(), loadTools(), loadStats()]);
}

async function loadSettings() {
  state.settings = await invoke("get_llm_settings");
  state.settingsDraft = {
    apiKey: "",
    clearApiKey: false,
    baseUrl: state.settings.baseUrl,
    model: state.settings.model,
    temperature: state.settings.temperature,
    maxContextMessages: state.settings.maxContextMessages,
    autoSystemCheckEnabled: Boolean(state.settings.autoSystemCheckEnabled),
    autoSystemCheckIntervalMinutes: Number(state.settings.autoSystemCheckIntervalMinutes ?? 10),
    systemPrompt: state.settings.systemPrompt,
  };
}

async function loadMessages() {
  const records = await invoke("get_recent_messages", { limit: 40 });
  state.messages = records.map((record) => ({
    role: record.role,
    content: record.content,
  }));
}

async function loadTools() {
  state.tools = await invoke("list_tools");
}

async function loadStats() {
  state.stats = await invoke("get_token_stats");
}

async function refreshStatsSilently() {
  try {
    await loadStats();
  } catch (error) {
    console.warn(error);
  }
}

function render() {
  const shell = document.querySelector(".app-shell");
  shell.classList.toggle("compact", state.compactMode);
  renderOverlay();
  if (state.compactMode) {
    pet.setLine("点击展开");
    return;
  }

  document.querySelectorAll("[data-tab]").forEach((button) => {
    const active = button.dataset.tab === state.activeTab;
    button.classList.toggle("active", active);
    button.setAttribute("aria-selected", active ? "true" : "false");
  });

  const panel = document.querySelector("#panel");
  if (state.activeTab === "chat") {
    renderChat(panel, state, { onSend: sendMessage, onStop: stopChat, onGoSettings: goToSettings });
  } else {
    renderSettings(panel, state, {
      onSettingsTab: changeSettingsTab,
      onSaveSettings: saveSettings,
      onToggleTop: toggleAlwaysOnTop,
      onTemporaryPassthrough: enableTemporaryPassthrough,
      onSetToolEnabled: setToolEnabled,
      onDeleteTool: requestDeleteTool,
      onSaveTool: saveTool,
      onImportTool: importTool,
      onSetComposerMode: setToolComposerMode,
      onSetTheme: setTheme,
      onRefreshStats: refreshStats,
    });
  }
}

/** Switch the tool composer between import / http / local (ui-plan §10.5). */
function setToolComposerMode(mode) {
  state.toolComposerMode = mode;
  render();
}

/** Jump to a settings sub-tab from elsewhere (e.g. empty-state chips). */
async function goToSettings(tab) {
  state.activeTab = "settings";
  state.settingsTab = tab;
  if (tab === "tools") await loadToolsSafely();
  if (tab === "stats") await refreshStats();
  render();
}

/** Render the floating overlay layer (toast + dialog). Cheap to call. */
function renderOverlay() {
  const overlay = document.querySelector("#overlay");
  if (!overlay) return;
  if (!state.dialog && !state.toast) {
    overlay.innerHTML = "";
    return;
  }

  let html = "";
  if (state.dialog) {
    const d = state.dialog;
    const confirmClass = d.danger ? "text-button danger" : "text-button primary";
    html += `
      <div class="scrim" data-role="scrim">
        <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="dialog-title">
          <h3 class="dialog-title" id="dialog-title">${escapeHtml(d.title)}</h3>
          ${d.body ? `<p class="dialog-body">${d.body}</p>` : ""}
          <div class="dialog-actions">
            <button class="text-button" type="button" data-role="dialog-cancel">${escapeHtml(d.cancelLabel || "取消")}</button>
            <button class="${confirmClass}" type="button" data-role="dialog-confirm">${escapeHtml(d.confirmLabel || "确认")}</button>
          </div>
        </div>
      </div>
    `;
  }
  if (state.toast) {
    html += `<div class="toast" data-tone="${state.toast.tone || "default"}" role="status">${escapeHtml(state.toast.message)}</div>`;
  }
  overlay.innerHTML = html;

  const scrim = overlay.querySelector('[data-role="scrim"]');
  if (scrim) {
    const cancel = overlay.querySelector('[data-role="dialog-cancel"]');
    const confirm = overlay.querySelector('[data-role="dialog-confirm"]');
    cancel?.addEventListener("click", () => closeDialog(false));
    confirm?.addEventListener("click", () => closeDialog(true));
    // Scrim click cancels only non-danger dialogs.
    scrim.addEventListener("mousedown", (event) => {
      if (event.target === scrim && !state.dialog?.danger) closeDialog(false);
    });
    // Focus trap: keep Tab within the dialog (ui-plan §15.1 / §12.3).
    const dialogEl = scrim.querySelector(".dialog");
    if (dialogEl) {
      confirm?.focus();
      scrim.addEventListener("keydown", (event) => {
        if (event.key !== "Tab" || !dialogEl) return;
        const focusable = dialogEl.querySelectorAll('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])');
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      });
    }
  }
}

/** Promise-based confirm dialog. Resolves true on confirm, false on cancel. */
function confirmDialog(config) {
  return new Promise((resolve) => {
    state.dialog = { ...config, _resolve: resolve };
    renderOverlay();
  });
}

function closeDialog(confirmed) {
  const dialog = state.dialog;
  if (!dialog) return;
  state.dialog = null;
  renderOverlay();
  dialog._resolve?.(confirmed);
}

/** Transient status toast. Auto-dismisses; one at a time. */
function showToast(message, tone = "default") {
  state.toast = { message, tone };
  if (state.toastTimer) window.clearTimeout(state.toastTimer);
  state.toastTimer = window.setTimeout(() => {
    state.toast = null;
    state.toastTimer = null;
    renderOverlay();
  }, 3000);
  renderOverlay();
}

async function changeSettingsTab(tab) {
  state.settingsTab = tab;
  state.settingsStatus = "";
  state.toolStatus = "";
  state.statsStatus = "";
  if (tab === "tools") await loadToolsSafely();
  if (tab === "stats") await refreshStats();
  render();
}

async function sendMessage(content) {
  if (state.chatBusy) return;
  if (!state.settings?.hasApiKey) {
    state.activeTab = "settings";
    state.settingsTab = "model";
    state.settingsStatus = "请先保存 API Key";
    pet.setMood("thinking");
    render();
    return;
  }

  const requestId = `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const outgoingMessages = [...state.messages, { role: "user", content }];
  state.messages = [...outgoingMessages, { role: "assistant", content: "" }];
  state.currentRequestId = requestId;
  state.chatBusy = true;
  state.stopRequested = false;
  beginThinking("发送中");
  pet.setMood("thinking");
  render();

  try {
    await invoke("send_chat_message", {
      request: {
        requestId,
        messages: outgoingMessages,
      },
    });
  } catch (error) {
    state.chatBusy = false;
    state.chatStatus = String(error);
    stopThinking();
    appendAssistantDelta(`\n${String(error)}`);
    pet.setMood("idle");
    render();
  }
}

function appendAssistantDelta(delta) {
  if (state.stopRequested) return;
  const last = state.messages[state.messages.length - 1];
  if (last?.role === "assistant") {
    last.content += delta;
  } else {
    state.messages.push({ role: "assistant", content: delta });
  }
}

/**
 * Locally stop the active chat: drop the requestId so the still-running backend
 * stream's later events are ignored, and return the UI to idle. The backend
 * request is not aborted (no abort channel wired through LlmClient yet), but
 * its output is discarded so the user regains control immediately.
 * (ui-plan.md §8.5 — Stop button keeps send-button position stable.)
 */
function stopChat() {
  if (!state.chatBusy) return;
  state.stopRequested = true;
  state.currentRequestId = null;
  state.chatBusy = false;
  state.chatStatus = "已停止";
  state.toolActivity = "";
  stopThinking();
  pet.setMood("idle");
  render();
}

function beginThinking(status) {
  if (!state.thinkingStartedAt) {
    state.thinkingStartedAt = Date.now();
    state.thinkingElapsedMs = 0;
  }
  state.chatStatus = status || state.chatStatus || "思考中";
  if (!state.thinkingTimer) {
    state.thinkingTimer = window.setInterval(() => {
      if (!state.thinkingStartedAt) return;
      state.thinkingElapsedMs = Date.now() - state.thinkingStartedAt;
      updateThinkingDisplay();
    }, 250);
  }
  updateThinkingDisplay();
}

function stopThinking() {
  state.thinkingStartedAt = null;
  state.thinkingElapsedMs = 0;
  if (state.thinkingTimer) {
    window.clearInterval(state.thinkingTimer);
    state.thinkingTimer = null;
  }
  updateThinkingDisplay();
}

async function toggleCompact(enabled) {
  state.compactMode = enabled;
  await appWindow.setCompact(enabled);
  render();
}

async function saveSettings(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const input = {
    apiKey: form.elements.apiKey.value,
    clearApiKey: form.elements.clearApiKey.checked,
    baseUrl: form.elements.baseUrl.value,
    model: form.elements.model.value,
    temperature: Number(form.elements.temperature.value),
    maxContextMessages: Number(form.elements.maxContextMessages.value),
    autoSystemCheckEnabled: form.elements.autoSystemCheckEnabled.checked,
    autoSystemCheckIntervalMinutes: Number(form.elements.autoSystemCheckIntervalMinutes.value),
    systemPrompt: form.elements.systemPrompt.value,
  };

  // Front-end field validation (ui-plan §9.5): catch obviously bad input before
  // hitting the backend, and surface the error next to the offending field.
  const errors = {};
  if (!input.baseUrl.trim()) errors.baseUrl = "Base URL 不能为空";
  else if (!/^https?:\/\//i.test(input.baseUrl.trim())) errors.baseUrl = "Base URL 需以 http:// 或 https:// 开头";
  if (!input.model.trim()) errors.model = "模型名不能为空";
  if (Object.keys(errors).length) {
    state.settingsFieldErrors = errors;
    showToast("请修正标红的字段", "error");
    render();
    return;
  }
  state.settingsFieldErrors = {};

  let settingsSaved = false;
  try {
    state.settings = await invoke("save_llm_settings", { input });
    state.settingsDraft = {
      ...input,
      apiKey: "",
      clearApiKey: false,
    };
    state.settingsStatus = "设置已保存";
    state.settingsSaveFailed = false;
    showToast("设置已保存", "success");
    pet.setLine(state.settings.hasApiKey ? "模型已连接" : "等待 API Key");
    settingsSaved = true;
  } catch (error) {
    state.settingsStatus = String(error);
    state.settingsSaveFailed = true;
    showToast(`保存失败：${String(error)}`, "error");
  }
  render();
  if (settingsSaved) scheduleAutoSystemCheck({ runNow: true });
}

async function toggleAlwaysOnTop() {
  state.alwaysOnTop = !state.alwaysOnTop;
  await invoke("set_always_on_top", { enabled: state.alwaysOnTop });
  render();
}

async function enableTemporaryPassthrough() {
  state.settingsStatus = "鼠标穿透已启用";
  render();
  await invoke("set_mouse_passthrough", { enabled: true });
  window.setTimeout(async () => {
    await invoke("set_mouse_passthrough", { enabled: false });
    state.settingsStatus = "鼠标穿透已关闭";
    render();
  }, 10000);
}

async function loadToolsSafely() {
  try {
    await loadTools();
  } catch (error) {
    state.toolStatus = String(error);
  }
}

async function setToolEnabled(name, enabled) {
  try {
    await invoke("set_tool_enabled", { name, enabled });
    await loadTools();
    state.toolStatus = enabled ? "工具已启用" : "工具已停用";
    showToast(enabled ? `已启用「${name}」` : `已停用「${name}」`, "success");
  } catch (error) {
    state.toolStatus = String(error);
    showToast(String(error), "error");
  }
  render();
}

async function deleteTool(name) {
  try {
    await invoke("delete_tool", { name });
    await loadTools();
    state.toolStatus = "工具已删除";
    showToast(`已删除工具「${name}」`, "success");
  } catch (error) {
    state.toolStatus = String(error);
    showToast(`删除失败：${String(error)}`, "error");
  }
  render();
}

/** Confirm tool deletion through the custom dialog (replaces window.confirm). */
async function requestDeleteTool(name) {
  const confirmed = await confirmDialog({
    title: `删除工具「${name}」？`,
    body: "删除后无法恢复。内置工具不可删除。",
    confirmLabel: "删除",
    cancelLabel: "取消",
    danger: true,
  });
  if (confirmed) deleteTool(name);
}

async function saveTool(raw) {
  // Local tools execute arbitrary programs on this machine; confirm before
  // persisting an enabled one so the risk is explicit (ui-plan.md §10.8).
  if (raw.kind === "local" && raw.enabled) {
    const confirmed = await confirmDialog({
      title: "启用本地工具？",
      body: "本地工具会在你的电脑上运行 <code>command</code> 指定的程序或脚本。请只添加你信任的工具。",
      confirmLabel: "继续保存",
      cancelLabel: "取消",
      danger: true,
    });
    if (!confirmed) return;
  }

  try {
    // Field-level validation (ui-plan §10.7).
    if (!raw.name) throw new Error("工具名称不能为空");
    if (!/^[a-zA-Z][a-zA-Z0-9_]*$/.test(raw.name)) throw new Error("工具名称需以字母开头，仅含字母数字下划线");
    if (raw.kind === "http" && !/^https?:\/\//i.test(raw.http?.url || "")) {
      throw new Error("HTTP 工具的 URL 需以 http:// 或 https:// 开头");
    }
    if (raw.kind === "local" && !raw.local?.command) throw new Error("本地工具的 command 不能为空");

    const parameters = JSON.parse(raw.parametersRaw);
    const input = {
      name: raw.name,
      displayName: raw.displayName,
      description: raw.description,
      kind: raw.kind,
      enabled: raw.enabled,
      parameters,
    };
    if (raw.kind === "http") {
      const headers = JSON.parse(raw.http.headersRaw || "[]");
      input.http = {
        method: raw.http.method,
        url: raw.http.url,
        headers,
      };
    } else if (raw.kind === "local") {
      input.local = {
        command: raw.local.command,
        args: raw.local.args ?? [],
        cwd: raw.local.cwd || null,
        timeoutSecs: raw.local.timeoutSecs ?? 30,
      };
    } else {
      throw new Error(`未知工具类型：${raw.kind}`);
    }
    await invoke("save_tool", { input });
    await loadTools();
    state.toolStatus = "工具已保存";
    showToast(`已保存工具「${raw.name}」`, "success");
  } catch (error) {
    state.toolStatus = `工具保存失败：${String(error)}`;
    showToast(`保存失败：${String(error)}`, "error");
  }
  render();
}

async function importTool(path) {
  state.toolStatus = `正在从 ${path} 导入...`;
  render();
  try {
    const tool = await invoke("import_tool_from_path", { path });
    await loadTools();
    state.toolStatus = `已导入工具：${tool.displayName}`;
    showToast(`已导入工具：${tool.displayName || tool.name}`, "success");
  } catch (error) {
    state.toolStatus = `导入失败：${String(error)}`;
    showToast(`导入失败：${String(error)}`, "error");
  }
  render();
}

async function refreshStats() {
  try {
    await loadStats();
    state.lastStatsRefreshAt = new Date();
    state.statsStatus = "统计已刷新";
    showToast("统计已刷新", "success");
  } catch (error) {
    state.statsStatus = String(error);
    showToast(`刷新失败：${String(error)}`, "error");
  }
  render();
}

function updateThinkingDisplay() {
  const status = document.querySelector('[data-role="chat-status-text"]');
  if (status) status.textContent = state.chatStatus || "";

  const clock = document.querySelector('[data-role="thinking-clock"]');
  if (!clock) return;
  if (!state.thinkingStartedAt) {
    clock.hidden = true;
    clock.textContent = "";
    return;
  }
  clock.hidden = false;
  clock.textContent = `思考 ${formatElapsed(state.thinkingElapsedMs)}`;
}

function formatElapsed(ms) {
  const totalSeconds = Math.max(0, Math.floor((Number(ms) || 0) / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function scheduleAutoSystemCheck({ runNow = false } = {}) {
  if (state.autoSystemCheckTimer) {
    window.clearInterval(state.autoSystemCheckTimer);
    state.autoSystemCheckTimer = null;
  }

  if (!state.settings?.autoSystemCheckEnabled) {
    state.autoSystemStatus = "未启用自动系统检查。";
    updateAutoSystemStatusDisplay();
    return;
  }

  const minutes = clampNumber(state.settings.autoSystemCheckIntervalMinutes, 1, 120);
  state.autoSystemStatus = state.autoSystemStatus || `已启用，每 ${minutes} 分钟自动调用 get_system_status。`;
  updateAutoSystemStatusDisplay();

  if (runNow) runAutoSystemCheck();
  state.autoSystemCheckTimer = window.setInterval(runAutoSystemCheck, minutes * 60 * 1000);
}

async function runAutoSystemCheck() {
  if (!state.settings?.autoSystemCheckEnabled || state.autoSystemCheckBusy) return;

  state.autoSystemCheckBusy = true;
  state.autoSystemStatus = "正在调用 get_system_status 检查系统状态...";
  updateAutoSystemStatusDisplay();

  try {
    const snapshot = await invoke("get_system_status", { processLimit: 8 });
    const summary = formatSystemSnapshot(snapshot);
    state.autoSystemStatus = `${formatClockTime(new Date())} 检查完成：${summary}`;
    if (!state.chatBusy) pet.setLine(summary);
  } catch (error) {
    state.autoSystemStatus = `自动检查失败：${String(error)}`;
  } finally {
    state.autoSystemCheckBusy = false;
    updateAutoSystemStatusDisplay();
  }
}

function updateAutoSystemStatusDisplay() {
  const target = document.querySelector('[data-role="auto-system-status"]');
  if (target) target.textContent = state.autoSystemStatus || "";
}

function formatSystemSnapshot(snapshot) {
  const cpu = formatPercent(snapshot?.cpuUsage);
  const memory = formatPercent(snapshot?.memory?.usagePercent);
  const topProcess = snapshot?.processes?.[0]?.name;
  return `CPU ${cpu} · 内存 ${memory}${topProcess ? ` · 高占用 ${topProcess}` : ""}`;
}

function formatPercent(value) {
  return `${(Number(value) || 0).toFixed(1)}%`;
}

function formatClockTime(date) {
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

function clampNumber(value, min, max) {
  return Math.max(min, Math.min(max, Number(value) || min));
}
