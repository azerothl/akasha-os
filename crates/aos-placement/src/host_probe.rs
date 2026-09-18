//! Live host hardware probe (Preview) — shared by aos-session and platform `system.hardware`.

use crate::{
    probe_host_bandwidth, BandwidthSignals, NpuCapabilities, ThermalSnapshot, WebGpuCapabilities,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HardwareTier {
    /// No usable NVIDIA GPU — CPU inference packs.
    Cpu,
    Low,
    Mid,
    High,
}

impl HardwareTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Low => "low",
            Self::Mid => "mid",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub gpu_name: String,
    pub vram_mib: u64,
    /// Live used VRAM when the probe could measure it (NVIDIA).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram_used_mib: Option<u64>,
    /// Live free VRAM when the probe could measure it (NVIDIA).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram_free_mib: Option<u64>,
    pub ram_mib: u64,
    pub disk_free_bytes: u64,
    pub driver_version: String,
    pub tier: HardwareTier,
    /// Bandwidth signals for Placement Manager (measured RAM + estimated GPU/PCIe).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<BandwidthSignals>,
    /// Optional independently managed Akasha adapter capabilities. These are
    /// preserved across session probes because the generic host probe cannot
    /// discover vendor runtimes by itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npu: Option<NpuCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webgpu: Option<WebGpuCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal: Option<ThermalSnapshot>,
    /// Unix ms when this snapshot was taken (agents can treat as freshness).
    #[serde(default)]
    pub probed_at_unix_ms: u64,
}

impl HardwareInfo {
    pub fn vram_bytes(&self) -> u64 {
        self.vram_mib.saturating_mul(1024 * 1024)
    }

    pub fn save(&self, home: &Path) -> Result<(), String> {
        let dir = home.join("var/run");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(dir.join("hardware.json"), raw).map_err(|e| e.to_string())
    }

