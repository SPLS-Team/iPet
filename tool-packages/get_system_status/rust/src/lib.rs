pub mod system_monitor;

pub use system_monitor::{
    CpuCoreSample, DiskSample, MemorySample, ProcessSample, SystemMonitor, SystemSnapshot,
};

pub fn run_tool(process_limit: Option<usize>) -> Result<String, serde_json::Error> {
    let mut monitor = SystemMonitor::new();
    let snapshot = monitor.snapshot(process_limit.unwrap_or(10).clamp(3, 30));
    serde_json::to_string(&snapshot)
}

