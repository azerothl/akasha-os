//! Détection matérielle Preview — re-export du probe partagé (aos-placement).

pub use aos_placement::host_probe::{probe, HardwareInfo, HardwareTier};

#[cfg(test)]
mod tests {
    use super::*;
    use aos_placement::host_probe::parse_thermal_line;
    use aos_placement::{NpuCapabilities, Quantization};
    use std::fs;

    #[test]
    fn probe_preserves_explicit_adapter_capabilities() {
        let home =
            std::env::temp_dir().join(format!("aos-session-hardware-{}", std::process::id()));
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
                supported_quantizations: vec![Quantization::Q8],
                runtime_endpoint: Some("tcp://127.0.0.1:38471".into()),
            }),
            webgpu: None,
            thermal: None,
            probed_at_unix_ms: 1,
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

        let idle = parse_thermal_line("40, 8.0, 210, 0x0000000000000001").unwrap();
        assert!(!idle.throttling);

        let hot = parse_thermal_line("87, 220.0, 300, 0x0000000000000020").unwrap();
        assert!(hot.throttling);
        assert!(parse_thermal_line("not-a-number, 1, 2, 0").is_none());
    }
}
