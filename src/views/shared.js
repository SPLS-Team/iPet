import { escapeHtml } from "../utils/markdown.js";

/** Default JSON Schema shown pre-filled in the tool editor forms. */
export const DEFAULT_SCHEMA = `{
  "type": "object",
  "properties": {
    "query": {
      "type": "string",
      "description": "请求参数"
    }
  },
  "required": ["query"]
}`;

/** Attribute-safe escape that also neutralizes backticks for template literals. */
export function escapeAttr(value) {
  return escapeHtml(value).replaceAll("`", "&#096;");
}

/** Inline field-error span (ui-plan §9.5). */
export function fieldError(state, name) {
  const msg = state.settingsFieldErrors?.[name];
  return msg ? `<span class="field-error">${escapeHtml(msg)}</span>` : "";
}

/** Tabular-nums number formatting for usage metrics. */
export function formatNum(value) {
  return Number(value || 0).toLocaleString("en-US");
}

/** `<select>` helper: marks the option matching `value` as selected. */
export function selected(value, option) {
  return String(value) === String(option) ? "selected" : "";
}

/** A grouped settings card with a heading (reuses the `.settings-card` style). */
export function card(heading, bodyHtml, { className = "" } = {}) {
  return `
    <section class="settings-card ${className}">
      ${heading ? `<h3>${escapeHtml(heading)}</h3>` : ""}
      ${bodyHtml}
    </section>
  `;
}
