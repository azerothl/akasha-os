//! Noyau de capacités (P4.2) : point d'application de confiance unique.
//!
//! Réalise la sémantique des « caps natives » en userspace (ADR 0001) :
//! toutes les vérifications passent par ce noyau, et une révocation y est
//! **immédiatement globale** — tout `check` ultérieur, quel que soit le
//! processus appelant, échoue sans délai.

use aos_caps::{CapId, CapStore, HolderId, Rights};
use std::collections::HashMap;

pub use aos_caps::object_matches;

/// Convertit une liste de noms de droits en bitmask.
pub fn rights_from(names: &[String]) -> Rights {
    let mut r = Rights::empty();
    for n in names {
        match n.as_str() {
            "read" => r |= Rights::READ,
            "write" => r |= Rights::WRITE,
            "execute" => r |= Rights::EXECUTE,
            "grant" => r |= Rights::GRANT,
            "revoke" => r |= Rights::REVOKE,
            _ => {}
        }
    }
    r
}

/// Convertit un bitmask en liste de noms de droits.
pub fn rights_to_names(r: Rights) -> Vec<String> {
    let mut v = Vec::new();
    if r.contains(Rights::READ) {
        v.push("read".into());
    }
    if r.contains(Rights::WRITE) {
        v.push("write".into());
    }
    if r.contains(Rights::EXECUTE) {
        v.push("execute".into());
    }
    if r.contains(Rights::GRANT) {
        v.push("grant".into());
    }
    if r.contains(Rights::REVOKE) {
        v.push("revoke".into());
    }
    v
}

/// Le noyau de capacités.
pub struct CapKernel {
    store: CapStore,
    /// nom de détenteur → HolderId.
    holders: HashMap<String, HolderId>,
    next_holder: u64,
}

impl Default for CapKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl CapKernel {
    pub fn new() -> Self {
        Self {
            store: CapStore::new(),
            holders: HashMap::new(),
            next_holder: 1,
        }
    }

    fn holder_id(&mut self, name: &str) -> HolderId {
        if let Some(id) = self.holders.get(name) {
            return *id;
        }
        let id = HolderId(self.next_holder);
        self.next_holder += 1;
        self.holders.insert(name.to_string(), id);
        id
    }

    fn holder_of(&self, name: &str) -> Option<HolderId> {
        self.holders.get(name).copied()
    }

    /// `cap.mint` — capacité racine (services de confiance uniquement).
    ///
    /// Invariant du noyau : toute capacité émise inclut le droit `REVOKE`,
    /// afin que le noyau (autorité émettrice) puisse la révoquer à tout
    /// moment — c'est la garantie de « révocation kernel immédiate » (P4.2).
    pub fn mint(&mut self, holder: &str, object: &str, rights: &[String]) -> u64 {
        let h = self.holder_id(holder);
        let r = rights_from(rights) | Rights::REVOKE;
        let id = self.store.mint(h, object.to_string(), r, None, None, 0);
        id.0
    }

    /// `cap.derive` — atténuation.
    pub fn derive(
        &mut self,
        holder: &str,
        parent: u64,
        rights: &[String],
    ) -> Result<u64, String> {
        let h = self
            .holder_of(holder)
            .ok_or_else(|| "détenteur inconnu".to_string())?;
        let r = rights_from(rights);
        self.store
            .derive(h, CapId(parent), r, None, None, 0)
            .map(|id| id.0)
            .map_err(|e| e.to_string())
    }

    /// `cap.grant` — transfert.
    pub fn grant(&mut self, holder: &str, cap: u64, to: &str) -> Result<u64, String> {
        let h = self
            .holder_of(holder)
            .ok_or_else(|| "détenteur inconnu".to_string())?;
        let to_h = self.holder_id(to);
        self.store
            .grant(h, CapId(cap), to_h)
            .map(|id| id.0)
            .map_err(|e| e.to_string())
    }

    /// `cap.revoke` — unitaire ou en arbre. Retourne le nombre de caps
    /// révoquées (1 pour unitaire, ≥1 pour arbre).
    pub fn revoke(&mut self, holder: &str, cap: u64, tree: bool) -> Result<u64, String> {
        let h = self
            .holder_of(holder)
            .ok_or_else(|| "détenteur inconnu".to_string())?;
        let before = self.store.live_count() as u64;
        if tree {
            self.store
                .revoke_tree(h, CapId(cap))
                .map_err(|e| e.to_string())?;
        } else {
            self.store
                .revoke(h, CapId(cap))
                .map_err(|e| e.to_string())?;
        }
        let after = self.store.live_count() as u64;
        Ok(before.saturating_sub(after).max(1))
    }

