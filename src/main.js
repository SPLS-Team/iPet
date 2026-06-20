import { appWindow, invoke, listen } from "./app/tauriBridge.js";
import { state } from "./app/state.js";
import { loadTheme, setThemePreference } from "./app/theme.js";
import { createOverlayController } from "./app/overlay.js";
import { createPetCharacter } from "./components/PetCharacter/PetCharacter.js";
import { renderChat, updateChatStreaming } from "./components/ChatBubble/ChatBubble.js";
import { renderAppShell, capsuleStatusText } from "./shell/AppShell.js";
import { talkActivityText } from "./shell/WindowChrome.js";
import { escapeHtml } from "./utils/markdown.js";
import "./styles.css";

const PERSONA_ONBOARDING_KEY = "ipet.personaOnboarding.v1";

const { renderOverlay, confirmDialog, closeDialog, showToast } = createOverlayController(state);

// The pet node is created once and moved between view slots on each render,
// so streaming mood/line updates always target live DOM (ref-plan §11.3).
const petRoot = document.createElement("div");
const pet = createPetCharacter(petRoot);

const ctx = {
  state,
  petRoot,
  appWindow,
  handlers: {
    onToggleControl: toggleControl,
    onCompact: () => setViewMode("capsule"),
    onExpand: () => setViewMode("talk"),
    onMinimize: () => appWindow.minimize(),
    onClose: () => appWindow.close(),
    onGoCapsule: () => setViewMode("capsule"),
    onControlSection: setControlSection,
    onRunSystemCheck: () => runAutoSystemCheck({ force: true }),
    onSaveSettings: saveSettings,
    onSavePersona: savePersonaSettings,
    onDismissPersonaGuide: dismissPersonaGuide,
    onToggleTop: toggleAlwaysOnTop,
    onTemporaryPassthrough: enableTemporaryPassthrough,
    onSetToolEnabled: setToolEnabled,
    onDeleteTool: requestDeleteTool,
    onSaveTool: saveTool,
    onImportTool: importTool,
    onSetComposerMode: setToolComposerMode,
    onSetTheme: setTheme,
    onSetPlatformStyle: setPlatformStyle,
    onSetDensity: setDensity,
    onSetReduceMotion: setReduceMotion,
    onRefreshStats: refreshStats,
    onSend: sendMessage,
    onStop: stopChat,
    onGoSettings: (section) => setControlSection(section === "stats" ? "usage" : section),
    onRefreshMemories: loadMemoriesSafely,
    onEditMemory: requestEditMemory,
    onDeleteMemory: requestDeleteMemory,
    onSwitchSession: switchSession,
    onNewSession: () => createSession(null),
  },
};

bootstrap();

async function setTheme(theme) {
  await setThemePreference(theme, { state, invoke, showToast, render });
}

async function bootstrap() {
  applyPlatformStyle();
  applyDensity();
  applyReduceMotion();
  await loadTheme({ state, invoke });
  await bindChatEvents();
  await loadInitialData();
  await maybeOpenPersonaOnboarding();
  render();
  scheduleAutoSystemCheck({ runNow: true });
}

// Esc closes the active dialog, or — with no dialog open — leaves the Control
// Center back to the Talk Workspace (ref-plan §4.3). Cmd/Ctrl+, toggles the
// Control Center; Cmd/Ctrl+L focuses the composer.
document.addEventListener("keydown", (event) => {
  const mod = event.metaKey || event.ctrlKey;
  if (event.key === "Escape") {
    if (state.dialog) {
      event.preventDefault();
      closeDialog(false);
    } else if (state.viewMode === "control") {
      event.preventDefault();
      setViewMode("talk");
    }
    return;
  }
  if (mod && event.key === ",") {
    event.preventDefault();
    if (state.viewMode !== "capsule") toggleControl();
    return;
  }
  if (mod && (event.key === "l" || event.key === "L")) {
    if (state.viewMode !== "talk") return;
    const textarea = document.querySelector('[data-role="form"] [name="prompt"]');
    if (textarea) {
      event.preventDefault();
      textarea.focus();
    }
  }
});

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
      // Fast path: patch only the last assistant bubble. A full render rebuilds
      // the shell + every message + re-binds the form, which goes quadratic on
      // long chats while streaming (ref-plan §11.3, §15.7).
      const panel = document.querySelector("#panel");
      const patched = state.viewMode === "talk" && updateChatStreaming(panel, state);
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
  await Promise.allSettled([
    loadSettings(),
    loadSessions(),
    loadMessages(),
    loadTools(),
    loadStats(),
  ]);
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

