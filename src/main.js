import { appWindow, invoke, listen } from "./tauriBridge.js";
import { createPetCharacter } from "./components/PetCharacter/PetCharacter.js";
import { renderChat } from "./components/ChatBubble/ChatBubble.js";
import { renderSettings } from "./components/SettingsPanel/SettingsPanel.js";
import "./styles.css";

const state = {
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
  chatBusy: false,
  chatStatus: "",
  currentRequestId: null,
  alwaysOnTop: true,
  compactMode: false,
  thinkingStartedAt: null,
  thinkingElapsedMs: 0,
  thinkingTimer: null,
  autoSystemCheckTimer: null,
  autoSystemCheckBusy: false,
  autoSystemStatus: "",
};

let pet;
let compactDragStart = null;
let compactDragStarted = false;

bootstrap();

async function bootstrap() {
  document.querySelector("#app").innerHTML = `
    <main class="app-shell">
      <header class="titlebar" data-tauri-drag-region>
        <div class="brand" data-tauri-drag-region>
          <span class="brand-mark" aria-hidden="true"></span>
          <span data-tauri-drag-region>iPet</span>
        </div>
        <div class="window-actions">
          <button class="window-button" data-action="compact" title="收起" aria-label="收起">◱</button>
          <button class="window-button" data-action="minimize" title="最小化" aria-label="最小化">−</button>
          <button class="window-button danger" data-action="close" title="关闭" aria-label="关闭">×</button>
        </div>
      </header>
      <section class="pet-wrap">
        <div id="pet"></div>
      </section>
      <nav class="tabbar" aria-label="main">
        <button class="tab active" data-tab="chat"><span>✦</span>聊天</button>
        <button class="tab" data-tab="settings"><span>⚙</span>设置</button>
      </nav>
      <section id="panel" class="panel"></section>
    </main>
  `;

  pet = createPetCharacter(document.querySelector("#pet"));
  bindShellEvents();
  await bindChatEvents();
  await loadInitialData();
  render();
  scheduleAutoSystemCheck({ runNow: true });
}

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
      pet.setMood("thinking");
    } else if (payload.kind === "tool") {
      beginThinking(payload.content || "正在使用工具");
      pet.setMood("thinking");
    } else if (payload.kind === "delta") {
      stopThinking();
      appendAssistantDelta(payload.content);
      state.chatStatus = "";
      pet.setMood("talking");
      shouldRender = true;
    } else if (payload.kind === "done") {
      state.chatBusy = false;
      state.chatStatus = "";
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
  if (state.compactMode) {
    pet.setLine("点击展开");
    return;
  }

  document.querySelectorAll("[data-tab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.tab === state.activeTab);
  });

  const panel = document.querySelector("#panel");
  if (state.activeTab === "chat") {
    renderChat(panel, state, { onSend: sendMessage });
  } else {
    renderSettings(panel, state, {
      onSettingsTab: changeSettingsTab,
      onSaveSettings: saveSettings,
      onToggleTop: toggleAlwaysOnTop,
      onTemporaryPassthrough: enableTemporaryPassthrough,
      onSetToolEnabled: setToolEnabled,
      onDeleteTool: deleteTool,
      onSaveTool: saveTool,
      onRefreshStats: refreshStats,
    });
  }
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
  const last = state.messages[state.messages.length - 1];
  if (last?.role === "assistant") {
    last.content += delta;
  } else {
    state.messages.push({ role: "assistant", content: delta });
  }
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

  let settingsSaved = false;
  try {
    state.settings = await invoke("save_llm_settings", { input });
    state.settingsDraft = {
      ...input,
      apiKey: "",
      clearApiKey: false,
    };
    state.settingsStatus = "设置已保存";
    pet.setLine(state.settings.hasApiKey ? "模型已连接" : "等待 API Key");
    settingsSaved = true;
  } catch (error) {
    state.settingsStatus = String(error);
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
  } catch (error) {
    state.toolStatus = String(error);
  }
  render();
}

async function deleteTool(name) {
  try {
    await invoke("delete_tool", { name });
    await loadTools();
    state.toolStatus = "工具已删除";
  } catch (error) {
    state.toolStatus = String(error);
  }
  render();
}

async function saveTool(raw) {
  try {
    const parameters = JSON.parse(raw.parametersRaw);
    const headers = JSON.parse(raw.http.headersRaw || "[]");
    const input = {
      name: raw.name,
      displayName: raw.displayName,
      description: raw.description,
      kind: "http",
      enabled: raw.enabled,
      parameters,
      http: {
        method: raw.http.method,
        url: raw.http.url,
        headers,
      },
    };
    await invoke("save_tool", { input });
    await loadTools();
    state.toolStatus = "工具已保存";
  } catch (error) {
    state.toolStatus = `工具保存失败：${String(error)}`;
  }
  render();
}

async function refreshStats() {
  try {
    await loadStats();
    state.statsStatus = "统计已刷新";
  } catch (error) {
    state.statsStatus = String(error);
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
