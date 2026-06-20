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
<details>
<summary><h2>English</h2></summary>

### Overview

**iPet** is a transparent, always-on-top desktop pet that lives on top of your real desktop. The pet talks to an OpenAI-compatible LLM, runs a small set of locally-executed tools (system metrics, disk usage scan, custom HTTP tools), and persists everything — chat history, settings, tool configs, token usage — into a local SQLite database. There is no telemetry and no cloud backend: the only network call is the one you configure to your chosen LLM endpoint.

The project is built with **Tauri 2** (Rust backend + WebView frontend), uses **Vite** for the frontend build, and ships as a single Windows executable plus optional MSI / portable-zip bundles.

### Highlights

- **Transparent, frameless, always-on-top window** — drag anywhere, mouse-passthrough toggle, "compact" floating-head mode.
- **CSS-animated pet character** with `idle / thinking / talking / tool / warning` states; ready to be swapped for a Live2D model later.
- **Apple-style UI redesign** — design-token system (typography / spacing / motion / radius), light & dark mode with manual override, per-platform profiles (macOS / Windows / Linux) via `data-platform`, dependency-free inline SVG icons, custom dialog + toast system (no more `window.confirm`), segmented tool composer, temperature slider, stats Bento + prompt/completion breakdown, dialog focus trap, `prefers-reduced-motion` aware.
- **Three-mode shell architecture (v0.4.0)** — the interface is rebuilt around a *Companion Capsule* (desktop-resident pet + one status line), a *Talk Workspace* (chat-first, the big pet stage replaced by a compact avatar header), and a *Control Center* (management surface separated from chat). Navigation is view-driven (`capsule` / `talk` / `control`) instead of the old chat/settings tabbar; `Cmd/Ctrl+,` opens the Control Center, `Esc` closes it, `Cmd/Ctrl+L` focuses the composer, arrow keys switch Control Center sections. Shell code is split into `src/shell/` (`AppShell`, `WindowChrome`, `CompanionCapsule`, `TalkWorkspace`, `ControlCenter`) and `src/views/` (Model / Tools / Usage / System / Appearance). Chat messages carry a `type` (`assistant` / `user` / `tool-event` / `system-event` / `error`) so tool calls and failures render as compact timeline cards instead of inlining into bubbles; the status strip shows a thinking timer, tool-activity chip and a live token hint. Per-platform profiles (macOS grouped rows + sheet dialogs, Windows Fluent cards + focus ring, Linux solid-surface low-blur fallback) layer on top of the shared tokens.
- **Streaming chat** against any OpenAI-compatible `/chat/completions` endpoint; live-typing thinking timer; Markdown rendering with GFM tables, task lists, code blocks (via `marked` + `DOMPurify`).
- **Function-calling with three tool kinds under one spec** — **19 built-in tools** (see below), user-defined HTTP tools, and `local` subprocess tools, all described by a `tool.json` manifest (schemaVersion 1 = http, 2 = +local) and runtime-hot-pluggable (per-request DB read, no cache).
- **19 built-in desktop tools** — `get_system_status`, `scan_disk`, `search_files`, `read_text_file`, `git_status`, `list_processes`, `network_status`, `clipboard_read`, `open_path`, `recent_system_errors`, `package_scripts`, `run_project_check`, `disk_cleanup_candidates`, `create_note`, `weather_lookup`, `web_search`, `screenshot_ocr`, plus `memory_save` / `memory_search` for long-term memory. Implemented as Rust functions in the `ipet-tool-desktop-tools` crate, routed through the dispatcher, and blocked-thread-pool-scheduled so a slow tool never pins the Tokio runtime.
- **Multi-session chat** — switch / create / rename / delete sessions from the talk-header switcher; messages are scoped per session and the active id is owned by the backend so restarts and cross-window writes stay consistent. Switching is optimistic (the list + select update on the click frame, not after the round-trip) with a sequence token that drops stale loads if you switch again mid-flight.
- **Long-term memory (Tier 1, dual-track)** — the model remembers facts/preferences across sessions: a bounded slice of recently-updated memories is appended to the system prompt every turn (stable, always-on context), and the `memory_save` / `memory_search` tools let it write/recall on demand. Memories are inspectable and editable from **Control Center → 记忆** — no opaque black-box memory.
- **SSRF-hardened HTTP tools** — URL allow/deny at save time and again at request time (resolves DNS, rejects loopback/private/link-local/CGNAT/ULA, IPv4-mapped IPv6 too); 30 s timeout, 5-redirect cap, 2 MiB response ceiling.
- **Local tools run as subprocesses** — iPet spawns the configured executable, ships the model's args as one JSON line on stdin, reads stdout back; bounded by a hard timeout, a 2 MiB stdout cap, and nonzero-exit reporting. Drop a script + register its `tool.json`, no recompile.
- **Baseline Tauri CSP** — `script-src 'self'`, `object-src 'none'`, `frame-ancestors 'none'`, explicit IPC / asset hosts.
- **Local SQLite persistence with versioned migrations** — preferences, chat history, sessions, memories, tool configs, token usage, system samples, disk-scan cache; schema evolves via `PRAGMA user_version` (currently v3).
- **At-rest API-key encryption** — the LLM API key is stored encrypted (ChaCha20-Poly1305) with a per-machine key, not plaintext.
- **Token statistics** — per-day, per-model, per-request views; merges the tool-decision call and final streaming reply into one record.
- **Cargo workspace** — the built-in tools live as `tool-packages/*/rust/` crates that `src-tauri` depends on, so runtime and the distributable package share one source of truth.
- **Cross-platform CI** — Windows (NSIS + portable zip) and macOS (DMG) build matrices in GitHub Actions; macOS uses native traffic-lights via a `tauri.macos.conf.json` overlay while Windows/Linux stay frameless.
- **~130 tests** (56 host lib + 9 desktop-tools crate + 8 scan-disk crate + 60 frontend) covering config validation, disk scanner, storage + migrations (incl. v3 session/memory back-fill), tool-package parsing, dispatcher (incl. local subprocess), HTTP safety, secret encryption, Markdown sanitization, the three-mode shell + Control Center section render dispatch, session-switcher + memory-view rendering, and chat message-type rendering.
- **`tracing` instrumentation** — runtime log level via `IPET_LOG` env var (e.g. `IPET_LOG=ipet_lib::tool_dispatcher=trace`).

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
│   src/main.js                           ← bootstrap + shell/event orchestration  │
│   src/app/                              ← state, platform, theme, overlay, IPC   │
│   src/shell/                            ← AppShell / WindowChrome / Capsule /    │
│   │                                       TalkWorkspace / ControlCenter          │
│   src/views/                            ← Control Center sections: Model / Tools │
│   │                                       / Usage / System / Appearance         │
│   src/ui/                               ← inline SVG icon set (no dependency)   │
│   src/utils/                            ← markdown renderer + frontend tests     │
│   src/styles.css                        ← CSS entrypoint — @imports the modules  │
│   src/styles/                           ← tokens / base / shell / chat /         │
│   │                                       control-center / dialog / responsive  │
│   src/components/ChatBubble/            ← chat UI, streaming bubbles             │
│   src/components/PetCharacter/          ← CSS pet sprite + state                 │
│                                                                                  │
└──────────────────────────────── Tauri IPC ───────────────────────────────────────┘
┌────────────────────────── Backend (Rust, src-tauri/src/) ────────────────────────┐
│                                                                                  │
│   lib.rs              ← 21 #[tauri::command]s + run() entrypoint + tracing init  │
│   app_error.rs        ← AppResult<T> / AppError shared error type                │
│   config.rs           ← LlmSettings shape, normalize + validate                  │
│   storage/            ← rusqlite wrapper, 6 tables, versioned migrations,        │
│                         split by domain (chat/tools/token_usage/caches/prefs)    │
│   llm_client.rs       ← OpenAI-compatible streaming chat + function calling      │
│   tool_dispatcher.rs  ← builtin + HTTP + local (subprocess) tool execution       │
│   http_safety.rs      ← URL allow/deny + DNS resolution checks                   │
│   secret.rs           ← ChaCha20-Poly1305 at-rest key encryption                 │
│   tool_package.rs     ← tool.json (schemaVersion 1/2) import parser              │
│                                                                                  │
│   ── built-in tool crates (workspace members, also the distributable source) ──  │
│   tool-packages/scan_disk/rust/           ← parallel rayon directory scan        │
│   tool-packages/get_system_status/rust/   ← sysinfo wrapper (CPU, RAM, disks)    │
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
| `cancel_disk_scan`       | Cancel an in-flight disk scan by its caller id               |
| `get_recent_messages`    | Load N most-recent chat messages                             |
| `list_tools`             | Return all tool configs (builtin + custom, http + local)     |
| `save_tool`              | Create/update a custom http or local tool (JSON-Schema params)|
| `import_tool_from_path`  | Import a tool.json package (schemaVersion 1/2) from a path   |
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
| `get_preference`         | Read an arbitrary preference string (e.g. UI theme)         |
| `set_preference`         | Persist an arbitrary preference string                      |

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

