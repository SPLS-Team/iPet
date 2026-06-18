import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

const isTauri = Boolean(window.__TAURI_INTERNALS__);

export async function invoke(command, args = {}) {
  if (isTauri) return tauriInvoke(command, args);
  return mockInvoke(command, args);
}

export async function listen(eventName, handler) {
  if (isTauri) return tauriListen(eventName, handler);
  return () => {};
}

export const appWindow = {
  minimize() {
    if (isTauri) return tauriInvoke("minimize_window");
  },
  close() {
    if (isTauri) return tauriInvoke("close_window");
  },
  async startDragging() {
    if (!isTauri) return undefined;
    try {
      return await getCurrentWindow().startDragging();
    } catch (_error) {
      return tauriInvoke("start_window_drag");
    }
  },
  setCompact(enabled) {
    if (isTauri) return tauriInvoke("set_compact_window", { enabled });
  },
};

async function mockInvoke(command, args) {
  if (command === "get_llm_settings") {
    return {
      hasApiKey: false,
      baseUrl: "https://api.openai.com/v1",
      model: "gpt-4.1-mini",
      temperature: 0.7,
      maxContextMessages: 18,
      autoSystemCheckEnabled: false,
      autoSystemCheckIntervalMinutes: 10,
      systemPrompt:
        "你是 iPet，一个常驻桌面的轻量助手。回答要简洁，必要时主动使用本地工具查看系统状态或分析目录占用。",
      settingsPath: "Browser preview mock",
    };
  }
  if (command === "save_llm_settings") {
    return {
      hasApiKey: Boolean(args.input?.apiKey && !args.input?.clearApiKey),
      baseUrl: args.input?.baseUrl,
      model: args.input?.model,
      temperature: args.input?.temperature,
      maxContextMessages: args.input?.maxContextMessages,
      autoSystemCheckEnabled: Boolean(args.input?.autoSystemCheckEnabled),
      autoSystemCheckIntervalMinutes: Number(args.input?.autoSystemCheckIntervalMinutes ?? 10),
      systemPrompt: args.input?.systemPrompt,
      settingsPath: "Browser preview mock",
    };
  }
  if (command === "get_recent_messages") {
    return [
      { role: "assistant", content: "设置页保存 API Key 后即可开始对话。\n\n- 工具可在设置里的工具 tab 管理\n- 统计 tab 会显示 token 用量" },
    ];
  }
  if (command === "list_tools") {
    return [
      {
        name: "get_system_status",
        displayName: "系统状态",
        description: "获取当前 CPU、内存、磁盘和高占用进程概览。",
        kind: "builtin",
        enabled: true,
        builtIn: true,
        parameters: {
          type: "object",
          properties: {
            process_limit: { type: "integer", minimum: 3, maximum: 30 },
          },
        },
      },
      {
        name: "scan_disk",
        displayName: "磁盘扫描",
        description: "扫描指定目录，按占用大小返回主要子目录和文件。",
        kind: "builtin",
        enabled: true,
        builtIn: true,
        parameters: {
          type: "object",
          required: ["path"],
          properties: {
            path: { type: "string" },
          },
        },
      },
      {
        name: "echo_local",
        displayName: "本地回显(示例)",
        description: "一个示例本地工具：把传入参数原样回显，用于演示 local 工具。",
        kind: "local",
        enabled: true,
        builtIn: false,
        parameters: {
          type: "object",
          properties: {
            text: { type: "string", description: "要回显的文本" },
          },
        },
        local: {
          command: "node",
          args: ["echo_tool.js"],
          cwd: null,
          timeoutSecs: 30,
        },
      },
    ];
  }
  if (command === "set_tool_enabled" || command === "save_tool" || command === "delete_tool") {
    return null;
  }
  if (command === "get_token_stats") {
    return {
      promptTokens: 1200,
      completionTokens: 840,
      totalTokens: 2040,
      requests: 6,
      toolCalls: 3,
      byDay: [
        { label: "2026-06-17", promptTokens: 1200, completionTokens: 840, totalTokens: 2040, requests: 6 },
      ],
      byModel: [
        { label: "gpt-4.1-mini", promptTokens: 1200, completionTokens: 840, totalTokens: 2040, requests: 6 },
      ],
      recent: [
        {
          id: 1,
          requestId: "preview",
          model: "gpt-4.1-mini",
          promptTokens: 200,
          completionTokens: 140,
          totalTokens: 340,
          toolCalls: 1,
          createdAt: "preview",
        },
      ],
    };
  }
  if (command === "get_system_status") {
    return {
      cpuUsage: 17.8,
      processCount: 96,
      memory: { usagePercent: 42.3, usedBytes: 6800000000, totalBytes: 16000000000 },
      disks: [
        {
          name: "Windows",
          mountPoint: "C:\\",
          usedBytes: 219000000000,
          totalBytes: 512000000000,
          usagePercent: 42.8,
        },
      ],
      processes: [
        { name: "Code.exe", cpuUsage: 4.2, memoryBytes: 420000000 },
        { name: "iPet.exe", cpuUsage: 1.1, memoryBytes: 86000000 },
      ],
    };
  }
  if (command === "scan_disk") {
    return {
      root: {
        name: args.request?.path || "C:\\",
        sizeBytes: 12300000000,
        children: [
          { name: "Users", sizeBytes: 8200000000, children: [] },
          { name: "Program Files", sizeBytes: 3100000000, children: [] },
        ],
      },
      scannedEntries: 1240,
      elapsedMs: 180,
    };
  }
  if (command === "send_chat_message") {
    throw new Error("浏览器预览不连接模型，请在 Tauri 应用中对话。");
  }
  return null;
}
