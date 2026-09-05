//! Host bandwidth signals for the Placement Manager (E21 / FreeToken-inspired).
//!
//! Values are either **measured** on the host (RAM read probe) or **estimated**
//! from public GPU/PCIe specs via `nvidia-smi` — never invented without a
//! documented source string in [`BandwidthSignal::detail`].

use serde::{Deserialize, Serialize};
use std::hint::black_box;
use std::process::Command;
use std::thread;
use std::time::Instant;

/// How a bandwidth number was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandwidthSource {
    /// Sequential read benchmark on this host (`probe_ram_read_bw`).
    Measured,
    /// Public memory-bandwidth spec for the detected GPU name.
    GpuSpec,
    /// Derived from `nvidia-smi` PCIe link gen × width (theoretical × efficiency).
    PcieEstimate,
    /// Machine reference profile (`HardwareProfile::reference_v1`, etc.).
    ReferenceDefault,
}

/// One bandwidth axis with provenance (bytes/s).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthSignal {
    pub bytes_per_sec: f64,
    pub source: BandwidthSource,
    /// Human-readable provenance (probe params, GPU name, PCIe gen×width, …).
    pub detail: String,
}

impl BandwidthSignal {
    pub fn measured(bytes_per_sec: f64, detail: impl Into<String>) -> Self {
        Self {
            bytes_per_sec,
            source: BandwidthSource::Measured,
            detail: detail.into(),
        }
    }

    pub fn gpu_spec(bytes_per_sec: f64, detail: impl Into<String>) -> Self {
        Self {
            bytes_per_sec,
            source: BandwidthSource::GpuSpec,
            detail: detail.into(),
        }
    }

    pub fn pcie_estimate(bytes_per_sec: f64, detail: impl Into<String>) -> Self {
        Self {
            bytes_per_sec,
            source: BandwidthSource::PcieEstimate,
            detail: detail.into(),
        }
    }

    pub fn reference_default(bytes_per_sec: f64, detail: impl Into<String>) -> Self {
        Self {
            bytes_per_sec,
            source: BandwidthSource::ReferenceDefault,
            detail: detail.into(),
        }
    }
}

/// Bandwidth inputs written to `var/run/hardware.json` and mapped into
/// [`crate::hardware::HardwareProfile`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthSignals {
    pub ram_mem_bw: BandwidthSignal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_mem_bw: Option<BandwidthSignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_to_device_bw: Option<BandwidthSignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_seq_bw: Option<BandwidthSignal>,
}

/// First-run probe: RAM measurement + optional NVIDIA estimates.
pub fn probe_host_bandwidth(cpu_only: bool) -> BandwidthSignals {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8);
    let ram = probe_ram_read_bw(threads);
    if cpu_only {
        return BandwidthSignals {
            ram_mem_bw: ram,
            gpu_mem_bw: None,
            host_to_device_bw: None,
            disk_seq_bw: Some(BandwidthSignal::reference_default(
                3e9,
                "cpu-only reference-v1 disk_seq_bw (not probed at first-run)",
            )),
        };
    }
    let (gpu_name, _vram_mib, pcie) = probe_nvidia_link();
    let gpu_bw = gpu_name
        .as_deref()
        .and_then(gpu_mem_bw_from_name)
        .map(|(bps, note)| {
            BandwidthSignal::gpu_spec(
                bps,
                format!("{}: {note}", gpu_name.as_deref().unwrap_or("gpu")),
            )
        });
    let h2d = pcie.map(|(gen, width, bps)| {
        BandwidthSignal::pcie_estimate(
            bps,
            format!("PCIe gen{gen} x{width} effective (nvidia-smi)"),
        )
    });
    BandwidthSignals {
        ram_mem_bw: ram,
        gpu_mem_bw: gpu_bw,
        host_to_device_bw: h2d,
        disk_seq_bw: Some(BandwidthSignal::reference_default(
            6e9,
            "reference-v1 disk_seq_bw (not probed at first-run)",
        )),
    }
}

/// Sequential RAM read bandwidth (bytes/s). Uses a 256 MiB buffer by default
/// so first-run stays fast; increase `size_bytes` for Gate P0 calibration.
pub fn probe_ram_read_bw(threads: usize) -> BandwidthSignal {
    const DEFAULT_SIZE: usize = 256 << 20; // 256 MiB
    probe_ram_read_bw_sized(threads, DEFAULT_SIZE)
}

pub fn probe_ram_read_bw_sized(threads: usize, size_bytes: usize) -> BandwidthSignal {
    let threads = threads.max(1);
    let words = size_bytes / 8;
    let buf = vec![0x5A5A_5A5A_5A5A_5A5Au64; words];
    // Warm-up (first touch / allocator).
    black_box(measure_read_bw(&buf, 1));
    let best = (0..3)
        .map(|_| measure_read_bw(&buf, threads))
        .fold(0.0_f64, f64::max);
    BandwidthSignal::measured(
        best,
        format!("host_probe read {size_bytes} B, {threads} thread(s), best-of-3"),
    )
}

fn measure_read_bw(buf: &[u64], threads: usize) -> f64 {
    let threads = threads.max(1);
    let chunk = buf.len() / threads;
    let start = Instant::now();
    thread::scope(|s| {
        for t in 0..threads {
            let slice = &buf[t * chunk..(t + 1) * chunk];
            s.spawn(move || {
                let mut acc = 0u64;
                for &v in slice {
                    acc = acc.wrapping_add(black_box(v));
                }
                black_box(acc);
            });
        }
    });
    let elapsed = start.elapsed().as_secs_f64().max(1e-9);
    (threads * chunk * 8) as f64 / elapsed
}

