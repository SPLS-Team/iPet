use serde::Serialize;
use std::cmp::Ordering;
use sysinfo::{Disks, ProcessesToUpdate, System};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuCoreSample {
    pub name: String,
    pub usage: f32,
    pub frequency_mhz: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySample {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSample {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSample {
    pub pid: String,
    pub name: String,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    pub cpu_usage: f32,
    pub cpus: Vec<CpuCoreSample>,
    pub memory: MemorySample,
    pub disks: Vec<DiskSample>,
    pub processes: Vec<ProcessSample>,
    pub process_count: usize,
    pub sampled_at: String,
}

pub struct SystemMonitor {
    system: System,
    disks: Disks,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_cpu_all();
        system.refresh_memory();
        system.refresh_processes(ProcessesToUpdate::All, true);

        Self {
            system,
            disks: Disks::new_with_refreshed_list(),
        }
    }

    pub fn snapshot(&mut self, process_limit: usize) -> SystemSnapshot {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system
            .refresh_processes(ProcessesToUpdate::All, true);
        self.disks.refresh_list();
        self.disks.refresh();

        let cpus = self
            .system
            .cpus()
            .iter()
            .map(|cpu| CpuCoreSample {
                name: cpu.name().to_string(),
                usage: cpu.cpu_usage(),
                frequency_mhz: cpu.frequency(),
            })
            .collect::<Vec<_>>();

        let cpu_usage = if cpus.is_empty() {
            0.0
        } else {
            cpus.iter().map(|cpu| cpu.usage).sum::<f32>() / cpus.len() as f32
        };

        let total_memory = self.system.total_memory();
        let used_memory = self.system.used_memory();
        let available_memory = self.system.available_memory();
        let memory_usage = percent(used_memory, total_memory);

        let disks = self
            .disks
            .list()
            .iter()
            .map(|disk| {
                let total = disk.total_space();
                let available = disk.available_space();
                let used = total.saturating_sub(available);
                DiskSample {
                    name: disk.name().to_string_lossy().to_string(),
                    mount_point: disk.mount_point().display().to_string(),
                    file_system: disk.file_system().to_string_lossy().to_string(),
                    total_bytes: total,
                    available_bytes: available,
                    used_bytes: used,
                    usage_percent: percent(used, total),
                }
            })
            .collect::<Vec<_>>();

        let mut processes = self
            .system
            .processes()
            .values()
            .map(|process| ProcessSample {
                pid: process.pid().to_string(),
                name: process.name().to_string_lossy().to_string(),
                cpu_usage: process.cpu_usage(),
                memory_bytes: process.memory(),
            })
            .collect::<Vec<_>>();

        processes.sort_by(|a, b| {
            b.cpu_usage
                .partial_cmp(&a.cpu_usage)
                .unwrap_or(Ordering::Equal)
                .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
        });
        let process_count = processes.len();
        processes.truncate(process_limit);

        SystemSnapshot {
            cpu_usage,
            cpus,
            memory: MemorySample {
                total_bytes: total_memory,
                used_bytes: used_memory,
                available_bytes: available_memory,
                usage_percent: memory_usage,
            },
            disks,
            processes,
            process_count,
            sampled_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

fn percent(value: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (value as f32 / total as f32) * 100.0
    }
}
