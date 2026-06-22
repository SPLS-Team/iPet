import { escapeHtml } from "../utils/markdown.js";
import { icon } from "../ui/icons.js";
import { card, escapeAttr, fieldError } from "./shared.js";

/**
 * ModelView — the LLM connection / provider / generation settings
 * (ref-plan §6.1). Persona moved out to its own Control Center section;
 * auto-check and window behavior moved out to System; theme moved out to
 * Appearance. Saves a partial override merged with the shared settings draft.
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
    <form class="settings-form settings-page" data-role="model-form">
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
            <span>接口格式 / 预设</span>
            <select name="providerPreset" data-role="provider-preset">
              ${providerPresetOptions(state)}
            </select>
            <span class="field-hint">选择常见 OpenAI 兼容服务商一键填入 Base URL，或选「自定义」手动填写。</span>
          </label>
          <label>
            <span>Base URL</span>
            <input name="baseUrl" value="${escapeAttr(draft.baseUrl || "")}" aria-invalid="${fieldError(state, "baseUrl") ? "true" : "false"}" />
            ${fieldError(state, "baseUrl")}
          </label>
          <label>
            <span>模型</span>
            <input name="model" list="model-list" value="${escapeAttr(draft.model || "")}" aria-invalid="${fieldError(state, "model") ? "true" : "false"}" />
            ${fieldError(state, "model")}
            <datalist id="model-list" data-role="model-list">
              ${(state.modelList || [])
                .map((m) => `<option value="${escapeAttr(m)}"></option>`)
                .join("")}
            </datalist>
          </label>
          <div class="form-actions">
            <button class="text-button" type="button" data-action="fetch-models" ${state.modelListBusy ? "disabled" : ""}>${icon("refresh")} ${state.modelListBusy ? "获取中..." : "刷新模型列表"}</button>
            <span class="field-hint" data-role="model-list-status">${escapeHtml(state.modelListStatus || "")}</span>
          </div>
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

      <div class="form-actions">
        <button class="text-button primary" type="submit">${icon("check")} 保存设置</button>
      </div>
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

  // Provider preset dropdown — apply a Base URL template on change. Editing
  // the Base URL by hand flips it to "custom".
  const preset = form.querySelector('[data-role="provider-preset"]');
  if (preset) {
    preset.addEventListener("change", () => handlers.onApplyProviderPreset?.(preset.value));
  }
  const baseUrlInput = form.querySelector('[name="baseUrl"]');
  if (baseUrlInput) {
    baseUrlInput.addEventListener("input", () => handlers.onApplyProviderPreset?.("custom"));
  }

  form.querySelector('[data-action="fetch-models"]')?.addEventListener("click", () => {
    handlers.onFetchModels?.();
  });
}

const PROVIDER_PRESET_LABELS = [
  ["custom", "自定义"],
  ["openai", "OpenAI"],
  ["deepseek", "DeepSeek"],
  ["anthropic_proxy", "Anthropic（OpenAI 兼容代理）"],
  ["openrouter", "OpenRouter"],
  ["siliconflow", "硅基流动 SiliconFlow"],
  ["moonshot", "Moonshot Kimi"],
  ["local", "本地 Ollama（11434）"],
];

function providerPresetOptions(state) {
  const current = state.providerPreset || "custom";
  return PROVIDER_PRESET_LABELS.map(
    ([value, label]) =>
      `<option value="${value}" ${value === current ? "selected" : ""}>${escapeHtml(label)}</option>`,
  ).join("");
}