iPet ships 19 built-in tools as Rust functions in the `ipet-tool-desktop-tools` crate, routed through `ToolDispatcher::dispatch_builtin` and scheduled on Tokio's blocking pool (`spawn_blocking`) so a slow tool never pins the runtime. Every network/subprocess call has a timeout and an output cap. Read-only tools are marked `readOnly: true` in their `tool.json`; the rest carry explicit safety notes.

**System & files (read-only)**
- `get_system_status` — CPU (overall + per-core), memory/swap, per-disk usage. Backed by `sysinfo`. Feeds the optional auto-system-check loop.
- `scan_disk` — parallel (`rayon`) directory scan; size-sorted tree with `max_depth`/`max_children` truncation. Cached in SQLite.
- `search_files` — walk a directory for name/path matches, optionally content-search small text files. Capped results, no symlink following, hidden files opt-in.
- `read_text_file` — read a text file slice with byte/line windows; 1 MiB / 2000-line caps.
- `git_status` — branch, HEAD, short status, diff stat (+ optional truncated diff) via fixed read-only `git` commands.
- `list_processes` — top processes by CPU/memory/name with a filter; sysinfo double-sample for stable CPU%.
- `network_status` — DNS resolution + TCP connectivity probe + per-OS interface summary; connect_timeout bounded.
- `recent_system_errors` — Windows Event Log / macOS unified log / `journalctl` error-level entries, honoring `sinceHours`.
- `disk_cleanup_candidates` — lists large files in temp/cache dirs by size; **read-only, never deletes**.

