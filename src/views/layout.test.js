import { describe, it, expect, beforeEach } from "vitest";
import { renderModelView } from "./ModelView.js";
import { renderPersonaView } from "./PersonaView.js";
import { renderToolsView } from "./ToolsView.js";
import { renderUsageView } from "./UsageView.js";
import { renderSystemView } from "./SystemView.js";
import { renderAppearanceView } from "./AppearanceView.js";
import { renderMemoryView } from "./MemoryView.js";

// Layout sanity for the six Control Center sections. We can't do real
// pixel layout in jsdom, but we CAN assert the structural things that cause
// overlap/overflow: the control-panel content not being wrapped in a height-
// stealing flex, complex labels (textarea/field-error/checkbox) surviving the
// macOS row-override, every card having a heading, and no element escaping the
// scroll container. These catch the regressions flagged in the review pass.

function makeState(overrides = {}) {
  return {
    platform: "windows",
    settings: {
      hasApiKey: true,
      settingsPath: "C:/Users/pc/.config/ipet/settings.json",
      baseUrl: "https://api.openai.com/v1",
      model: "gpt-4.1-mini",
      temperature: 0.7,
      maxContextMessages: 18,
      systemPrompt: "你是 iPet，一个常驻桌面的轻量助手。",
      autoSystemCheckEnabled: true,
      autoSystemCheckIntervalMinutes: 10,
    },
    settingsDraft: {
      apiKey: "",
      clearApiKey: false,
      baseUrl: "https://api.openai.com/v1",
      model: "gpt-4.1-mini",
      temperature: 0.7,
      maxContextMessages: 18,
      autoSystemCheckEnabled: true,
      autoSystemCheckIntervalMinutes: 10,
      systemPrompt: "你是 iPet，一个常驻桌面的轻量助手。",
    },
    tools: [],
    toolStatus: "",
    toolSearch: "",
    toolComposerMode: "http",
    stats: {
      promptTokens: 1200,
      completionTokens: 840,
      totalTokens: 2040,
      requests: 6,
      toolCalls: 3,
      byDay: [{ label: "2026-06-17", promptTokens: 1200, completionTokens: 840, totalTokens: 2040, requests: 6 }],
      byModel: [{ label: "gpt-4.1-mini", promptTokens: 1200, completionTokens: 840, totalTokens: 2040, requests: 6 }],
      recent: [{ model: "gpt-4.1-mini", promptTokens: 200, completionTokens: 140, totalTokens: 340, toolCalls: 1 }],
    },
    lastStatsRefreshAt: new Date(),
    systemSnapshot: null,
    autoSystemStatus: "",
    autoSystemCheckBusy: false,
    alwaysOnTop: false,
    theme: "system",
    platformStyle: "auto",
    density: "comfortable",
    reduceMotion: false,
    settingsFieldErrors: {},
    settingsSaveFailed: false,
    settingsStatus: "",
    chatBusy: false,
    ...overrides,
  };
}

const handlers = {
  onSaveSettings: () => {},
  onSavePersona: () => {},
  onDismissPersonaGuide: () => {},
  onToggleTop: () => {},
  onTemporaryPassthrough: () => {},
  onGoCapsule: () => {},
  onRunSystemCheck: () => {},
  onSetTheme: () => {},
  onSetPlatformStyle: () => {},
  onSetDensity: () => {},
  onSetReduceMotion: () => {},
  onRefreshStats: () => {},
  onSetToolEnabled: () => {},
  onDeleteTool: () => {},
  onSaveTool: () => {},
  onImportTool: () => {},
  onSetComposerMode: () => {},
  onRefreshMemories: () => {},
  onEditMemory: () => {},
  onDeleteMemory: () => {},
};

const PLATFORMS = ["windows", "macos", "linux", "unknown"];

let container;
beforeEach(() => {
  container = document.createElement("div");
  document.body.innerHTML = "";
  document.body.appendChild(container);
});

