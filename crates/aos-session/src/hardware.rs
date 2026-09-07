//! Détection matérielle Preview (NVIDIA + RAM + disque + bande passante E21).

use aos_placement::{
    probe_host_bandwidth, BandwidthSignals, NpuCapabilities, ThermalSnapshot, WebGpuCapabilities,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

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
}

pub fn probe(home: &Path) -> HardwareInfo {
    let previous = HardwareInfo::load(home);
    let force_cpu = std::env::var_os("AOS_CPU_ONLY").is_some()
        || std::env::var("AOS_INFERENCE")
            .map(|v| v.eq_ignore_ascii_case("cpu"))
            .unwrap_or(false);
    let (gpu_name, vram_mib, driver_version) = if force_cpu {
        ("cpu-only".into(), 0, String::new())
    } else {
        #[cfg(target_os = "macos")]
        {
            probe_apple_gpu()
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
    HardwareInfo {
        gpu_name,
        vram_mib,
        ram_mib,
        disk_free_bytes,
        driver_version,
        tier,
        bandwidth,
        npu: previous.as_ref().and_then(|info| info.npu.clone()),
        webgpu: previous.as_ref().and_then(|info| info.webgpu.clone()),
        thermal,
    }
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

fn probe_nvidia() -> (String, u64, String) {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let Ok(out) = out else {
        return ("unknown".into(), 0, String::new());
    };
    if !out.status.success() {
        return ("unknown".into(), 0, String::new());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or("").trim();
    // e.g. "NVIDIA GeForce RTX 4080 SUPER, 16376, 560.94"
    let parts: Vec<_> = line.split(',').map(|s| s.trim()).collect();
    if parts.len() >= 3 {
        let name = parts[0].to_string();
        let vram = parts[1].parse::<u64>().unwrap_or(0);
        let driver = parts[2].to_string();
        (name, vram, driver)
    } else {
        ("unknown".into(), 0, String::new())
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

fn parse_thermal_line(line: &str) -> Option<ThermalSnapshot> {
    let fields: Vec<_> = line.split(',').map(str::trim).collect();
    let temperature_c = fields.first()?.parse::<f32>().ok()?;
    let power_w = fields.get(1).and_then(|value| value.parse::<f32>().ok());
    let throttling = fields
        .get(3)
        .is_some_and(|value| !value.is_empty() && *value != "0x0000000000000000");
    Some(ThermalSnapshot {
        temperature_c: Some(temperature_c),
        sustained_temperature_c: None,
        throttling,
        power_w,
    })
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
            std::env::temp_dir().join(format!("aos-session-hardware-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let original = HardwareInfo {
            gpu_name: "test".into(),
            vram_mib: 0,
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
                supported_quantizations: vec![aos_placement::Quantization::Q8],
                runtime_endpoint: Some("tcp://127.0.0.1:38471".into()),
            }),
            webgpu: None,
            thermal: None,
        };
        original.save(&home).unwrap();
        let probed = probe(&home);
        assert_eq!(probed.npu, original.npu);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn parses_nvidia_thermal_line_and_throttle_reason() {
        let cool = parse_thermal_line("52, 118.4, 210, 0x0000000000000000").unwrap();
        assert_eq!(cool.temperature_c, Some(52.0));
        assert_eq!(cool.power_w, Some(118.4));
        assert!(!cool.throttling);

        let hot = parse_thermal_line("87, 220.0, 300, 0x0000000000000001").unwrap();
        assert!(hot.throttling);
        assert!(parse_thermal_line("not-a-number, 1, 2, 0").is_none());
    }
}