**Project & clipboard**
- `package_scripts` — read a project's `package.json` `scripts` (read-only).
- `run_project_check` — run an **allow-listed** build/test command (`npm test`, `npm run build`, `cargo check`, …) with a hard timeout and drained pipes. Anything outside the allow-list is rejected.
- `clipboard_read` — read OS clipboard text (PowerShell / `pbpaste` / `xclip`), capped.

**Write / side-effecting (model-initiated, user-auditable)**
- `open_path` — open a local path or `http(s)` URL with the system handler.
- `create_note` — write/append a Markdown note under `~/.ipet/notes/` (slugified filename, 256 KB cap).
- `screenshot_ocr` — capture the primary screen + `tesseract` OCR; degrades to a clear error when tesseract is absent. Privacy-sensitive.

**Web (keyless)**
- `weather_lookup` — current conditions via wttr.in (temp / feels-like / humidity / wind); c or f units.
- `web_search` — DuckDuckGo Instant Answer; documented as summary-only, not full web search.

**Long-term memory**
- `memory_save` — write/upsert a fact or preference (keyed) to the cross-session memory store.
- `memory_search` — substring recall over key/content/category; bumps usage counters on hit.

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

### Custom local tools

A `kind: "local"` tool is any executable or script on your machine — `.exe`, `.py`, `.js`, `.bat`, etc. iPet spawns it per call, writes the model's arguments as a single JSON line to its **stdin**, and treats the child's **stdout** as the return value. The child is a separate process, so a crash or hang is bounded by the timeout — it can't take the host down. Add one from **Settings → Tools → 添加本地工具**, or import a `tool.json` (schemaVersion 2) package.

```json
{
  "schemaVersion": 2,
  "name": "echo_local",
  "displayName": "Echo",
  "description": "Echo the text argument back.",
  "kind": "local",
  "parameters": {
    "type": "object",
    "required": ["text"],
    "properties": { "text": { "type": "string" } }
  },
  "local": {
    "command": "node",
    "args": ["echo_tool.js"],
    "timeoutSecs": 15
  }
}
```

Contract & guardrails:

- Model args travel on stdin (one JSON line), never on the command line — so there's no shell-injection surface from arguments.
- Relative `command`/`cwd` that look path-like (`./script.js`, `dir/x`) are anchored to the package directory at import time; bare interpreter names (`node`, `python`) stay PATH lookups.
- Hard timeout (`timeoutSecs`, default 30) kills the child on expiry; nonzero exit is surfaced as an error with stderr; stdout is capped at 2 MiB.
- **Security:** a local tool runs as your user with full permissions — equivalent to running the command yourself. Only add tools from sources you trust. The timeout / stdin / output cap are guardrails, not a sandbox. See `docs/TOOL_PACKAGE.md`.

A working example lives at `tool-packages/echo_local/`.

### Tool package import

Any tool ships as a `tool.json` package (optionally with a `README.md` and, for local tools, the script). Import from **Settings → Tools → 导入工具包** by pointing at the directory or `tool.json` file — see `docs/TOOL_PACKAGE.md` for the full schema. The `openai-function.json` envelope is regenerated from `tool.json` by `npm run sync:tools`, so the parameter schema has a single source of truth.

### Token statistics

**Settings → Statistics** shows cumulative tokens, prompt / completion split, request count, tool-call count, per-day and per-model breakdowns, and the most recent requests. The non-streaming tool-decision call and the streaming final reply are merged into one record where the backend returns usage; if a particular OpenAI-compatible service omits usage info, that request is skipped from the stats.

### SQLite schema