    /// `cap.check` — vérification d'autorisation (sans contrainte d'objet).
    pub fn check(&self, holder: &str, cap: u64, rights: &[String]) -> (bool, String) {
        self.check_object(holder, cap, rights, None)
    }

    /// `cap.check` avec objet visé : la cap doit porter des droits suffisants
    /// **et** viser `object` (égalité ou glob `/**`).
    pub fn check_object(
        &self,
        holder: &str,
        cap: u64,
        rights: &[String],
        object: Option<&str>,
    ) -> (bool, String) {
        let Some(h) = self.holder_of(holder) else {
            return (false, "détenteur inconnu".into());
        };
        let r = rights_from(rights);
        match self.store.authorize(h, CapId(cap), r) {
            Ok(grant) => {
                if let Some(obj) = object {
                    if !object_matches(&grant.object, obj) {
                        return (
                            false,
                            format!("objet {} n'autorise pas {}", grant.object, obj),
                        );
                    }
                }
                (true, "autorisé".into())
            }
            Err(e) => (false, e.to_string()),
        }
    }

    /// `cap.list` — capacités vivantes d'un détenteur.
    pub fn list(&self, holder: &str) -> Vec<(u64, String, Vec<String>)> {
        let Some(h) = self.holder_of(holder) else {
            return Vec::new();
        };
        let ids = self.store.snapshot_of(h);
        let mut out = Vec::new();
        for id in ids {
            if let Some(cap) = self.store.get(id) {
                out.push((id.0, cap.object.clone(), rights_to_names(cap.rights)));
            }
        }
        out.sort_by_key(|(id, _, _)| *id);
        out
    }

    pub fn live_count(&self) -> usize {
        self.store.live_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rd() -> Vec<String> {
        vec!["read".into()]
    }
    fn rdw() -> Vec<String> {
        vec!["read".into(), "write".into()]
    }

    #[test]
    fn mint_check_revoke_immediat() {
        let mut k = CapKernel::new();
        let cap = k.mint("agent:1", "fs:/notes/a.md", &rdw());
        let (ok, _) = k.check("agent:1", cap, &rd());
        assert!(ok);
        // Révocation au noyau → immédiatement invalide.
        k.revoke("agent:1", cap, false).unwrap();
        let (ok2, reason) = k.check("agent:1", cap, &rd());
        assert!(!ok2);
        assert!(reason.contains("révoquée") || reason.contains("inconnu"));
    }

    #[test]
    fn attenuation_stricte() {
        let mut k = CapKernel::new();
        let parent = k.mint("agent:1", "fs:/x", &rd());
        // Dériver WRITE depuis une cap READ-only → refus.
        let err = k.derive("agent:1", parent, &rdw());
        assert!(err.is_err());
    }

    #[test]
    fn revoke_tree_cascade() {
        let mut k = CapKernel::new();
        let root = k.mint("agent:1", "fs:/x", &rdw());
        let child = k.derive("agent:1", root, &rd()).unwrap();
        assert!(k.check("agent:1", child, &rd()).0);
        k.revoke("agent:1", root, true).unwrap();
        assert!(!k.check("agent:1", child, &rd()).0);
    }

    #[test]
    fn grant_transfert() {
        let mut k = CapKernel::new();
        let cap = k.mint("agent:1", "fs:/x", &rdw());
        // grant nécessite le droit GRANT : mint avec grant.
        let cap_g = k.mint("agent:1", "fs:/y", &{
            let mut v = rdw();
            v.push("grant".into());
            v
        });
        let granted = k.grant("agent:1", cap_g, "agent:2").unwrap();
        assert!(k.check("agent:2", granted, &rd()).0);
        // La cap sans GRANT ne peut pas être transférée.
        assert!(k.grant("agent:1", cap, "agent:2").is_err());
    }

    #[test]
    fn check_object_exact_et_glob() {
        let mut k = CapKernel::new();
        let cap = k.mint("agent:1", "fs:/p4/gate.md", &rd());
        assert!(k.check_object("agent:1", cap, &rd(), Some("fs:/p4/gate.md")).0);
        assert!(!k.check_object("agent:1", cap, &rd(), Some("fs:/other.md")).0);
        let glob = k.mint("agent:1", "fs:/p4/**", &rd());
        assert!(k.check_object("agent:1", glob, &rd(), Some("fs:/p4/gate.md")).0);
        assert!(!k.check_object("agent:1", glob, &rd(), Some("fs:/other.md")).0);
    }

    #[test]
    fn object_matches_glob() {
        assert!(object_matches("fs:/p4/gate.md", "fs:/p4/gate.md"));
        assert!(object_matches("fs:/p4/**", "fs:/p4/gate.md"));
        assert!(object_matches("fs:/p4/**", "fs:/p4"));
        assert!(!object_matches("fs:/p4/**", "fs:/other.md"));
    }
}
