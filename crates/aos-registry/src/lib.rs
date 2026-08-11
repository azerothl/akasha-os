//! # aos-registry — Catalogue de modèles YAML + backends simulés (P0.3)
//!
//! - [`ModelRegistry`] : charge le catalogue YAML (format aligné sur le
//!   Model Registry des specs-techniques §3.2) et produit les
//!   [`ModelDesc`] du Placement Manager ;
//! - [`FakeLocalBackend`] / [`FakeRemoteBackend`] : backends mockés qui
//!   simulent les temps de réponse via le [`CostModel`] (API unifiée §3.3).

mod backend;
mod registry;

pub use backend::{
    BackendError, FakeLocalBackend, FakeRemoteBackend, Health, InferRequest, SimBackend,
    SimulatedGeneration, TokenEvent,
};
pub use registry::{Catalog, CatalogEntry, ModelRegistry, RegistryError};
