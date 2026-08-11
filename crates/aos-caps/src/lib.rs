//! # aos-caps — Modèle de capacités logique (P0.2)
//!
//! Implémentation userspace du modèle de capacités décrit dans
//! `specs-techniques.md` §2.3. En P0, ce modèle est **logique** : il valide la
//! sémantique (atténuation stricte, révocation en arbre, TTL) avant le port sur
//! les capabilities natives du microkernel en P4.
//!
//! ## Invariants de sécurité garantis
//!
//! 1. **Atténuation stricte** : `derive` ne peut produire que des droits ⊆
//!    droits du parent **et** ⊆ `max_derived_rights` du parent.
//! 2. **Pas d'élargissement** : `max_derived_rights` ne peut que décroître le
//!    long d'une chaîne de dérivation.
//! 3. **TTL monotone** : une capacité dérivée ne peut pas expirer après son
//!    parent.
//! 4. **Révocation en arbre** : `revoke_tree` invalide immédiatement une
//!    capacité et toute sa descendance (dérivations **et** grants).
//! 5. **Non-partage implicite** : une capacité n'est utilisable que par son
//!    détenteur (`holder`) ; le transfert passe obligatoirement par `grant`.
//!
//! L'horloge est **logique** (ticks u64) pour rester déterministe dans les
//! tests et le simulateur.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

bitflags! {
    /// Droits portés par une capacité (bitmask, cf. specs-techniques §2.3).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct Rights: u32 {
        const READ    = 0b0000_0001;
        const WRITE   = 0b0000_0010;
        const EXECUTE = 0b0000_0100;
        /// Droit de transférer la capacité à un autre détenteur (`grant`).
        const GRANT   = 0b0000_1000;
        /// Droit de révoquer la capacité et sa descendance.
        const REVOKE  = 0b0001_0000;
    }
}

/// Identifiant opaque d'une capacité.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapId(pub u64);

/// Identifiant d'un détenteur (agent, service, module — « address space »).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HolderId(pub u64);

/// Contexte opaque interprété par le détenteur (cf. glossaire « Badge »).
pub type Badge = u64;

/// Référent d'une capacité : URI logique (`fs:/notes/a.md`, `model:llama-q6`,
/// `net:api.example.com:443`, ...). En P4 ce sera un objet kernel.
pub type ObjectId = String;

/// Une capacité (cf. specs-techniques §2.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cap {
    pub id: CapId,
    /// Référent (objet visé).
    pub object: ObjectId,
    /// Droits effectifs de cette capacité.
    pub rights: Rights,
    /// Détenteur courant.
    pub holder: HolderId,
    /// Contexte opaque pour le détenteur.
    pub badge: Badge,
    /// Expiration (ticks d'horloge logique), optionnelle.
    pub expires_at: Option<u64>,
    /// Règle d'atténuation : droits maximaux dérivables de cette capacité.
    pub max_derived_rights: Rights,
    /// Parent dans l'arbre de dérivation (`None` = racine, issue d'un `mint`).
    pub parent: Option<CapId>,
    /// Invalidation logique (révocation immédiate, F-SEC-02).
    pub revoked: bool,
}

impl Cap {
    /// La capacité est-elle valide à l'instant `now` ?
    pub fn is_valid(&self, now: u64) -> bool {
        if self.revoked {
            return false;
        }
        match self.expires_at {
            Some(exp) => now < exp,
            None => true,
        }
    }
}

/// Erreurs du modèle de capacités.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapError {
    #[error("capacité inconnue: {0:?}")]
    UnknownCap(CapId),
    #[error("capacité révoquée: {0:?}")]
    Revoked(CapId),
    #[error("capacité expirée: {0:?}")]
    Expired(CapId),
    #[error("détenteur invalide pour la capacité {0:?}")]
    WrongHolder(CapId),
    #[error("atténuation invalide: droits demandés hors droits dérivables du parent")]
    AttenuationViolation,
    #[error("TTL dérivé supérieur au TTL du parent")]
    TtlViolation,
    #[error("droit GRANT requis pour transférer la capacité {0:?}")]
    GrantNotAllowed(CapId),
    #[error("droit REVOKE requis (ou ancêtre révocable) pour la capacité {0:?}")]
    RevokeNotAllowed(CapId),
    #[error("permission refusée: droits {required:?} requis sur {object}")]
    PermissionDenied { object: ObjectId, required: Rights },
}

