const THEME_KEY = "ipet:theme";
const THEME_OPTIONS = ["system", "light", "dark"];

export async function loadTheme({ state, invoke }) {
  try {
    const stored = await invoke("get_preference", { key: THEME_KEY });
    if (stored && THEME_OPTIONS.includes(stored)) {
      state.theme = stored;
    }
  } catch {
    // Older builds may not expose preference commands. Fall back to system.
  }
  applyTheme(state);
}

export function applyTheme(state) {
  if (state.theme === "system") {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = state.theme;
  }
}

export async function setThemePreference(theme, { state, invoke, showToast, render }) {
  if (!THEME_OPTIONS.includes(theme)) return;
  state.theme = theme;
  applyTheme(state);
  try {
    await invoke("set_preference", { key: THEME_KEY, value: theme });
  } catch (error) {
    showToast(`主题保存失败：${String(error)}`, "error");
  }
  render();
}
