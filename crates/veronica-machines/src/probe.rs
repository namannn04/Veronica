//! Reading a machine's vital signs.
//!
//! One shell snippet gathers everything in a single round trip, because over
//! SSH each extra command is another whole connection's latency. The snippet
//! reads only procfs and `df`, so it needs no privileges and nothing installed
//! on the far end beyond a POSIX shell.
//!
//! CPU usage cannot be read from a single sample: `/proc/stat` holds cumulative
//! counters, so a percentage requires two reads and the difference between
//! them. The snippet takes both itself, with a short sleep in between, rather
//! than making the caller connect twice.

use serde::Serialize;

/// The snippet run on the machine being probed.
///
/// Output is a line protocol Veronica controls, so parsing does not depend on
/// the formatting of any tool: `key value...` per line, with the two CPU
/// samples tagged separately.
pub const PROBE_SCRIPT: &str = r#"
echo "host $(uname -n)"
echo "kernel $(uname -r)"
echo "os $(. /etc/os-release 2>/dev/null && echo "$PRETTY_NAME" || uname -s)"
echo "uptime $(cut -d' ' -f1 /proc/uptime)"
echo "load $(cut -d' ' -f1-3 /proc/loadavg)"
echo "cpu1 $(grep '^cpu ' /proc/stat)"
sleep 0.3
echo "cpu2 $(grep '^cpu ' /proc/stat)"
grep -E '^(MemTotal|MemAvailable|SwapTotal|SwapFree):' /proc/meminfo | sed 's/^/mem /'
df -B1 --output=target,size,avail -x tmpfs -x devtmpfs -x squashfs -x overlay 2>/dev/null \
  | tail -n +2 | sed 's/^/disk /'
"#;

/// Cumulative CPU jiffies from one `/proc/stat` sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuSample {
    pub idle: u64,
    pub total: u64,
}

impl CpuSample {
    /// Parse a `cpu  user nice system idle iowait irq softirq steal ...` line.
    ///
    /// Idle time is idle plus iowait: a machine waiting on disk is not busy in
    /// any sense the user cares about.
    pub fn parse(line: &str) -> Option<Self> {
        let mut fields = line.split_whitespace();
        let label = fields.next()?;
        if !label.starts_with("cpu") {
            return None;
        }
        let values: Vec<u64> = fields.filter_map(|f| f.parse::<u64>().ok()).collect();
        if values.len() < 4 {
            return None;
        }
        let idle = values[3] + values.get(4).copied().unwrap_or(0);
        Some(Self {
            idle,
            total: values.iter().sum(),
        })
    }

