//! Reading a machine's vital signs.
//!
//! One shell snippet gathers everything in a single round trip, because over
//! SSH each extra command is another whole connection's latency. The snippet
//! reads procfs, sysfs, `df` and `ps`, so it needs no privileges and nothing
//! installed on the far end beyond a POSIX shell. GPUs are the one exception:
//! they are read from `nvidia-smi` where it exists, and simply absent where it
//! does not, because there is no vendor-neutral file to read them from.
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
# Thermal zones and hwmon both expose millidegrees with a label beside them.
# Two globs rather than one because the label file is named differently in each.
for zone in /sys/class/thermal/thermal_zone*; do
  [ -r "$zone/temp" ] || continue
  echo "temp $(cat "$zone/type" 2>/dev/null || echo zone) $(cat "$zone/temp")"
done 2>/dev/null
for input in /sys/class/hwmon/hwmon*/temp*_input; do
  [ -r "$input" ] || continue
  # A multi-channel chip names each channel in a sibling _label file; without
  # one, every channel would come back under the same chip name.
  channel="$(cat "${input%_input}_label" 2>/dev/null)"
  chip="$(cat "$(dirname "$input")/name" 2>/dev/null || echo hwmon)"
  [ -n "$channel" ] && chip="$chip $channel"
  echo "temp $chip $(cat "$input")"
done 2>/dev/null
# Fans, where the board exposes them. RPM, not millidegrees.
for input in /sys/class/hwmon/hwmon*/fan*_input; do
  [ -r "$input" ] || continue
  echo "fan $(basename "$input" _input) $(cat "$input")"
done 2>/dev/null
# GPUs. Only NVIDIA publishes this without a vendor library, so an AMD or Intel
# machine reports none rather than a wrong number.
command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi \
  --query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu \
  --format=csv,noheader,nounits 2>/dev/null | sed 's/^/gpu /'
# The busiest processes, which is what a fleet view is for: finding the box
# that is pegged and the thing pegging it.
ps -eo pid=,pcpu=,rss=,comm= --sort=-pcpu 2>/dev/null | head -n 8 | sed 's/^/proc /'
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

/// One temperature sensor on the machine being probed.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Temperature {
    pub label: String,
    pub celsius: f64,
}

/// One fan, in RPM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fan {
    pub label: String,
    pub rpm: u32,
}

/// One GPU, as `nvidia-smi` reports it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub name: String,
    pub utilization_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    /// `None` where the driver does not report one, rather than a zero that
    /// would read as a GPU running at freezing point.
    pub temperature_celsius: Option<f64>,
}