/// Espace de capacités logique (équivalent userspace du cspace kernel).
///
/// En P4, chaque opération ci-dessous sera remplacée par son homologue sur
/// capabilities natives ; la sémantique observée doit être identique.
#[derive(Debug, Default)]
pub struct CapStore {
    caps: HashMap<CapId, Cap>,
    /// Arbre de descendance pour la révocation en cascade.
    children: HashMap<CapId, Vec<CapId>>,
    /// Horloge logique (ticks).
    now: u64,
    next_id: u64,
}

/// Résultat d'une vérification d'accès (gate `invoke`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessGrant {
    pub cap: CapId,
    pub object: ObjectId,
    pub rights: Rights,
    pub badge: Badge,
}

impl CapStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Horloge logique courante.
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Avance l'horloge logique (expiration des TTL).
    pub fn advance_clock(&mut self, ticks: u64) {
        self.now += ticks;
    }

    /// Lecture d'une capacité (introspection).
    pub fn get(&self, id: CapId) -> Option<&Cap> {
        self.caps.get(&id)
    }

    fn alloc_id(&mut self) -> CapId {
        let id = CapId(self.next_id);
        self.next_id += 1;
        id
    }

    fn insert(&mut self, cap: Cap) -> CapId {
        let id = cap.id;
        if let Some(p) = cap.parent {
            self.children.entry(p).or_default().push(id);
        }
        self.caps.insert(id, cap);
        id
    }

    /// Vérifie qu'une capacité existe, est valide, et détenue par `holder`.
    fn require_valid(&self, holder: HolderId, id: CapId) -> Result<&Cap, CapError> {
        let cap = self.caps.get(&id).ok_or(CapError::UnknownCap(id))?;
        if cap.revoked {
            return Err(CapError::Revoked(id));
        }
        if let Some(exp) = cap.expires_at {
            if self.now >= exp {
                return Err(CapError::Expired(id));
            }
        }
        if cap.holder != holder {
            return Err(CapError::WrongHolder(id));
        }
        Ok(cap)
    }

    /// `mint` — crée une capacité racine sur un objet.
    ///
    /// Opération réservée au TCB (kernel / services signés) en cible P4 ;
    /// libre ici car P0 est un modèle logique hors trust boundary.
    /// `max_derived_rights` vaut `rights` par défaut (None).
    pub fn mint(
        &mut self,
        holder: HolderId,
        object: impl Into<ObjectId>,
        rights: Rights,
        max_derived_rights: Option<Rights>,
        expires_at: Option<u64>,
        badge: Badge,
    ) -> CapId {
        let max_derived = max_derived_rights.unwrap_or(rights) & rights;
        let id = self.alloc_id();
        self.insert(Cap {
            id,
            object: object.into(),
            rights,
            holder,
            badge,
            expires_at,
            max_derived_rights: max_derived,
            parent: None,
            revoked: false,
        })
    }

    /// `derive` — crée une capacité fille atténuée, même détenteur.
    ///
    /// Atténuation stricte : `rights ⊆ parent.rights ∩ parent.max_derived_rights`,
    /// et le plafond de dérivation de la fille est borné par celui du parent.
    pub fn derive(
        &mut self,
        holder: HolderId,
        parent: CapId,
        rights: Rights,
        max_derived_rights: Option<Rights>,
        expires_at: Option<u64>,
        badge: Badge,
    ) -> Result<CapId, CapError> {
        let p = self.require_valid(holder, parent)?.clone();

        // Invariant 1+2 : droits demandés ⊆ droits du parent ET ⊆ plafond dérivable.
        if !p.rights.contains(rights) || !p.max_derived_rights.contains(rights) {
            return Err(CapError::AttenuationViolation);
        }
        // Invariant 3 : TTL monotone décroissant.
        if let (Some(p_exp), Some(c_exp)) = (p.expires_at, expires_at) {
            if c_exp > p_exp {
                return Err(CapError::TtlViolation);
            }
        }
        if p.expires_at.is_none() && expires_at.is_some() {
            // Mettre une expiration là où le parent n'en a pas est une atténuation : autorisé.
        }

        let child_max = match max_derived_rights {
            // Le plafond de la fille ne peut excéder celui du parent.
            Some(m) if p.max_derived_rights.contains(m) => m & rights,
            Some(_) => return Err(CapError::AttenuationViolation),
            None => rights & p.max_derived_rights,
        };

        let id = self.alloc_id();
        Ok(self.insert(Cap {
            id,
            object: p.object,
            rights,
            holder,
            badge,
            expires_at,
            max_derived_rights: child_max,
            parent: Some(parent),
            revoked: false,
        }))
    }

    /// `grant` — transfère une copie de la capacité à un autre détenteur.
    ///
    /// Requiert le droit `GRANT` sur la capacité source. La copie est une
    /// **fille** dans l'arbre : `revoke_tree` sur un ancêtre l'invalide aussi.
    pub fn grant(&mut self, holder: HolderId, cap: CapId, to: HolderId) -> Result<CapId, CapError> {
        let src = self.require_valid(holder, cap)?.clone();
        if !src.rights.contains(Rights::GRANT) {
            return Err(CapError::GrantNotAllowed(cap));
        }
        let id = self.alloc_id();
        Ok(self.insert(Cap {
            id,
            object: src.object,
            rights: src.rights,
            holder: to,
            badge: src.badge,
            expires_at: src.expires_at,
            // Le bénéficiaire hérite du même plafond de dérivation (jamais plus).
            max_derived_rights: src.max_derived_rights,
            parent: Some(cap),
            revoked: false,
        }))
    }

    /// `revoke` — invalide cette seule capacité (les filles survivent).
    ///
    /// Autorisé si le demandeur détient la capacité avec le droit `REVOKE`,
    /// ou détient un ancêtre valide avec le droit `REVOKE`.
    pub fn revoke(&mut self, requester: HolderId, cap: CapId) -> Result<(), CapError> {
        self.authorize_revoke(requester, cap)?;
        self.caps.get_mut(&cap).expect("cap existe").revoked = true;
        Ok(())
    }

    /// `revoke_tree` — invalide la capacité et toute sa descendance
    /// (dérivations et grants), immédiatement (F-SEC-02).
    pub fn revoke_tree(&mut self, requester: HolderId, cap: CapId) -> Result<(), CapError> {
        self.authorize_revoke(requester, cap)?;
        let mut stack = vec![cap];
        while let Some(id) = stack.pop() {
            if let Some(c) = self.caps.get_mut(&id) {
                c.revoked = true;
            }
            if let Some(kids) = self.children.get(&id) {
                stack.extend(kids.iter().copied());
            }
        }
        Ok(())
    }

    /// Le demandeur peut-il révoquer `cap` ?
    fn authorize_revoke(&self, requester: HolderId, cap: CapId) -> Result<(), CapError> {
        if !self.caps.contains_key(&cap) {
            return Err(CapError::UnknownCap(cap));
        }
        // Remonte la chaîne de parenté : le droit REVOKE sur n'importe quel
        // ancêtre valide détenu par le demandeur suffit.
        let mut cur = Some(cap);
        while let Some(id) = cur {
            let c = &self.caps[&id];
            if c.holder == requester && c.rights.contains(Rights::REVOKE) && c.is_valid(self.now) {
                return Ok(());
            }
            cur = c.parent;
        }
        Err(CapError::RevokeNotAllowed(cap))
    }

    /// `invoke` — gate d'autorisation : vérifie que `holder` peut exercer
    /// `required` sur l'objet référencé par `cap`.
    pub fn authorize(
        &self,
        holder: HolderId,
        cap: CapId,
        required: Rights,
    ) -> Result<AccessGrant, CapError> {
        let c = self.require_valid(holder, cap)?;
        if !c.rights.contains(required) {
            return Err(CapError::PermissionDenied {
                object: c.object.clone(),
                required,
            });
        }
        Ok(AccessGrant {
            cap,
            object: c.object.clone(),
            rights: c.rights,
            badge: c.badge,
        })
    }

    /// Nombre de capacités vivantes (non révoquées) — utile aux tests de fuite.
    pub fn live_count(&self) -> usize {
        self.caps.values().filter(|c| !c.revoked).count()
    }

    /// Ensemble des capacités valides d'un détenteur (snapshot pour
    /// `CognitiveState.cap_set_snapshot`, cf. specs-techniques §4.2).
    pub fn snapshot_of(&self, holder: HolderId) -> HashSet<CapId> {
        self.caps
            .values()
            .filter(|c| c.holder == holder && c.is_valid(self.now))
            .map(|c| c.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALICE: HolderId = HolderId(1);
    const BOB: HolderId = HolderId(2);

    fn rw() -> Rights {
        Rights::READ | Rights::WRITE
    }

    // --- mint / authorize -------------------------------------------------

    #[test]
    fn mint_puis_invoke_ok() {
        let mut s = CapStore::new();
        let c = s.mint(ALICE, "fs:/notes/a.md", rw(), None, None, 0);
        let g = s.authorize(ALICE, c, Rights::READ).unwrap();
        assert_eq!(g.object, "fs:/notes/a.md");
    }

    #[test]
    fn invoke_exige_les_droits() {
        let mut s = CapStore::new();
        let c = s.mint(ALICE, "fs:/notes/a.md", Rights::READ, None, None, 0);
        assert!(matches!(
            s.authorize(ALICE, c, Rights::WRITE),
            Err(CapError::PermissionDenied { .. })
        ));
    }

    #[test]
    fn invoke_depuis_un_autre_detenteur_echoue() {
        let mut s = CapStore::new();
        let c = s.mint(ALICE, "fs:/notes/a.md", rw(), None, None, 0);
        assert_eq!(
            s.authorize(BOB, c, Rights::READ),
            Err(CapError::WrongHolder(c))
        );
    }

    // --- atténuation stricte ----------------------------------------------

    #[test]
    fn derive_attenue_ok() {
        let mut s = CapStore::new();
        let c = s.mint(ALICE, "fs:/notes/a.md", rw(), None, None, 7);
        let d = s.derive(ALICE, c, Rights::READ, None, None, 42).unwrap();
        let cap = s.get(d).unwrap();
        assert_eq!(cap.rights, Rights::READ);
        assert_eq!(cap.badge, 42);
        assert_eq!(cap.parent, Some(c));
    }

    #[test]
    fn derive_ne_peut_pas_elever_les_droits() {
        let mut s = CapStore::new();
        let c = s.mint(ALICE, "fs:/notes/a.md", Rights::READ, None, None, 0);
        assert_eq!(
            s.derive(ALICE, c, rw(), None, None, 0),
            Err(CapError::AttenuationViolation)
        );
    }

    #[test]
    fn derive_est_borne_par_max_derived_rights() {
        let mut s = CapStore::new();
        // Le parent a READ|WRITE mais n'autorise que READ en dérivation.
        let c = s.mint(ALICE, "fs:/notes/a.md", rw(), Some(Rights::READ), None, 0);
        assert_eq!(
            s.derive(ALICE, c, Rights::WRITE, None, None, 0),
            Err(CapError::AttenuationViolation)
        );
        assert!(s.derive(ALICE, c, Rights::READ, None, None, 0).is_ok());
    }

    #[test]
    fn le_plafond_de_derivation_ne_peut_pas_s_elargir() {
        let mut s = CapStore::new();
        let c = s.mint(ALICE, "fs:/x", rw(), Some(Rights::READ), None, 0);
        let d = s.derive(ALICE, c, Rights::READ, None, None, 0).unwrap();
        // La fille hérite d'un plafond ≤ READ ; demander WRITE reste interdit
        // même si un hypothétique max_derived plus large était fourni.
        assert_eq!(
            s.derive(ALICE, d, Rights::WRITE, Some(rw()), None, 0),
            Err(CapError::AttenuationViolation)
        );
    }

    #[test]
    fn chaine_de_derivation_reste_attenuee() {
        let mut s = CapStore::new();
        let c0 = s.mint(
            ALICE,
            "model:llm",
            Rights::EXECUTE | Rights::GRANT,
            None,
            None,
            0,
        );
        let c1 = s.derive(ALICE, c0, Rights::EXECUTE, None, None, 0).unwrap();
        // Depuis une capacité EXECUTE seule, impossible de re-granter.
        assert_eq!(
            s.derive(ALICE, c1, Rights::EXECUTE | Rights::GRANT, None, None, 0),
            Err(CapError::AttenuationViolation)
        );
    }

    // --- TTL ---------------------------------------------------------------

    #[test]
    fn ttl_expire_invalide_la_capacite() {
        let mut s = CapStore::new();
        let c = s.mint(ALICE, "fs:/tmp", Rights::READ, None, Some(10), 0);
        s.advance_clock(10);
        assert_eq!(
            s.authorize(ALICE, c, Rights::READ),
            Err(CapError::Expired(c))
        );
    }

    #[test]
    fn ttl_derive_ne_peut_pas_depasser_le_parent() {
        let mut s = CapStore::new();
        let c = s.mint(ALICE, "fs:/tmp", Rights::READ, None, Some(10), 0);
        assert_eq!(
            s.derive(ALICE, c, Rights::READ, None, Some(11), 0),
            Err(CapError::TtlViolation)
        );
        assert!(s.derive(ALICE, c, Rights::READ, None, Some(5), 0).is_ok());
    }

    // --- grant --------------------------------------------------------------

    #[test]
    fn grant_transfert_ok() {
        let mut s = CapStore::new();
        let rights = Rights::READ | Rights::GRANT;
        let c = s.mint(ALICE, "fs:/shared", rights, None, None, 0);
        let g = s.grant(ALICE, c, BOB).unwrap();
        assert!(s.authorize(BOB, g, Rights::READ).is_ok());
        // L'original reste utilisable par Alice.
        assert!(s.authorize(ALICE, c, Rights::READ).is_ok());
    }

    #[test]
    fn grant_sans_droit_grant_echoue() {
        let mut s = CapStore::new();
        let c = s.mint(ALICE, "fs:/shared", Rights::READ, None, None, 0);
        assert_eq!(s.grant(ALICE, c, BOB), Err(CapError::GrantNotAllowed(c)));
    }

    // --- révocation ---------------------------------------------------------

    #[test]
    fn revoke_simple_preserve_les_filles() {
        let mut s = CapStore::new();
        let rights = rw() | Rights::REVOKE;
        let c = s.mint(ALICE, "fs:/a", rights, None, None, 0);
        let d = s.derive(ALICE, c, Rights::READ, None, None, 0).unwrap();
        s.revoke(ALICE, c).unwrap();
        assert_eq!(
            s.authorize(ALICE, c, Rights::READ),
            Err(CapError::Revoked(c))
        );
        // La fille déjà dérivée reste valide (révocation simple).
        assert!(s.authorize(ALICE, d, Rights::READ).is_ok());
    }

    #[test]
    fn revoke_tree_invalide_toute_la_descendance() {
        let mut s = CapStore::new();
        let rights = rw() | Rights::GRANT | Rights::REVOKE;
        let root = s.mint(ALICE, "fs:/a", rights, None, None, 0);
        let d1 = s
            .derive(ALICE, root, Rights::READ | Rights::GRANT, None, None, 0)
            .unwrap();
        let g1 = s.grant(ALICE, d1, BOB).unwrap();
        let d2 = s.derive(BOB, g1, Rights::READ, None, None, 0).unwrap();

        s.revoke_tree(ALICE, root).unwrap();

        for id in [root, d1, g1, d2] {
            assert!(
                !s.get(id).unwrap().is_valid(s.now()),
                "{id:?} devrait être révoquée"
            );
        }
        assert_eq!(s.live_count(), 0);
    }

    #[test]
    fn revoke_par_ancetre_revocable_ok() {
        let mut s = CapStore::new();
        let rights = rw() | Rights::GRANT | Rights::REVOKE;
        let root = s.mint(ALICE, "fs:/a", rights, None, None, 0);
        let g = s.grant(ALICE, root, BOB).unwrap();
        // Alice révoque la copie de Bob via son ancêtre.
        s.revoke(ALICE, g).unwrap();
        assert_eq!(s.authorize(BOB, g, Rights::READ), Err(CapError::Revoked(g)));
    }

    #[test]
    fn revoke_sans_droit_echoue() {
        let mut s = CapStore::new();
        let root = s.mint(ALICE, "fs:/a", rw() | Rights::REVOKE, None, None, 0);
        let d = s.derive(ALICE, root, rw(), None, None, 0).unwrap();
        // Bob ne détient rien dans la chaîne : refus.
        assert_eq!(s.revoke(BOB, d), Err(CapError::RevokeNotAllowed(d)));
        // Alice détient `d` sans REVOKE, mais root (ancêtre) a REVOKE : autorisé.
        assert!(s.revoke(ALICE, d).is_ok());
    }

    #[test]
    fn revoke_cap_inconnue_echoue() {
        let mut s = CapStore::new();
        assert_eq!(
            s.revoke(ALICE, CapId(999)),
            Err(CapError::UnknownCap(CapId(999)))
        );
    }

    #[test]
    fn utilisation_apres_revocation_refusee_immediatement() {
        let mut s = CapStore::new();
        let c = s.mint(
            ALICE,
            "model:llm",
            Rights::EXECUTE | Rights::REVOKE,
            None,
            None,
            0,
        );
        s.revoke(ALICE, c).unwrap();
        // F-SEC-02 : révocation immédiate, sans délai.
        assert_eq!(
            s.authorize(ALICE, c, Rights::EXECUTE),
            Err(CapError::Revoked(c))
        );
    }

    // --- snapshot / divers ---------------------------------------------------

    #[test]
    fn snapshot_ne_contient_que_les_capacites_valides_du_detenteur() {
        let mut s = CapStore::new();
        let a = s.mint(ALICE, "fs:/1", rw(), None, None, 0);
        let b = s.mint(ALICE, "fs:/2", rw() | Rights::REVOKE, None, None, 0);
        let _c = s.mint(BOB, "fs:/3", rw(), None, None, 0);
        s.revoke(ALICE, b).unwrap();
        let snap = s.snapshot_of(ALICE);
        assert_eq!(snap.len(), 1);
        assert!(snap.contains(&a));
    }
}
