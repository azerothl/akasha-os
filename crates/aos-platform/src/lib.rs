//! # aos-platform — plateforme P2
//!
//! - [`audit`] : journal append-only signé (§9.3, §12) ;
//! - [`storage`] : FS versionné COW logique, transactions, undo, caps (§6) ;
//! - [`memory`] : working + episodic vectoriel (§5) ;
//! - [`module_rt`] : runtime WASM sandboxé avec injection de caps (§7) ;
//! - [`subsystem`] : assemblage exposé sur le bus par `aos-platformd`.

pub mod audit;
pub mod canvas_raster;
pub mod catalogue;
pub mod chat_session;
pub mod confirm;
pub mod extract;
pub mod mem_sweep;
pub mod skill_pass;
pub mod feedback;
pub mod files_gen;
pub mod intents;
pub mod memory;
pub mod module_compile;
pub mod module_rt;
pub mod net;
pub mod net_services;
pub mod policy;
pub mod product_rag;
pub mod user_docs;
pub mod secrets;
pub mod skill;
pub mod storage;
pub mod subsystem;
pub mod supervisor;
pub mod trust;

pub use audit::AuditJournal;
pub use chat_session::ChatSessionStore;
pub use confirm::ConfirmManager;
pub use memory::MemoryStore;
pub use module_compile::ModuleAuthor;
pub use module_rt::ModuleRuntime;
pub use net::EgressControl;
pub use policy::PolicyEngine;
pub use secrets::{tpm_present, MasterBackend, SecretStore};
pub use skill::SkillStore;
pub use storage::StorageFs;
pub use subsystem::PlatformSubsystem;
pub use supervisor::Supervisor;
pub use trust::TrustManager;
