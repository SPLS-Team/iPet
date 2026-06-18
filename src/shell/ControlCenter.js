import { icon } from "../ui/icons.js";
import { renderModelView } from "../views/ModelView.js";
import { renderToolsView } from "../views/ToolsView.js";
import { renderUsageView } from "../views/UsageView.js";
import { renderSystemView } from "../views/SystemView.js";
import { renderAppearanceView } from "../views/AppearanceView.js";

/**
 * ControlCenter — the management surface (ref-plan §3.3, §5.3, §6). Navigation
 * is a top segmented control under 720px and a left sidebar at/above it; the
 * active section renders into `#panel`. Sections: Model / Tools / Usage /
 * System / Appearance — the old nested SettingsPanel tabs are gone.
 */

const SECTIONS = [
  { id: "model", label: "模型", icon: "model" },
  { id: "tools", label: "工具", icon: "tools" },
  { id: "usage", label: "用量", icon: "stats" },
  { id: "system", label: "系统", icon: "settings" },
  { id: "appearance", label: "外观", icon: "eyeOff" },
];

export function renderControlCenter(_ctx) {
  return `
    <nav class="control-nav" aria-label="控制中心" role="tablist" data-role="control-nav"></nav>
    <section id="panel" class="panel control-panel" data-role="control-panel"></section>
  `;
}

export function bindControlCenter(ctx) {
  const { state, handlers } = ctx;
  const nav = document.querySelector('[data-role="control-nav"]');
  if (nav) {
    nav.innerHTML = SECTIONS.map((s) => {
      const active = state.controlSection === s.id;
      return `<button class="control-nav-item ${active ? "active" : ""}" type="button" role="tab" aria-selected="${active ? "true" : "false"}" data-control-section="${s.id}">${icon(s.icon)}<span>${s.label}</span></button>`;
    }).join("");
    nav.querySelectorAll("[data-control-section]").forEach((button) => {
      button.addEventListener("click", () => handlers.onControlSection(button.dataset.controlSection));
    });
  }

  const panel = document.querySelector("#panel");
  if (!panel) return;
  const section = state.controlSection;
  if (section === "tools") renderToolsView(panel, state, handlers);
  else if (section === "usage") renderUsageView(panel, state, handlers);
  else if (section === "system") renderSystemView(panel, state, handlers);
  else if (section === "appearance") renderAppearanceView(panel, state, handlers);
  else renderModelView(panel, state, handlers);
}
