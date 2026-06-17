# get_system_status

`get_system_status` 是 iPet 的内置系统状态工具，用于让模型读取当前机器的轻量运行状态。

## 功能

- CPU 总使用率和各核心使用率
- 内存总量、已用量、可用量和使用率
- 磁盘挂载点、总容量、已用量、可用量和使用率
- 当前进程数量
- CPU/内存占用较高的进程列表

## 工具配置

工具配置见 `tool.json`。这是一个 `builtin` 工具，需要宿主应用提供 Rust 内置调度逻辑：

```json
{
  "name": "get_system_status",
  "kind": "builtin",
  "parameters": {
    "type": "object",
    "properties": {
      "process_limit": {
        "type": "integer",
        "minimum": 3,
        "maximum": 30
      }
    }
  }
}
```

## 源码

本包包含 Rust 源码：

```text
rust/
├── Cargo.toml
├── INTEGRATION.md
└── src/
    ├── lib.rs
    └── system_monitor.rs
```

核心入口：

- `rust/src/system_monitor.rs`：系统监控实现
- `rust/src/lib.rs`：独立包导出和 `run_tool(process_limit)` 示例入口

独立调用示例：

```rust
let json = ipet_tool_get_system_status::run_tool(Some(10))?;
```

## 参数

| 参数 | 类型 | 必填 | 默认 | 说明 |
|---|---|---:|---:|---|
| `process_limit` | integer | 否 | 10 | 返回的高占用进程数量，范围 3-30。 |

## 返回数据

返回 JSON 字符串，结构由 `SystemSnapshot` 序列化而来，主要字段包括：

- `cpuUsage`
- `cpus`
- `memory`
- `disks`
- `processes`
- `processCount`
- `sampledAt`

## 模型调用场景

适合在用户询问以下内容时调用：

- “现在电脑卡不卡？”
- “CPU/内存占用怎么样？”
- “哪个进程占用高？”
- “帮我看看系统状态。”

## 接入说明

1. 将 `tool.json` 复制到工具注册表或导入流程。
2. 将 `rust/src/system_monitor.rs` 集成到宿主 Rust 后端，或直接使用 `rust/` 作为独立 crate。
3. 将 `openai-function.json` 的内容加入 OpenAI 兼容接口的 `tools` 列表。
4. 宿主调度器收到 `get_system_status` 调用后执行 `SystemMonitor::snapshot(...)` 或 `run_tool(...)`。
