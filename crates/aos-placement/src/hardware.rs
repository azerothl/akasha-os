//! Profils matériels (entrée du Placement Manager).
//!
//! La machine de référence v1 est définie dans specs-techniques §13 :
//! CPU 8 cœurs, 32 GB RAM, GPU 8 GB VRAM, SSD NVMe 512 GB.
//!
//! Les bandes passantes et FLOPs sont des **ordres de grandeur à étalonner**
//! contre des mesures llama.cpp réelles (cf. `adr/0002-model-placement.md`).

use serde::{Deserialize, Serialize};

use crate::bandwidth::{BandwidthSignals, signals_to_profile_fields};

/// Un GPU physique (E9 / P5.2 — partition pipeline inter-GPU).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDevice {
    pub id: u32,
    pub name: String,
    /// VRAM totale de ce device (octets).
    pub vram_total: u64,
}

/// Caractéristiques d'une machine hôte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub name: String,
    pub has_gpu: bool,

    // Capacités brutes (octets)
    pub vram_total: u64,
    pub ram_total: u64,
    pub disk_total: u64,

    /// Réserves OS intouchables (§3.5.3 `reserve_os_budgets`).
    pub os_reserve_vram: u64,
    pub os_reserve_ram: u64,

    // Bandes passantes mémoire (octets/s, mesurées brutes)
    pub gpu_mem_bw: f64,
    pub ram_mem_bw: f64,
    /// NVMe séquentiel lu (streaming de couches).
    pub disk_seq_bw: f64,
    /// Transfert RAM→VRAM (PCIe ou équivalent) — migrations, prefetch.
    pub host_to_device_bw: f64,

    // FLOPs de prefill (FP16/INT8 effectifs selon backend)
    pub gpu_flops: f64,
    pub cpu_flops: f64,

    /// Inventaire multi-GPU (E9). Vide ⇒ pool unique via [`Self::vram_total`].
    #[serde(default)]
    pub gpus: Vec<GpuDevice>,
}

impl HardwareProfile {
    /// Nombre de GPU utilisables pour le placement (0 = CPU-only).
    pub fn n_gpus(&self) -> usize {
        if !self.has_gpu {
            return 0;
        }
        if self.gpus.is_empty() {
            if self.vram_total > 0 {
                1
            } else {
                0
            }
        } else {
            self.gpus.len()
        }
    }

    /// VRAM agrégée (somme des devices si présents, sinon `vram_total`).
    pub fn vram_total_effective(&self) -> u64 {
        if self.gpus.is_empty() {
            self.vram_total
        } else {
            self.gpus.iter().map(|g| g.vram_total).sum()
        }
    }

    /// Budget VRAM allouable au placement.
    pub fn vram_budget(&self) -> u64 {
        if self.has_gpu {
            self.vram_total_effective()
                .saturating_sub(self.os_reserve_vram)
        } else {
            0
        }
    }

    /// Budget RAM allouable au placement.
    pub fn ram_budget(&self) -> u64 {
        self.ram_total.saturating_sub(self.os_reserve_ram)
    }

    /// Budget disque allouable au cache de couches.
    pub fn disk_budget(&self) -> u64 {
        self.disk_total
    }

    /// Machine de référence v1 (specs-techniques §13).
    ///
    /// Hypothèses chiffrées (à étalonner) : GPU classe RTX 4060/4070
    /// (~300 GB/s, ~20 TFLOPS effectifs inférence), DDR5 double canal
    /// (~80 GB/s, ~0,8 TFLOP INT8 effectif), NVMe Gen4 (~6 GB/s en
    /// lecture séquentielle), PCIe Gen4 x16 (~25 GB/s utiles).
    pub fn reference_v1() -> Self {
        const GIB: u64 = 1 << 30;
        Self {
            name: "reference-v1".into(),
            has_gpu: true,
            vram_total: 8 * GIB,
            ram_total: 32 * GIB,
            disk_total: 512 * GIB,
            os_reserve_vram: GIB / 2, // affichage, compositor
            os_reserve_ram: 4 * GIB,  // OS + services
            gpu_mem_bw: 300e9,
            ram_mem_bw: 80e9,
            disk_seq_bw: 6e9,
            host_to_device_bw: 25e9,
            gpu_flops: 20e12,
            cpu_flops: 0.8e12,
            gpus: vec![],
        }
    }

