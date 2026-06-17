# Rust Integration

Source entrypoint:

- `src/disk_scanner.rs`

Standalone helper:

```rust
let json = ipet_tool_scan_disk::run_tool("C:\\Users", Some(4), Some(12))?;
```

iPet integration:

```rust
let request = DiskScanRequest {
    path,
    max_depth,
    max_children,
};
let result = tokio::task::spawn_blocking(move || disk_scanner::scan_path(request)).await??;
let json = serde_json::to_string(&result)?;
```

Dependencies:

- `rayon`
- `walkdir`
- `serde`
- `serde_json`
- `chrono`
- `thiserror`

