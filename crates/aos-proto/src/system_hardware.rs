//! Contrat IPC `system.hardware` — snapshot machine à jour pour agents.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod intents {
    pub const HARDWARE: &str = "system.hardware";
}

/// Demande un probe matériel frais (pas le cache de démarrage).
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SystemHardwareRequest {
    /// Si true, force aussi la réécriture de `var/run/hardware.json` (défaut true côté serveur).
    #[serde(default)]
    pub refresh: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SystemHardwareResponse {
    /// Résumé agent-friendly (GPU/VRAM libre, RAM, disque, tier, thermique…).
    pub summary: serde_json::Value,
}
