//! Local Illustration pack catalogue + marketplace integration hooks.
//!
//! Preview ships an **offline** embedded index under `/assets/illustration/`.
//! This is not a public store: catalogue load never uses the network, packs are
//! declarative-only (no scripts / executables), and [`marketplace_fetch_pack`]
//! is fail-closed without an explicit `network.fetch` grant (and still refused
//! in Preview even when that hook is later wired).

use crate::assets::{
    assert_asset_path, embedded_primitives_pack, AssetError, AssetPack,
};
#[cfg(test)]
use crate::assets::load_asset_pack_yaml;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Cap: outbound network fetch (opt-in; Illustration Studio does **not** ship with it).
pub const NETWORK_FETCH_CAP: &str = "network.fetch";

/// DeclUI: list installed / local catalogue packs (offline).
pub const ASSET_PACK_LIST_SERVICE: &str = "asset.pack.list";
/// DeclUI: describe one local pack (metadata + entry ids when asset kind).
pub const ASSET_PACK_DESCRIBE_SERVICE: &str = "asset.pack.describe";
/// DeclUI: marketplace / remote pack fetch hook — fail-closed in Preview.
pub const ASSET_MARKETPLACE_FETCH_SERVICE: &str = "asset.marketplace.fetch";

/// Catalogue format version (independent of per-pack `format_version`).
pub const PACK_CATALOGUE_FORMAT_VERSION: u32 = 1;

/// Embedded local catalogue YAML (offline; source of truth also on disk).
pub const EMBEDDED_PACK_CATALOGUE_YAML: &str =
    include_str!("../../../share/assets/illustration/catalogue.yaml");

