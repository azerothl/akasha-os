//! # aos-platform — plateforme P2
//!
//! - [`audit`] : journal append-only signé (§9.3, §12) ;
//! - [`storage`] : FS versionné COW logique, transactions, undo, caps (§6) ;
//! - [`memory`] : working + episodic vectoriel (§5) ;
//! - [`module_rt`] : runtime WASM sandboxé avec injection de caps (§7) ;
//! - [`subsystem`] : assemblage exposé sur le bus par `aos-platformd`.

pub mod audit;
pub mod confirm;
pub mod feedback;
pub mod memory;
pub mod module_rt;
pub mod net;
pub mod policy;
pub mod secrets;
pub mod storage;
pub mod subsystem;
pub mod supervisor;
pub mod trust;

pub use audit::AuditJournal;
pub use confirm::ConfirmManager;
pub use memory::MemoryStore;
pub use module_rt::ModuleRuntime;
pub use net::EgressControl;
pub use policy::PolicyEngine;
pub use secrets::SecretStore;
pub use storage::StorageFs;
pub use subsystem::PlatformSubsystem;
pub use supervisor::Supervisor;
pub use trust::TrustManager;