// --- sessions -------------------------------------------------------------
// The backend owns the active session id (AppState::current_session_id) so
// restarts and cross-window writes stay consistent. loadSessions reconciles
// the frontend mirror; switchSession persists the choice and reloads messages.

async function loadSessions() {
  state.sessionsLoading = true;
  try {
    state.sessions = await invoke("list_sessions");
    const current = await invoke("get_current_session");
    state.currentSessionId = current;
  } catch (error) {
    state.toast = { type: "error", message: String(error) };
  } finally {
    state.sessionsLoading = false;
  }
}

async function createSession(title) {
  const session = await invoke("create_session", { title: title || null });
  state.sessions = [session, ...state.sessions];
  await switchSession(session.id);
}

async function switchSession(id) {
  if (id === state.currentSessionId) return;
  await invoke("set_current_session", { id });
  state.currentSessionId = id;
  await loadMessages();
}

async function renameSession(id, title) {
  const updated = await invoke("rename_session", { id, title });
  if (updated) {
    state.sessions = state.sessions.map((s) => (s.id === id ? updated : s));
  }
}

async function deleteSession(id) {
  // Backend returns the next active id (it re-seeds if the deleted one was
  // current), so the UI always has a valid session to show.
  const nextId = await invoke("delete_session", { id });
  state.sessions = state.sessions.filter((s) => s.id !== id);
  if (nextId !== state.currentSessionId) {
    state.currentSessionId = nextId;
    await loadMessages();
  }
}

// --- memories -------------------------------------------------------------

async function loadMemories() {
  state.memoriesLoading = true;
  try {
    state.memories = await invoke("list_memories");
  } catch (error) {
    state.memoryStatus = String(error);
  } finally {
    state.memoriesLoading = false;
  }
}

async function updateMemory(id, content, category) {
  const updated = await invoke("update_memory", { id, content, category: category || null });
  if (updated) {
    state.memories = state.memories.map((m) => (m.id === id ? updated : m));
  }
  return updated;
}

async function deleteMemory(id) {
  await invoke("delete_memory", { id });
  state.memories = state.memories.filter((m) => m.id !== id);
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
  const root = document.querySelector("#app");
  renderAppShell(root, ctx);

  if (state.viewMode === "capsule") {
    pet.setLine(capsuleStatusText(state));
    renderOverlay();
    return;
  }

  // Talk workspace mounts the conversation into #panel. The Control Center's
  // #panel is filled by bindControlCenter() inside renderAppShell.
  if (state.viewMode === "talk") {
    const panel = document.querySelector("#panel");
    renderChat(panel, state, { onSend: sendMessage, onStop: stopChat, onGoSettings: ctx.handlers.onGoSettings });
  }

  renderOverlay();
}

/* -------------------------------------------------------------------------- */
/* View-mode navigation (ref-plan §4.3)                                       */
/*                                                                            */
/* Window sizing rule (was the bug source): only crossing the *capsule*       */
/* boundary changes the window size — talk↔control switches leave the window   */
/* alone. Entering the Control Center widens the window up to the sidebar     */
/* layout threshold (≥720px) so the left nav is actually visible; leaving it  */
/* keeps whatever size the user has, instead of snapping back to a stale saved*/
/* value (which is what the old unconditional setCompact(false) did).         */
/* -------------------------------------------------------------------------- */

