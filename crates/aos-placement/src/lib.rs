//! # aos-placement — Simulateur du Placement Manager (P0.1)
//!
//! Implémente, en Rust standalone, l'algorithme de placement RAM/GPU/disque
//! de `specs-techniques.md` §3.5 :
//!
//! - [`PlacementManager`] : algorithme de placement initial (§3.5.3),
//!   profils `latency` / `balanced` / `memory-saver` / `cpu-only` (§3.5.6),
//!   repli automatique avec suggestion (§16, F-PLC-09) ;
//! - [`CostModel`] : estimation tok/s / TTFT paramétrique, étalonnable sur
//!   mesures llama.cpp (Gate P0) ;
//! - [`PlacementSim`] : état runtime multi-modèles — éviction fair/priority
//!   (F-PLC-06), échelle de pression (§3.5.5), re-profilage à chaud.
//!
//! Les hypothèses du modèle de coût sont documentées dans
//! `adr/0002-model-placement.md`.

pub mod adaptive;
pub mod adapter_rpc;
pub mod adapters;
pub mod bandwidth;
pub mod benchmark;
pub mod cost;
pub mod discovery;
pub mod distributed;
pub mod hardware;
pub mod gguf;
pub mod manager;
pub mod model;
pub mod layer_rpc;
pub mod layer_pipeline;
pub mod low_bit;
pub mod model_io;
pub mod plan;
pub mod sim;
pub mod tensor_wire;

pub use adaptive::{
    AdaptivePlanner, BackendDescriptor, BackendKind, BackendRegistry, InferencePlan,
    InferencePlanDiagnostic, PlannerOptions, Quantization, SpeculativeStrategy, ThermalAction,
    ThermalController, ThermalPolicy, WorkloadKind,
};
pub use adapters::{
    probe as probe_adapter, validate_handshake, AdapterHandshake, AdapterState, AdapterStatus,
    ADAPTER_PROTOCOL_VERSION,
};
pub use adapter_rpc::{
    AdapterExecutionPhase, AdapterRpcClient, AdapterRpcMessage, ADAPTER_RPC_MAX_FRAME_BYTES,
    ADAPTER_RPC_MAX_TENSOR_BYTES,
};
pub use bandwidth::{
    probe_host_bandwidth, probe_ram_read_bw, BandwidthSignal, BandwidthSignals, BandwidthSource,
};
pub use benchmark::{
    extended_scenarios, reference_scenarios, run_extended_matrix, run_layer_pipeline_benchmark,
    run_reference_matrix, run_scenario, BenchmarkResult, BenchmarkScenario,
    LayerPipelineBenchmarkResult,
};
pub use cost::{Bound, CostModel, Estimate};
pub use discovery::{
    LanDiscoveryAdvertisement, LanDiscoverySocket, LAN_DISCOVERY_MAGIC, LAN_DISCOVERY_PORT,
    LAN_DISCOVERY_PROTOCOL_VERSION,
};
pub use distributed::{
    DistributedWork, LanActivationAssembly, LanChatMessage, LanCluster, LanJobState, LanNode,
    LanPairingRegistry, LanRecovery, LanSecureChannel, LanSecureFrame, LanSessionKey,
    LanShardAssignment, LanShardManifest, LanTcpListener, LanTcpTransport, LanWeightRange,
    LanWorkMessage, LanWorkPlan, LanWorkerJob, LanWorkerJobState, LanWorkerRegistry, NodeTrust,
};
pub use hardware::{
    CpuIsa, CpuTopology, GpuBackend, GpuDevice, HardwareProfile, NpuCapabilities, ThermalSnapshot,
    WebGpuCapabilities,
};
pub use gguf::{GgufMetadataValue, GgufModel, GgufTensorInfo, GgufTensorType};
pub use manager::{Budgets, PlacementError, PlacementManager};
pub use layer_rpc::{
    LayerRpcExecutor, LayerRpcReceiver, LayerRpcRequest, LayerRpcResult, LinearLayerExecutor,
    TransformerBlockExecutor, LAYER_RPC_PAGE_BYTES, LAYER_RPC_PROTOCOL_VERSION,
};
pub use layer_pipeline::{
    partition_layer_stages, LayerPipelineExecutor, LayerPipelineMetrics, LayerPipelinePlan,
    LayerStage,
};
pub use low_bit::{KernelPhase, LowBitKernel, LowBitKernelRegistry, LowBitLayout};
pub use model_io::CpuGgufModelIo;
pub use model::{KvCacheType, ModelDesc, PrivacyClass, QuantizationMetadata};
pub use plan::{PlacementPlan, PlacementProfile, Priority, Shard, ShardKind, Tier};
pub use sim::{PlacedModel, PlacementSim, PressureReport, ReprofileReport, RunState, SimEvent};
pub use tensor_wire::{CpuKvCache, CpuLayerExecutor, CpuTransformerBlock, F32Tensor, TensorDType};

