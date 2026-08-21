//! Simulateur runtime du Placement Manager (§3.5.4 → §3.5.6).
//!
//! Gère l'état partagé : plusieurs modèles placés, budgets consommés,
//! éviction fair/priority, échelle de pression mémoire (§3.5.5) et
//! re-profilage à chaud (passage `latency` → `memory-saver` sans rechargement).

use crate::cost::CostModel;
use crate::hardware::HardwareProfile;
use crate::manager::{Budgets, PlacementError, PlacementManager};
use crate::model::ModelDesc;
use crate::plan::{PlacementPlan, PlacementProfile, Priority, Shard, ShardKind, Tier};
use std::collections::HashMap;

/// État d'exécution d'un modèle placé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Active,
    /// Inférence suspendue par la pression mémoire (§3.5.5 action 4).
    Suspended,
}

/// Modèle placé dans le simulateur.
#[derive(Debug, Clone)]
pub struct PlacedModel {
    pub desc: ModelDesc,
    pub plan: PlacementPlan,
    pub priority: Priority,
    pub state: RunState,
    /// Batch size réduit par la pression (§3.5.5 action 1).
    pub batch_reduced: bool,
}

/// Événement du simulateur (journal pour assertions de scénarios).
#[derive(Debug, Clone, PartialEq)]
pub enum SimEvent {
    Placed {
        model: String,
        profile: PlacementProfile,
    },
    /// Shard migré vers un tier plus froid (éviction ou pression).
    Migrated {
        model: String,
        shard_id: u32,
        from: Tier,
        to: Tier,
    },
    BatchReduced {
        model: String,
    },
    Suspended {
        model: String,
    },
    Refused {
        model: String,
        reason: String,
    },
    Reprofiled {
        model: String,
        from: PlacementProfile,
        to: PlacementProfile,
        migrated_bytes: u64,
        est_switch_ms: f64,
    },
}

/// Rapport de l'échelle de pression (§3.5.5).
#[derive(Debug, Clone, Default)]
pub struct PressureReport {
    pub satisfied: bool,
    pub freed_vram: u64,
    pub actions: Vec<String>,
}

/// Rapport de re-profilage à chaud.
#[derive(Debug, Clone)]
pub struct ReprofileReport {
    pub migrated_bytes: u64,
    pub est_switch_ms: f64,
    pub plan: PlacementPlan,
}

/// Simulateur de placement multi-modèles.
#[derive(Debug)]
pub struct PlacementSim {
    pub hw: HardwareProfile,
    pub cost: CostModel,
    manager: PlacementManager,
    used: Budgets,
    placed: HashMap<String, PlacedModel>,
    tick: u64,
    events: Vec<SimEvent>,
    /// `auto` device policy (E17): demote when VRAM is tight, promote when it recovers.
    auto_demoted: bool,
}

impl PlacementSim {
    pub fn new(hw: HardwareProfile, cost: CostModel) -> Self {
        let manager = PlacementManager::new(hw.clone(), cost.clone());
        Self {
            hw,
            cost,
            manager,
            used: Budgets::default(),
            placed: HashMap::new(),
            tick: 0,
            events: Vec::new(),
            auto_demoted: false,
        }
    }

    /// Hysteresis for Settings **auto**: MemorySaver when free VRAM < 15%,
    /// Balanced when it rises above 30%. Pin `cpu`/`gpu` is applied by the
    /// session (process choice), not here.
    pub const AUTO_DEMOTE_FREE_FRAC: f64 = 0.15;
    pub const AUTO_PROMOTE_FREE_FRAC: f64 = 0.30;

    pub fn auto_hysteresis_profile(&mut self) -> PlacementProfile {
        if !self.hw.has_gpu || self.hw.vram_budget() == 0 {
            return PlacementProfile::CpuOnly;
        }
        let frac = self.free().vram as f64 / self.hw.vram_budget() as f64;
        if self.auto_demoted {
            if frac > Self::AUTO_PROMOTE_FREE_FRAC {
                self.auto_demoted = false;
                PlacementProfile::Balanced
            } else {
                PlacementProfile::MemorySaver
            }
        } else if frac < Self::AUTO_DEMOTE_FREE_FRAC {
            self.auto_demoted = true;
            PlacementProfile::MemorySaver
        } else {
            PlacementProfile::Balanced
        }
    }

    pub fn events(&self) -> &[SimEvent] {
        &self.events
    }

