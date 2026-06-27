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
  { id: "model", label: "模型", icon: "model", title: "模型连接", desc: "配置供应商、模型和生成参数，决定 iPet 如何回答。" },
  { id: "persona", label: "人设", icon: "persona", title: "人格与边界", desc: "定义 iPet 的身份、语气、主动性和工具使用边界。" },
  { id: "tools", label: "工具", icon: "tools", title: "工具库", desc: "管理模型可以调用的内置、本地和 HTTP 工具。" },
  { id: "usage", label: "用量", icon: "stats", title: "用量仪表盘", desc: "查看 token、工具调用、应用使用时长和番茄钟记录。" },
  { id: "system", label: "系统", icon: "settings", title: "系统与自动化", desc: "配置系统状态感知、通知、窗口行为和本机诊断。" },
  { id: "memory", label: "记忆", icon: "bookmark", title: "长期记忆", desc: "查看、刷新、编辑和删除跨会话持久保存的事实与偏好。" },
  { id: "appearance", label: "外观", icon: "eyeOff", title: "外观偏好", desc: "调整主题、平台风格、动效和界面密度。" },
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
  const active = SECTIONS.find((s) => s.id === section) || SECTIONS[0];
  panel.innerHTML = `
    <div class="control-page-shell">
      <header class="control-page-hero">
        <div class="control-page-title">
          <span class="control-page-icon" aria-hidden="true">${icon(active.icon)}</span>
          <div>
            <p class="control-page-kicker">控制中心</p>
            <h2>${escapeHtml(active.title)}</h2>
            <p>${escapeHtml(active.desc)}</p>
          </div>
        </div>
        <span class="control-page-badge">${escapeHtml(active.label)}</span>
      </header>
      <div class="control-page-body" data-role="control-page-body"></div>
    </div>
  `;

  const body = panel.querySelector('[data-role="control-page-body"]');
  if (section === "persona") renderPersonaView(body, state, handlers);
  else if (section === "tools") renderToolsView(body, state, handlers);
  else if (section === "usage") renderUsageView(body, state, handlers);
  else if (section === "system") renderSystemView(body, state, handlers);
  else if (section === "memory") renderMemoryView(body, state, handlers);
  else if (section === "appearance") renderAppearanceView(body, state, handlers);
  else renderModelView(body, state, handlers);
}