| Table              | What it stores                                              |
|--------------------|-------------------------------------------------------------|
| `preferences`      | LLM settings, window prefs, auto-check toggle, persona      |
| `sessions`         | Chat sessions (title, timestamps, last_message_at) — multi-session |
| `chat_messages`    | Full chat history (role, content, timestamps) scoped by `session_id` |
| `memories`         | Long-term, cross-session memory entries (key/content/category + usage) |
| `disk_scan_cache`  | Cached `scan_disk` results keyed by path + options          |
| `system_samples`   | Recent `get_system_status` snapshots                        |
| `tool_configs`     | Builtin + custom tool definitions (http + local), enabled flag |
| `token_usage`      | Per-request token accounting (prompt / completion / total)  |

Schema is at **v3** (v2 → v3 added `sessions` + `memories` and a `session_id` FK on `chat_messages`; pre-v3 rows are back-filled onto a single default session). Migrations are append-only under `MIGRATIONS` in `src-tauri/src/storage/mod.rs`.

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

# Rust unit tests (51 host + 8 scan-disk crate — see "Why --release" note below)
cargo test --release --lib --manifest-path src-tauri/Cargo.toml
```

> **Why `--release`?** The debug `cargo test` build statically links the full Tauri stack and the resulting >200 MiB exe trips a Windows loader bug (`STATUS_ENTRYPOINT_NOT_FOUND`) before any test code runs. Release-mode optimizations shrink it past the threshold. Build time is still under a second for incremental edits.

### Release artifacts

```powershell
# Full Tauri bundle (frontend build + Rust release + bundling)
npm run tauri:build
```

Produces (`tauri.conf.json` is configured for `["nsis"]`; the portable zip is the stable artifact today):

| Artifact      | Path                                                            |
|---------------|-----------------------------------------------------------------|
| Executable    | `src-tauri/target/release/ipet.exe`                             |
| NSIS installer| `src-tauri/target/release/bundle/nsis/iPet_0.1.0_x64-setup.exe` |
| Portable zip  | `src-tauri/target/release/bundle/zip/iPet_0.1.0_x64_en-US.zip`  |

#### Known issue: WiX MSI ICE validation

On the current development machine, building an MSI bundle (`tauri build` with the `msi` target) can fail in WiX's ICE-validation phase. This is a **local Windows Installer / WiX environment** problem, not an app source bug — which is why the config ships NSIS, not MSI.

**Workaround** (if you want an MSI anyway) — after Tauri has emitted `target/release/wix/x64/main.wixobj`, skip ICE validation manually:

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
├── Cargo.toml               # Cargo workspace root (members: src-tauri + tool crates)
├── package.json             # npm scripts (dev, build, tauri:*, sync:tools)
├── vite.config.js
├── index.html               # WebView entry
├── scripts/
│   └── sync-openai-functions.js  # regenerates openai-function.json from tool.json
├── src/                     # Frontend
│   ├── main.js              # bootstrap + shell/event orchestration
│   ├── app/                 # state / platform / theme / overlay / Tauri bridge
│   ├── ui/                  # inline SVG icon system
│   ├── utils/               # markdown renderer + Vitest coverage
│   ├── styles.css           # style entrypoint and component styles
│   ├── styles/              # design tokens, platform profiles, theme variables
│   └── components/
│       ├── ChatBubble/
│       ├── PetCharacter/
│       └── SettingsPanel/
├── src-tauri/               # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── lib.rs           # Tauri commands + run()
│       ├── main.rs          # binary entry → ipet_lib::run()
│       ├── app_error.rs
│       ├── config.rs
│       ├── storage/         # per-domain: chat/tools/token_usage/caches/preferences + migrations
│       ├── llm_client.rs
│       ├── tool_dispatcher.rs
│       ├── tool_package.rs
│       ├── http_safety.rs
│       ├── secret.rs        # at-rest key encryption
│       └── testutil.rs
└── tool-packages/           # tool.json packages + built-in tool crates (single source of truth)
    ├── get_system_status/   # tool.json + rust/ crate (workspace member)
    ├── scan_disk/           # tool.json + rust/ crate (workspace member)
    └── echo_local/          # example local (subprocess) tool, schemaVersion 2
```

### Roadmap

- Harden `local` tools: import-time confirmation, command/path constraints, or pre-call user approval.
- Tighter CSP (drop `style-src 'unsafe-inline'`).
- Replace the CSS sprite with a Live2D model inside `src/components/PetCharacter/`.
- Frontend smoke tests; multi-round tool-call loop; backend chat abort (a local Stop button already cancels the UI side).
- macOS / Linux smoke tests (the code is portable; macOS DMG is built in CI, Linux is not yet built).

</details>

---

<a id="-中文"></a>
<details open>
<summary><h2>中文</h2></summary>

### 概览

**iPet** 是一个透明、置顶的桌面宠物，悬浮在你真实的桌面之上。桌宠会和 OpenAI 兼容的 LLM 对话，能调用一组本地工具（系统指标、磁盘扫描、自定义 HTTP 工具），并把所有数据（聊天记录、设置、工具配置、token 使用）都持久化到本地 SQLite 数据库。**没有遥测，没有云端后端**：唯一的外部网络请求是你自己配置的 LLM 接口。

