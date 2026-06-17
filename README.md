<div align="center">

# 🐾 iPet

<p>
  <em>A lightweight transparent desktop pet, powered by Tauri + Rust + LLM tool-calling.</em><br/>
  <em>一个轻量级透明桌宠，使用 Tauri + Rust + LLM 工具调用驱动。</em>
</p>

<p>
  <img alt="Tauri" src="https://img.shields.io/badge/Tauri-2.11-24C8DB?logo=tauri&logoColor=white">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2021-CE422B?logo=rust&logoColor=white">
  <img alt="Vite" src="https://img.shields.io/badge/Vite-8-646CFF?logo=vite&logoColor=white">
  <img alt="SQLite" src="https://img.shields.io/badge/SQLite-bundled-003B57?logo=sqlite&logoColor=white">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows-0078D6?logo=windows&logoColor=white">
  <img alt="License" src="https://img.shields.io/badge/license-private-lightgrey">
</p>

<p>
  <strong>🌐 Language / 语言：</strong>
  <a href="#-english"><strong>English</strong></a> ·
  <a href="#-中文"><strong>中文</strong></a>
</p>

<sub>Use the collapsible sections below — only one language expanded at a time keeps the README scannable.</sub>

</div>

---

<a id="-english"></a>
<details open>
<summary><h2>📖 English</h2></summary>

### Overview

**iPet** is a transparent, always-on-top desktop pet that lives on top of your real desktop. The pet talks to an OpenAI-compatible LLM, runs a small set of locally-executed tools (system metrics, disk usage scan, custom HTTP tools), and persists everything — chat history, settings, tool configs, token usage — into a local SQLite database. There is no telemetry and no cloud backend: the only network call is the one you configure to your chosen LLM endpoint.

The project is built with **Tauri 2** (Rust backend + WebView frontend), uses **Vite** for the frontend build, and ships as a single Windows executable plus optional MSI / portable-zip bundles.

### Highlights

- 🪟 **Transparent, frameless, always-on-top window** — drag anywhere, mouse-passthrough toggle, "compact" floating-head mode.
- 🐱 **CSS-animated pet character** with `idle / thinking / talking` states; ready to be swapped for a Live2D model later.
- 💬 **Streaming chat** against any OpenAI-compatible `/chat/completions` endpoint; live-typing thinking timer; Markdown rendering with GFM tables, task lists, code blocks (via `marked` + `DOMPurify`).
- 🔧 **Function-calling with local tools** — two built-ins (`get_system_status`, `scan_disk`) plus user-defined HTTP tools whose URL/parameters are validated against a JSON Schema.
- 🛡️ **SSRF-hardened HTTP tools** — URL allow/deny at save time and again at request time (resolves DNS, rejects loopback/private/link-local/CGNAT/ULA, IPv4-mapped IPv6 too); 30 s timeout, 5-redirect cap, 2 MiB response ceiling.
- 🔒 **Baseline Tauri CSP** — `script-src 'self'`, `object-src 'none'`, `frame-ancestors 'none'`, explicit IPC / asset hosts.
- 💾 **Local SQLite persistence** — preferences, chat history, tool configs, token usage, system samples, disk-scan cache.
- 📊 **Token statistics** — per-day, per-model, per-request views; merges the tool-decision call and final streaming reply into one record.
- 📋 **31 unit tests** covering config validation, disk scanner, storage, and HTTP safety.
- 📝 **`tracing` instrumentation** — runtime log level via `IPET_LOG` env var (e.g. `IPET_LOG=ipet_lib::tool_dispatcher=trace`).

### Quick start

```powershell
# Prerequisites: Node.js 18+, Rust stable toolchain, Windows 10/11
npm install
npm run tauri:dev
```