/// Modèles de test partagés entre modules.
#[cfg(test)]
pub(crate) mod testutil {
    use crate::model::{ModelDesc, PrivacyClass};

    pub const GIB: u64 = 1 << 30;

    /// 32B Q6 ≈ 26 GiB, 80 couches — exemple de l'annexe A des specs.
    pub fn model_32b() -> ModelDesc {
        ModelDesc {
            id: "local:llama-q6-32b".into(),
            name: "Llama 32B Q6".into(),
            n_layers: 80,
            n_params: 32e9,
            weights_bytes: 26 * GIB,
            embed_bytes: 800_000_000,
            kv_bytes_per_token: 400_000,
            context_length: 131072,
            supports_layer_offload: true,
            privacy_class: PrivacyClass::Local,
            quantization: Default::default(),
            backends_compatible: vec![],
        }
    }

    /// 3B Q4 ≈ 2 GiB, 28 couches — modèle embarqué (§3.4).
    pub fn model_3b() -> ModelDesc {
        ModelDesc {
            id: "local:embedded-instruct".into(),
            name: "Embedded Instruct 3B Q4".into(),
            n_layers: 28,
            n_params: 3e9,
            weights_bytes: 2 * GIB,
            embed_bytes: 200_000_000,
            kv_bytes_per_token: 120_000,
            context_length: 8192,
            supports_layer_offload: true,
            privacy_class: PrivacyClass::Local,
            quantization: Default::default(),
            backends_compatible: vec![],
        }
    }

    /// SD 1.5 ≈ 4 GiB — pack image E16 (un shard MediaWeights).
    pub fn model_sd15() -> ModelDesc {
        ModelDesc {
            id: "local:sd-v1-5".into(),
            name: "Stable Diffusion 1.5".into(),
            n_layers: 0,
            n_params: 8.6e8,
            weights_bytes: 4 * GIB,
            embed_bytes: 0,
            kv_bytes_per_token: 0,
            context_length: 0,
            supports_layer_offload: false,
            privacy_class: PrivacyClass::Local,
            quantization: Default::default(),
            backends_compatible: vec![],
        }
    }

    /// Voix Piper ~64 MiB — TTS CPU, pas de VRAM.
    pub fn model_piper() -> ModelDesc {
        ModelDesc {
            id: "local:piper-en-us".into(),
            name: "Piper en_US".into(),
            n_layers: 0,
            n_params: 1.0e7,
            weights_bytes: 64 * 1024 * 1024,
            embed_bytes: 0,
            kv_bytes_per_token: 0,
            context_length: 0,
            supports_layer_offload: false,
            privacy_class: PrivacyClass::Local,
            quantization: Default::default(),
            backends_compatible: vec![],
        }
    }

    /// 70B Q4 ≈ 40 GiB, 80 couches — dépasse la RAM de la machine de référence.
    pub fn model_70b() -> ModelDesc {
        ModelDesc {
            id: "local:llama-q4-70b".into(),
            name: "Llama 70B Q4".into(),
            n_layers: 80,
            n_params: 70e9,
            weights_bytes: 40 * GIB,
            embed_bytes: 1_200_000_000,
            kv_bytes_per_token: 640_000,
            context_length: 131072,
            supports_layer_offload: true,
            privacy_class: PrivacyClass::Local,
            quantization: Default::default(),
            backends_compatible: vec![],
        }
    }
}