项目使用 **Tauri 2**（Rust 后端 + WebView 前端），前端构建走 **Vite**，最终产物是一个 Windows 可执行文件，外加可选的 MSI / portable-zip。

### 亮点

- **透明、无边框、默认置顶窗口** —— 任意位置拖拽、鼠标穿透开关、"紧凑"浮头模式。
- **CSS 动画桌宠**，具备 `idle / thinking / talking / tool / warning` 多态；后续可平滑替换为 Live2D 模型。
- **Apple 风格 UI 重构** —— 设计 token 体系（排版/间距/动效/圆角）、浅色与暗色模式（可手动覆盖）、按平台 profile（macOS/Windows/Linux，经 `data-platform`）、无依赖内联 SVG 图标、自定义 dialog + toast 系统（不再用 `window.confirm`）、分段式工具 composer、Temperature 滑块、统计 Bento + Prompt/Completion 占比条、dialog focus trap、支持 `prefers-reduced-motion`。
- **流式对话**，对接任何 OpenAI 兼容的 `/chat/completions` 接口；实时打字 + 思考计时器；用 `marked` + `DOMPurify` 渲染 Markdown（GFM 表格、任务列表、代码块）。
- **三类工具、统一规范的 function calling** —— **19 个内置工具**（见下）、自定义 HTTP 工具、以及 `local` 子进程工具，全部由 `tool.json` 描述（schemaVersion 1=http / 2=+local），运行时热插拔（每请求实时查 DB，无缓存）。内置工具实现为 `ipet-tool-desktop-tools` crate 的 Rust 函数，经 dispatcher 路由，并在 Tokio 阻塞线程池上调度，慢工具不会卡住运行时。
- **19 个内置桌面工具** —— `get_system_status`、`scan_disk`、`search_files`、`read_text_file`、`git_status`、`list_processes`、`network_status`、`clipboard_read`、`open_path`、`recent_system_errors`、`package_scripts`、`run_project_check`、`disk_cleanup_candidates`、`create_note`、`weather_lookup`、`web_search`、`screenshot_ocr`，以及用于长期记忆的 `memory_save` / `memory_search`。每个网络/子进程调用都带超时和输出上限。
- **多会话聊天** —— 在对话头部切换器里切换 / 新建 / 重命名 / 删除会话；消息按会话隔离，活跃会话 id 由后端持有，重启与跨窗口写入保持一致。切换为乐观更新（列表和下拉框在点击同一帧就更新，不等后端往返），并用序列号丢弃切换途中的过期加载。
- **长期记忆（Tier 1，双轨）** —— 模型跨会话记住事实/偏好：每轮把最近更新的若干条记忆拼进系统提示词（稳定常驻上下文），并可通过 `memory_save` / `memory_search` 工具按需写入/召回。记忆在「控制中心 → 记忆」分区可查看与编辑，无黑箱记忆。
- **HTTP 工具 SSRF 加固** —— 保存时和发起请求时都做 URL 黑名单校验（DNS 解析后再检查；拒绝 loopback / 私网 / 链路本地 / CGNAT / ULA，含 IPv4-mapped IPv6）；30 秒超时、最多 5 次重定向、响应体 2 MiB 上限。
- **local 工具走子进程** —— iPet spawn 配置的可执行文件，把模型参数作为一行 JSON 写到 stdin、读 stdout 作为返回值；硬超时、2 MiB stdout 上限、非零退出码报错兜底。丢一个脚本 + 注册 `tool.json` 即用，无需重编译。
- **Tauri 基础 CSP** —— `script-src 'self'`、`object-src 'none'`、`frame-ancestors 'none'`，IPC / asset host 显式列出。
- **本地 SQLite 持久化 + 版本化迁移** —— 偏好、会话、聊天历史、记忆、工具配置、token 使用、系统采样、磁盘扫描缓存；schema 通过 `PRAGMA user_version` 演进（当前 v3）。
- **API Key 本机加密** —— LLM API key 用 ChaCha20-Poly1305 + 每机密钥加密落库，非明文。
- **Token 统计** —— 按天 / 按模型 / 最近请求拆分；同一次对话里工具决策调用和最终流式回复的 usage 会合并成一条记录。
- **Cargo workspace** —— 内置工具作为 `tool-packages/*/rust/` crate 被 `src-tauri` 依赖，运行时与可分发包共用同一份真源。
- **跨平台 CI** —— GitHub Actions 矩阵构建 Windows（NSIS + 绿色版 zip）与 macOS（DMG）；macOS 经 `tauri.macos.conf.json` 叠加配置启用原生红绿灯，Windows/Linux 保持无边框。
- **约 130 个测试**（56 主机 + 9 desktop-tools crate + 8 scan-disk crate + 60 前端），覆盖配置校验、磁盘扫描、存储与迁移（含 v3 会话/记忆回填）、工具包解析、dispatcher（含 local 子进程）、HTTP 安全、密钥加密、Markdown 清洗、三模式 shell + 控制中心分区渲染、会话切换器 + 记忆视图渲染、聊天消息类型渲染。
- **`tracing` 日志埋点** —— 通过 `IPET_LOG` 环境变量实时切日志级别（例如 `IPET_LOG=ipet_lib::tool_dispatcher=trace`）。

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
│   src/main.js                           ← 启动、窗口事件、业务调度               │
│   src/app/                              ← state / platform / theme / overlay / IPC│
│   src/shell/                            ← AppShell / WindowChrome / Capsule /    │
│   │                                       TalkWorkspace / ControlCenter          │
│   src/views/                            ← 控制中心分区：模型 / 工具 / 用量 /     │
│   │                                       系统 / 外观                            │
│   src/ui/                               ← 内联 SVG 图标集（无依赖）              │
│   src/utils/                            ← Markdown 渲染器 + 前端测试             │
│   src/styles.css                        ← 样式入口，@import 各模块               │
│   src/styles/                           ← tokens / base / shell / chat /         │
│   │                                       control-center / dialog / responsive  │
│   src/components/ChatBubble/            ← 聊天 UI、流式气泡                      │
│   src/components/PetCharacter/          ← CSS 角色精灵 + 状态切换                │
│                                                                                  │
└──────────────────────────────── Tauri IPC ───────────────────────────────────────┘
┌────────────────────────── 后端（Rust，src-tauri/src/）───────────────────────────┐
│                                                                                  │
│   lib.rs              ← 21 个 #[tauri::command] + run() 入口 + tracing 初始化    │
│   app_error.rs        ← AppResult<T> / AppError 统一错误类型                     │
│   config.rs           ← LlmSettings 数据结构与归一化、校验                       │
│   storage/            ← rusqlite 封装，6 张表，版本化迁移，按表域拆分子模块      │
│                         （chat / tools / token_usage / caches / preferences）    │
│   llm_client.rs       ← OpenAI 兼容流式聊天 + function calling                   │
│   tool_dispatcher.rs  ← 内置 + HTTP + local（子进程）工具执行                    │
│   http_safety.rs      ← URL 黑白名单 + DNS 解析校验                              │
│   secret.rs           ← ChaCha20-Poly1305 本机密钥加密                           │
│   tool_package.rs     ← tool.json（schemaVersion 1/2）导入解析                   │
│                                                                                  │
│   ── 内置工具 crate（workspace 成员，同时也是可分发真源）──                      │
│   tool-packages/scan_disk/rust/           ← rayon 并行目录扫描                   │
│   tool-packages/get_system_status/rust/   ← sysinfo 封装（CPU / RAM / 磁盘）      │
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
| `cancel_disk_scan`       | 按 caller id 取消进行中的磁盘扫描                             |
| `get_recent_messages`    | 拉取最近 N 条聊天记录                                         |
| `list_tools`             | 列出所有工具配置（内置 + 自定义，http + local）               |
| `save_tool`              | 新建 / 更新一个自定义 http 或 local 工具（JSON Schema 参数）  |
| `import_tool_from_path`  | 从路径导入 tool.json 工具包（schemaVersion 1/2）              |
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
| `get_preference`         | 读取任意偏好字符串（如 UI 主题）                              |
| `set_preference`         | 持久化任意偏好字符串                                          |

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

