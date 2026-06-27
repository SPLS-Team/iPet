import { describe, it, expect, beforeEach } from "vitest";
import { renderChat, updateChatStreaming } from "./ChatBubble.js";

// Renders the chat panel into a container with a no-op handler set, then
// asserts the DOM. Guards the message-type rendering (ref-plan §12.3) and the
// streaming fast-path skip rules.

function baseState(overrides = {}) {
  return {
    messages: [],
    chatBusy: false,
    chatStatus: "",
    toolActivity: "",
    thinkingStartedAt: null,
    thinkingElapsedMs: 0,
    stats: null,
    ...overrides,
  };
}

const handlers = {
  onSend: () => {},
  onStop: () => {},
  onGoSettings: () => {},
};

describe("ChatBubble message rendering", () => {
  let container;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.innerHTML = "";
    document.body.appendChild(container);
  });

  it("renders assistant + user bubbles with distinct roles", () => {
    renderChat(container, baseState({ messages: [
      { role: "assistant", content: "hello" },
      { role: "user", content: "hi" },
    ] }), handlers);
    const rows = container.querySelectorAll("[data-role='message']");
    expect(rows).toHaveLength(2);
    expect(rows[0].dataset.messageRole).toBe("assistant");
    expect(rows[1].dataset.messageRole).toBe("user");
    expect(rows[0].querySelector(".message-avatar")).toBeTruthy();
    expect(rows[1].querySelector(".message-avatar-user")).toBeTruthy();
  });

  it("renders tool-event and system-event as compact timeline cards", () => {
    renderChat(container, baseState({ messages: [
      { role: "assistant", type: "tool-event", content: "get_system_status" },
      { role: "assistant", type: "system-event", content: "CPU 12%" },
    ] }), handlers);
    const events = container.querySelectorAll(".event-card");
    expect(events).toHaveLength(2);
    expect(container.querySelector(".event-tool")).toBeTruthy();
    expect(container.querySelector(".event-system")).toBeTruthy();
  });

  it("renders error messages as a red-tinted bubble", () => {
    renderChat(container, baseState({ messages: [
      { role: "assistant", type: "error", content: "boom" },
    ] }), handlers);
    expect(container.querySelector(".message-error")).toBeTruthy();
    expect(container.querySelector("[data-message-type='error']")).toBeTruthy();
  });

  it("shows a token hint when stats have tokens", () => {
    renderChat(container, baseState({ stats: { totalTokens: 2048 } }), handlers);
    expect(container.querySelector(".token-hint")?.textContent).toContain("2,048");
  });

  it("streaming fast path patches the last assistant bubble", () => {
    const state = baseState({ messages: [{ role: "assistant", content: "part" }] });
    renderChat(container, state, handlers);
    expect(updateChatStreaming(container, state)).toBe(true);
  });

  it("streaming fast path skips typed (event/error) last messages", () => {
    const state = baseState({ messages: [{ role: "assistant", type: "error", content: "x" }] });
    renderChat(container, state, handlers);
    expect(updateChatStreaming(container, state)).toBe(false);
  });

  it("renders a collapsible reasoning chain only when reasoning is present", () => {
    renderChat(container, baseState({ messages: [
      { role: "assistant", content: "answer", reasoning: "let me think..." },
      { role: "assistant", content: "no reasoning here" },
    ] }), handlers);
    const chains = container.querySelectorAll('[data-role="reasoning-chain"]');
    expect(chains).toHaveLength(1);
    expect(chains[0].tagName).toBe("DETAILS");
    expect(chains[0].querySelector(".reasoning-body").textContent).toContain("let me think");
  });

  it("streaming fast path patches the reasoning chain live and preserves open state", () => {
    const state = baseState({ messages: [{ role: "assistant", content: "", reasoning: "step 1" }] });
    renderChat(container, state, handlers);
    // Open the disclosure, then stream more reasoning + answer text.
    const details = container.querySelector('[data-role="reasoning-chain"]');
    details.open = true;
    state.messages[0].reasoning = "step 1\nstep 2";
    state.messages[0].content = "done";
    expect(updateChatStreaming(container, state)).toBe(true);
    const updated = container.querySelector('[data-role="reasoning-chain"]');
    expect(updated.open).toBe(true);
    expect(updated.querySelector(".reasoning-body").textContent).toContain("step 2");
  });
});