    pub fn get(&self, model_id: &str) -> Option<&PlacedModel> {
        self.placed.get(model_id)
    }

    pub fn free(&self) -> Budgets {
        Budgets {
            vram: self.hw.vram_budget().saturating_sub(self.used.vram),
            ram: self.hw.ram_budget().saturating_sub(self.used.ram),
            disk: self.hw.disk_budget().saturating_sub(self.used.disk),
        }
    }

    fn bump_tick(&mut self) {
        self.tick += 1;
    }

    /// Place un modèle, avec éviction fair/priority si nécessaire (F-PLC-06).
    ///
    /// Politique :
    /// 1. **Préemption** : un modèle plus prioritaire réclame la VRAM qu'un
    ///    placement idéal lui donnerait — les shards VRAM des modèles
    ///    **strictement moins prioritaires** migrent vers les tiers froids ;
    /// 2. placement avec repli de profil (§16) ;
    /// 3. en dernier recours, éviction de secours (priorité ≤, LRU d'abord) ;
    /// 4. refus explicite audité (F-PLC-09).
    pub fn place(
        &mut self,
        model: &ModelDesc,
        profile: PlacementProfile,
        priority: Priority,
        kv_tokens: u32,
    ) -> Result<(), PlacementError> {
        self.bump_tick();

        // (1) Préemption : VRAM « due » à ce modèle selon un placement idéal.
        if let Ok(ideal) =
            self.manager
                .place_auto(model, profile, kv_tokens, Budgets::full(&self.hw))
        {
            let desired_vram = ideal.bytes_on(Tier::Vram);
            self.preempt_vram_for(desired_vram, priority);
        }

        // (2)+(3) Essai de placement, avec éviction de secours en cas d'échec.
        let mut last_err = None;
        for attempt in 0.. {
            let free = self.free();
            match self.manager.place_auto(model, profile, kv_tokens, free) {
                Ok(plan) => {
                    let used = Budgets {
                        vram: plan.bytes_on(Tier::Vram),
                        ram: plan.bytes_on(Tier::Ram),
                        disk: plan.bytes_on(Tier::Disk),
                    };
                    self.used.vram += used.vram;
                    self.used.ram += used.ram;
                    self.used.disk += used.disk;
                    let eff_profile = plan.profile;
                    self.placed.insert(
                        model.id.clone(),
                        PlacedModel {
                            desc: model.clone(),
                            plan,
                            priority,
                            state: RunState::Active,
                            batch_reduced: false,
                        },
                    );
                    self.events.push(SimEvent::Placed {
                        model: model.id.clone(),
                        profile: eff_profile,
                    });
                    return Ok(());
                }
                Err(e) => {
                    last_err = Some(e);
                    // Tente une éviction d'un shard de priorité ≤ demandeur.
                    if !self.evict_one(priority) {
                        break; // plus rien d'évictable
                    }
                    if attempt > 256 {
                        break; // garde-fou
                    }
                }
            }
        }
        let err = last_err.unwrap_or(PlacementError::Impossible {
            reason: "budgets insuffisants".into(),
            suggestion: "décharger un modèle".into(),
        });
        self.events.push(SimEvent::Refused {
            model: model.id.clone(),
            reason: err.to_string(),
        });
        Err(err)
    }