    /// Busy share between two samples, 0-100.
    ///
    /// Returns zero when the counters did not advance, and survives a counter
    /// reset (a reboot between samples) without reporting nonsense.
    pub fn usage_between(first: Self, second: Self) -> f64 {
        let total = second.total.saturating_sub(first.total);
        let idle = second.idle.saturating_sub(first.idle);
        if total == 0 || idle > total {
            return 0.0;
        }
        ((total - idle) as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl DiskUsage {
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

/// A machine's state at one moment.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineStats {
    pub host_name: String,
    pub kernel: String,
    pub os: String,
    pub uptime_secs: u64,
    pub load_average: [f64; 3],
    pub cpu_percent: f64,
    pub memory_total_bytes: u64,
    pub memory_available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_free_bytes: u64,
    pub disks: Vec<DiskUsage>,
}

impl MachineStats {
    pub fn memory_used_bytes(&self) -> u64 {
        self.memory_total_bytes
            .saturating_sub(self.memory_available_bytes)
    }

    pub fn memory_used_percent(&self) -> f64 {
        if self.memory_total_bytes == 0 {
            return 0.0;
        }
        self.memory_used_bytes() as f64 / self.memory_total_bytes as f64 * 100.0
    }

    /// The filesystem the user most likely means, for a one-line summary.
    pub fn root_disk(&self) -> Option<&DiskUsage> {
        self.disks
            .iter()
            .find(|disk| disk.mount_point == "/")
            .or_else(|| self.disks.first())
    }
}

/// Mounts that are not real storage and would clutter a fleet view.
///
/// The probe filters most of these by filesystem type, but EFI variable and
/// boot-firmware mounts come through as tiny real filesystems.
fn is_uninteresting_mount(mount_point: &str) -> bool {
    mount_point.starts_with("/sys")
        || mount_point.starts_with("/proc")
        || mount_point.starts_with("/run")
        || mount_point.starts_with("/snap/")
        || mount_point.starts_with("/var/snap/")
}

/// Parse the probe's output.
///
/// Tolerant by design: a machine missing `df`, or a BSD-ish `/proc`, should
/// still yield whatever did parse rather than nothing at all.
pub fn parse(output: &str) -> MachineStats {
    let mut stats = MachineStats::default();
    let mut first_cpu: Option<CpuSample> = None;
    let mut second_cpu: Option<CpuSample> = None;

    for line in output.lines() {
        let line = line.trim();
        let Some((key, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let rest = rest.trim();
        match key {
            "host" => stats.host_name = rest.to_string(),
            "kernel" => stats.kernel = rest.to_string(),
            "os" => stats.os = rest.to_string(),
            "uptime" => {
                stats.uptime_secs = rest.parse::<f64>().map(|v| v as u64).unwrap_or(0);
            }
            "load" => {
                let values: Vec<f64> =
                    rest.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                for (index, value) in values.into_iter().take(3).enumerate() {
                    stats.load_average[index] = value;
                }
            }
            "cpu1" => first_cpu = CpuSample::parse(rest),
            "cpu2" => second_cpu = CpuSample::parse(rest),
            "mem" => {
                // "MemTotal: 15078116 kB"
                let mut parts = rest.split_whitespace();
                let Some(field) = parts.next() else { continue };
                let Some(value) = parts.next().and_then(|v| v.parse::<u64>().ok()) else {
                    continue;
                };
                // procfs reports kibibytes, and the interface wants bytes.
                let bytes = value.saturating_mul(1024);
                match field.trim_end_matches(':') {
                    "MemTotal" => stats.memory_total_bytes = bytes,
                    "MemAvailable" => stats.memory_available_bytes = bytes,
                    "SwapTotal" => stats.swap_total_bytes = bytes,
                    "SwapFree" => stats.swap_free_bytes = bytes,
                    _ => {}
                }
            }
            "disk" => {
                // The mount point comes first and may contain spaces, so the
                // two numeric columns are taken from the end.
                let fields: Vec<&str> = rest.split_whitespace().collect();
                if fields.len() < 3 {
                    continue;
                }
                let available = fields[fields.len() - 1].parse::<u64>().ok();
                let total = fields[fields.len() - 2].parse::<u64>().ok();
                let mount = fields[..fields.len() - 2].join(" ");
                if let (Some(total), Some(available)) = (total, available) {
                    if total > 0 && !is_uninteresting_mount(&mount) {
                        stats.disks.push(DiskUsage {
                            mount_point: mount,
                            total_bytes: total,
                            available_bytes: available,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    if let (Some(first), Some(second)) = (first_cpu, second_cpu) {
        stats.cpu_percent = CpuSample::usage_between(first, second);
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real output shape, with values captured from a live machine.
    const SAMPLE: &str = "\
host namannn04-ROG-Strix-G513RC-G513RC
kernel 7.0.0-28-generic
os Ubuntu 26.04 LTS
uptime 123088.65
load 0.76 0.72 0.79
cpu1 cpu  5820643 18103 1803359 97686090 244985 0 72401 0 0 0
cpu2 cpu  5820700 18103 1803400 97686390 244985 0 72401 0 0 0
mem MemTotal:       15078116 kB
mem MemAvailable:    7096580 kB
mem SwapTotal:       4194300 kB
mem SwapFree:        1200072 kB
disk /                         512778035200 322225905664
disk /sys/firmware/efi/efivars       131072        67616
disk /boot                       2040373248   1649893376
disk /boot/efi                    268435456    228634624
";

    #[test]
    fn parses_the_real_probe_output() {
        let stats = parse(SAMPLE);
        assert_eq!(stats.host_name, "namannn04-ROG-Strix-G513RC-G513RC");
        assert_eq!(stats.kernel, "7.0.0-28-generic");
        assert_eq!(stats.os, "Ubuntu 26.04 LTS");
        assert_eq!(stats.uptime_secs, 123_088);
        assert_eq!(stats.load_average, [0.76, 0.72, 0.79]);
        // 15078116 KiB, which procfs reports in kibibytes.
        assert_eq!(stats.memory_total_bytes, 15_078_116 * 1024);
        assert_eq!(stats.swap_total_bytes, 4_194_300 * 1024);
    }

    #[test]
    fn computes_cpu_from_the_two_samples() {
        let stats = parse(SAMPLE);
        // 398 jiffies advanced, 300 of them idle, so ~24.6% busy.
        assert!(
            (stats.cpu_percent - 24.62).abs() < 0.1,
            "got {}",
            stats.cpu_percent
        );
    }

    #[test]
    fn a_single_cpu_sample_yields_zero_rather_than_a_wrong_number() {
        let one_sample = "cpu1 cpu  100 0 100 800 0 0 0 0 0 0\n";
        assert_eq!(parse(one_sample).cpu_percent, 0.0);
    }

    #[test]
    fn identical_cpu_samples_are_zero_not_a_division_by_zero() {
        let line = "cpu  100 0 100 800 0 0 0 0 0 0";
        let sample = CpuSample::parse(line).unwrap();
        assert_eq!(CpuSample::usage_between(sample, sample), 0.0);
    }

    #[test]
    fn a_counter_reset_between_samples_does_not_report_nonsense() {
        // A reboot resets the counters, so the second sample is smaller.
        let first = CpuSample { idle: 900, total: 1000 };
        let second = CpuSample { idle: 5, total: 10 };
        assert_eq!(CpuSample::usage_between(first, second), 0.0);
    }

    #[test]
    fn cpu_idle_counts_iowait_as_idle() {
        // user=100 idle=0 iowait=900: waiting on disk is not busy.
        let sample = CpuSample::parse("cpu 100 0 0 0 900 0 0").unwrap();
        assert_eq!(sample.idle, 900);
        assert_eq!(sample.total, 1000);
    }

    #[test]
    fn drops_firmware_and_pseudo_mounts_but_keeps_real_ones() {
        let stats = parse(SAMPLE);
        let mounts: Vec<&str> = stats.disks.iter().map(|d| d.mount_point.as_str()).collect();
        assert_eq!(mounts, vec!["/", "/boot", "/boot/efi"]);
    }

    #[test]
    fn a_mount_point_containing_spaces_still_parses() {
        // The numeric columns are taken from the end for exactly this reason.
        let line = "disk /media/My Backup Drive 1000 400\n";
        let stats = parse(line);
        assert_eq!(stats.disks.len(), 1);
        assert_eq!(stats.disks[0].mount_point, "/media/My Backup Drive");
        assert_eq!(stats.disks[0].total_bytes, 1000);
        assert_eq!(stats.disks[0].available_bytes, 400);
    }

    #[test]
    fn disk_percentages_and_root_selection() {
        let stats = parse(SAMPLE);
        let root = stats.root_disk().expect("root should be found");
        assert_eq!(root.mount_point, "/");
        // 512778035200 total, 322225905664 free -> ~37% used
        assert!((root.used_percent() - 37.16).abs() < 0.1, "got {}", root.used_percent());
    }

    #[test]
    fn memory_percentage_uses_available_not_free() {
        let stats = parse(SAMPLE);
        // Available is what a program can actually get, so it drives the figure.
        assert!(
            (stats.memory_used_percent() - 52.94).abs() < 0.1,
            "got {}",
            stats.memory_used_percent()
        );
    }

    #[test]
    fn garbage_and_partial_output_do_not_panic() {
        for input in ["", "nonsense", "disk /only-two-fields 100", "mem NotANumber: x kB"] {
            let stats = parse(input);
            assert_eq!(stats.cpu_percent, 0.0);
        }
    }

    #[test]
    fn percentages_never_divide_by_zero() {
        let empty = MachineStats::default();
        assert_eq!(empty.memory_used_percent(), 0.0);
        assert_eq!(empty.memory_used_bytes(), 0);
        assert!(empty.root_disk().is_none());
        assert_eq!(DiskUsage::default().used_percent(), 0.0);
    }

    #[test]
    fn the_probe_script_takes_two_cpu_samples() {
        // Without both, CPU would always read zero, which is the subtlest way
        // for this to look like it works.
        assert!(PROBE_SCRIPT.contains("cpu1"));
        assert!(PROBE_SCRIPT.contains("cpu2"));
        assert!(PROBE_SCRIPT.contains("sleep"));
    }
}
