import { icon } from "../ui/icons.js";
import { escapeHtml } from "../utils/markdown.js";
import { renderModelView } from "../views/ModelView.js";
import { renderPersonaView } from "../views/PersonaView.js";
import { renderToolsView } from "../views/ToolsView.js";
import { renderUsageView } from "../views/UsageView.js";
import { renderSystemView } from "../views/SystemView.js";
import { renderAppearanceView } from "../views/AppearanceView.js";
import { renderMemoryView } from "../views/MemoryView.js";

/**
 * ControlCenter — the management surface (ref-plan §3.3, §5.3, §6). Navigation
 * is a top segmented control under 720px and a left sidebar at/above it; the
 * active section renders into `#panel`. Sections: Model / Tools / Usage /
 * System / Appearance — the old nested SettingsPanel tabs are gone.
 */

const SECTIONS = [
  { id: "model", label: "模型", icon: "model" },
  { id: "persona", label: "人设", icon: "persona" },
  { id: "tools", label: "工具", icon: "tools" },
  { id: "usage", label: "用量", icon: "stats" },
  { id: "system", label: "系统", icon: "settings" },
  { id: "memory", label: "记忆", icon: "bookmark" },
  { id: "appearance", label: "外观", icon: "eyeOff" },
];

export function renderControlCenter(_ctx) {
  return `
    <nav class="control-nav" aria-label="控制中心" role="tablist" data-role="control-nav"></nav>
    <section id="panel" class="control-panel" data-role="control-panel"></section>
  `;
}

export function bindControlCenter(ctx) {
  const { state, handlers } = ctx;
  const nav = document.querySelector('[data-role="control-nav"]');
  if (nav) {
    nav.innerHTML = SECTIONS.map((s) => {
      const active = state.controlSection === s.id;
      const label = escapeHtml(s.label);
      return `<button class="control-nav-item ${active ? "active" : ""}" type="button" role="tab" aria-selected="${active ? "true" : "false"}" tabindex="${active ? "0" : "-1"}" title="${label}" aria-label="${label}" data-control-section="${s.id}">${icon(s.icon)}<span>${label}</span></button>`;
    }).join("");
    const buttons = Array.from(nav.querySelectorAll("[data-control-section]"));
    buttons.forEach((button) => {
      button.addEventListener("click", () => handlers.onControlSection(button.dataset.controlSection));
    });
    // Arrow-key section switching (ref-plan §15.6 — keyboard-accessible nav).
    nav.addEventListener("keydown", (event) => {
      const keys = ["ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp", "Home", "End"];
      if (!keys.includes(event.key)) return;
      event.preventDefault();
      const idx = Math.max(0, buttons.findIndex((b) => b.dataset.controlSection === state.controlSection));
      const dir = event.key === "ArrowRight" || event.key === "ArrowDown" ? 1 : -1;
      let next = buttons[(idx + dir + buttons.length) % buttons.length];
      if (event.key === "Home") next = buttons[0];
      if (event.key === "End") next = buttons[buttons.length - 1];
      handlers.onControlSection(next.dataset.controlSection);
      // Focus the newly-active tab after re-render.
      window.requestAnimationFrame(() => {
        document
          .querySelector(`[data-control-section="${next.dataset.controlSection}"]`)
          ?.focus();
      });
    });
  }

  const panel = document.querySelector("#panel");
  if (!panel) return;
  const section = state.controlSection;
  if (section === "persona") renderPersonaView(panel, state, handlers);
  else if (section === "tools") renderToolsView(panel, state, handlers);
  else if (section === "usage") renderUsageView(panel, state, handlers);
  else if (section === "system") renderSystemView(panel, state, handlers);
  else if (section === "memory") renderMemoryView(panel, state, handlers);
  else if (section === "appearance") renderAppearanceView(panel, state, handlers);
  else renderModelView(panel, state, handlers);
}
