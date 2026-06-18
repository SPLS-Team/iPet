import { escapeHtml } from "../utils/markdown.js";
import { icon } from "../ui/icons.js";
import { card, escapeAttr, fieldError } from "./shared.js";

/**
 * ModelView — the LLM connection / provider / generation / persona settings
 * (ref-plan §6.1). Auto-check and window behavior moved out to System; theme
 * moved out to Appearance. Saves a partial override merged with the shared
 * settings draft so the backend still receives a full LlmSettingsInput.
 */
export function renderModelView(container, state, handlers) {
  const settings = state.settings;
  const draft = state.settingsDraft ?? {};

  const statusClass = state.settingsSaveFailed
    ? "error"
    : settings?.hasApiKey
      ? "ok"
      : "warn";
  const statusTitle = state.settingsSaveFailed
    ? "设置保存失败"
    : settings?.hasApiKey
      ? "API Key 已配置"
      : "API Key 未配置";

  container.innerHTML = `
    <form class="settings-form" data-role="model-form">
      <div class="settings-status ${statusClass}">
        <strong>${escapeHtml(statusTitle)}</strong>
        <span title="${escapeAttr(settings?.settingsPath || "")}">${escapeHtml(settings?.settingsPath || "")}</span>
      </div>

      ${card(
        "连接",
        `
          <label>
            <span>API Key</span>
            <input name="apiKey" type="password" autocomplete="off" placeholder="${settings?.hasApiKey ? "留空则保持原值" : "sk-..."}" />
          </label>
          <label class="checkbox-row">
            <input name="clearApiKey" type="checkbox" ${draft.clearApiKey ? "checked" : ""} />
            <span>清除已保存的 API Key</span>
          </label>
        `,
      )}

      ${card(
        "Provider",
        `
          <label>
            <span>Base URL</span>
            <input name="baseUrl" value="${escapeAttr(draft.baseUrl || "")}" aria-invalid="${fieldError(state, "baseUrl") ? "true" : "false"}" />
            ${fieldError(state, "baseUrl")}
          </label>
          <label>
            <span>模型</span>
            <input name="model" value="${escapeAttr(draft.model || "")}" aria-invalid="${fieldError(state, "model") ? "true" : "false"}" />
            ${fieldError(state, "model")}
          </label>
        `,
      )}

      ${card(
        "生成参数",
        `
          <div class="field-slider">
            <label class="slider-head">
              <span>Temperature</span>
              <output class="slider-value" data-role="temperature-out" for="temperature">${Number(draft.temperature ?? 0.7).toFixed(1)}</output>
            </label>
            <input id="temperature" name="temperature" type="range" min="0" max="2" step="0.1" value="${Number(draft.temperature ?? 0.7)}" />
            <span class="field-hint">0 更稳定，2 更发散</span>
          </div>
          <label>
            <span>上下文</span>
            <input name="maxContextMessages" type="number" min="4" max="64" step="1" value="${Number(draft.maxContextMessages ?? 18)}" />
            <span class="field-hint">保留最近 N 条消息</span>
          </label>
        `,
      )}

      ${card(
        "人设",
        `
          <label>
            <span>System Prompt</span>
            <textarea name="systemPrompt" rows="4">${escapeHtml(draft.systemPrompt || "")}</textarea>
          </label>
        `,
      )}

      <button class="text-button primary" type="submit">${icon("check")} 保存设置</button>
    </form>
  `;

  const form = container.querySelector('[data-role="model-form"]');
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    handlers.onSaveSettings({
      apiKey: form.elements.apiKey.value,
      clearApiKey: form.elements.clearApiKey.checked,
      baseUrl: form.elements.baseUrl.value,
      model: form.elements.model.value,
      temperature: Number(form.elements.temperature.value),
      maxContextMessages: Number(form.elements.maxContextMessages.value),
      systemPrompt: form.elements.systemPrompt.value,
    });
  });

  // Live temperature output (slider + number, ui-plan §9.6).
  const slider = form.querySelector('[name="temperature"]');
  const out = form.querySelector('[data-role="temperature-out"]');
  if (slider && out) {
    slider.addEventListener("input", () => {
      out.textContent = Number(slider.value).toFixed(1);
    });
  }
}
