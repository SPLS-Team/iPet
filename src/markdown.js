import createDOMPurify from "dompurify";
import { marked } from "marked";

const renderer = new marked.Renderer();
let purifier;

renderer.code = function ({ text, lang, escaped }) {
  const language = normalizeLanguage(lang);
  const className = language ? ` class="language-${escapeAttribute(language)}"` : "";
  const code = escaped ? text : escapeHtml(text);
  return `<pre class="md-code-block"><code${className}>${code}</code></pre>`;
};

renderer.html = function () {
  return "";
};

renderer.image = function ({ href, title, text }) {
  const titleAttr = title ? ` title="${escapeAttribute(title)}"` : "";
  return `<img src="${escapeAttribute(href)}" alt="${escapeAttribute(text)}"${titleAttr} loading="lazy">`;
};

renderer.link = function ({ href, title, tokens }) {
  const titleAttr = title ? ` title="${escapeAttribute(title)}"` : "";
  const label = this.parser.parseInline(tokens);
  return `<a href="${escapeAttribute(href)}"${titleAttr} target="_blank" rel="noreferrer noopener">${label}</a>`;
};

const sanitizeOptions = {
  ADD_ATTR: ["target", "rel", "loading"],
};

marked.setOptions({
  async: false,
  breaks: true,
  gfm: true,
  renderer,
});

export function renderMarkdown(source) {
  const text = String(source ?? "").trimEnd();
  if (!text) return "";

  const html = marked.parse(text, { async: false });
  return sanitizeHtml(html);
}

function sanitizeHtml(html) {
  if (!purifier) {
    purifier =
      typeof createDOMPurify.sanitize === "function"
        ? createDOMPurify
        : typeof window !== "undefined"
          ? createDOMPurify(window)
          : null;
  }

  if (!purifier) {
    return stripUnsafeHtml(html);
  }

  return purifier.sanitize(html, sanitizeOptions);
}

function stripUnsafeHtml(html) {
  return String(html)
    .replace(/<script\b[\s\S]*?<\/script>/gi, "")
    .replace(/\s(?:href|src)=["']\s*javascript:[^"']*["']/gi, "")
    .replace(/\son\w+=["'][^"']*["']/gi, "");
}

function normalizeLanguage(value) {
  return String(value ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]/g, "");
}

function escapeAttribute(value) {
  return escapeHtml(value).replaceAll("`", "&#096;");
}

export function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}