// Width below which the Control Center collapses to the segmented (no-sidebar)
// nav. Matches the @media (min-width: 720px) breakpoint in responsive.css.
const CONTROL_SIDEBAR_WIDTH = 720;

async function setViewMode(mode) {
  const wasCapsule = state.viewMode === "capsule";
  const isCapsule = mode === "capsule";
  state.viewMode = mode;
  state.compactMode = isCapsule;

  // Only resize when crossing the capsule boundary. talk↔control must NOT
  // touch the window — that was forcing the window to a stale narrow size on
  // every return to talk.
  if (wasCapsule !== isCapsule) {
    await appWindow.setCompact(isCapsule);
  }

  // Entering the Control Center from a narrow window: widen so the sidebar
  // nav is visible. (Height unchanged.)
  if (mode === "control" && !wasCapsule) {
    await ensureControlSidebarWidth();
  }

  render();
}

/** If the window is narrower than the sidebar threshold, grow it to 720px so
 *  the Control Center's left nav shows. No-op in browser preview. */
async function ensureControlSidebarWidth() {
  const size = await appWindow.innerSize();
  if (!size) return;
  if (size.width < CONTROL_SIDEBAR_WIDTH) {
    await appWindow.setInnerSize(CONTROL_SIDEBAR_WIDTH, Math.max(size.height, 720));
  }
}

function toggleControl() {
  setViewMode(state.viewMode === "control" ? "talk" : "control");
}

async function maybeOpenPersonaOnboarding() {
  if (hasCompletedPersonaOnboarding()) return;
  state.personaOnboardingVisible = true;
  state.controlSection = "persona";
  state.viewMode = "control";
  state.compactMode = false;
  await ensureControlSidebarWidth();
}

function hasCompletedPersonaOnboarding() {
  try {
    return window.localStorage.getItem(PERSONA_ONBOARDING_KEY) === "done";
  } catch (_error) {
    return true;
  }
}

function completePersonaOnboarding() {
  state.personaOnboardingVisible = false;
  try {
    window.localStorage.setItem(PERSONA_ONBOARDING_KEY, "done");
  } catch (_error) {
    // Storage can be unavailable in hardened webviews; the current session still
    // hides the guide once the user dismisses or saves it.
  }
}

function dismissPersonaGuide() {
  completePersonaOnboarding();
  render();
}

async function setControlSection(section) {
  state.controlSection = section;
  if (state.viewMode !== "control") {
    state.viewMode = "control";
    state.compactMode = false;
    await ensureControlSidebarWidth();
  }
  if (section === "tools") await loadToolsSafely();
  if (section === "usage") await refreshStatsSilently();
  if (section === "memory") await loadMemoriesSafely();
  render();
}

/** Switch the tool composer between import / http / local (ui-plan §10.5). */
function setToolComposerMode(mode) {
  state.toolComposerMode = mode;
  render();
}

async function savePersonaSettings(partial) {
  const saved = await saveSettings(partial);
  if (saved) {
    completePersonaOnboarding();
    render();
  }
}