iPet 内置 19 个工具，实现为 `ipet-tool-desktop-tools` crate 的 Rust 函数，经 `ToolDispatcher::dispatch_builtin` 路由，并在 Tokio 阻塞线程池上调度（`spawn_blocking`），慢工具不会卡住运行时。每个网络/子进程调用都带超时和输出上限；只读工具在其 `tool.json` 标注 `readOnly: true`，其余带明确安全说明。

**系统与文件（只读）**
- `get_system_status` —— CPU（总体 + 每核）、内存/swap、各磁盘占用，底层 `sysinfo`。供可选的自动系统检查循环使用。
- `scan_disk` —— 基于 `rayon` 的并行目录扫描，按大小排序返回树形摘要，带 `max_depth`/`max_children` 裁剪。结果缓存到 SQLite。
- `search_files` —— 在目录下按文件名/路径搜索，可选内容搜索小文本文件。结果数有上限，不跟随符号链接，隐藏文件需显式开启。
- `read_text_file` —— 按字节/行窗口读取文本文件片段；1 MiB / 2000 行上限。
- `git_status` —— 经固定只读 `git` 命令返回分支、HEAD、简短状态、diff 统计（+ 可选截断 diff）。
- `list_processes` —— 按 CPU/内存/名称排序的进程列表，可过滤；sysinfo 双采样得到稳定 CPU%。
- `network_status` —— DNS 解析 + TCP 连通性探测 + 各平台网络接口摘要；connect_timeout 有界。
- `recent_system_errors` —— Windows 事件日志 / macOS unified log / `journalctl` 的 error 级条目，支持 `sinceHours`。
- `disk_cleanup_candidates` —— 按大小列出 temp/缓存目录里的大文件；**只读，绝不删除**。

