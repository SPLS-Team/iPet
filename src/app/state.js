import { detectPlatform } from "./platform.js";

export const state = {
  platform: detectPlatform(),
  // New three-mode product model (ref-plan §3, §4.2). `viewMode` is the single
  // source of truth for which shell is on screen; `compactMode` is kept in
  // sync as the boolean the Tauri window command + legacy `.compact` CSS expect.
  viewMode: "talk", // "capsule" | "talk" | "control"
  controlSection: "model", // "model" | "tools" | "usage" | "system" | "appearance"
  messages: [],
  settings: null,
  settingsDraft: null,
  settingsStatus: "",
  tools: [],
  toolStatus: "",
  toolSearch: "",
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
  // System view live data (ref-plan §6.4) — last get_system_status snapshot.
  systemSnapshot: null,
  // Appearance overrides (ref-plan §6.5). Theme is persisted; platformStyle /
  // density / reduceMotion are in-memory overrides for v0.4.0 (persistence is
  // the plan's phase-2 item).
  platformStyle: "auto", // "auto" | "macos" | "windows" | "linux"
  density: "comfortable", // "comfortable" | "compact"
  reduceMotion: false,
};
