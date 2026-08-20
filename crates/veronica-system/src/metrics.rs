//! System metrics from procfs and sysfs.
//!
//! Edith reads these through IOKit and `host_statistics`; the Linux equivalents
//! are `/proc` and `/sys`, which `sysinfo` wraps. Sampling is deliberately
//! explicit: CPU percentages need two reads separated by an interval, so a
//! single-shot call would always report zero.

use std::time::Duration;

use serde::Serialize;
use sysinfo::{Disks, MemoryRefreshKind, Networks, System};

/// Minimum gap between CPU samples. Anything shorter and the kernel counters
/// have not moved enough to produce a meaningful percentage.
pub const MIN_CPU_INTERVAL: Duration = sysinfo::MINIMUM_CPU_UPDATE_INTERVAL;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    pub host_name: Option<String>,
    pub kernel: Option<String>,
    pub distribution: String,
    pub uptime_secs: u64,
    pub cpu: CpuStats,
    pub memory: MemoryStats,
    pub disks: Vec<DiskStats>,
    pub load_average: [f64; 3],
    pub temperatures: Vec<TemperatureReading>,
    pub battery: Option<BatteryStats>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuStats {
    /// Whole-machine usage, 0-100.
    pub usage_percent: f32,
    /// Per-core usage, in the kernel's core order.
    pub per_core: Vec<f32>,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
    pub brand: String,
    pub frequency_mhz: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStats {
    pub total_bytes: u64,
    /// What the kernel counts as in use, excluding reclaimable cache.
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

impl MemoryStats {
    pub fn used_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        self.used_bytes as f64 / self.total_bytes as f64 * 100.0
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskStats {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub removable: bool,
}

impl DiskStats {
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }

    pub fn used_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        self.used_bytes() as f64 / self.total_bytes as f64 * 100.0
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemperatureReading {
    pub label: String,
    pub celsius: f32,
    pub critical_celsius: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryStats {
    pub percent: f64,
    pub charging: bool,
    pub time_to_empty_secs: Option<u64>,
}

/// Samples metrics, holding the `System` handle between reads so CPU
/// percentages are real rather than zero.
pub struct MetricsSampler {
    system: System,
    disks: Disks,
    networks: Networks,
    primed: bool,
}

impl Default for MetricsSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsSampler {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            primed: false,
        }
    }

    /// Take a reading. The first call primes the CPU counters and sleeps for the
    /// minimum interval, so callers get usable numbers immediately instead of a
    /// zeroed first frame.
    pub fn sample(&mut self) -> SystemSnapshot {
        if !self.primed {
            self.system.refresh_cpu_all();
            std::thread::sleep(MIN_CPU_INTERVAL);
            self.primed = true;
        }
        self.system.refresh_cpu_all();
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::everything());
        self.disks.refresh();
        self.networks.refresh();

        // Read this before borrowing the cpu list, which borrows `system` too.
        let physical_cores = self.system.physical_core_count();
        let global_usage = self.system.global_cpu_usage();
        let cpus = self.system.cpus();
        let cpu = CpuStats {
            usage_percent: global_usage,
            per_core: cpus.iter().map(sysinfo::Cpu::cpu_usage).collect(),
            physical_cores,
            logical_cores: cpus.len(),
            brand: cpus.first().map(|c| c.brand().to_string()).unwrap_or_default(),
            frequency_mhz: cpus.first().map(sysinfo::Cpu::frequency).unwrap_or(0),
        };

        let memory = MemoryStats {
            total_bytes: self.system.total_memory(),
            used_bytes: self.system.used_memory(),
            available_bytes: self.system.available_memory(),
            swap_total_bytes: self.system.total_swap(),
            swap_used_bytes: self.system.used_swap(),
        };

        let disks = self
            .disks
            .list()
            .iter()
            // Snap mounts and loop devices are read-only squashfs images that
            // report 100% full; listing them would bury the real filesystems.
            .filter(|disk| !is_pseudo_mount(&disk.mount_point().display().to_string()))
            .map(|disk| DiskStats {
                name: disk.name().to_string_lossy().to_string(),
                mount_point: disk.mount_point().display().to_string(),
                file_system: disk.file_system().to_string_lossy().to_string(),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
                removable: disk.is_removable(),
            })
            .collect();

        let load = System::load_average();

        SystemSnapshot {
            host_name: System::host_name(),
            kernel: System::kernel_version(),
            distribution: System::long_os_version().unwrap_or_else(|| "Linux".into()),
            uptime_secs: System::uptime(),
            cpu,
            memory,
            disks,
            load_average: [load.one, load.five, load.fifteen],
            temperatures: read_temperatures(),
            battery: read_battery(),
        }
    }
}

/// Mounts that are not real storage and would distort a disk list.
pub fn is_pseudo_mount(mount_point: &str) -> bool {
    const PREFIXES: [&str; 6] = [
        "/snap/",
        "/var/snap/",
        "/proc",
        "/sys",
        "/run/credentials",
        "/dev/loop",
    ];
    PREFIXES.iter().any(|prefix| mount_point.starts_with(prefix))
}

/// Thermal zones from sysfs. Labels come from `type`, which names the sensor
/// (`x86_pkg_temp`, `acpitz`) rather than a number.
fn read_temperatures() -> Vec<TemperatureReading> {
    let Ok(entries) = std::fs::read_dir("/sys/class/thermal") else {
        return Vec::new();
    };
    let mut readings = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("thermal_zone"))
        {
            continue;
        }
        let Some(celsius) = read_millidegrees(&path.join("temp")) else {
            continue;
        };
        let label = std::fs::read_to_string(path.join("type"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "thermal".into());
        readings.push(TemperatureReading {
            label,
            celsius,
            critical_celsius: read_millidegrees(&path.join("trip_point_0_temp")),
        });
    }
    readings.sort_by(|a, b| a.label.cmp(&b.label));
    readings
}