    pub fn load(home: &Path) -> Option<Self> {
        let raw = fs::read_to_string(home.join("var/run/hardware.json")).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Compact JSON for agent tools (no bandwidth internals).
    pub fn agent_summary(&self) -> serde_json::Value {
        let disk_free_gib =
            (self.disk_free_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
        let mut v = serde_json::json!({
            "probed_at_unix_ms": self.probed_at_unix_ms,
            "fresh": true,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "gpu_name": self.gpu_name,
            "vram_mib": self.vram_mib,
            "ram_mib": self.ram_mib,
            "disk_free_gib": (disk_free_gib * 10.0).round() / 10.0,
            "driver_version": self.driver_version,
            "tier": self.tier.as_str(),
        });
        if let Some(used) = self.vram_used_mib {
            v["vram_used_mib"] = serde_json::json!(used);
        }
        if let Some(free) = self.vram_free_mib {
            v["vram_free_mib"] = serde_json::json!(free);
        }
        if let Some(t) = &self.thermal {
            v["thermal"] = serde_json::json!({
                "temperature_c": t.temperature_c,
                "power_w": t.power_w,
                "throttling": t.throttling,
            });
        }
        if let Some(npu) = &self.npu {
            v["npu"] = serde_json::json!({
                "name": npu.name,
                "memory_bytes": npu.memory_bytes,
            });
        }
        v
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Fresh host probe. Persists `var/run/hardware.json` and preserves prior NPU/WebGPU caps.
pub fn probe(home: &Path) -> HardwareInfo {
    let previous = HardwareInfo::load(home);
    let force_cpu = std::env::var_os("AOS_CPU_ONLY").is_some()
        || std::env::var("AOS_INFERENCE")
            .map(|v| v.eq_ignore_ascii_case("cpu"))
            .unwrap_or(false);
    let (gpu_name, vram_mib, vram_used_mib, vram_free_mib, driver_version) = if force_cpu {
        ("cpu-only".into(), 0, None, None, String::new())
    } else {
        #[cfg(target_os = "macos")]
        {
            let (name, vram, driver) = probe_apple_gpu();
            (name, vram, None, None, driver)
        }
        #[cfg(not(target_os = "macos"))]
        {
            probe_nvidia()
        }
    };
    let ram_mib = probe_ram_mib();
    let disk_free_bytes = probe_disk_free(home).unwrap_or(0);
    let tier = if force_cpu || vram_mib == 0 {
        HardwareTier::Cpu
    } else {
        tier_from_vram(vram_mib)
    };
    let cpu_only = tier == HardwareTier::Cpu;
    let bandwidth = Some(probe_host_bandwidth(cpu_only));
    let thermal = if force_cpu { None } else { probe_thermal() };
    let info = HardwareInfo {
        gpu_name,
        vram_mib,
        vram_used_mib,
        vram_free_mib,
        ram_mib,
        disk_free_bytes,
        driver_version,
        tier,
        bandwidth,
        npu: previous.as_ref().and_then(|info| info.npu.clone()),
        webgpu: previous.as_ref().and_then(|info| info.webgpu.clone()),
        thermal,
        probed_at_unix_ms: now_unix_ms(),
    };
    let _ = info.save(home);
    info
}

fn tier_from_vram(vram_mib: u64) -> HardwareTier {
    if vram_mib >= 20 * 1024 {
        HardwareTier::High
    } else if vram_mib >= 10 * 1024 {
        HardwareTier::Mid
    } else {
        HardwareTier::Low
    }
}

fn probe_nvidia() -> (String, u64, Option<u64>, Option<u64>, String) {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.used,memory.free,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let Ok(out) = out else {
        return ("unknown".into(), 0, None, None, String::new());
    };
    if !out.status.success() {
        return ("unknown".into(), 0, None, None, String::new());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or("").trim();
    let parts: Vec<_> = line.split(',').map(|s| s.trim()).collect();
    if parts.len() >= 5 {
        let name = parts[0].to_string();
        let vram = parts[1].parse::<u64>().unwrap_or(0);
        let used = parts[2].parse::<u64>().ok();
        let free = parts[3].parse::<u64>().ok();
        let driver = parts[4].to_string();
        (name, vram, used, free, driver)
    } else if parts.len() >= 3 {
        // Older fallback if used/free missing from query.
        let name = parts[0].to_string();
        let vram = parts[1].parse::<u64>().unwrap_or(0);
        let driver = parts[2].to_string();
        (name, vram, None, None, driver)
    } else {
        ("unknown".into(), 0, None, None, String::new())
    }
}

fn probe_thermal() -> Option<ThermalSnapshot> {
    #[cfg(not(target_os = "macos"))]
    {
        let output = Command::new("nvidia-smi")
            .args([
                "--query-gpu=temperature.gpu,power.draw,clocks.sm,clocks_throttle_reasons.active",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        parse_thermal_line(text.lines().next()?)
    }
    #[cfg(target_os = "macos")]
    {
        None
    }
}

pub fn parse_thermal_line(line: &str) -> Option<ThermalSnapshot> {
    let fields: Vec<_> = line.split(',').map(str::trim).collect();
    let temperature_c = fields.first()?.parse::<f32>().ok()?;
    let power_w = fields.get(1).and_then(|value| value.parse::<f32>().ok());
    // nvidia-smi `clocks_throttle_reasons.active` is a bit mask. Bit 0 (GPU Idle)
    // and bit 1 (applications clocks) are normal parked-clock states — treating
    // them as throttling forces Quiet → MemorySaver (0 VRAM layers) and makes
    // CUDA look slower than CPU. Only real slowdown bits demote placement.
    const GPU_IDLE: u64 = 0x1;
    const APP_CLOCKS: u64 = 0x2;
    const IGNORE: u64 = GPU_IDLE | APP_CLOCKS;
    let throttling = fields.get(3).is_some_and(|value| {
        let raw = value.trim();
        if raw.is_empty() {
            return false;
        }
        let mask = if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16).unwrap_or(0)
        } else {
            raw.parse::<u64>().unwrap_or(0)
        };
        mask & !IGNORE != 0
    });
    Some(ThermalSnapshot {
        temperature_c: Some(temperature_c),
        sustained_temperature_c: None,
        throttling,
        power_w,
    })
}

/// One NVIDIA card from `nvidia-smi --query-gpu`. `None` fields are `[N/A]`.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuLiveSample {
    pub index: u32,
    pub name: String,
    pub util_percent: Option<f32>,
    pub vram_used_mib: Option<u64>,
    pub vram_total_mib: Option<u64>,
    pub temp_c: Option<f32>,
    pub power_w: Option<f32>,
}

/// Cached ~1s so `model.metrics` does not spawn `nvidia-smi` on every bus call.
/// Failure and a missing binary both return an empty list (no invented zeros).
pub fn gpu_live_snapshot() -> Vec<GpuLiveSample> {
    const TTL: Duration = Duration::from_secs(1);
    static CACHE: Mutex<Option<(Instant, Vec<GpuLiveSample>)>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, gpus)) = guard.as_ref() {
        if at.elapsed() < TTL {
            return gpus.clone();
        }
    }
    let gpus = query_gpu_live();
    *guard = Some((Instant::now(), gpus.clone()));
    gpus
}

fn query_gpu_live() -> Vec<GpuLiveSample> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_gpu_live_csv(&String::from_utf8_lossy(&output.stdout))
}

/// Parse `nvidia-smi` CSV (`noheader,nounits`). Names may contain commas;
/// the last five columns are the counters.
pub fn parse_gpu_live_csv(text: &str) -> Vec<GpuLiveSample> {
    text.lines()
        .filter_map(|line| parse_gpu_live_line(line.trim()))
        .collect()
}

fn parse_gpu_live_line(line: &str) -> Option<GpuLiveSample> {
    if line.is_empty() {
        return None;
    }
    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
    if parts.len() < 7 {
        return None;
    }
    let index = parts[0].parse::<u32>().ok()?;
    let tail = &parts[parts.len() - 5..];
    let name = parts[1..parts.len() - 5].join(", ");
    if name.is_empty() {
        return None;
    }
    Some(GpuLiveSample {
        index,
        name,
        util_percent: parse_optional_f32(tail[0]),
        vram_used_mib: parse_optional_u64(tail[1]),
        vram_total_mib: parse_optional_u64(tail[2]),
        temp_c: parse_optional_f32(tail[3]),
        power_w: parse_optional_f32(tail[4]),
    })
}

fn parse_optional_f32(raw: &str) -> Option<f32> {
    let raw = raw.trim();
    if nvidia_missing(raw) {
        return None;
    }
    raw.parse().ok()
}

fn parse_optional_u64(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    if nvidia_missing(raw) {
        return None;
    }
    raw.parse::<f64>().ok().map(|v| v as u64)
}

fn nvidia_missing(raw: &str) -> bool {
    raw.is_empty()
        || raw.eq_ignore_ascii_case("n/a")
        || raw.eq_ignore_ascii_case("[n/a]")
        || raw.eq_ignore_ascii_case("[not supported]")
        || raw.eq_ignore_ascii_case("not supported")
}

/// Apple Silicon unified memory — no discrete VRAM; report chip GPU name + RAM budget heuristic.
#[cfg(target_os = "macos")]
fn probe_apple_gpu() -> (String, u64, String) {
    if std::env::consts::ARCH != "aarch64" {
        return ("unsupported-host".into(), 0, String::new());
    }
    let name = Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Apple Silicon".into());
    let ram_mib = probe_ram_mib();
    // Unified memory: treat ~70% of RAM as usable GPU budget for tiering.
    let vram_mib = ram_mib.saturating_mul(70).saturating_div(100);
    let driver = Command::new("sw_vers")
        .args(["-productVersion"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| format!("macOS {}", String::from_utf8_lossy(&o.stdout).trim()))
        .unwrap_or_else(|| "macOS".into());
    (name, vram_mib, driver)
}

fn probe_ram_mib() -> u64 {
    #[cfg(windows)]
    {
        let out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ])
            .output();
        if let Ok(out) = out {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Ok(bytes) = s.parse::<u64>() {
                return bytes / (1024 * 1024);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(raw) = fs::read_to_string("/proc/meminfo") {
            for line in raw.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    let kib: u64 = rest
                        .split_whitespace()
                        .next()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    return kib / 1024;
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("sysctl").args(["-n", "hw.memsize"]).output();
        if let Ok(out) = out {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if let Ok(bytes) = s.parse::<u64>() {
                    return bytes / (1024 * 1024);
                }
            }
        }
    }
    0
}

fn probe_disk_free(home: &Path) -> Result<u64, String> {
    let target = if home.exists() {
        home.to_path_buf()
    } else {
        std::path::PathBuf::from(".")
    };
    #[cfg(windows)]
    {
        let drive = target
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_else(|| "C:\\".into());
        let out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "(Get-PSDrive -Name '{d}').Free",
                    d = drive.trim_end_matches('\\').trim_end_matches(':')
                ),
            ])
            .output()
            .map_err(|e| e.to_string())?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return s.parse::<u64>().map_err(|e| e.to_string());
    }
    #[cfg(target_os = "linux")]
    {
        let out = Command::new("df")
            .args(["-B1", target.to_str().unwrap_or(".")])
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(line) = text.lines().nth(1) {
            let cols: Vec<_> = line.split_whitespace().collect();
            if cols.len() >= 4 {
                return cols[3].parse::<u64>().map_err(|e| e.to_string());
            }
        }
        return Err("df parse failed".into());
    }
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("df")
            .args(["-kP", target.to_str().unwrap_or(".")])
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(line) = text.lines().nth(1) {
            let cols: Vec<_> = line.split_whitespace().collect();
            if cols.len() >= 4 {
                let avail_kib = cols[3].parse::<u64>().map_err(|e| e.to_string())?;
                return Ok(avail_kib * 1024);
            }
        }
        return Err("df parse failed".into());
    }
    #[allow(unreachable_code)]
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_preserves_explicit_adapter_capabilities() {
        let home =
            std::env::temp_dir().join(format!("aos-placement-host-probe-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let original = HardwareInfo {
            gpu_name: "test".into(),
            vram_mib: 0,
            vram_used_mib: None,
            vram_free_mib: None,
            ram_mib: 1024,
            disk_free_bytes: 1 << 30,
            driver_version: String::new(),
            tier: HardwareTier::Cpu,
            bandwidth: None,
            npu: Some(NpuCapabilities {
                name: "akasha-test-npu".into(),
                memory_bytes: 1 << 30,
                int8: true,
                experimental: true,
                supported_operations: vec!["gemm".into()],
                supported_quantizations: vec![crate::Quantization::Q8],
                runtime_endpoint: Some("tcp://127.0.0.1:38471".into()),
            }),
            webgpu: None,
            thermal: None,
            probed_at_unix_ms: 1,
        };
        original.save(&home).unwrap();
        let probed = probe(&home);
        assert_eq!(probed.npu, original.npu);
        assert!(probed.probed_at_unix_ms >= 1);
        let summary = probed.agent_summary();
        assert_eq!(summary["fresh"], true);
        assert!(summary.get("tier").is_some());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn parses_nvidia_thermal_line_and_throttle_reason() {
        let cool = parse_thermal_line("52, 118.4, 210, 0x0000000000000000").unwrap();
        assert_eq!(cool.temperature_c, Some(52.0));
        assert_eq!(cool.power_w, Some(118.4));
        assert!(!cool.throttling);

        let idle = parse_thermal_line("40, 8.0, 210, 0x0000000000000001").unwrap();
        assert!(!idle.throttling);

        let hot = parse_thermal_line("87, 220.0, 300, 0x0000000000000020").unwrap();
        assert!(hot.throttling);
        assert!(parse_thermal_line("not-a-number, 1, 2, 0").is_none());
    }

    #[test]
    fn parses_gpu_live_csv_and_skips_bad_lines() {
        let text = "\
0, NVIDIA GeForce RTX 4080 SUPER, 12, 4200, 16376, 54, 85.2
not a gpu line
1, Tesla P100-PCIE-16GB, [N/A], 100, 16384, 41, N/A
";
        let gpus = parse_gpu_live_csv(text);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].index, 0);
        assert_eq!(gpus[0].name, "NVIDIA GeForce RTX 4080 SUPER");
        assert_eq!(gpus[0].util_percent, Some(12.0));
        assert_eq!(gpus[0].vram_used_mib, Some(4200));
        assert_eq!(gpus[0].vram_total_mib, Some(16376));
        assert_eq!(gpus[0].temp_c, Some(54.0));
        assert_eq!(gpus[0].power_w, Some(85.2));
        assert_eq!(gpus[1].util_percent, None);
        assert_eq!(gpus[1].power_w, None);
        assert_eq!(gpus[1].vram_total_mib, Some(16384));
        assert!(parse_gpu_live_csv("").is_empty());
        assert!(parse_gpu_live_csv("failed").is_empty());
    }

    #[test]
    fn gpu_live_name_may_contain_commas() {
        let gpus = parse_gpu_live_csv("0, NVIDIA, RTX, 3, 10, 20, 30, 40\n");
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].name, "NVIDIA, RTX");
        assert_eq!(gpus[0].util_percent, Some(3.0));
        assert_eq!(gpus[0].power_w, Some(40.0));
    }
}
