import { renderWindowChrome, bindWindowChrome } from "./WindowChrome.js";
import { renderCompanionCapsule, capsulePillText } from "./CompanionCapsule.js";
import { renderTalkWorkspace, bindTalkWorkspace } from "./TalkWorkspace.js";
import { renderControlCenter, bindControlCenter } from "./ControlCenter.js";

/**
 * AppShell — top-level view dispatch (ref-plan §11.2). Picks one of the three
 * product modes based on `state.viewMode` and renders its chrome + content:
 *
 *   capsule → CompanionCapsule (no titlebar)
 *   talk    → WindowChrome + TalkWorkspace
 *   control → WindowChrome + ControlCenter
 *
 * The overlay (#overlay) is always present so the overlay controller can mount
 * dialogs/toasts regardless of view. After painting, the live pet node owned
 * by main.js is placed into the active view's pet slot.
 */

let capsuleDragBound = false;
let dragStart = null;
let dragMoved = false;

export function renderAppShell(root, ctx) {
  const { state } = ctx;
  const view = state.viewMode;

  let body = "";
  if (view === "capsule") {
    body = renderCompanionCapsule(ctx);
  } else if (view === "control") {
    body = `${renderWindowChrome(ctx)}${renderControlCenter(ctx)}`;
  } else {
    body = `${renderWindowChrome(ctx)}${renderTalkWorkspace(ctx)}`;
  }

  root.innerHTML = `
    <main class="app-shell ${view === "capsule" ? "compact" : ""}" data-view="${view}">
      ${body}
      <div id="overlay" class="overlay" aria-live="polite"></div>
    </main>
  `;

  placePet(ctx);

  if (view !== "capsule") bindWindowChrome(ctx);
  if (view === "capsule") bindCapsule(ctx);
  if (view === "talk") bindTalkWorkspace(ctx);
  if (view === "control") bindControlCenter(ctx);
}

/** Move the persistent pet node into the active view's pet slot (capsule only). */
function placePet(ctx) {
  const { petRoot } = ctx;
  if (!petRoot) return;
  const slot = document.querySelector("[data-pet-slot]");
  if (slot && slot !== petRoot.parentElement) {
    slot.appendChild(petRoot);
  } else if (!slot && petRoot.parentElement) {
    petRoot.parentElement.removeChild(petRoot);
  }
}

/** Capsule: drag to move, click (that isn't a drag) to expand into talk. */
function bindCapsule(ctx) {
  // Document-level drag listeners are attached once; per-render we only wire
  // the capsule element itself (ref-plan §11.3 — avoid re-binding globals).
  dragCtx = ctx;
  if (!capsuleDragBound) {
    capsuleDragBound = true;
    document.addEventListener("mousemove", onDragMove);
    document.addEventListener("mouseup", onDragUp);
  }

  const capsule = document.querySelector("[data-capsule]");
  if (!capsule) return;

  capsule.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || event.target.closest("button")) return;
    dragStart = { x: event.clientX, y: event.clientY };
    dragMoved = false;
  });
  capsule.addEventListener(
    "click",
    (event) => {
      if (dragMoved) {
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      ctx.handlers.onExpand();
    },
    true,
  );
  // Right-click → expand into talk. The pill window is only ~46px tall, so an
  // in-window context menu gets clipped (only the first item shows); expand is
  // the one action users actually want from the shrunken form, so right-click
  // does that directly. Prevent the native browser menu either way.
  capsule.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    ctx.handlers.onExpand();
  });
}

function onDragMove(event) {
  if (!dragStart || dragMoved) return;
  const moved = Math.hypot(event.clientX - dragStart.x, event.clientY - dragStart.y);
  if (moved < 4) return;
  dragMoved = true;
  dragCtx?.appWindow.startDragging();
}

function onDragUp() {
  dragStart = null;
  window.setTimeout(() => {
    dragMoved = false;
  }, 0);
}

let dragCtx = null;

export { capsulePillText };
