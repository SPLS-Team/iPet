// Inline SVG icon system (ui-plan.md §5.4, phase-2 — no dependency).
// Hand-written stroke icons in a 24x24 viewBox, currentColor fill/stroke so
// they inherit the surrounding text color. Returns an SVG string so it can be
// dropped into template literals alongside the existing markup.

const PATHS = {
  // Window actions
  compact: '<path d="M4 9h16M4 15h10"/><rect x="3" y="3" width="18" height="18" rx="3"/>',
  minimize: '<path d="M5 12h14"/>',
  close: '<path d="M6 6l12 12M18 6L6 18"/>',

  // Primary tabs
  chat: '<path d="M21 11.5a8.38 8.38 0 0 1-8.5 8.5 8.5 8.5 0 0 1-3.6-.8L3 21l1.9-5.7A8.38 8.38 0 0 1 4 11.5 8.5 8.5 0 0 1 12.5 3 8.38 8.38 0 0 1 21 11.5z"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>',

  // Settings sub-tabs
  model: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  persona: '<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/><path d="M18 5l2-2M20 8h3M6 5 4 3M4 8H1"/>',
  tools: '<path d="M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.5 2.5-2.1-2.1z"/>',
  stats: '<path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/>',

  // Actions
  send: '<path d="M22 2 11 13M22 2l-7 20-4-9-9-4z"/>',
  play: '<path d="M6 4l14 8L6 20z"/>',
  pause: '<path d="M9 4v16M15 4v16"/>',
  skip: '<path d="M5 4l11 8-11 8z"/><path d="M20 5v14"/>',
  reset: '<path d="M3 12a9 9 0 1 0 3-6.7M3 4v4h4"/>',
  coffee: '<path d="M4 8h13v5a5 5 0 0 1-5 5H9a5 5 0 0 1-5-5z"/><path d="M17 9h2a2 2 0 0 1 0 6h-2"/><path d="M7 2v2M11 2v2"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  refresh: '<path d="M3 12a9 9 0 0 1 15-6.7L21 8M21 3v5h-5M21 12a9 9 0 0 1-15 6.7L3 16M3 21v-5h5"/>',
  trash: '<path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6M10 11v6M14 11v6"/>',
  upload: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M17 8l-5-5-5 5M12 3v12"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-4.2-4.2"/>',
  edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4z"/>',
  bookmark: '<path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z"/>',
  warning: '<path d="M10.3 3.3 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.3a2 2 0 0 0-3.4 0z"/><path d="M12 9v4M12 17h.01"/>',
  chevronDown: '<path d="m6 9 6 6 6-6"/>',
  pin: '<path d="M12 17v5M9 10.8V3h6v7.8l3 3.2H6z"/>',
  eyeOff: '<path d="M9.9 4.2A10.9 10.9 0 0 1 12 4c7 0 10 8 10 8a18.5 18.5 0 0 1-2.16 3.19M6.6 6.6A18.5 18.5 0 0 0 2 12s3 8 10 8a10.9 10.9 0 0 0 5.4-1.4M3 3l18 18M9.9 9.9a3 3 0 0 0 4.2 4.2"/>',
};

/**
 * Render an icon by name. `label` becomes the accessible name (title +
 * aria-label); pass null/empty for purely decorative icons that already sit in
 * a labelled container.
 */
export function icon(name, { size = 16, label = "" } = {}) {
  const path = PATHS[name];
  if (!path) return "";
  const title = label ? `<title>${escapeHtml(label)}</title>` : "";
  const a11y = label ? ` role="img" aria-label="${escapeAttr(label)}"` : ' aria-hidden="true"';
  return `<svg class="icon" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"${a11y}>${title}${path}</svg>`;
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function escapeAttr(value) {
  return escapeHtml(value).replaceAll("`", "&#096;");
}