**项目与剪贴板**
- `package_scripts` —— 读取项目 `package.json` 的 `scripts`（只读）。
- `run_project_check` —— 运行**白名单内**的构建/测试命令（`npm test`、`npm run build`、`cargo check` 等），带硬超时和管道排空。白名单外的命令一律拒绝。
- `clipboard_read` —— 读取系统剪贴板文本（PowerShell / `pbpaste` / `xclip`），有上限。

**写 / 有副作用（模型发起，用户可审计）**
- `open_path` —— 用系统默认方式打开本地路径或 `http(s)` 网址。
- `create_note` —— 在 `~/.ipet/notes/` 下写入/追加 Markdown 备忘录（slugify 文件名，256 KB 上限）。
- `screenshot_ocr` —— 截主屏 + `tesseract` OCR；tesseract 缺失时返回明确错误。隐私敏感。

**网络（免 Key）**
- `weather_lookup` —— 经 wttr.in 查当前天气（温度/体感/湿度/风速），支持 c/f。
- `web_search` —— DuckDuckGo Instant Answer；仅摘要/百科类结果，非完整网页搜索。

**长期记忆**
- `memory_save` —— 写入/更新一条跨会话记忆（按 key）。
- `memory_search` —— 按 key/content/category 子串召回；命中即更新使用计数。

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

### 自定义 local 工具

`kind: "local"` 工具是本机任意可执行文件或脚本（`.exe` / `.py` / `.js` / `.bat` …）。iPet 调用时 spawn 子进程，把模型参数作为一行 JSON 写到子进程 **stdin**，把子进程 **stdout** 作为返回值。子进程独立运行，崩了或卡住都由超时兜底，不会拖垮宿主。在 **设置 → 工具 → 添加本地工具** 添加，或导入 `tool.json`（schemaVersion 2）包。

```json
{
  "schemaVersion": 2,
  "name": "echo_local",
  "displayName": "回显",
  "description": "把传入的 text 原样返回。",
  "kind": "local",
  "parameters": {
    "type": "object",
    "required": ["text"],
    "properties": { "text": { "type": "string" } }
  },
  "local": {
    "command": "node",
    "args": ["echo_tool.js"],
    "timeoutSecs": 15
  }
}
```

约定与护栏：

- 模型参数走 stdin（一行 JSON），不拼进命令行——参数层面没有 shell 注入面。
- 路径型的相对 `command`/`cwd`（如 `./script.js`、`dir/x`）导入时按包目录解析为绝对路径；bare 解释器名（`node`、`python`）留作 PATH 查找。
- 硬超时（`timeoutSecs`，默认 30）到点杀进程；非零退出码报错（带 stderr）；stdout 上限 2 MiB。
- **安全**：local 工具以当前用户权限运行，等同于你手动执行该命令。只添加可信来源的工具。超时 / stdin / 输出上限是护栏而非沙箱，详见 `docs/TOOL_PACKAGE.md`。

完整示例在 `tool-packages/echo_local/`。

### 工具包导入

任何工具都以 `tool.json` 包形式分发（可选附 `README.md`，local 工具通常附脚本）。在 **设置 → 工具 → 导入工具包** 指向目录或 `tool.json` 文件即可导入——完整 schema 见 `docs/TOOL_PACKAGE.md`。`openai-function.json` 由 `npm run sync:tools` 从 `tool.json` 生成，参数 schema 只维护一处。

### Token 统计

**设置 → 统计** 展示累计 token、prompt / completion 拆分、请求数、工具调用数、按天 / 按模型聚合、以及最近请求。非流式工具决策和流式最终回复的 usage 会尽量合并成一条记录；如果某个 OpenAI 兼容服务不返回 usage，那次请求不会写入统计。

### SQLite 表结构

| 表名              | 存储内容                                                        |
|-------------------|-----------------------------------------------------------------|
| `preferences`     | LLM 设置、窗口偏好、自动检查开关、人设                          |
| `sessions`        | 聊天会话（标题、时间戳、last_message_at）—— 多会话             |
| `chat_messages`   | 完整聊天历史（role / content / 时间戳），按 `session_id` 隔离   |
| `memories`        | 跨会话长期记忆条目（key/content/category + 使用计数）           |
| `disk_scan_cache` | 缓存的 `scan_disk` 结果，按路径 + 选项做 key                    |
| `system_samples`  | 最近的 `get_system_status` 采样                                 |
| `tool_configs`    | 内置 + 自定义工具定义（http + local），含启用状态              |
| `token_usage`     | 按请求记账（prompt / completion / total）                       |

