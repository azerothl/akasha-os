//! Détection matérielle Preview (NVIDIA + RAM + disque).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HardwareTier {
    Low,
    Mid,
    High,
}

impl HardwareTier {
    pub fn as_str(self) -> &'static str {
        match self {
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
    let (gpu_name, vram_mib, driver_version) = probe_nvidia();
    let ram_mib = probe_ram_mib();
    let disk_free_bytes = probe_disk_free(home).unwrap_or(0);
    let tier = tier_from_vram(vram_mib);
    HardwareInfo {
        gpu_name,
        vram_mib,
        ram_mib,
        disk_free_bytes,
        driver_version,
        tier,
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
    #[allow(unreachable_code)]
    Ok(0)
}
