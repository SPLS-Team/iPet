import { escapeHtml } from "../utils/markdown.js";
import { talkActivityText } from "./WindowChrome.js";

/**
 * TalkWorkspace — the default expanded view (ref-plan §3.2, §5.2). It owns the
 * talk header (a small pet avatar + current state) and the conversation canvas
 * mount point. The actual message list, composer and status strip are still
 * rendered by the legacy `renderChat` into `#panel` for Phase 1 — they get
 * rebuilt as ChatView in Phase 2.
 */

export function renderTalkWorkspace(ctx) {
  const { state } = ctx;
  return `
    <section class="talk-header">
      <div class="talk-avatar" aria-hidden="true">
        <span class="talk-avatar-mark"></span>
      </div>
      <div class="talk-meta">
        <strong>iPet</strong>
        <span class="talk-state" data-role="talk-state" aria-live="polite">${escapeHtml(talkActivityText(state))}</span>
      </div>
    </section>
    <section id="panel" class="panel" data-role="talk-panel"></section>
  `;
}