async function sendMessage(content) {
  if (state.chatBusy) return;
  if (!state.settings?.hasApiKey) {
    state.controlSection = "model";
    state.viewMode = "control";
    state.compactMode = false;
    state.settingsStatus = "请先保存 API Key";
    await ensureControlSidebarWidth();
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
    // Drop the empty trailing assistant placeholder and surface the failure as
    // a distinct error bubble (ref-plan §12.3) instead of inlining raw text.
    const last = state.messages[state.messages.length - 1];
    if (last?.role === "assistant" && !last.content) state.messages.pop();
    state.messages.push({ role: "assistant", type: "error", content: String(error) });
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

/* -------------------------------------------------------------------------- */
/* Settings save — partial override merged with the shared draft              */
/* (Model and System views each save their own subset; the backend still      */
/* receives a full LlmSettingsInput, ref-plan §6.1/§6.4.)                     */
/* -------------------------------------------------------------------------- */

async function saveSettings(partial = {}) {
  const base = state.settingsDraft ?? {};
  const input = {
    apiKey: partial.apiKey ?? "",
    clearApiKey: partial.clearApiKey ?? false,
    baseUrl: partial.baseUrl ?? base.baseUrl,
    model: partial.model ?? base.model,
    temperature: partial.temperature ?? base.temperature,
    maxContextMessages: partial.maxContextMessages ?? base.maxContextMessages,
    systemPrompt: partial.systemPrompt ?? base.systemPrompt,
    autoSystemCheckEnabled: partial.autoSystemCheckEnabled ?? base.autoSystemCheckEnabled,
    autoSystemCheckIntervalMinutes:
      partial.autoSystemCheckIntervalMinutes ?? base.autoSystemCheckIntervalMinutes,
  };

  // Validate only the fields this view is responsible for (ui-plan §9.5).
  const errors = {};
  if ("baseUrl" in partial) {
    if (!input.baseUrl.trim()) errors.baseUrl = "Base URL 不能为空";
    else if (!/^https?:\/\//i.test(input.baseUrl.trim()))
      errors.baseUrl = "Base URL 需以 http:// 或 https:// 开头";
  }
  if ("model" in partial && !input.model.trim()) errors.model = "模型名不能为空";
  if (Object.keys(errors).length) {
    state.settingsFieldErrors = errors;
    showToast("请修正标红的字段", "error");
    render();
    return false;
  }
  state.settingsFieldErrors = {};

  let saved = false;
  try {
    state.settings = await invoke("save_llm_settings", { input });
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
    state.settingsStatus = "设置已保存";
    state.settingsSaveFailed = false;
    showToast("设置已保存", "success");
    saved = true;
  } catch (error) {
    state.settingsStatus = String(error);
    state.settingsSaveFailed = true;
    showToast(`保存失败：${String(error)}`, "error");
  }
  render();
  if (saved) {
    const autoChanged =
      "autoSystemCheckEnabled" in partial || "autoSystemCheckIntervalMinutes" in partial;
    scheduleAutoSystemCheck({ runNow: autoChanged && state.settings.autoSystemCheckEnabled });
  }
  return saved;
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

async function loadMemoriesSafely() {
  try {
    await loadMemories();
  } catch (error) {
    state.memoryStatus = String(error);
  }
}

async function requestDeleteMemory(id, key) {
  const confirmed = await confirmDialog({
    title: `删除记忆「${key ?? id}」？`,
    body: "删除后无法恢复。下次对话起模型将不再记得这条信息。",
    confirmLabel: "删除",
    cancelLabel: "取消",
    danger: true,
  });
  if (!confirmed) return;
  try {
    await deleteMemory(id);
    showToast(`已删除记忆「${key ?? id}」`, "success");
  } catch (error) {
    state.memoryStatus = String(error);
    showToast(`删除失败：${String(error)}`, "error");
  }
  render();
}

async function requestEditMemory(id) {
  const memory = (state.memories ?? []).find((m) => m.id === id);
  if (!memory) return;
  // Inline-edit: swap the content <p> for a textarea + save/cancel. Kept
  // lightweight rather than a full dialog — most edits are small content tweaks.
  const contentEl = document.querySelector(`[data-role="memory-content-${id}"]`);
  if (!contentEl) return;
  const card = contentEl.closest(".memory-card");
  if (!card || card.dataset.editing === "true") return;
  card.dataset.editing = "true";
  const original = memory.content;
  contentEl.innerHTML = `
    <textarea class="memory-edit-input" rows="3">${escapeHtml(original)}</textarea>
    <div class="memory-edit-actions">
      <input class="memory-edit-category" type="text" value="${escapeHtml(memory.category || "general")}" placeholder="分类" />
      <button class="text-button primary" type="button" data-memory-save="${id}">保存</button>
      <button class="text-button" type="button" data-memory-cancel="${id}">取消</button>
    </div>
  `;
  const saveBtn = card.querySelector(`[data-memory-save="${id}"]`);
  const cancelBtn = card.querySelector(`[data-memory-cancel="${id}"]`);
  const input = card.querySelector(".memory-edit-input");
  const categoryInput = card.querySelector(".memory-edit-category");
  saveBtn?.addEventListener("click", async () => {
    const content = input.value.trim();
    if (!content) {
      showToast("内容不能为空", "error");
      return;
    }
    try {
      await updateMemory(id, content, categoryInput.value.trim());
      showToast("记忆已更新", "success");
    } catch (error) {
      state.memoryStatus = String(error);
      showToast(`保存失败：${String(error)}`, "error");
    }
    delete card.dataset.editing;
    render();
  });
  cancelBtn?.addEventListener("click", () => {
    delete card.dataset.editing;
    render();
  });
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

/* -------------------------------------------------------------------------- */
/* Appearance overrides (ref-plan §6.5)                                       */
/* -------------------------------------------------------------------------- */

function applyPlatformStyle() {
  const p = state.platformStyle === "auto" ? state.platform : state.platformStyle;
  document.documentElement.dataset.platform = p || "unknown";
}

function applyDensity() {
  document.documentElement.dataset.density = state.density || "comfortable";
}

function applyReduceMotion() {
  document.documentElement.classList.toggle("reduce-motion", Boolean(state.reduceMotion));
}

function setPlatformStyle(style) {
  state.platformStyle = style;
  applyPlatformStyle();
  render();
}

function setDensity(density) {
  state.density = density;
  applyDensity();
  render();
}

function setReduceMotion(enabled) {
  state.reduceMotion = enabled;
  applyReduceMotion();
}

/* -------------------------------------------------------------------------- */
/* Thinking timer + auto system check                                         */
/* -------------------------------------------------------------------------- */

function updateThinkingDisplay() {
  const status = document.querySelector('[data-role="chat-status-text"]');
  if (status) status.textContent = state.chatStatus || "";

  const clock = document.querySelector('[data-role="thinking-clock"]');
  if (clock) {
    if (!state.thinkingStartedAt) {
      clock.hidden = true;
      clock.textContent = "";
    } else {
      clock.hidden = false;
      clock.textContent = `思考 ${formatElapsed(state.thinkingElapsedMs)}`;
    }
  }

  // Keep the capsule line + talk/chrome status in sync without a full render
  // (ref-plan §11.3 — status changes patch only their own elements).
  pet.setLine(capsuleStatusText(state));
  const talkState = document.querySelector('[data-role="talk-state"]');
  if (talkState) talkState.textContent = talkActivityText(state);
  const chromeActivity = document.querySelector('[data-role="chrome-activity"]');
  if (chromeActivity) chromeActivity.textContent = talkActivityText(state);
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

async function runAutoSystemCheck({ force = false } = {}) {
  if (!force && (!state.settings?.autoSystemCheckEnabled || state.autoSystemCheckBusy)) return;

  state.autoSystemCheckBusy = true;
  state.autoSystemStatus = "正在调用 get_system_status 检查系统状态...";
  updateAutoSystemStatusDisplay();
  updateThinkingDisplay();

  try {
    const snapshot = await invoke("get_system_status", { processLimit: 8 });
    state.systemSnapshot = snapshot;
    const summary = formatSystemSnapshot(snapshot);
    state.autoSystemStatus = `${formatClockTime(new Date())} 检查完成：${summary}`;
    if (!state.chatBusy) pet.setLine(summary);
  } catch (error) {
    state.autoSystemStatus = `自动检查失败：${String(error)}`;
  } finally {
    state.autoSystemCheckBusy = false;
    updateAutoSystemStatusDisplay();
    updateThinkingDisplay();
    // If the System view is open, refresh its live-status card with the new snapshot.
    if (state.viewMode === "control" && state.controlSection === "system") render();
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
