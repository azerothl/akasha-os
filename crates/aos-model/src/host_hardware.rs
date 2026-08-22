//! Load `var/run/hardware.json` written by aos-session first-run probe.

use aos_placement::{BandwidthSignals, HardwareProfile};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
struct HardwareJson {
    gpu_name: String,
    #[serde(default)]
    vram_mib: u64,
    #[serde(default)]
    ram_mib: u64,
    disk_free_bytes: u64,
    #[serde(default)]
    bandwidth: Option<BandwidthSignals>,
}

/// Build a [`HardwareProfile`] from session probe output when present.
pub fn hardware_profile_from_json(
    home: &Path,
    config_gpu: bool,
    vram_total_bytes: u64,
    ram_total_bytes: u64,
    os_reserve_vram: u64,
    os_reserve_ram: u64,
    n_gpus: usize,
    gpus: Vec<aos_placement::GpuDevice>,
) -> HardwareProfile {
    let path = home.join("var/run/hardware.json");
    let raw = std::fs::read_to_string(&path).ok();
    let parsed: Option<HardwareJson> = raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    let has_gpu = config_gpu && vram_total_bytes > 0;
    let disk_total = parsed
        .as_ref()
        .map(|h| h.disk_free_bytes.max(1 << 30))
        .unwrap_or(1 << 40);
    let name = parsed
        .as_ref()
        .map(|h| format!("host-{}", h.gpu_name))
        .unwrap_or_else(|| "host-p1".into());

    if let Some(bw) = parsed.as_ref().and_then(|h| h.bandwidth.clone()) {
        return HardwareProfile::from_host_caps(
            name,
            has_gpu,
            vram_total_bytes,
            ram_total_bytes,
            disk_total,
            os_reserve_vram,
            os_reserve_ram,
            gpus,
            &bw,
        );
    }

    // Legacy hardware.json without bandwidth block: reference defaults + RAM from probe file if any.
    let mut hw = if has_gpu && n_gpus > 1 {
        HardwareProfile::dual_gpu_8g()
    } else if has_gpu {
        HardwareProfile::reference_v1()
    } else {
        HardwareProfile::cpu_only_laptop()
    };
    hw.name = name;
    hw.vram_total = vram_total_bytes;
    hw.ram_total = ram_total_bytes;
    hw.disk_total = disk_total;
    hw.os_reserve_vram = os_reserve_vram;
    hw.os_reserve_ram = os_reserve_ram;
    hw.has_gpu = has_gpu;
    hw.gpus = gpus;
    hw
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn loads_bandwidth_from_hardware_json() {
        let base = std::env::temp_dir().join("aos-host-hw-test");
        let run = base.join("var/run");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&run).unwrap();
        let json = r#"{
  "gpu_name": "NVIDIA GeForce RTX 4080 SUPER",
  "vram_mib": 16376,
  "ram_mib": 65536,
  "disk_free_bytes": 500000000000,
  "driver_version": "560.94",
  "tier": "high",
  "bandwidth": {
    "ram_mem_bw": {
      "bytes_per_sec": 45000000000.0,
      "source": "measured",
      "detail": "test probe"
    },
    "gpu_mem_bw": {
      "bytes_per_sec": 736000000000.0,
      "source": "gpu_spec",
      "detail": "RTX 4080 SUPER datasheet"
    },
    "host_to_device_bw": {
      "bytes_per_sec": 25000000000.0,
      "source": "pcie_estimate",
      "detail": "PCIe gen4 x16"
    }
  }
}"#;
        fs::write(run.join("hardware.json"), json).unwrap();
        let hw = hardware_profile_from_json(
            &base,
            true,
            16 << 30,
            64 << 30,
            1 << 30,
            4 << 30,
            1,
            vec![],
        );
        assert!(hw.has_gpu);
        assert!((hw.ram_mem_bw - 45e9).abs() < 1e8);
        assert!((hw.gpu_mem_bw - 736e9).abs() < 1e9);
        assert!((hw.host_to_device_bw - 25e9).abs() < 1e9);
        assert_eq!(hw.name, "host-NVIDIA GeForce RTX 4080 SUPER");
        let _ = fs::remove_dir_all(&base);
    }
}
