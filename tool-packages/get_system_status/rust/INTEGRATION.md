# Rust Integration

Source entrypoint:

- `src/system_monitor.rs`

Standalone helper:

```rust
let json = ipet_tool_get_system_status::run_tool(Some(10))?;
```

iPet integration:

```rust
let mut monitor = self.system.lock().await;
let snapshot = monitor.snapshot(process_limit.unwrap_or(10).clamp(3, 30));
let json = serde_json::to_string(&snapshot)?;
```

Dependencies:

- `sysinfo`
- `serde`
- `serde_json`
- `chrono`

