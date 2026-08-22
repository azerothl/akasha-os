//! # aos-model — Model Subsystem v1 (P1.1, P1.2, P1.3)
//!
//! - **Registry** : réutilise `aos-registry` (catalogue YAML) + overlay dev
//!   (chemins réels des GGUF, métadonnées mesurées) ;
//! - **Placement Manager réel** (P1.2) : le plan calculé par `aos-placement`
//!   est piloté vers llama.cpp (`n_gpu_layers`, mmap, KV offload) ;
//! - **Inference Scheduler** (P1.3 / P5.1) : files par priorité, continuous
//!   batching (jusqu'à `n_seq_max` séquences / `llama_decode`), annulation
//!   coopérative par job ;
//! - **Metrics Exporter** : TTFT/tok/s réels, octets par tier (plan), RAM/CPU
//!   hôte (sysinfo).

pub mod backend;
pub mod config;
pub mod host_hardware;
pub mod media;
pub mod providers;
pub mod subsystem;

pub use backend::RemoteOpenAiBackend;
pub use config::ModeldConfig;
pub use subsystem::{resume_messages, ModelSubsystem};
