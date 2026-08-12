//! # aos-model — Model Subsystem v1 (P1.1, P1.2, P1.3)
//!
//! - **Registry** : réutilise `aos-registry` (catalogue YAML) + overlay dev
//!   (chemins réels des GGUF, métadonnées mesurées) ;
//! - **Placement Manager réel** (P1.2) : le plan calculé par `aos-placement`
//!   est piloté vers llama.cpp (`n_gpu_layers`, mmap, KV offload) ;
//! - **Inference Scheduler v1** (P1.3) : files par priorité par modèle,
//!   annulation coopérative (abort à la frontière de token), un contexte
//!   d'inférence par modèle (sérialisation sûre) ;
//! - **Metrics Exporter** : TTFT/tok/s réels, octets par tier (plan), RAM/CPU
//!   hôte (sysinfo).

pub mod config;
pub mod subsystem;

pub use config::ModeldConfig;
pub use subsystem::ModelSubsystem;