Then open the **Settings** page inside the app and paste an OpenAI-compatible API key (see [API key configuration](#api-key-configuration) below).

### Architecture at a glance

```
┌────────────────────────── Frontend (Vite + vanilla JS) ──────────────────────────┐
│                                                                                  │
│   src/main.js                           ← top-level state + render loop          │
│   src/components/ChatBubble/            ← chat UI, streaming bubbles             │
│   src/components/SettingsPanel/         ← model / tools / stats tabs             │
│   src/components/PetCharacter/          ← CSS pet sprite + state                 │
│   src/markdown.js                       ← marked + DOMPurify wrapper             │
│   src/tauriBridge.js                    ← thin invoke / listen / window facade   │
│                                                                                  │
└──────────────────────────────── Tauri IPC ───────────────────────────────────────┘
┌────────────────────────── Backend (Rust, src-tauri/src/) ────────────────────────┐
│                                                                                  │
│   lib.rs              ← 17 #[tauri::command]s + run() entrypoint + tracing init  │
│   app_error.rs        ← AppResult<T> / AppError shared error type                │
│   config.rs           ← LlmSettings shape, normalize + validate                  │
│   storage.rs          ← rusqlite wrapper, 6 tables, builtin tool seed            │
│   llm_client.rs       ← OpenAI-compatible streaming chat + function calling      │
│   tool_dispatcher.rs  ← builtin + HTTP tool execution (30s/2MiB caps)            │
│   http_safety.rs      ← URL allow/deny + DNS resolution checks                   │
│   system_monitor.rs   ← sysinfo wrapper (CPU, RAM, disks)                        │
│   disk_scanner.rs     ← parallel rayon directory scan                            │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### Tauri commands exposed to the frontend

| Command                  | Purpose                                                      |
|--------------------------|--------------------------------------------------------------|
| `get_llm_settings`       | Read settings status (never returns the saved key)           |
| `save_llm_settings`      | Persist API key, base URL, model, temperature, persona       |
| `get_system_status`      | Snapshot CPU / RAM / disk usage via `sysinfo`                |
| `scan_disk`              | Parallel rayon-based directory scan, size-sorted summary     |
| `get_recent_messages`    | Load N most-recent chat messages                             |
| `list_tools`             | Return all tool configs (builtin + custom)                   |
| `save_tool`              | Create/update a custom HTTP tool with JSON-Schema params     |
| `set_tool_enabled`       | Toggle a tool on/off                                         |
| `delete_tool`            | Delete a custom tool (builtins are protected)                |
| `get_token_stats`        | Aggregated token usage by day / model / recent requests      |
| `send_chat_message`      | Send a message; emits `chat-stream` events back              |
| `set_always_on_top`      | Toggle always-on-top window flag                             |
| `set_mouse_passthrough`  | Make the window click-through (over the desktop)             |
| `minimize_window`        | Minimize window                                              |
| `close_window`           | Close window                                                 |
| `start_window_drag`      | Begin a drag gesture on the borderless window                |
| `set_compact_window`     | Shrink to the floating-head "compact" mode                   |

### API key configuration

API keys are **never** committed to `.env` or hard-coded. Inside the app, open **Settings**:

1. Paste your OpenAI-compatible key into the `API Key` field.
2. Adjust `Base URL`, model name, temperature, context-window size, and persona prompt as needed.
3. Click `Save Settings`.

Settings live in a SQLite database under the app's data directory. The page only displays `Configured` / `Not configured` — it never echoes a saved key. Saving with the field empty **preserves** the existing key; tick `Clear saved API key` to wipe the local copy.

Defaults:

- Base URL: `https://api.openai.com/v1`
- Model: `gpt-4.1-mini`

### Built-in tools

#### `get_system_status`
Returns a snapshot of CPU usage (overall + per-core), total / used / free memory, swap, and per-disk usage. Backed by [`sysinfo`](https://crates.io/crates/sysinfo). Used by the optional auto-system-check loop that pulses the pet's status line.

#### `scan_disk`
Parallel directory scan (via `rayon`). Returns a size-sorted tree summary with `max_depth` and `max_children` truncation knobs, so a 1 M-file home directory still finishes in seconds and fits in the chat. Results are cached in SQLite.

### Custom HTTP tools

Tools are managed under **Settings → Tools**. Custom tools follow this shape:

```json
{
  "name": "search_docs",
  "displayName": "Search docs",
  "description": "Call this when the user needs to look something up in internal docs.",
  "kind": "http",
  "enabled": true,
  "parameters": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "description": "Search query" }
    },
    "required": ["query"]
  },
  "http": {
    "method": "POST",
    "url": "https://example.com/tool",
    "headers": []
  }
}
```

Rules enforced at save time **and** at request time:

- `name` must be a valid function identifier — ASCII letters, digits, underscores; no leading digit.
- `parameters` must be a JSON-Schema `object`.
- HTTP verbs: `GET`, `POST`, `PUT`, `PATCH`. `GET` lifts params into the query string; the others send a JSON body.
- URLs must be `http(s)://`; IP literals in loopback / private / link-local / CGNAT / ULA ranges are rejected (IPv4-mapped IPv6 forms included). At request time the host is resolved via DNS and re-checked.
- Request budget: 30 s timeout, 5 redirects max, 2 MiB response body cap.

### Token statistics

**Settings → Statistics** shows cumulative tokens, prompt / completion split, request count, tool-call count, per-day and per-model breakdowns, and the most recent requests. The non-streaming tool-decision call and the streaming final reply are merged into one record where the backend returns usage; if a particular OpenAI-compatible service omits usage info, that request is skipped from the stats.

### SQLite schema

| Table              | What it stores                                              |
|--------------------|-------------------------------------------------------------|
| `preferences`      | LLM settings, window prefs, auto-check toggle, persona      |
| `chat_messages`    | Full chat history (role, content, timestamps)               |
| `disk_scan_cache`  | Cached `scan_disk` results keyed by path + options          |
| `system_samples`   | Recent `get_system_status` snapshots                        |
| `tool_configs`     | Builtin + custom tool definitions, enabled flag             |
| `token_usage`      | Per-request token accounting (prompt / completion / total)  |

### Window UX

- **Transparent, frameless, always-on-top** by default.
- **Drag from anywhere** — the title bar and brand area carry `data-tauri-drag-region`.
- **Compact mode** (`◱` button) shrinks to a floating-head bubble that you can drag around with `mousedown`.
- **Mouse pass-through** toggle lets clicks fall through to the real desktop.
- **Minimize / close** controls live in the custom title bar.

### Logging

Tracing is initialized once at startup; the level is read from the `IPET_LOG` env var (defaults to `info`):

```powershell
$env:IPET_LOG = "ipet_lib=debug"; .\ipet.exe
$env:IPET_LOG = "ipet_lib::tool_dispatcher=trace"; .\ipet.exe
```

Logged points include startup with the resolved data dir, storage open errors, cache-write failures (now `warn!`), and a `debug!` on every tool dispatch + `warn!` on tool errors.

### Build & test

```powershell
# Install dependencies
npm install

# Frontend dev (Vite dev server)
npm run dev

# Frontend build
npm run build

# Tauri dev (runs Vite + Rust backend together)
npm run tauri:dev

# Rust type-check (no codegen, fast)
cargo check --manifest-path src-tauri/Cargo.toml

# Rust unit tests (31 tests — see "Why --release" note below)
cargo test --release --lib --manifest-path src-tauri/Cargo.toml
```

> **Why `--release`?** The debug `cargo test` build statically links the full Tauri stack and the resulting >200 MiB exe trips a Windows loader bug (`STATUS_ENTRYPOINT_NOT_FOUND`) before any test code runs. Release-mode optimizations shrink it past the threshold. Build time is still under a second for incremental edits.

### Release artifacts

```powershell
# Full Tauri bundle (frontend build + Rust release + bundling)
npm run tauri:build
```

Produces:

| Artifact      | Path                                                            |
|---------------|-----------------------------------------------------------------|
| Executable    | `src-tauri/target/release/ipet.exe`                             |
| MSI installer | `src-tauri/target/release/bundle/msi/iPet_0.1.0_x64_en-US.msi`  |
| Portable zip  | `src-tauri/target/release/bundle/zip/iPet_0.1.0_x64_en-US.zip`  |

#### Known issue: WiX MSI ICE validation

On the current development machine, `npm run tauri:build` can fail in WiX's ICE-validation phase. This is a **local Windows Installer / WiX environment** problem, not an app source bug.

**Workaround** — after Tauri has emitted `target/release/wix/x64/main.wixobj`, skip ICE validation manually:

```powershell
light.exe -sval -out "src-tauri\target\release\bundle\msi\iPet_0.1.0_x64_en-US.msi" "src-tauri\target\release\wix\x64\main.wixobj"
```

If the WiX toolset isn't installed:

```powershell
cargo install cargo-wix
```

The **portable zip** is the stable release artifact for now.

### Project layout

```
ipet/
├── package.json              # npm scripts (dev, build, tauri:*)
├── vite.config.js
├── index.html                # WebView entry
├── src/                      # Frontend
│   ├── main.js
│   ├── styles.css
│   ├── markdown.js
│   ├── tauriBridge.js
│   ├── live2d/               # (placeholder for future Live2D)
│   └── components/
│       ├── ChatBubble/
│       ├── PetCharacter/
│       └── SettingsPanel/
├── src-tauri/                # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── lib.rs            # Tauri commands + run()
│       ├── main.rs           # binary entry → ipet_lib::run()
│       ├── app_error.rs
│       ├── config.rs
│       ├── storage.rs
│       ├── llm_client.rs
│       ├── tool_dispatcher.rs
│       ├── http_safety.rs
│       ├── system_monitor.rs
│       ├── disk_scanner.rs
│       └── testutil.rs
└── tool-packages/            # Standalone exports of the built-in tools
    ├── get_system_status/
    └── scan_disk/
```

### Roadmap

- ⏳ Secret-store integration (OS keychain / Tauri stronghold) for the API key and HTTP-tool headers.
- ⏳ Tighter CSP (drop `style-src 'unsafe-inline'`).
- ⏳ Replace the CSS sprite with a Live2D model (the `src/live2d/` slot is reserved).
- ⏳ Frontend unit/E2E tests.
- ⏳ macOS / Linux smoke tests (the code is portable but only Windows is verified today).

</details>

---

<a id="-中文"></a>
<details>
<summary><h2>📖 中文</h2></summary>

### 概览

**iPet** 是一个透明、置顶的桌面宠物，悬浮在你真实的桌面之上。桌宠会和 OpenAI 兼容的 LLM 对话，能调用一组本地工具（系统指标、磁盘扫描、自定义 HTTP 工具），并把所有数据（聊天记录、设置、工具配置、token 使用）都持久化到本地 SQLite 数据库。**没有遥测，没有云端后端**：唯一的外部网络请求是你自己配置的 LLM 接口。

项目使用 **Tauri 2**（Rust 后端 + WebView 前端），前端构建走 **Vite**，最终产物是一个 Windows 可执行文件，外加可选的 MSI / portable-zip。

### 亮点

- 🪟 **透明、无边框、默认置顶窗口** —— 任意位置拖拽、鼠标穿透开关、"紧凑"浮头模式。
- 🐱 **CSS 动画桌宠**，具备 `idle / thinking / talking` 三态；后续可平滑替换为 Live2D 模型。
- 💬 **流式对话**，对接任何 OpenAI 兼容的 `/chat/completions` 接口；实时打字 + 思考计时器；用 `marked` + `DOMPurify` 渲染 Markdown（GFM 表格、任务列表、代码块）。
- 🔧 **本地工具的 function calling** —— 两个内置工具（`get_system_status`、`scan_disk`），加上用户自定义的 HTTP 工具（URL / 参数都按 JSON Schema 校验）。
- 🛡️ **HTTP 工具 SSRF 加固** —— 保存时和发起请求时都做 URL 黑名单校验（DNS 解析后再检查；拒绝 loopback / 私网 / 链路本地 / CGNAT / ULA，含 IPv4-mapped IPv6）；30 秒超时、最多 5 次重定向、响应体 2 MiB 上限。
- 🔒 **Tauri 基础 CSP** —— `script-src 'self'`、`object-src 'none'`、`frame-ancestors 'none'`，IPC / asset host 显式列出。
- 💾 **本地 SQLite 持久化** —— 偏好、聊天历史、工具配置、token 使用、系统采样、磁盘扫描缓存。
- 📊 **Token 统计** —— 按天 / 按模型 / 最近请求拆分；同一次对话里工具决策调用和最终流式回复的 usage 会合并成一条记录。
- 📋 **31 个单元测试**，覆盖配置校验、磁盘扫描、存储和 HTTP 安全。
- 📝 **`tracing` 日志埋点** —— 通过 `IPET_LOG` 环境变量实时切日志级别（例如 `IPET_LOG=ipet_lib::tool_dispatcher=trace`）。

### 快速开始

```powershell
# 前置：Node.js 18+，Rust stable，Windows 10/11
npm install
npm run tauri:dev
```

启动后进入应用内的 **设置** 页，粘贴 OpenAI 兼容的 API Key（详见下文的 [API Key 配置](#api-key-配置)）。

### 架构概览

```
┌────────────────────────── 前端（Vite + 原生 JS）─────────────────────────────────┐
│                                                                                  │
│   src/main.js                           ← 顶层状态与渲染循环                     │
│   src/components/ChatBubble/            ← 聊天 UI、流式气泡                      │
│   src/components/SettingsPanel/         ← 模型 / 工具 / 统计 三个 tab            │
│   src/components/PetCharacter/          ← CSS 角色精灵 + 状态切换                │
│   src/markdown.js                       ← marked + DOMPurify 封装                │
│   src/tauriBridge.js                    ← invoke / listen / window 的轻封装      │
│                                                                                  │
└──────────────────────────────── Tauri IPC ───────────────────────────────────────┘
┌────────────────────────── 后端（Rust，src-tauri/src/）───────────────────────────┐
│                                                                                  │
│   lib.rs              ← 17 个 #[tauri::command] + run() 入口 + tracing 初始化    │
│   app_error.rs        ← AppResult<T> / AppError 统一错误类型                     │
│   config.rs           ← LlmSettings 数据结构与归一化、校验                       │
│   storage.rs          ← rusqlite 封装，6 张表，内置工具种子                      │
│   llm_client.rs       ← OpenAI 兼容流式聊天 + function calling                   │
│   tool_dispatcher.rs  ← 内置 + HTTP 工具执行（30s / 2MiB 上限）                  │
│   http_safety.rs      ← URL 黑白名单 + DNS 解析校验                              │
│   system_monitor.rs   ← sysinfo 封装（CPU / RAM / 磁盘）                         │
│   disk_scanner.rs     ← rayon 并行目录扫描                                       │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### 暴露给前端的 Tauri 命令

| 命令                     | 用途                                                          |
|--------------------------|---------------------------------------------------------------|
| `get_llm_settings`       | 读取设置状态（不会返回已保存的 Key 明文）                     |
| `save_llm_settings`      | 保存 Key / Base URL / 模型 / temperature / 人设               |
| `get_system_status`      | 通过 `sysinfo` 抓 CPU / RAM / 磁盘快照                        |
| `scan_disk`              | rayon 并行目录扫描，按大小排序的树形摘要                      |
| `get_recent_messages`    | 拉取最近 N 条聊天记录                                         |
| `list_tools`             | 列出所有工具配置（内置 + 自定义）                             |
| `save_tool`              | 新建 / 更新一个自定义 HTTP 工具（JSON Schema 参数）           |
| `set_tool_enabled`       | 启用 / 停用某个工具                                           |
| `delete_tool`            | 删除自定义工具（内置工具受保护）                              |
| `get_token_stats`        | 聚合的 token 使用，按天 / 按模型 / 最近请求                   |
| `send_chat_message`      | 发送一条消息，通过 `chat-stream` 事件回推流式内容             |
| `set_always_on_top`      | 切换窗口置顶                                                  |
| `set_mouse_passthrough`  | 切换鼠标穿透（点击穿过窗口落到桌面）                          |
| `minimize_window`        | 最小化                                                        |
| `close_window`           | 关闭                                                          |
| `start_window_drag`      | 在无边框窗口上启动拖拽手势                                    |
| `set_compact_window`     | 切换到"紧凑"浮头模式                                          |

### API Key 配置

API key **不写入** `.env`，**不会** 在代码中硬编码。启动应用后进入 **设置**：

1. 在 `API Key` 输入框填入 OpenAI 兼容接口的 key。
2. 按需修改 `Base URL`、模型名、temperature、上下文消息数和人设。
3. 点击 `保存设置`。

设置存储在应用数据目录下的 SQLite 数据库里。设置页只显示 `已配置` / `未配置`，**不会** 回显已保存的 key；留空保存会**保留**原 key，勾选 `清除已保存的 API Key` 会删除本地 key。

默认接口：

- Base URL: `https://api.openai.com/v1`
- Model: `gpt-4.1-mini`

### 内置工具

#### `get_system_status`
返回 CPU 使用率（总体 + 每核）、内存总量 / 已用 / 空闲、swap、各磁盘占用。底层使用 [`sysinfo`](https://crates.io/crates/sysinfo)。可选的自动系统检查循环会用它来周期性地更新桌宠的状态行。

#### `scan_disk`
基于 `rayon` 的并行目录扫描，按大小排序返回树形摘要。提供 `max_depth` 与 `max_children` 两个裁剪参数，1M+ 文件的 home 目录也能在秒级返回并能塞进聊天面板。结果缓存到 SQLite。

### 自定义 HTTP 工具

工具在 **设置 → 工具** 中管理。自定义工具格式：

```json
{
  "name": "search_docs",
  "displayName": "搜索文档",
  "description": "当需要查询内部文档时调用。",
  "kind": "http",
  "enabled": true,
  "parameters": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "description": "搜索关键词" }
    },
    "required": ["query"]
  },
  "http": {
    "method": "POST",
    "url": "https://example.com/tool",
    "headers": []
  }
}
```

保存时和发起请求时同时强制：

- `name` 必须是函数名格式：只能包含英文字母、数字、下划线，且不能以数字开头。
- `parameters` 必须是 JSON Schema 的 `object`。
- HTTP 方法：`GET` / `POST` / `PUT` / `PATCH`。`GET` 把参数提到 query string，其余把参数作为 JSON body 发送。
- URL 必须是 `http(s)://`；loopback / 私网 / 链路本地 / CGNAT / ULA 范围内的 IP 字面量会被拒绝（含 IPv4-mapped IPv6 形式）。发起请求前会再做一次 DNS 解析复核。
- 请求预算：30 秒超时、最多 5 次重定向、响应体 2 MiB 上限。

### Token 统计

**设置 → 统计** 展示累计 token、prompt / completion 拆分、请求数、工具调用数、按天 / 按模型聚合、以及最近请求。非流式工具决策和流式最终回复的 usage 会尽量合并成一条记录；如果某个 OpenAI 兼容服务不返回 usage，那次请求不会写入统计。

### SQLite 表结构

| 表名              | 存储内容                                                        |
|-------------------|-----------------------------------------------------------------|
| `preferences`     | LLM 设置、窗口偏好、自动检查开关、人设                          |
| `chat_messages`   | 完整聊天历史（role / content / 时间戳）                         |
| `disk_scan_cache` | 缓存的 `scan_disk` 结果，按路径 + 选项做 key                    |
| `system_samples`  | 最近的 `get_system_status` 采样                                 |
| `tool_configs`    | 内置 + 自定义工具定义，含启用状态                               |
| `token_usage`     | 按请求记账（prompt / completion / total）                       |

### 窗口交互

- **透明、无边框、默认置顶**。
- **任意位置拖拽** —— 标题栏与 brand 区都带 `data-tauri-drag-region`。
- **紧凑模式**（`◱` 按钮）收起到一个浮头气泡，可以直接 `mousedown` 拖动。
- **鼠标穿透** 开关让点击穿过窗口落到真实桌面。
- **最小化 / 关闭** 按钮在自定义标题栏右侧。

### 日志

启动时一次性初始化 tracing；级别从 `IPET_LOG` 环境变量读取，默认 `info`：

```powershell
$env:IPET_LOG = "ipet_lib=debug"; .\ipet.exe
$env:IPET_LOG = "ipet_lib::tool_dispatcher=trace"; .\ipet.exe
```

已埋点的关键日志：启动时输出解析到的数据目录、SQLite open 失败、缓存写入失败（现在升级为 `warn!`）、每次工具分发 `debug!`、工具报错 `warn!`。

### 构建与测试

```powershell
# 安装依赖
npm install

# 前端 dev（Vite dev server）
npm run dev

# 前端构建
npm run build

# Tauri 开发模式（同时拉起 Vite + Rust 后端）
npm run tauri:dev

# Rust 类型检查（不生成产物，秒级）
cargo check --manifest-path src-tauri/Cargo.toml

# Rust 单元测试（31 个测试 —— 见下方 "为什么 --release" 说明）
cargo test --release --lib --manifest-path src-tauri/Cargo.toml
```

> **为什么 `--release`？** debug 模式的 `cargo test` 会静态链接完整 Tauri 栈，产生 >200 MiB 的测试二进制，会在测试代码运行之前就触发 Windows 加载器 bug（`STATUS_ENTRYPOINT_NOT_FOUND`）。release 模式的优化能把体积压到阈值以下。增量构建依然在 1 秒内。

### 发布产物

```powershell
# 完整 Tauri 打包（前端构建 + Rust release 编译 + 打包）
npm run tauri:build
```

会产出：

| 产物          | 路径                                                            |
|---------------|-----------------------------------------------------------------|
| 可执行文件    | `src-tauri/target/release/ipet.exe`                             |
| MSI 安装包    | `src-tauri/target/release/bundle/msi/iPet_0.1.0_x64_en-US.msi`  |
| Portable zip  | `src-tauri/target/release/bundle/zip/iPet_0.1.0_x64_en-US.zip`  |

#### 已知问题：WiX MSI ICE 校验

当前开发机执行 `npm run tauri:build` 时，WiX 生成 MSI 的 ICE 校验阶段可能失败，这是**本机 Windows Installer / WiX 环境**问题，不是应用源码错误。

**Workaround** —— Tauri 已经生成 `target/release/wix/x64/main.wixobj` 后，手动跳过 ICE 校验：

```powershell
light.exe -sval -out "src-tauri\target\release\bundle\msi\iPet_0.1.0_x64_en-US.msi" "src-tauri\target\release\wix\x64\main.wixobj"
```

如果 WiX toolset 没装，先：

```powershell
cargo install cargo-wix
```

**Portable zip** 是目前稳定的发布产物。

### 项目结构

```
ipet/
├── package.json              # npm 脚本（dev / build / tauri:*）
├── vite.config.js
├── index.html                # WebView 入口
├── src/                      # 前端
│   ├── main.js
│   ├── styles.css
│   ├── markdown.js
│   ├── tauriBridge.js
│   ├── live2d/               # （预留给后续的 Live2D）
│   └── components/
│       ├── ChatBubble/
│       ├── PetCharacter/
│       └── SettingsPanel/
├── src-tauri/                # Rust 后端
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── lib.rs            # Tauri 命令 + run()
│       ├── main.rs           # 二进制入口 → ipet_lib::run()
│       ├── app_error.rs
│       ├── config.rs
│       ├── storage.rs
│       ├── llm_client.rs
│       ├── tool_dispatcher.rs
│       ├── http_safety.rs
│       ├── system_monitor.rs
│       ├── disk_scanner.rs
│       └── testutil.rs
└── tool-packages/            # 两个内置工具的独立打包
    ├── get_system_status/
    └── scan_disk/
```

### 路线图

- ⏳ 接入操作系统 keychain / Tauri stronghold，加密保管 API Key 和 HTTP 工具 headers。
- ⏳ 收紧 CSP（去掉 `style-src 'unsafe-inline'`）。
- ⏳ 用 Live2D 模型替换 CSS 精灵（`src/live2d/` 目录已预留）。
- ⏳ 前端单元 / E2E 测试。
- ⏳ macOS / Linux 冒烟测试（代码本身可移植，但目前仅在 Windows 上验证过）。

</details>

---

<div align="center">
<sub>Built with <a href="https://tauri.app/">Tauri 2</a> · <a href="https://www.rust-lang.org/">Rust</a> · <a href="https://vitejs.dev/">Vite</a></sub>
</div>
