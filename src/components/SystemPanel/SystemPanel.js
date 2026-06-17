export function renderSystemPanel(container, state, handlers) {
  const snapshot = state.system;
  const disks = snapshot?.disks ?? [];
  const processes = snapshot?.processes ?? [];
  const diskRows = disks
    .map(
      (disk) => `
      <tr>
        <td>${escapeHtml(disk.mountPoint || disk.name)}</td>
        <td>${formatBytes(disk.usedBytes)} / ${formatBytes(disk.totalBytes)}</td>
        <td>
          <div class="meter"><span style="width:${clamp(disk.usagePercent)}%"></span></div>
        </td>
      </tr>
    `,
    )
    .join("");

  const processRows = processes
    .map(
      (process) => `
      <tr>
        <td>${escapeHtml(process.name)}</td>
        <td>${process.cpuUsage.toFixed(1)}%</td>
        <td>${formatBytes(process.memoryBytes)}</td>
      </tr>
    `,
    )
    .join("");

  container.innerHTML = `
    <section class="system-panel">
      <div class="metric-grid">
        <div class="metric">
          <span>CPU</span>
          <strong>${snapshot ? snapshot.cpuUsage.toFixed(1) : "--"}%</strong>
        </div>
        <div class="metric">
          <span>内存</span>
          <strong>${snapshot ? snapshot.memory.usagePercent.toFixed(1) : "--"}%</strong>
        </div>
        <div class="metric">
          <span>进程</span>
          <strong>${snapshot ? snapshot.processCount : "--"}</strong>
        </div>
      </div>
      <button class="text-button" data-action="refresh">刷新状态</button>
      <div class="table-wrap">
        <table>
          <thead><tr><th>磁盘</th><th>占用</th><th></th></tr></thead>
          <tbody>${diskRows || '<tr><td colspan="3">暂无数据</td></tr>'}</tbody>
        </table>
      </div>
      <div class="table-wrap">
        <table>
          <thead><tr><th>进程</th><th>CPU</th><th>内存</th></tr></thead>
          <tbody>${processRows || '<tr><td colspan="3">暂无数据</td></tr>'}</tbody>
        </table>
      </div>
      <form class="disk-form" data-role="disk-form">
        <input name="path" placeholder="例如 C:\\\\Users" value="${escapeHtml(state.diskPath)}" />
        <button class="text-button" type="submit" ${state.diskBusy ? "disabled" : ""}>扫描目录</button>
      </form>
      <div class="scan-tree">${renderDiskResult(state.diskResult, state.diskBusy)}</div>
    </section>
  `;

  container.querySelector('[data-action="refresh"]').addEventListener("click", handlers.onRefresh);
  container.querySelector('[data-role="disk-form"]').addEventListener("submit", (event) => {
    event.preventDefault();
    const path = event.currentTarget.elements.path.value.trim();
    handlers.onScan(path);
  });
}

function renderDiskResult(result, busy) {
  if (busy) return '<div class="empty-state">扫描中...</div>';
  if (!result) return '<div class="empty-state">选择目录后查看占用</div>';
  return `
    <div class="scan-summary">
      <strong>${formatBytes(result.root.sizeBytes)}</strong>
      <span>${result.scannedEntries} 项 · ${result.elapsedMs} ms</span>
    </div>
    ${renderNode(result.root, result.root.sizeBytes)}
  `;
}

function renderNode(node, rootSize) {
  const percent = rootSize > 0 ? (node.sizeBytes / rootSize) * 100 : 0;
  const children = (node.children || []).map((child) => renderNode(child, rootSize)).join("");
  return `
    <div class="tree-node">
      <div class="tree-row">
        <span class="tree-name">${escapeHtml(node.name)}</span>
        <span class="tree-size">${formatBytes(node.sizeBytes)}</span>
      </div>
      <div class="meter"><span style="width:${clamp(percent)}%"></span></div>
      <div class="tree-children">${children}</div>
    </div>
  `;
}

function clamp(value) {
  return Math.max(0, Math.min(100, Number(value) || 0));
}

function formatBytes(bytes) {
  const value = Number(bytes) || 0;
  const units = ["B", "KB", "MB", "GB", "TB"];
  let current = value;
  let index = 0;
  while (current >= 1024 && index < units.length - 1) {
    current /= 1024;
    index += 1;
  }
  return `${current.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