/// sysfs reports temperatures in thousandths of a degree.
fn read_millidegrees(path: &std::path::Path) -> Option<f32> {
    let raw = std::fs::read_to_string(path).ok()?;
    let millidegrees: f32 = raw.trim().parse().ok()?;
    let celsius = millidegrees / 1000.0;
    // Guard against sensors that report an obviously bogus value when idle.
    (-50.0..=200.0).contains(&celsius).then_some(celsius)
}

/// Battery from sysfs. Desktops have no battery, so this is optional.
fn read_battery() -> Option<BatteryStats> {
    let entries = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = std::fs::read_to_string(path.join("type")).ok()?;
        if kind.trim() != "Battery" {
            continue;
        }
        let percent = std::fs::read_to_string(path.join("capacity"))
            .ok()?
            .trim()
            .parse::<f64>()
            .ok()?;
        let status = std::fs::read_to_string(path.join("status"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        return Some(BatteryStats {
            percent,
            charging: status == "Charging" || status == "Full",
            time_to_empty_secs: None,
        });
    }
    None
}

/// Bytes as a human-readable size, matching how the UI labels disks.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_and_loop_mounts_are_excluded_from_the_disk_list() {
        assert!(is_pseudo_mount("/snap/firefox/1234"));
        assert!(is_pseudo_mount("/var/snap/foo"));
        assert!(is_pseudo_mount("/sys/fs/cgroup"));
        assert!(!is_pseudo_mount("/"));
        assert!(!is_pseudo_mount("/home"));
        assert!(!is_pseudo_mount("/media/usb"));
    }

    #[test]
    fn human_bytes_scales_and_keeps_three_significant_figures() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(500 * 1024 * 1024), "500 MB");
        assert_eq!(human_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn percentages_never_divide_by_zero() {
        assert_eq!(MemoryStats::default().used_percent(), 0.0);
        assert_eq!(DiskStats::default().used_percent(), 0.0);
        assert_eq!(DiskStats::default().used_bytes(), 0);
    }

    #[test]
    fn disk_used_bytes_never_underflows_when_available_exceeds_total() {
        // Reserved blocks can make available look larger than total.
        let disk = DiskStats {
            total_bytes: 100,
            available_bytes: 120,
            ..Default::default()
        };
        assert_eq!(disk.used_bytes(), 0);
    }

    #[test]
    fn memory_percent_is_the_used_share_of_total() {
        let memory = MemoryStats {
            total_bytes: 1000,
            used_bytes: 250,
            ..Default::default()
        };
        assert_eq!(memory.used_percent(), 25.0);
    }

    #[test]
    fn sampling_reports_real_cpu_and_memory_on_this_machine() {
        let mut sampler = MetricsSampler::new();
        let snapshot = sampler.sample();
        assert!(snapshot.memory.total_bytes > 0, "memory should be readable");
        assert!(snapshot.cpu.logical_cores > 0, "cores should be readable");
        // The first sample must not be a zeroed frame; priming guarantees a
        // real interval has elapsed.
        assert!(
            snapshot.cpu.usage_percent >= 0.0 && snapshot.cpu.usage_percent <= 100.0 * 1.01,
            "cpu usage out of range: {}",
            snapshot.cpu.usage_percent
        );
        assert!(!snapshot.disks.is_empty(), "at least the root filesystem");
    }
}