Schema 当前为 **v3**（v2 → v3 新增 `sessions` + `memories`，并在 `chat_messages` 上加 `session_id` 外键；v3 之前的历史消息会回填到一个默认会话）。迁移在 `src-tauri/src/storage/mod.rs` 的 `MIGRATIONS` 下按 append-only 维护。

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

# Rust 单元测试（51 主机 + 8 scan-disk crate —— 见下方 "为什么 --release" 说明）
cargo test --release --lib --manifest-path src-tauri/Cargo.toml
```

> **为什么 `--release`？** debug 模式的 `cargo test` 会静态链接完整 Tauri 栈，产生 >200 MiB 的测试二进制，会在测试代码运行之前就触发 Windows 加载器 bug（`STATUS_ENTRYPOINT_NOT_FOUND`）。release 模式的优化能把体积压到阈值以下。增量构建依然在 1 秒内。

### 发布产物

```powershell
# 完整 Tauri 打包（前端构建 + Rust release 编译 + 打包）
npm run tauri:build
```

会产出（`tauri.conf.json` 当前配置 `["nsis"]`；portable zip 是目前的稳定产物）：

| 产物          | 路径                                                            |
|---------------|-----------------------------------------------------------------|
| 可执行文件    | `src-tauri/target/release/ipet.exe`                             |
| NSIS 安装包   | `src-tauri/target/release/bundle/nsis/iPet_0.1.0_x64-setup.exe` |
| Portable zip  | `src-tauri/target/release/bundle/zip/iPet_0.1.0_x64_en-US.zip`  |

#### 已知问题：WiX MSI ICE 校验

当前开发机执行 MSI 打包（`tauri build` 带 `msi` target）时，WiX 的 ICE 校验阶段可能失败，这是**本机 Windows Installer / WiX 环境**问题，不是应用源码错误——这也是配置里用 NSIS 而非 MSI 的原因。

**Workaround**（若确实需要 MSI）—— Tauri 已经生成 `target/release/wix/x64/main.wixobj` 后，手动跳过 ICE 校验：

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
├── Cargo.toml               # Cargo workspace 根（成员：src-tauri + 工具 crate）
├── package.json             # npm 脚本（dev / build / tauri:* / sync:tools）
├── vite.config.js
├── index.html               # WebView 入口
├── scripts/
│   └── sync-openai-functions.js  # 从 tool.json 重新生成 openai-function.json
├── src/                     # 前端
│   ├── main.js              # 启动、窗口事件、业务调度
│   ├── app/                 # state / platform / theme / overlay / Tauri bridge
│   ├── ui/                  # 无依赖 inline SVG 图标
│   ├── utils/               # Markdown 渲染器 + Vitest 测试
│   ├── styles.css           # 样式入口，继续承载组件样式
│   ├── styles/              # 设计 token、平台 profile、主题变量
│   └── components/
│       ├── ChatBubble/
│       ├── PetCharacter/
│       └── SettingsPanel/
├── src-tauri/               # Rust 后端
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── lib.rs           # Tauri 命令 + run()
│       ├── main.rs          # 二进制入口 → ipet_lib::run()
│       ├── app_error.rs
│       ├── config.rs
│       ├── storage/         # 按表域拆分：chat/tools/token_usage/caches/preferences + 迁移
│       ├── llm_client.rs
│       ├── tool_dispatcher.rs
│       ├── tool_package.rs
│       ├── http_safety.rs
│       ├── secret.rs        # 本机密钥加密
│       └── testutil.rs
└── tool-packages/           # tool.json 包 + 内置工具 crate（唯一真源）
    ├── get_system_status/   # tool.json + rust/ crate（workspace 成员）
    ├── scan_disk/           # tool.json + rust/ crate（workspace 成员）
    └── echo_local/          # 示例 local（子进程）工具，schemaVersion 2
```

### 路线图

- 收敛 local 工具安全：导入时确认提示、命令/路径约束，或调用前用户确认。
- 收紧 CSP（去掉 `style-src 'unsafe-inline'`）。
- 在 `src/components/PetCharacter/` 内用 Live2D 模型替换当前 CSS 精灵。
- 前端 smoke 测试；多轮工具调用循环；后端聊天中断（本地 Stop 按钮已可在 UI 侧取消）。
- macOS / Linux 冒烟测试（代码本身可移植；macOS DMG 已在 CI 构建，Linux 暂未构建）。

</details>

---

<div align="center">
<sub>Built with <a href="https://tauri.app/">Tauri 2</a> · <a href="https://www.rust-lang.org/">Rust</a> · <a href="https://vitejs.dev/">Vite</a></sub>
</div>