    /// Variante sans GPU (profil `cpu-only` obligatoire).
    pub fn cpu_only_laptop() -> Self {
        const GIB: u64 = 1 << 30;
        Self {
            name: "cpu-only-laptop".into(),
            has_gpu: false,
            vram_total: 0,
            ram_total: 16 * GIB,
            disk_total: 256 * GIB,
            os_reserve_vram: 0,
            os_reserve_ram: 3 * GIB,
            gpu_mem_bw: 0.0,
            ram_mem_bw: 60e9,
            disk_seq_bw: 3e9,
            host_to_device_bw: 0.0,
            gpu_flops: 0.0,
            cpu_flops: 0.5e12,
            gpus: vec![],
        }
    }

    /// Grosse machine de dev (modèles lourds, multi-GPU ultérieur).
    pub fn workstation() -> Self {
        const GIB: u64 = 1 << 30;
        Self {
            name: "workstation".into(),
            has_gpu: true,
            vram_total: 24 * GIB,
            ram_total: 128 * GIB,
            disk_total: 2 * 1024 * GIB,
            os_reserve_vram: GIB,
            os_reserve_ram: 8 * GIB,
            gpu_mem_bw: 1000e9,
            ram_mem_bw: 100e9,
            disk_seq_bw: 7e9,
            host_to_device_bw: 25e9,
            gpu_flops: 60e12,
            cpu_flops: 1.5e12,
            gpus: vec![],
        }
    }

    /// Profil dual-GPU pour tests de partition pipeline (E9).
    pub fn dual_gpu_8g() -> Self {
        const GIB: u64 = 1 << 30;
        let mut hw = Self::reference_v1();
        hw.name = "dual-gpu-8g".into();
        hw.vram_total = 16 * GIB;
        hw.gpus = vec![
            GpuDevice {
                id: 0,
                name: "gpu0".into(),
                vram_total: 8 * GIB,
            },
            GpuDevice {
                id: 1,
                name: "gpu1".into(),
                vram_total: 8 * GIB,
            },
        ];
        hw
    }

    /// Build a host profile from first-run capacity + bandwidth signals (E21).
    #[allow(clippy::too_many_arguments)]
    pub fn from_host_caps(
        name: impl Into<String>,
        has_gpu: bool,
        vram_total: u64,
        ram_total: u64,
        disk_total: u64,
        os_reserve_vram: u64,
        os_reserve_ram: u64,
        gpus: Vec<GpuDevice>,
        bandwidth: &BandwidthSignals,
    ) -> Self {
        let (ram_mem_bw, gpu_mem_bw, host_to_device_bw, disk_seq_bw) =
            signals_to_profile_fields(bandwidth);
        let mut hw = Self {
            name: name.into(),
            has_gpu,
            vram_total,
            ram_total,
            disk_total,
            os_reserve_vram,
            os_reserve_ram,
            gpu_mem_bw,
            ram_mem_bw,
            disk_seq_bw,
            host_to_device_bw,
            // FLOPs stay reference until llama-bench calibration on this host.
            gpu_flops: if has_gpu { 20e12 } else { 0.0 },
            cpu_flops: 0.8e12,
            gpus,
        };
        if !has_gpu {
            hw.gpu_mem_bw = 0.0;
            hw.host_to_device_bw = 0.0;
        } else if hw.gpu_mem_bw <= 0.0 {
            // Unknown GPU name: keep reference_v1 default, not a fake measurement.
            hw.gpu_mem_bw = Self::reference_v1().gpu_mem_bw;
        }
        if hw.host_to_device_bw <= 0.0 && has_gpu {
            hw.host_to_device_bw = Self::reference_v1().host_to_device_bw;
        }
        hw
    }
}