    /// Préemption VRAM (F-PLC-06) : migre vers les tiers froids les shards
    /// VRAM des modèles **strictement moins prioritaires** jusqu'à disposer
    /// de `needed` octets libres. Ordre : priorité croissante, puis LRU.
    fn preempt_vram_for(&mut self, needed: u64, requester: Priority) {
        let mut guard = 0;
        while self.free().vram < needed {
            let victim = self
                .placed
                .iter()
                .filter(|(_, pm)| pm.priority < requester)
                .flat_map(|(id, pm)| {
                    pm.plan
                        .shards
                        .iter()
                        .filter(|s| s.residency == Tier::Vram && s.evictable())
                        .map(|s| {
                            (
                                pm.priority,
                                s.last_use_tick,
                                s.priority_boost,
                                id.clone(),
                                s.shard_id,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .min_by_key(|(prio, tick, boost, _, _)| (*prio, *tick, *boost));
            match victim {
                Some((_, _, _, id, sid)) => {
                    if !self.migrate_shard_colder(&id, sid) {
                        break;
                    }
                }
                None => break,
            }
            guard += 1;
            if guard > 1024 {
                break;
            }
        }
    }

    /// Migre un shard évictable vers un tier plus froid.
    ///
    /// Candidats triés par (priorité du modèle croissante, `last_use_tick`
    /// croissant, `priority_boost` croissant) — fair (LRU) puis priority.
    /// Retourne `false` si rien n'est évictable.
    fn evict_one(&mut self, requester: Priority) -> bool {
        let mut candidates: Vec<(String, u32)> = Vec::new();
        for (id, pm) in &self.placed {
            if pm.priority > requester {
                continue; // jamais d'éviction sur plus prioritaire (F-PLC-06)
            }
            for s in &pm.plan.shards {
                if s.evictable() && s.residency.colder().is_some() {
                    candidates.push((id.clone(), s.shard_id));
                }
            }
        }
        // Tri fair/priority : priorité croissante, puis LRU.
        candidates.sort_by_key(|(id, sid)| {
            let pm = &self.placed[id];
            let s = pm.plan.shards.iter().find(|s| s.shard_id == *sid).unwrap();
            (pm.priority, s.last_use_tick, s.priority_boost)
        });

        for (id, sid) in candidates {
            if self.migrate_shard_colder(&id, sid) {
                return true;
            }
        }
        false
    }

    /// Migre un shard vers le tier immédiatement plus froid, en cascadant
    /// si le tier cible est plein (VRAM→RAM→DISK). Met à jour les budgets.
    fn migrate_shard_colder(&mut self, model: &str, shard_id: u32) -> bool {
        let (from, size) = {
            let pm = match self.placed.get(model) {
                Some(pm) => pm,
                None => return false,
            };
            let s = match pm.plan.shards.iter().find(|s| s.shard_id == shard_id) {
                Some(s) => s,
                None => return false,
            };
            (s.residency, s.size_bytes)
        };
        let to = match from.colder() {
            Some(t) => t,
            None => return false,
        };

        // Cascade : assure la place sur le tier cible en poussant d'abord
        // les shards les plus froids de ce tier vers plus froid encore.
        let mut guard = 0;
        while !self.fits(to, size) {
            let victim = self.coldest_shard_on(to, model);
            match victim {
                Some((vm, vsid)) => {
                    if !self.migrate_shard_colder(&vm, vsid) {
                        return false;
                    }
                }
                None => return false,
            }
            guard += 1;
            if guard > 1024 {
                return false;
            }
        }

        let pm = self.placed.get_mut(model).unwrap();
        let s = pm
            .plan
            .shards
            .iter_mut()
            .find(|s| s.shard_id == shard_id)
            .unwrap();
        s.residency = to;
        match from {
            Tier::Vram => self.used.vram -= size,
            Tier::Ram => self.used.ram -= size,
            Tier::Disk => self.used.disk -= size,
        }
        match to {
            Tier::Vram => self.used.vram += size,
            Tier::Ram => self.used.ram += size,
            Tier::Disk => self.used.disk += size,
        }
        self.events.push(SimEvent::Migrated {
            model: model.into(),
            shard_id,
            from,
            to,
        });
        true
    }

    fn fits(&self, tier: Tier, bytes: u64) -> bool {
        let free = self.free();
        match tier {
            Tier::Vram => free.vram >= bytes,
            Tier::Ram => free.ram >= bytes,
            Tier::Disk => free.disk >= bytes,
        }
    }

    /// Shard évictable le plus froid d'un tier (hors modèle exclu).
    fn coldest_shard_on(&self, tier: Tier, exclude: &str) -> Option<(String, u32)> {
        let mut best: Option<(String, Shard)> = None;
        for (id, pm) in &self.placed {
            if id == exclude {
                continue;
            }
            for s in &pm.plan.shards {
                if s.residency == tier && s.evictable() {
                    let better = match &best {
                        None => true,
                        Some((_, bs)) => {
                            (s.last_use_tick, s.priority_boost)
                                < (bs.last_use_tick, bs.priority_boost)
                        }
                    };
                    if better {
                        best = Some((id.clone(), s.clone()));
                    }
                }
            }
        }
        best.map(|(id, s)| (id, s.shard_id))
    }

    /// Échelle de pression mémoire (§3.5.5) déclenchée par une demande VRAM
    /// de `bytes` à la priorité `by`.
    ///
    /// Actions dans l'ordre : (1) réduire le batch des low-priority,
    /// (2) migrer VRAM→RAM, (3) migrer RAM→DISK, (4) suspendre les
    /// inférences low-priority, (5) refus explicite si encore insuffisant.
    pub fn handle_pressure(&mut self, bytes: u64, by: Priority) -> PressureReport {
        self.bump_tick();
        let mut report = PressureReport::default();
        if self.free().vram >= bytes {
            report.satisfied = true;
            return report;
        }

        let low_prio: Vec<String> = self
            .placed
            .iter()
            .filter(|(_, pm)| pm.priority < by)
            .map(|(id, _)| id.clone())
            .collect();

        // (1) Réduire le batch : libère la moitié du KV VRAM des low-prio.
        for id in &low_prio {
            let pm = self.placed.get_mut(id).unwrap();
            if pm.batch_reduced {
                continue;
            }
            pm.batch_reduced = true;
            let mut freed = 0u64;
            for s in pm.plan.shards.iter_mut() {
                if s.kind == ShardKind::KvCache && s.residency == Tier::Vram {
                    let half = s.size_bytes / 2;
                    s.size_bytes -= half;
                    freed += half;
                }
            }
            self.used.vram -= freed;
            report.freed_vram += freed;
            report.actions.push(format!("batch réduit: {id}"));
            self.events
                .push(SimEvent::BatchReduced { model: id.clone() });
            if self.free().vram >= bytes {
                report.satisfied = true;
                return report;
            }
        }

        // (2)+(3) Migrer les shards low-prio vers les tiers froids.
        for id in &low_prio {
            loop {
                let victim = {
                    let pm = &self.placed[id];
                    pm.plan
                        .shards
                        .iter()
                        .filter(|s| s.evictable() && s.residency == Tier::Vram)
                        .min_by_key(|s| (s.last_use_tick, s.priority_boost))
                        .map(|s| s.shard_id)
                };
                match victim {
                    Some(sid) => {
                        if !self.migrate_shard_colder(id, sid) {
                            break;
                        }
                        report
                            .actions
                            .push(format!("migration VRAM→froid: {id}#{sid}"));
                    }
                    None => break,
                }
                if self.free().vram >= bytes {
                    report.satisfied = true;
                    return report;
                }
            }
        }

        // (4) Suspendre les inférences low-priority : leur KV VRAM est libéré.
        for id in &low_prio {
            let pm = self.placed.get_mut(id).unwrap();
            if pm.state == RunState::Suspended {
                continue;
            }
            pm.state = RunState::Suspended;
            let mut freed = 0u64;
            for s in pm.plan.shards.iter_mut() {
                if s.kind == ShardKind::KvCache && s.residency == Tier::Vram {
                    freed += s.size_bytes;
                    s.size_bytes = 0;
                }
            }
            self.used.vram -= freed;
            report.freed_vram += freed;
            report.actions.push(format!("suspendu: {id}"));
            self.events.push(SimEvent::Suspended { model: id.clone() });
            if self.free().vram >= bytes {
                report.satisfied = true;
                return report;
            }
        }

        // (5) Refus explicite (F-PLC-09).
        report.satisfied = false;
        report
            .actions
            .push("refus des nouvelles inférences non critiques".into());
        report
    }

    /// Re-profilage à chaud (§3.5.6) : recalcule le plan sous le nouveau
    /// profil et ne paie que la **migration** des shards qui changent de tier.
    pub fn reprofile(
        &mut self,
        model_id: &str,
        new_profile: PlacementProfile,
    ) -> Result<ReprofileReport, PlacementError> {
        self.bump_tick();
        let (desc, kv_tokens, old_plan) = {
            let pm = self
                .placed
                .get(model_id)
                .ok_or_else(|| PlacementError::Impossible {
                    reason: format!("modèle {model_id} non placé"),
                    suggestion: "le placer d'abord".into(),
                })?;
            (pm.desc.clone(), pm.plan.kv_tokens, pm.plan.clone())
        };

        // Budgets disponibles = libres + ce que le modèle occupe déjà.
        let mut budgets = self.free();
        budgets.vram += old_plan.bytes_on(Tier::Vram);
        budgets.ram += old_plan.bytes_on(Tier::Ram);
        budgets.disk += old_plan.bytes_on(Tier::Disk);

        let new_plan =
            self.manager
                .place_model_with_budgets(&desc, new_profile, kv_tokens, budgets)?;

        // Diff de résidence : seuls les shards déplacés coûtent un transfert.
        let mut migrated_bytes = 0u64;
        for new_s in &new_plan.shards {
            let same_kind_old = old_plan.shards.iter().find(|o| o.kind == new_s.kind);
            if let Some(o) = same_kind_old {
                if o.residency != new_s.residency {
                    migrated_bytes += new_s.size_bytes;
                }
            }
        }

        // Mise à jour des budgets utilisés.
        self.used.vram =
            self.used.vram - old_plan.bytes_on(Tier::Vram) + new_plan.bytes_on(Tier::Vram);
        self.used.ram = self.used.ram - old_plan.bytes_on(Tier::Ram) + new_plan.bytes_on(Tier::Ram);
        self.used.disk =
            self.used.disk - old_plan.bytes_on(Tier::Disk) + new_plan.bytes_on(Tier::Disk);

        // Estimation du temps de bascule : le goulot est le plus lent des
        // liens impliqués (PCIe pour RAM↔VRAM, disque sinon).
        let involves_disk = new_plan.bytes_on(Tier::Disk) != old_plan.bytes_on(Tier::Disk);
        let link_bw = if involves_disk {
            self.hw.disk_seq_bw * self.cost.eff_disk
        } else {
            self.hw.host_to_device_bw
        };
        let est_switch_ms = migrated_bytes as f64 / link_bw * 1000.0;

        let from = old_plan.profile;
        let pm = self.placed.get_mut(model_id).unwrap();
        pm.plan = new_plan.clone();
        pm.batch_reduced = false;
        self.events.push(SimEvent::Reprofiled {
            model: model_id.into(),
            from,
            to: new_plan.profile,
            migrated_bytes,
            est_switch_ms,
        });
        Ok(ReprofileReport {
            migrated_bytes,
            est_switch_ms,
            plan: new_plan,
        })
    }

    /// Décharge un modèle et libère ses budgets (F-MDL : unload).
    pub fn unload(&mut self, model_id: &str) {
        if let Some(pm) = self.placed.remove(model_id) {
            self.used.vram -= pm.plan.bytes_on(Tier::Vram);
            self.used.ram -= pm.plan.bytes_on(Tier::Ram);
            self.used.disk -= pm.plan.bytes_on(Tier::Disk);
        }
    }

    /// Marque l'usage d'un modèle (met à jour le LRU de ses shards).
    pub fn touch(&mut self, model_id: &str) {
        self.bump_tick();
        if let Some(pm) = self.placed.get_mut(model_id) {
            for s in pm.plan.shards.iter_mut() {
                s.last_use_tick = self.tick;
            }
        }
    }

    /// Épingle les shards d'un modèle (semantic pin §3.5.8, modèle système).
    pub fn pin(&mut self, model_id: &str) {
        if let Some(pm) = self.placed.get_mut(model_id) {
            for s in pm.plan.shards.iter_mut() {
                s.pin_count += 1;
            }
        }
    }

    /// Estimation de performance pour un modèle placé.
    pub fn estimate(
        &self,
        model_id: &str,
        prompt_tokens: u32,
        ctx_tokens: u32,
    ) -> Option<crate::cost::Estimate> {
        let pm = self.placed.get(model_id)?;
        Some(
            self.manager
                .estimate(&pm.plan, &pm.desc, prompt_tokens, ctx_tokens),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Tier;
    use crate::testutil::{model_32b, model_3b};

    fn sim() -> PlacementSim {
        PlacementSim::new(HardwareProfile::reference_v1(), CostModel::default())
    }

    #[test]
    fn deux_modeles_cohabitent_par_eviction() {
        let mut s = sim();
        let m32 = model_32b();
        let m3 = model_3b();
        s.place(
            &m32,
            PlacementProfile::Balanced,
            Priority::AgentNormal,
            2048,
        )
        .unwrap();
        s.place(&m3, PlacementProfile::Latency, Priority::Interactive, 2048)
            .unwrap();
        assert!(s.get(&m32.id).is_some());
        assert!(s.get(&m3.id).is_some());
    }

    #[test]
    fn eviction_epargne_les_epingles() {
        let mut s = sim();
        let m3 = model_3b();
        let m32 = model_32b();
        s.place(&m3, PlacementProfile::Latency, Priority::AgentNormal, 2048)
            .unwrap();
        s.pin(&m3.id);
        // Le 32B doit se placer sans toucher aux shards épinglés du 3B.
        s.place(&m32, PlacementProfile::Balanced, Priority::AgentHigh, 2048)
            .unwrap();
        let m3_after = s.get(&m3.id).unwrap();
        assert!(m3_after.plan.bytes_on(Tier::Vram) > 0);
        assert!(!s
            .events()
            .iter()
            .any(|e| matches!(e, SimEvent::Migrated { model, .. } if *model == m3.id)));
    }

    #[test]
    fn pression_suit_l_echelle_du_3_5_5() {
        let mut s = sim();
        let m32a = model_32b();
        let mut m32b = model_32b();
        m32b.id = "local:llama-q6-32b-bis".into();
        s.place(
            &m32a,
            PlacementProfile::Balanced,
            Priority::AgentNormal,
            2048,
        )
        .unwrap();
        // Un agent haute priorité réclame 6 GiB de VRAM.
        let rep = s.handle_pressure(6 * (1 << 30), Priority::SystemCritical);
        assert!(rep.satisfied, "actions: {:?}", rep.actions);
        // La pression a dû migrer/suspendre le modèle normal.
        assert!(s.events().iter().any(|e| matches!(
            e,
            SimEvent::Migrated { .. } | SimEvent::Suspended { .. } | SimEvent::BatchReduced { .. }
        )));
        let _ = m32b;
    }

    #[test]
    fn reprofilage_a_chaud_migre_sans_recharger() {
        let mut s = sim();
        let m32 = model_32b();
        s.place(&m32, PlacementProfile::Latency, Priority::AgentNormal, 2048)
            .unwrap();
        let before = s.get(&m32.id).unwrap().plan.clone();
        let rep = s.reprofile(&m32.id, PlacementProfile::MemorySaver).unwrap();
        assert!(rep.migrated_bytes > 0);
        assert!(rep.plan.bytes_on(Tier::Vram) < before.bytes_on(Tier::Vram));
        assert!(rep.plan.bytes_on(Tier::Disk) > before.bytes_on(Tier::Disk));
        assert!(rep.est_switch_ms.is_finite());
    }

    #[test]
    fn unload_libere_les_budgets() {
        let mut s = sim();
        let m3 = model_3b();
        s.place(&m3, PlacementProfile::Latency, Priority::AgentNormal, 2048)
            .unwrap();
        let free_before = s.free().vram;
        s.unload(&m3.id);
        assert!(s.free().vram > free_before);
        assert!(s.get(&m3.id).is_none());
    }

    #[test]
    fn media_evince_ou_cohabite_avec_llm() {
        let mut s = sim();
        let llm = model_3b();
        let img = crate::testutil::model_sd15();
        s.place(&llm, PlacementProfile::Latency, Priority::AgentNormal, 2048)
            .unwrap();
        s.place(&img, PlacementProfile::Balanced, Priority::Interactive, 0)
            .unwrap();
        assert!(s.get(&llm.id).is_some());
        assert!(s.get(&img.id).is_some());
        let img_pm = s.get(&img.id).unwrap();
        assert_eq!(
            img_pm.plan.shards[0].kind,
            crate::plan::ShardKind::MediaWeights
        );
        s.unload(&img.id);
        assert!(s.get(&img.id).is_none());
    }

    #[test]
    fn media_refuse_si_budgets_insuffisants() {
        let tiny = HardwareProfile {
            name: "tiny".into(),
            has_gpu: true,
            vram_total: 512 * 1024 * 1024,
            ram_total: 256 * 1024 * 1024,
            disk_total: 256 * 1024 * 1024,
            os_reserve_vram: 0,
            os_reserve_ram: 0,
            gpu_mem_bw: 100e9,
            ram_mem_bw: 20e9,
            disk_seq_bw: 1e9,
            host_to_device_bw: 10e9,
            gpu_flops: 1e12,
            cpu_flops: 1e11,
            gpus: vec![],
        };
        let mut s = PlacementSim::new(tiny, CostModel::default());
        let img = crate::testutil::model_sd15();
        let err = s
            .place(&img, PlacementProfile::Latency, Priority::Interactive, 0)
            .unwrap_err();
        assert!(matches!(err, PlacementError::Impossible { .. }));
        assert!(s.events().iter().any(|e| matches!(e, SimEvent::Refused { .. })));
    }

    #[test]
    fn auto_hysteresis_demote_then_promote() {
        let mut s = sim();
        assert_eq!(s.auto_hysteresis_profile(), PlacementProfile::Balanced);
        s.used.vram = (s.hw.vram_budget() as f64 * 0.90) as u64;
        assert_eq!(s.auto_hysteresis_profile(), PlacementProfile::MemorySaver);
        s.used.vram = 0;
        assert_eq!(s.auto_hysteresis_profile(), PlacementProfile::Balanced);
    }
}
