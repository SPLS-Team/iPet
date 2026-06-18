import { describe, it, expect, beforeEach } from "vitest";
import { renderAppShell } from "./AppShell.js";
import { createPetCharacter } from "../components/PetCharacter/PetCharacter.js";

// Smoke test: the new three-mode shell must paint without throwing and expose
// the right regions per view (ref-plan §5, Phase 1 acceptance). This guards the
// render dispatch + pet-node placement against regressions in later phases.

function makeCtx(viewMode) {
  const state = {
    platform: "windows",
    viewMode,
    controlSection: "model",
    settings: { hasApiKey: false },
    chatBusy: false,
    chatStatus: "",
    toolActivity: "",
    autoSystemCheckBusy: false,
  };
  const petRoot = document.createElement("div");
  createPetCharacter(petRoot);
  const calls = [];
  return {
    ctx: {
      state,
      petRoot,
      appWindow: { startDragging: () => calls.push("drag"), setCompact: () => calls.push("compact") },
      handlers: {
        onToggleControl: () => calls.push("toggleControl"),
        onCompact: () => calls.push("compact-btn"),
        onExpand: () => calls.push("expand"),
        onMinimize: () => calls.push("minimize"),
        onClose: () => calls.push("close"),
      },
    },
    state,
    petRoot,
    calls,
  };
}

describe("AppShell view dispatch", () => {
  let root;

  beforeEach(() => {
    root = document.createElement("div");
    document.body.innerHTML = "";
    document.body.appendChild(root);
  });

  it("renders talk workspace with chrome, header and panel", () => {
    const { ctx } = makeCtx("talk");
    renderAppShell(root, ctx);
    expect(root.querySelector(".titlebar")).toBeTruthy();
    expect(root.querySelector(".talk-header")).toBeTruthy();
    expect(root.querySelector("#panel")).toBeTruthy();
    expect(root.querySelector("#overlay")).toBeTruthy();
    expect(root.querySelector(".app-shell").dataset.view).toBe("talk");
  });

  it("renders capsule without titlebar and places the pet", () => {
    const { ctx, petRoot } = makeCtx("capsule");
    renderAppShell(root, ctx);
    expect(root.querySelector(".titlebar")).toBeNull();
    expect(root.querySelector("[data-capsule]")).toBeTruthy();
    expect(root.querySelector(".app-shell").classList.contains("compact")).toBe(true);
    // The persistent pet node should have been moved into the capsule slot.
    expect(petRoot.parentElement).toBeTruthy();
    expect(petRoot.closest("[data-capsule]")).toBeTruthy();
  });

  it("renders control center with chrome and a panel", () => {
    const { ctx } = makeCtx("control");
    renderAppShell(root, ctx);
    expect(root.querySelector(".titlebar")).toBeTruthy();
    expect(root.querySelector("#panel")).toBeTruthy();
    expect(root.querySelector(".app-shell").dataset.view).toBe("control");
  });

  it("detaches the pet when the view has no pet slot (talk/control)", () => {
    const { ctx, petRoot } = makeCtx("talk");
    renderAppShell(root, ctx);
    expect(petRoot.parentElement).toBeNull();
  });

  it("renders every Control Center section without throwing", () => {
    const sections = ["model", "tools", "usage", "system", "appearance"];
    for (const section of sections) {
      const { ctx } = makeCtx("control");
      ctx.state.controlSection = section;
      ctx.state.tools = [];
      ctx.state.stats = null;
      ctx.state.settingsDraft = {
        baseUrl: "https://api.openai.com/v1",
        model: "gpt-4.1-mini",
        temperature: 0.7,
        maxContextMessages: 18,
        systemPrompt: "",
        autoSystemCheckEnabled: false,
        autoSystemCheckIntervalMinutes: 10,
      };
      ctx.state.handlers = {
        ...ctx.handlers,
        onSaveSettings: () => {},
        onToggleTop: () => {},
        onTemporaryPassthrough: () => {},
        onGoCapsule: () => {},
        onRunSystemCheck: () => {},
        onSetToolEnabled: () => {},
        onDeleteTool: () => {},
        onSaveTool: () => {},
        onImportTool: () => {},
        onSetComposerMode: () => {},
        onSetTheme: () => {},
        onSetPlatformStyle: () => {},
        onSetDensity: () => {},
        onSetReduceMotion: () => {},
        onRefreshStats: () => {},
      };
      expect(() => renderAppShell(root, ctx)).not.toThrow();
      expect(root.querySelector("#panel").innerHTML).not.toBe("");
      expect(root.querySelector(".control-nav").innerHTML).not.toBe("");
    }
  });

  it("control panel does not carry the .panel class (would override overflow:auto and block scroll)", () => {
    const { ctx } = makeCtx("control");
    ctx.state.controlSection = "model";
    ctx.state.settingsDraft = { baseUrl: "https://x", model: "m", temperature: 0.7, maxContextMessages: 18, systemPrompt: "", autoSystemCheckEnabled: false, autoSystemCheckIntervalMinutes: 10 };
    ctx.state.handlers = { ...ctx.handlers, onSaveSettings: () => {}, onToggleTop: () => {}, onTemporaryPassthrough: () => {}, onGoCapsule: () => {}, onRunSystemCheck: () => {}, onSetTheme: () => {}, onSetPlatformStyle: () => {}, onSetDensity: () => {}, onSetReduceMotion: () => {}, onRefreshStats: () => {}, onSetToolEnabled: () => {}, onDeleteTool: () => {}, onSaveTool: () => {}, onImportTool: () => {}, onSetComposerMode: () => {} };
    renderAppShell(root, ctx);
    const panel = root.querySelector("#panel");
    expect(panel.classList.contains("control-panel")).toBe(true);
    expect(panel.classList.contains("panel")).toBe(false);
  });
});