/// Logical path of the local catalogue.
pub const PACK_CATALOGUE_PATH: &str = "/assets/illustration/catalogue.yaml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackKind {
    Asset,
    Style,
    Pose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogueNetworkPolicy {
    Deny,
    /// Reserved for a future opt-in signed remote index (never default in Preview).
    OptIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogueSource {
    LocalOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackCatalogue {
    pub format_version: u32,
    pub catalogue_id: String,
    #[serde(default = "default_source")]
    pub source: CatalogueSource,
    #[serde(default = "default_network")]
    pub network: CatalogueNetworkPolicy,
    #[serde(default)]
    pub packs: Vec<PackCatalogueEntry>,
}

fn default_source() -> CatalogueSource {
    CatalogueSource::LocalOnly
}

fn default_network() -> CatalogueNetworkPolicy {
    CatalogueNetworkPolicy::Deny
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackCatalogueEntry {
    pub id: String,
    pub kind: PackKind,
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub name_fr: Option<String>,
    /// Logical pack path under `/assets/illustration/**`.
    pub path: String,
    #[serde(default)]
    pub license: Option<String>,
    /// Spec §333: packs must stay declarative — scripts denied.
    #[serde(default)]
    pub allows_scripts: bool,
    /// Spec §326–328: network denied by default.
    #[serde(default)]
    pub allows_network: bool,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub summary_fr: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PackCatalogueError {
    #[error("unsupported catalogue format_version {0}")]
    UnsupportedVersion(u32),
    #[error("unknown pack `{0}`")]
    UnknownPack(String),
    #[error("pack `{0}` is not an asset pack")]
    NotAssetPack(String),
    #[error("pack `{0}` declares scripts (denied)")]
    ScriptsDenied(String),
    #[error("pack `{0}` declares network (denied in local catalogue)")]
    NetworkDeclared(String),
    #[error("catalogue network policy is not deny (Preview offline-only)")]
    CatalogueNetworkNotDenied,
    #[error("path outside illustration assets tree: {0}")]
    PathDenied(String),
    #[error("marketplace fetch disabled: {0}")]
    MarketplaceFetchDisabled(String),
    #[error("missing capability `{0}`")]
    MissingCapability(String),
    #[error("yaml: {0}")]
    Yaml(String),
    #[error("asset: {0}")]
    Asset(String),
}

impl From<AssetError> for PackCatalogueError {
    fn from(value: AssetError) -> Self {
        match value {
            AssetError::PathDenied(p) => Self::PathDenied(p),
            other => Self::Asset(other.to_string()),
        }
    }
}

impl PackCatalogue {
    pub fn validate(&self) -> Result<(), PackCatalogueError> {
        if self.format_version != PACK_CATALOGUE_FORMAT_VERSION {
            return Err(PackCatalogueError::UnsupportedVersion(self.format_version));
        }
        if self.network != CatalogueNetworkPolicy::Deny {
            return Err(PackCatalogueError::CatalogueNetworkNotDenied);
        }
        let mut ids = std::collections::HashSet::new();
        for entry in &self.packs {
            if !ids.insert(entry.id.clone()) {
                return Err(PackCatalogueError::Asset(format!(
                    "duplicate pack id `{}`",
                    entry.id
                )));
            }
            entry.validate()?;
        }
        Ok(())
    }

    pub fn get(&self, pack_id: &str) -> Option<&PackCatalogueEntry> {
        self.packs.iter().find(|p| p.id == pack_id)
    }
}

impl PackCatalogueEntry {
    pub fn validate(&self) -> Result<(), PackCatalogueError> {
        assert_asset_path(&self.path)?;
        if self.allows_scripts {
            return Err(PackCatalogueError::ScriptsDenied(self.id.clone()));
        }
        if self.allows_network {
            return Err(PackCatalogueError::NetworkDeclared(self.id.clone()));
        }
        Ok(())
    }

    pub fn display_name(&self, lang: &str) -> &str {
        if lang.eq_ignore_ascii_case("fr") {
            self.name_fr.as_deref().unwrap_or(self.name.as_str())
        } else {
            self.name.as_str()
        }
    }

    pub fn display_summary(&self, lang: &str) -> Option<&str> {
        if lang.eq_ignore_ascii_case("fr") {
            self.summary_fr
                .as_deref()
                .or(self.summary.as_deref())
        } else {
            self.summary.as_deref()
        }
    }
}

pub fn load_pack_catalogue_yaml(yaml: &str) -> Result<PackCatalogue, PackCatalogueError> {
    let catalogue: PackCatalogue =
        serde_yaml::from_str(yaml).map_err(|e| PackCatalogueError::Yaml(e.to_string()))?;
    catalogue.validate()?;
    Ok(catalogue)
}

pub fn embedded_pack_catalogue() -> Result<PackCatalogue, PackCatalogueError> {
    load_pack_catalogue_yaml(EMBEDDED_PACK_CATALOGUE_YAML)
}

/// List local / embedded packs (never touches the network).
pub fn list_local_packs() -> Result<Vec<PackCatalogueEntry>, PackCatalogueError> {
    Ok(embedded_pack_catalogue()?.packs)
}

/// Resolve an **asset** pack from the local catalogue into an [`AssetPack`].
///
/// Style / pose kinds are catalogue hooks for later tracks — not loaded here.
pub fn resolve_asset_pack(pack_id: &str) -> Result<AssetPack, PackCatalogueError> {
    let catalogue = embedded_pack_catalogue()?;
    let entry = catalogue
        .get(pack_id)
        .ok_or_else(|| PackCatalogueError::UnknownPack(pack_id.into()))?;
    match entry.kind {
        PackKind::Asset => {}
        PackKind::Style | PackKind::Pose => {
            return Err(PackCatalogueError::NotAssetPack(pack_id.into()));
        }
    }
    entry.validate()?;
    // Embedded primitives are the only bundled asset pack today.
    if entry.path == "/assets/illustration/primitives/pack.yaml"
        || entry.id == "illustration-primitives-v0"
    {
        return embedded_primitives_pack().map_err(PackCatalogueError::from);
    }
    Err(PackCatalogueError::Asset(format!(
        "pack `{pack_id}` path `{}` is not embedded in this Preview build",
        entry.path
    )))
}

/// Describe a local pack; for asset packs, include entry ids from the resolved YAML.
pub fn describe_local_pack(pack_id: &str) -> Result<PackDescribeResult, PackCatalogueError> {
    let catalogue = embedded_pack_catalogue()?;
    let entry = catalogue
        .get(pack_id)
        .ok_or_else(|| PackCatalogueError::UnknownPack(pack_id.into()))?
        .clone();
    let entry_ids = match entry.kind {
        PackKind::Asset => {
            let pack = resolve_asset_pack(pack_id)?;
            pack.entries.iter().map(|e| e.id().to_string()).collect()
        }
        PackKind::Style | PackKind::Pose => Vec::new(),
    };
    Ok(PackDescribeResult { entry, entry_ids })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PackDescribeResult {
    pub entry: PackCatalogueEntry,
    pub entry_ids: Vec<String>,
}

/// Marketplace / remote fetch integration point — **always fail-closed** in Preview.
///
/// Requires `network.fetch`. Even when granted, Preview refuses: no unauthorized
/// network, offline-first Illustration Studio (spec §326–328, project context).
pub fn marketplace_fetch_pack(
    pack_id: &str,
    granted_caps: &[String],
) -> Result<AssetPack, PackCatalogueError> {
    if !granted_caps.iter().any(|c| c == NETWORK_FETCH_CAP) {
        return Err(PackCatalogueError::MissingCapability(
            NETWORK_FETCH_CAP.into(),
        ));
    }
    let _ = pack_id;
    // Preview host constraint: illustration asset distribution is local-only.
    // A future opt-in signed index may soft-open this path under explicit policy.
    Err(PackCatalogueError::MarketplaceFetchDisabled(
        "Preview offline-only: remote illustration marketplace fetch is disabled \
         (local catalogue under /assets/illustration/ only; no unauthorized network)"
            .into(),
    ))
}

/// Human-readable one-line summary of installed packs (EN or FR).
pub fn format_pack_list_summary(lang: &str) -> Result<String, PackCatalogueError> {
    let packs = list_local_packs()?;
    if packs.is_empty() {
        return Ok(if lang.eq_ignore_ascii_case("fr") {
            "Aucun pack local installé.".into()
        } else {
            "No local packs installed.".into()
        });
    }
    let lines: Vec<String> = packs
        .iter()
        .map(|p| {
            let kind = match p.kind {
                PackKind::Asset => "asset",
                PackKind::Style => "style",
                PackKind::Pose => "pose",
            };
            let title = match (p.name_fr.as_deref(), lang.eq_ignore_ascii_case("fr")) {
                (Some(fr), true) => format!("{fr} / {}", p.name),
                (Some(fr), false) if fr != p.name => format!("{} / {fr}", p.name),
                _ => p.display_name(lang).to_string(),
            };
            format!("• {title} ({kind} {}) — {}", p.version, p.path)
        })
        .collect();
    Ok(lines.join("\n"))
}

/// Parse optional pack YAML string for tests / future FS-backed loads.
#[cfg(test)]
pub fn load_asset_pack_from_catalogue_path(
    catalogue: &PackCatalogue,
    pack_id: &str,
    pack_yaml: &str,
) -> Result<AssetPack, PackCatalogueError> {
    let entry = catalogue
        .get(pack_id)
        .ok_or_else(|| PackCatalogueError::UnknownPack(pack_id.into()))?;
    match entry.kind {
        PackKind::Asset => {}
        PackKind::Style | PackKind::Pose => {
            return Err(PackCatalogueError::NotAssetPack(pack_id.into()));
        }
    }
    entry.validate()?;
    load_asset_pack_yaml(pack_yaml).map_err(PackCatalogueError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{
        assert_asset_path, ASSET_ILLUSTRATION_READ_CAP, ILLUSTRATION_ASSETS_PREFIX,
    };

    #[test]
    fn embedded_catalogue_lists_primitives() {
        let cat = embedded_pack_catalogue().expect("catalogue");
        assert_eq!(cat.catalogue_id, "illustration-local-v0");
        assert_eq!(cat.network, CatalogueNetworkPolicy::Deny);
        assert_eq!(cat.source, CatalogueSource::LocalOnly);
        let p = cat.get("illustration-primitives-v0").expect("primitives");
        assert_eq!(p.kind, PackKind::Asset);
        assert!(!p.allows_scripts);
        assert!(!p.allows_network);
        assert!(p.path.starts_with(ILLUSTRATION_ASSETS_PREFIX));
        let _ = ASSET_ILLUSTRATION_READ_CAP;
    }

    #[test]
    fn resolve_primitives_via_catalogue() {
        let pack = resolve_asset_pack("illustration-primitives-v0").expect("pack");
        assert!(pack.get("humanoid.placeholder").is_some());
        assert!(pack.get("prop.box").is_some());
    }

    #[test]
    fn describe_includes_entry_ids() {
        let d = describe_local_pack("illustration-primitives-v0").expect("describe");
        assert!(d.entry_ids.iter().any(|id| id == "humanoid.slim"));
        assert_eq!(d.entry.id, "illustration-primitives-v0");
    }

    #[test]
    fn scripts_and_network_flags_fail_closed() {
        let bad = r#"
format_version: 1
catalogue_id: bad
network: deny
packs:
  - id: evil
    kind: asset
    version: "1"
    name: Evil
    path: /assets/illustration/evil/pack.yaml
    allows_scripts: true
"#;
        let err = load_pack_catalogue_yaml(bad).unwrap_err();
        assert!(matches!(err, PackCatalogueError::ScriptsDenied(_)));

        let net = r#"
format_version: 1
catalogue_id: bad
network: deny
packs:
  - id: netty
    kind: asset
    version: "1"
    name: Net
    path: /assets/illustration/netty/pack.yaml
    allows_network: true
"#;
        let err = load_pack_catalogue_yaml(net).unwrap_err();
        assert!(matches!(err, PackCatalogueError::NetworkDeclared(_)));
    }

    #[test]
    fn catalogue_opt_in_network_rejected() {
        let yaml = r#"
format_version: 1
catalogue_id: remote
network: opt_in
packs: []
"#;
        let err = load_pack_catalogue_yaml(yaml).unwrap_err();
        assert_eq!(err, PackCatalogueError::CatalogueNetworkNotDenied);
    }

    #[test]
    fn marketplace_fetch_requires_cap_then_still_disabled() {
        let err = marketplace_fetch_pack("anything", &[]).unwrap_err();
        assert!(matches!(
            err,
            PackCatalogueError::MissingCapability(ref c) if c == NETWORK_FETCH_CAP
        ));
        let err = marketplace_fetch_pack(
            "anything",
            &[NETWORK_FETCH_CAP.into(), ASSET_ILLUSTRATION_READ_CAP.into()],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            PackCatalogueError::MarketplaceFetchDisabled(_)
        ));
    }

    #[test]
    fn pack_list_summary_en_fr() {
        let en = format_pack_list_summary("en").expect("en");
        assert!(en.contains("Illustration primitives"));
        let fr = format_pack_list_summary("fr").expect("fr");
        assert!(fr.contains("Primitives Illustration"));
    }

    #[test]
    fn path_traversal_denied_in_entry() {
        let yaml = r#"
format_version: 1
catalogue_id: bad
network: deny
packs:
  - id: escape
    kind: asset
    version: "1"
    name: Escape
    path: /assets/illustration/../etc/passwd
"#;
        let err = load_pack_catalogue_yaml(yaml).unwrap_err();
        assert!(matches!(err, PackCatalogueError::PathDenied(_)));
    }

    #[test]
    fn catalogue_path_constant_is_under_prefix() {
        assert!(assert_asset_path(PACK_CATALOGUE_PATH).is_ok());
    }

    #[test]
    fn load_pack_yaml_via_catalogue_helper() {
        let cat = embedded_pack_catalogue().expect("catalogue");
        let yaml = crate::assets::EMBEDDED_PRIMITIVES_PACK_YAML;
        let pack = load_asset_pack_from_catalogue_path(&cat, "illustration-primitives-v0", yaml)
            .expect("load");
        assert_eq!(pack.pack_id, "illustration-primitives-v0");
    }
}