/// `(gpu_name, vram_mib, pcie_gen, pcie_width)` from nvidia-smi when available.
fn probe_nvidia_link() -> (Option<String>, u64, Option<(u32, u32, f64)>) {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,pcie.link.gen.current,pcie.link.width.current",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let Ok(out) = out else {
        return (None, 0, None);
    };
    if !out.status.success() {
        return (None, 0, None);
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let line = line.lines().next().unwrap_or("").trim();
    let parts: Vec<_> = line.split(',').map(|s| s.trim()).collect();
    if parts.len() < 4 {
        return (None, 0, None);
    }
    let name = parts[0].to_string();
    let vram = parts[1].parse::<u64>().unwrap_or(0);
    let gen = parts[2].parse::<u32>().ok().filter(|&g| g > 0);
    let width = parts[3].parse::<u32>().ok().filter(|&w| w > 0);
    let pcie = match (gen, width) {
        (Some(g), Some(w)) => Some((g, w, pcie_effective_bytes_per_sec(g, w))),
        _ => None,
    };
    (Some(name), vram, pcie)
}

/// Theoretical PCIe lane bandwidth (GB/s per lane, one direction) × lanes × efficiency.
fn pcie_effective_bytes_per_sec(gen: u32, width: u32) -> f64 {
    let per_lane_gbps: f64 = match gen {
        1 => 0.25,
        2 => 0.5,
        3 => 0.985,
        4 => 1.969,
        5 => 3.938,
        _ => 1.969, // assume gen4 if unknown
    };
    let theoretical = per_lane_gbps * 1e9 * width as f64;
    theoretical * 0.80 // protocol + DMA efficiency (documented estimate)
}

/// Public memory bandwidth (bytes/s) from GPU marketing / datasheet tables.
/// Returns `None` when the name is unknown — caller must not invent a value.
pub fn gpu_mem_bw_from_name(name: &str) -> Option<(f64, &'static str)> {
    let n = name.to_ascii_lowercase();
    // Substring match on normalized name → (bytes/s, note).
    const TABLE: &[(&str, f64, &str)] = &[
        ("rtx 4090", 1008e9, "GDDR6X 384-bit datasheet"),
        ("rtx 4080 super", 736e9, "GDDR6X 256-bit datasheet"),
        ("rtx 4080", 717e9, "GDDR6X 256-bit datasheet"),
        ("rtx 4070 ti super", 672e9, "GDDR6X 192-bit datasheet"),
        ("rtx 4070 ti", 504e9, "GDDR6X 192-bit datasheet"),
        ("rtx 4070 super", 504e9, "GDDR6X 192-bit datasheet"),
        ("rtx 4070", 504e9, "GDDR6X 192-bit datasheet"),
        ("rtx 4060 ti", 288e9, "GDDR6 128-bit datasheet"),
        ("rtx 4060", 272e9, "GDDR6 128-bit datasheet"),
        ("rtx 3090", 936e9, "GDDR6X 384-bit datasheet"),
        ("rtx 3080", 760e9, "GDDR6X 320-bit datasheet"),
        ("rtx 3070", 448e9, "GDDR6 256-bit datasheet"),
        ("rtx 3060", 360e9, "GDDR6 192-bit datasheet"),
        ("a100", 1935e9, "HBM2e 5120-bit datasheet"),
        ("h100", 3350e9, "HBM3 datasheet"),
        ("l40s", 864e9, "GDDR6 384-bit datasheet"),
        ("a6000", 768e9, "GDDR6 384-bit datasheet"),
        ("a5000", 768e9, "GDDR6 384-bit datasheet"),
        ("tesla t4", 320e9, "GDDR6 256-bit datasheet"),
    ];
    for (needle, bps, note) in TABLE {
        if n.contains(needle) {
            return Some((*bps, note));
        }
    }
    None
}

/// Map probed signals into scalar fields for [`crate::hardware::HardwareProfile`].
pub fn signals_to_profile_fields(signals: &BandwidthSignals) -> (f64, f64, f64, f64) {
    let ram = signals.ram_mem_bw.bytes_per_sec;
    let gpu = signals
        .gpu_mem_bw
        .as_ref()
        .map(|s| s.bytes_per_sec)
        .unwrap_or(0.0);
    let h2d = signals
        .host_to_device_bw
        .as_ref()
        .map(|s| s.bytes_per_sec)
        .unwrap_or(0.0);
    let disk = signals
        .disk_seq_bw
        .as_ref()
        .map(|s| s.bytes_per_sec)
        .unwrap_or(6e9);
    (ram, gpu, h2d, disk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_probe_returns_positive_measured() {
        let sig = probe_ram_read_bw(2);
        assert!(sig.bytes_per_sec > 1e9);
        assert_eq!(sig.source, BandwidthSource::Measured);
        assert!(sig.detail.contains("host_probe"));
    }

    #[test]
    fn gpu_lookup_known_card() {
        let (bps, _) = gpu_mem_bw_from_name("NVIDIA GeForce RTX 4080 SUPER").unwrap();
        assert!((bps - 736e9).abs() < 1e6);
    }

    #[test]
    fn gpu_lookup_unknown_is_none() {
        assert!(gpu_mem_bw_from_name("Totally Fake GPU 9000").is_none());
    }

    #[test]
    fn pcie_gen4_x16_in_reasonable_range() {
        let bps = pcie_effective_bytes_per_sec(4, 16);
        // ~25 GB/s effective for gen4 x16
        assert!(bps > 20e9 && bps < 30e9, "bps={bps}");
    }

    #[test]
    fn probe_host_cpu_only_has_no_gpu_fields() {
        let s = probe_host_bandwidth(true);
        assert!(s.gpu_mem_bw.is_none());
        assert!(s.host_to_device_bw.is_none());
        assert!(s.ram_mem_bw.bytes_per_sec > 0.0);
    }
}
