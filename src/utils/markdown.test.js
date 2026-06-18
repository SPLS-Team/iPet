import { describe, expect, it } from "vitest";
import { renderMarkdown, escapeHtml } from "./markdown.js";

// These tests run under happy-dom (see vite.config.js test.environment) so the
// DOMPurify path is exercised — that's the same code that runs in the Tauri
// webview. The regex fallback in markdown.js is only there for environments
// without a window.

describe("renderMarkdown", () => {
  describe("xss / sanitization", () => {
    it("strips raw <script> tags", () => {
      const out = renderMarkdown("hi\n\n<script>alert(1)</script>\n\nbye");
      expect(out).not.toMatch(/<script/i);
      expect(out).not.toContain("alert(1)");
    });

    it("rewrites javascript: URLs to be inert", () => {
      const out = renderMarkdown("[click](javascript:alert(1))");
      // DOMPurify drops the dangerous protocol; the visible label survives.
      expect(out).not.toMatch(/href=["']?javascript:/i);
      expect(out).toContain("click");
    });

    it("strips event-handler attributes from raw html", () => {
      const out = renderMarkdown(
        "regular text\n\n<img src=x onerror=\"alert(1)\">\n\nmore",
      );
      expect(out).not.toMatch(/onerror=/i);
      // DOMPurify also blocks bare data: handlers etc.; we don't assert about
      // the <img> itself, just that nothing executable survives.
      expect(out).not.toContain("alert(1)");
    });

    it("does not pass through arbitrary html blocks", () => {
      // markdown.js's custom renderer.html returns "" for raw HTML blocks, so
      // even before DOMPurify a <iframe> never reaches the output.
      const out = renderMarkdown("<iframe src='https://evil'></iframe>");
      expect(out).not.toContain("<iframe");
    });
  });

  describe("safe content", () => {
    it("renders bold/italic/lists", () => {
      const out = renderMarkdown("**a** _b_\n\n- one\n- two");
      expect(out).toMatch(/<strong>a<\/strong>/);
      expect(out).toMatch(/<em>b<\/em>/);
      expect(out).toMatch(/<ul>[\s\S]*one[\s\S]*two[\s\S]*<\/ul>/);
    });

    it("renders fenced code blocks with the language class", () => {
      const out = renderMarkdown("```rust\nlet x = 1;\n```");
      expect(out).toMatch(/<pre class="md-code-block"><code class="language-rust">/);
      expect(out).toContain("let x = 1;");
    });

    it("renders GFM tables", () => {
      const out = renderMarkdown("| a | b |\n| - | - |\n| 1 | 2 |");
      expect(out).toMatch(/<table>/);
      expect(out).toMatch(/<th[^>]*>a<\/th>/);
      expect(out).toMatch(/<td[^>]*>1<\/td>/);
    });

    it("opens external links in a new tab with safe rel", () => {
      const out = renderMarkdown("[hi](https://example.com)");
      expect(out).toMatch(/<a [^>]*href="https:\/\/example\.com"/);
      expect(out).toMatch(/target="_blank"/);
      // DOMPurify normalizes spacing; just make sure both tokens survive.
      expect(out).toMatch(/rel="[^"]*noopener/);
      expect(out).toMatch(/rel="[^"]*noreferrer/);
    });

    it("returns empty string for empty / whitespace input", () => {
      expect(renderMarkdown("")).toBe("");
      expect(renderMarkdown("   \n\n")).toBe("");
      expect(renderMarkdown(null)).toBe("");
      expect(renderMarkdown(undefined)).toBe("");
    });
  });

  describe("escapeHtml", () => {
    it("escapes the five canonical characters", () => {
      expect(escapeHtml("<a href=\"x\">'b' & c</a>")).toBe(
        "&lt;a href=&quot;x&quot;&gt;&#039;b&#039; &amp; c&lt;/a&gt;",
      );
    });

    it("coerces null / undefined to empty string", () => {
      expect(escapeHtml(null)).toBe("");
      expect(escapeHtml(undefined)).toBe("");
    });
  });
});
