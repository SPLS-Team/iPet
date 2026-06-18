/**
 * CompanionCapsule — the always-on-desktop shrunken form (ref-plan §3.1, §5.1).
 * Shows only the pet (whose own status line carries the one-line state) and is
 * fully draggable; a click that wasn't a drag expands into the Talk Workspace.
 *
 * The live pet node is owned by main.js and placed into `[data-pet-slot]` after
 * each render so it survives view switches without being rebuilt (mood/line
 * updates from the streaming handler keep targeting the same node).
 */

export function renderCompanionCapsule(_ctx) {
  return `
    <div class="capsule" data-capsule data-tauri-drag-region>
      <div class="pet-wrap" data-pet-slot data-tauri-drag-region></div>
    </div>
  `;
}

/** The one status line shown via the pet's own line (capsule is 148px wide). */
export function capsuleStatusText(state) {
  if (state.chatBusy) return state.toolActivity || state.chatStatus || "思考中";
  if (state.chatStatus === "已停止") return "已停止";
  if (state.autoSystemCheckBusy) return "检查系统…";
  if (!state.settings?.hasApiKey) return "API Key 未配置";
  return "点击展开";
}