/// One of the busiest processes on the machine.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteProcess {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
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
    pub temperatures: Vec<Temperature>,
    pub fans: Vec<Fan>,
    pub gpus: Vec<Gpu>,
    /// The busiest processes, hottest first.
    pub processes: Vec<RemoteProcess>,
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

    /// The hottest sensor, which is the one a fleet view should show.
    pub fn peak_temperature(&self) -> Option<&Temperature> {
        self.temperatures.iter().max_by(|left, right| {
            left.celsius
                .partial_cmp(&right.celsius)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
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
                let values: Vec<f64> = rest
                    .split_whitespace()
                    .filter_map(|v| v.parse().ok())
                    .collect();
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
            "temp" => {
                // "temp <label> <millidegrees>", and a label may contain spaces.
                let fields: Vec<&str> = rest.split_whitespace().collect();
                if fields.len() < 2 {
                    continue;
                }
                let Some(milli) = fields[fields.len() - 1].parse::<f64>().ok() else {
                    continue;
                };
                let celsius = milli / 1000.0;
                // Sensors that are off or unplugged report zero or a nonsense
                // value; showing them would fill the panel with noise.
                if !(1.0..=150.0).contains(&celsius) {
                    continue;
                }
                stats.temperatures.push(Temperature {
                    label: fields[..fields.len() - 1].join(" "),
                    celsius,
                });
            }
            "fan" => {
                let fields: Vec<&str> = rest.split_whitespace().collect();
                if fields.len() < 2 {
                    continue;
                }
                let Some(rpm) = fields[fields.len() - 1].parse::<u32>().ok() else {
                    continue;
                };
                // A stopped fan is a real reading; a board that exposes an
                // absent header reports zero forever, so those are dropped.
                if rpm == 0 {
                    continue;
                }
                stats.fans.push(Fan {
                    label: fields[..fields.len() - 1].join(" "),
                    rpm,
                });
            }
            "gpu" => {
                // "gpu NVIDIA GeForce RTX 3050, 12, 900, 4096, 46" — comma
                // separated, because the name itself contains spaces.
                let fields: Vec<&str> = rest.split(',').map(str::trim).collect();
                if fields.len() < 4 {
                    continue;
                }
                let number = |index: usize| fields.get(index).and_then(|v| v.parse::<f64>().ok());
                stats.gpus.push(Gpu {
                    name: fields[0].to_string(),
                    utilization_percent: number(1).unwrap_or(0.0),
                    // nvidia-smi reports mebibytes with `nounits`.
                    memory_used_bytes: number(2).unwrap_or(0.0) as u64 * 1024 * 1024,
                    memory_total_bytes: number(3).unwrap_or(0.0) as u64 * 1024 * 1024,
                    temperature_celsius: number(4),
                });
            }
            "proc" => {
                // "proc <pid> <pcpu> <rss> <comm>", and comm may contain spaces.
                let fields: Vec<&str> = rest.split_whitespace().collect();
                if fields.len() < 4 {
                    continue;
                }
                let (Some(pid), Some(cpu), Some(rss)) = (
                    fields[0].parse::<u32>().ok(),
                    fields[1].parse::<f64>().ok(),
                    fields[2].parse::<u64>().ok(),
                ) else {
                    continue;
                };
                stats.processes.push(RemoteProcess {
                    pid,
                    name: fields[3..].join(" "),
                    cpu_percent: cpu,
                    // ps reports the resident set in kibibytes.
                    memory_bytes: rss.saturating_mul(1024),
                });
            }
            _ => {}
        }
    }

    // `dedup_by` only removes *adjacent* repeats, so the list is grouped by
    // label before the hottest-first sort below reorders it.
    stats
        .temperatures
        .sort_by(|left, right| left.label.cmp(&right.label));
    // A sensor exposed through both `thermal_zone` and `hwmon` is reported
    // twice, identically. Showing it twice would imply two components running
    // that hot, so an exact repeat is dropped — while two genuinely distinct
    // sensors that share a chip name are kept, since their readings differ.
    stats.temperatures.dedup_by(|left, right| {
        left.label == right.label && (left.celsius - right.celsius).abs() < f64::EPSILON
    });

    // The hottest sensor first, so a summary can take the head of the list.
    stats.temperatures.sort_by(|left, right| {
        right
            .celsius
            .partial_cmp(&left.celsius)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

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
        let first = CpuSample {
            idle: 900,
            total: 1000,
        };
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
        assert!(
            (root.used_percent() - 37.16).abs() < 0.1,
            "got {}",
            root.used_percent()
        );
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
        for input in [
            "",
            "nonsense",
            "disk /only-two-fields 100",
            "mem NotANumber: x kB",
        ] {
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

    /// The extra sections, in the shapes procfs, sysfs, nvidia-smi and ps
    /// actually produce.
    const EXTRAS: &str = "\
temp acpitz 59000
temp k10temp 47125
temp nvme 0
temp coretemp 999000
fan fan1 2400
fan fan2 0
gpu NVIDIA GeForce RTX 3050 Laptop GPU, 12, 900, 4096, 46
proc 428936 8.8 496640 chrome
proc 10440 5.9 83558 gnome shell
proc bad line here
";

    #[test]
    fn temperatures_are_converted_from_millidegrees_and_sorted_hottest_first() {
        let stats = parse(EXTRAS);
        let labels: Vec<&str> = stats
            .temperatures
            .iter()
            .map(|reading| reading.label.as_str())
            .collect();
        assert_eq!(
            labels,
            ["acpitz", "k10temp"],
            "got {:#?}",
            stats.temperatures
        );
        assert_eq!(stats.temperatures[0].celsius, 59.0);
        assert_eq!(stats.peak_temperature().unwrap().label, "acpitz");
    }

    #[test]
    fn a_sensor_reading_zero_or_nonsense_is_dropped_rather_than_shown() {
        // An unplugged sensor reports 0, and a broken one reports 999 °C;
        // either would fill the panel with noise.
        let stats = parse(EXTRAS);
        assert!(stats.temperatures.iter().all(|r| r.label != "nvme"));
        assert!(stats.temperatures.iter().all(|r| r.label != "coretemp"));
    }

    #[test]
    fn a_fan_header_with_nothing_plugged_into_it_is_not_a_fan() {
        let stats = parse(EXTRAS);
        assert_eq!(stats.fans.len(), 1);
        assert_eq!(stats.fans[0].label, "fan1");
        assert_eq!(stats.fans[0].rpm, 2400);
    }

    #[test]
    fn a_gpu_name_containing_commas_worth_of_spaces_still_parses() {
        let stats = parse(EXTRAS);
        assert_eq!(stats.gpus.len(), 1);
        let gpu = &stats.gpus[0];
        assert_eq!(gpu.name, "NVIDIA GeForce RTX 3050 Laptop GPU");
        assert_eq!(gpu.utilization_percent, 12.0);
        // nvidia-smi reports mebibytes with --nounits.
        assert_eq!(gpu.memory_used_bytes, 900 * 1024 * 1024);
        assert_eq!(gpu.memory_total_bytes, 4096 * 1024 * 1024);
        assert_eq!(gpu.temperature_celsius, Some(46.0));
    }

    #[test]
    fn a_machine_with_no_nvidia_gpu_reports_none_rather_than_a_wrong_number() {
        assert!(parse(SAMPLE).gpus.is_empty());
    }

    #[test]
    fn processes_carry_their_pid_cpu_and_resident_memory() {
        let stats = parse(EXTRAS);
        assert_eq!(stats.processes.len(), 2, "the malformed line is skipped");
        assert_eq!(stats.processes[0].pid, 428936);
        assert_eq!(stats.processes[0].name, "chrome");
        assert_eq!(stats.processes[0].cpu_percent, 8.8);
        // ps reports the resident set in kibibytes.
        assert_eq!(stats.processes[0].memory_bytes, 496_640 * 1024);
        assert_eq!(
            stats.processes[1].name, "gnome shell",
            "a spaced comm survives"
        );
    }

    #[test]
    fn a_machine_missing_every_extra_still_parses_what_it_did_report() {
        // A container, a BSD-ish host, a box with no sensors: the point of the
        // parser being tolerant is that none of these produce nothing at all.
        let stats = parse(SAMPLE);
        assert!(!stats.host_name.is_empty());
        assert!(stats.temperatures.is_empty());
        assert!(stats.fans.is_empty());
        assert!(stats.processes.is_empty());
    }

    #[test]
    fn the_probe_script_asks_for_every_section_the_parser_reads() {
        for marker in ["thermal_zone", "fan", "nvidia-smi", "ps -eo"] {
            assert!(PROBE_SCRIPT.contains(marker), "the script lost {marker}");
        }
    }

    #[test]
    fn a_sensor_reported_by_both_thermal_zone_and_hwmon_appears_once() {
        // Two rows for one component would imply two things running that hot.
        let stats = parse("temp acpitz 59000\ntemp acpitz 59000\n");
        assert_eq!(stats.temperatures.len(), 1);
    }

    #[test]
    fn two_distinct_sensors_sharing_a_chip_name_both_survive() {
        // Two DIMM slots on one spd5118 chip are two real readings.
        let stats = parse("temp spd5118 46500\ntemp spd5118 44250\n");
        assert_eq!(stats.temperatures.len(), 2);
        assert_eq!(stats.temperatures[0].celsius, 46.5, "hottest first");
    }

    #[test]
    fn the_probe_script_asks_hwmon_for_its_channel_labels() {
        // Without them every channel of a multi-channel chip reports under the
        // chip's own name, and the dedup above cannot tell them apart.
        assert!(PROBE_SCRIPT.contains("_label"));
    }
}