describe("Control Center section layout sanity", () => {
  it.each(PLATFORMS)("model: connection and generation cards have headings (%s)", (plat) => {
    document.documentElement.dataset.platform = plat;
    renderModelView(container, makeState(), handlers);
    expect(container.querySelector(".settings-page")).toBeTruthy();
    const cards = container.querySelectorAll(".settings-card");
    expect(cards.length).toBeGreaterThanOrEqual(3);
    cards.forEach((card) => expect(card.querySelector("h3")).toBeTruthy());
    expect(container.querySelector('[name="systemPrompt"]')).toBeNull();
    // Submit button is the only primary action.
    expect(container.querySelectorAll('button[type="submit"]').length).toBe(1);
    expect(container.querySelector(".form-actions button[type='submit']")).toBeTruthy();
  });

  it.each(PLATFORMS)("persona: presets, structured fields and prompt preview render (%s)", (plat) => {
    document.documentElement.dataset.platform = plat;
    renderPersonaView(container, makeState(), handlers);
    expect(container.querySelector(".persona-form.settings-page")).toBeTruthy();
    expect(container.querySelectorAll(".persona-preset").length).toBe(3);
    expect(container.querySelector('[name="displayName"]')).toBeTruthy();
    expect(container.querySelector('[name="toolPolicy"]')).toBeTruthy();
    const prompt = container.querySelector('[name="systemPrompt"]');
    expect(prompt).toBeTruthy();
    expect(prompt.tagName).toBe("TEXTAREA");
    expect(container.querySelectorAll('button[type="submit"]').length).toBe(1);
  });

  it.each(PLATFORMS)("tools: composer shows exactly one form, not three (%s)", (plat) => {
    document.documentElement.dataset.platform = plat;
    renderToolsView(container, makeState({ toolComposerMode: "http" }), handlers);
    // Only the active composer form renders.
    const forms = container.querySelectorAll('[data-role="tool-form"], [data-role="local-tool-form"], [data-role="import-form"]');
    expect(forms.length).toBe(1);
    // Search box present.
    expect(container.querySelector(".tool-page")).toBeTruthy();
    expect(container.querySelector(".tool-toolbar")).toBeTruthy();
    expect(container.querySelector(".tool-list-section")).toBeTruthy();
    expect(container.querySelector(".tool-list-window")).toBeTruthy();
    expect(container.querySelector(".tool-reference")?.tagName).toBe("DETAILS");
    expect(container.querySelector('[name="toolSearch"]')).toBeTruthy();
    expect(container.querySelector('[data-composer-mode="http"]').getAttribute("tabindex")).toBe("0");
    expect(container.querySelector('[data-composer-mode="import"]').getAttribute("tabindex")).toBe("-1");
  });

  it.each(PLATFORMS)("usage: trend bars render, metrics present, empty-state hidden (%s)", (plat) => {
    document.documentElement.dataset.platform = plat;
    renderUsageView(container, makeState(), handlers);
    expect(container.querySelector(".usage-page")).toBeTruthy();
    expect(container.querySelectorAll(".metric").length).toBe(4);
    expect(container.querySelector(".metric-primary")).toBeTruthy();
    expect(container.querySelector(".trend-card")).toBeTruthy();
    expect(container.querySelector(".trend-card [data-action='refresh-stats']")).toBeTruthy();
    expect(container.querySelectorAll(".trend-bar").length).toBeGreaterThan(0);
    expect(container.querySelector(".empty-state")).toBeNull();
  });

  it("usage: empty stats show explanatory empty state", () => {
    renderUsageView(container, makeState({ stats: null }), handlers);
    expect(container.querySelector(".empty-state")).toBeTruthy();
    expect(container.querySelector(".usage-empty")).toBeTruthy();
    expect(container.querySelector(".metric")).toBeNull();
  });

  it.each(PLATFORMS)("system: live-status card + window toggles present (%s)", (plat) => {
    document.documentElement.dataset.platform = plat;
    renderSystemView(container, makeState(), handlers);
    expect(container.querySelector(".system-page")).toBeTruthy();
    expect(container.querySelector('[data-role="auto-system-status"]')).toBeTruthy();
    expect(container.querySelector('[data-action="top"]')).toBeTruthy();
    expect(container.querySelector('[data-action="passthrough"]')).toBeTruthy();
    expect(container.querySelector('[data-action="compact"]')).toBeTruthy();
    expect(container.querySelector('[data-action="run-check"]')).toBeTruthy();
  });

  it("system: notification toggles render and submit carries them", () => {
    const calls = [];
    const localHandlers = { ...handlers, onSaveSettings: (partial) => calls.push(partial) };
    renderSystemView(
      container,
      makeState({ settingsDraft: { ...makeState().settingsDraft, notifyOnReply: true } }),
      localHandlers,
    );
    expect(container.querySelector('[name="notifyOnReply"]')).toBeTruthy();
    expect(container.querySelector('[name="notifyOnSystemAlert"]')).toBeTruthy();
    expect(container.querySelector('[name="notifyOnReply"]').checked).toBe(true);
    const form = container.querySelector('[data-role="system-form"]');
    form.requestSubmit();
    expect(calls[0].notifyOnReply).toBe(true);
    expect(calls[0].notifyOnSystemAlert).toBe(false);
  });

  it.each(PLATFORMS)("appearance: theme/platform/density segments + motion toggle (%s)", (plat) => {
    document.documentElement.dataset.platform = plat;
    renderAppearanceView(container, makeState(), handlers);
    expect(container.querySelector(".appearance-page")).toBeTruthy();
    expect(container.querySelectorAll('[data-theme-mode]').length).toBe(3);
    expect(container.querySelectorAll('[data-platform-style]').length).toBe(4);
    expect(container.querySelectorAll('[data-density]').length).toBe(2);
    expect(container.querySelector('[data-theme-mode="system"]').getAttribute("tabindex")).toBe("0");
    expect(container.querySelector('[data-density="comfortable"]').getAttribute("tabindex")).toBe("0");
    expect(container.querySelector('[name="reduceMotion"]')).toBeTruthy();
  });

  it("macos profile keeps the persona prompt preview as a full textarea", () => {
    document.documentElement.dataset.platform = "macos";
    renderPersonaView(container, makeState(), handlers);
    const prompt = container.querySelector('[name="systemPrompt"]');
    expect(prompt).toBeTruthy();
    expect(prompt.getAttribute("rows")).toBe("10");
  });

  it("model: provider preset dropdown + model datalist + fetch button present", () => {
    renderModelView(container, makeState({ providerPreset: "openai", modelList: ["gpt-4o", "gpt-4o-mini"] }), handlers);
    expect(container.querySelector('[data-role="provider-preset"]')).toBeTruthy();
    expect(container.querySelector('[data-role="provider-preset"]').value).toBe("openai");
    expect(container.querySelector('[data-role="model-list"]')).toBeTruthy();
    expect(container.querySelectorAll('[data-role="model-list"] option').length).toBe(2);
    expect(container.querySelector('[data-action="fetch-models"]')).toBeTruthy();
  });

  it("model: .settings-form is a plain container — cards are its children, not peers of a card wrapper", () => {
    renderModelView(container, makeState(), handlers);
    const form = container.querySelector(".settings-form");
    expect(form).toBeTruthy();
    // The form's direct children are the status + cards; the form itself is not
    // tagged as a card surface (no card-in-card).
    expect(form.classList.contains("settings-card")).toBe(false);
    expect(form.classList.contains("settings-status")).toBe(false);
    // And it does contain card surfaces.
    expect(form.querySelectorAll(":scope > .settings-card").length).toBeGreaterThanOrEqual(3);
    expect(form.querySelector(":scope > .settings-status")).toBeTruthy();
  });

  it("tools: the composer form is NOT nested inside a .settings-card", () => {
    renderToolsView(container, makeState({ toolComposerMode: "http" }), handlers);
    const form = container.querySelector('[data-role="tool-form"]');
    expect(form).toBeTruthy();
    // The form is itself the card; its nearest section ancestor must not be a
    // .settings-card (that was the card-in-card nesting).
    expect(form.closest(".settings-card")).toBeNull();
    expect(form.closest(".section-block")).toBeTruthy();
  });

  it("tools: every tool-meta runtime badge is the last span and carries the URL/command text", () => {
    renderToolsView(
      container,
      makeState({
        tools: [
          {
            name: "long_http",
            displayName: "Long URL",
            description: "x",
            kind: "http",
            enabled: true,
            builtIn: false,
            http: { method: "GET", url: "https://example.com/very/long/path" },
            parameters: { type: "object" },
          },
        ],
      }),
      handlers,
    );
    const meta = container.querySelector(".tool-meta");
    expect(meta).toBeTruthy();
    const spans = meta.querySelectorAll("span");
    // badge, kind badge, runtime badge — runtime is last and holds the URL.
    expect(spans.length).toBe(3);
    expect(spans[2].textContent).toContain("https://example.com");
  });

  it("memory: shows empty state when no memories", () => {
    renderMemoryView(container, makeState({ memories: [] }), handlers);
    expect(container.querySelector(".empty-state")).toBeTruthy();
    expect(container.querySelector(".memory-card")).toBeNull();
  });

  it("memory: renders one card per memory with key, category, content", () => {
    renderMemoryView(
      container,
      makeState({
        memories: [
          {
            id: 1,
            key: "user_role",
            content: "Rust developer",
            category: "user",
            createdAt: "2026-06-20T10:00:00Z",
            updatedAt: "2026-06-20T10:00:00Z",
            lastUsedAt: null,
            useCount: 0,
          },
        ],
      }),
      handlers,
    );
    const card = container.querySelector(".memory-card");
    expect(card).toBeTruthy();
    expect(card.querySelector("code").textContent).toBe("user_role");
    expect(card.textContent).toContain("Rust developer");
    expect(card.querySelector(".badge").textContent).toBe("user");
    // edit + delete buttons are wired with the memory id
    expect(card.querySelector('[data-memory-edit="1"]')).toBeTruthy();
    expect(card.querySelector('[data-memory-delete="1"]')).toBeTruthy();
  });
});
