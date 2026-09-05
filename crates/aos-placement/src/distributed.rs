//! Safe LAN distribution contracts.
//!
//! This is intentionally a policy layer, not a listener. A transport may be
//! added later, but it must use these states so an unpaired/revoked node can
//! never receive a job by discovery alone.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeTrust {
    Unpaired,
    Paired,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanNode {
    pub node_id: String,
    pub display_name: String,
    pub address: String,
    pub public_key_fingerprint: String,
    pub trust: NodeTrust,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedWork {
    pub work_id: String,
    pub model_id: String,
    #[serde(default)]
    pub shard_ids: Vec<u32>,
    #[serde(default)]
    pub allow_sensitive_data: bool,
    /// Required by the transport adapter; false is rejected by the registry.
    pub encrypted_transport: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LanJobState {
    Planned,
    Running,
    Degraded,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanShardAssignment {
    pub node_id: String,
    pub shard_ids: Vec<u32>,
    #[serde(default)]
    pub kv_tokens: u32,
    pub encrypted_transport: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanWorkPlan {
    pub work_id: String,
    pub state: LanJobState,
    pub assignments: Vec<LanShardAssignment>,
    #[serde(default)]
    pub unassigned_shards: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanRecovery {
    pub work_id: String,
    pub lost_node_id: String,
    pub state: LanJobState,
    pub reassigned_shards: Vec<u32>,
    pub cancelled_nodes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct LanPairingRegistry {
    nodes: HashMap<String, LanNode>,
}

impl LanPairingRegistry {
    pub fn discover(&mut self, node: LanNode) {
        self.nodes.entry(node.node_id.clone()).or_insert(node);
    }

    pub fn pair(&mut self, node_id: &str, fingerprint: &str) -> Result<(), String> {
        let node = self.nodes.get_mut(node_id).ok_or("nœud inconnu")?;
        if node.public_key_fingerprint != fingerprint {
            return Err("empreinte de clé inattendue".into());
        }
        node.trust = NodeTrust::Paired;
        Ok(())
    }

    pub fn revoke(&mut self, node_id: &str) -> bool {
        self.nodes
            .get_mut(node_id)
            .map(|node| {
                node.trust = NodeTrust::Revoked;
                true
            })
            .unwrap_or(false)
    }

    pub fn authorize(&self, node_id: &str, work: &DistributedWork) -> Result<(), String> {
        let node = self.nodes.get(node_id).ok_or("nœud non appairé")?;
        if node.trust != NodeTrust::Paired {
            return Err("nœud non appairé ou révoqué".into());
        }
        let host = node
            .address
            .parse::<std::net::SocketAddr>()
            .map_err(|_| "adresse LAN invalide".to_string())?
            .ip();
        let local = match host {
            std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
            std::net::IpAddr::V6(ip) => {
                ip.is_loopback()
                    || (ip.segments()[0] & 0xfe00) == 0xfc00 // fc00::/7
                    || (ip.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10
            }
        };
        if !local {
            return Err("adresse hors LAN refusée".into());
        }
        if !work.encrypted_transport {
            return Err("transport chiffré obligatoire".into());
        }
        if work.allow_sensitive_data && !node.capabilities.iter().any(|cap| cap == "sensitive-data")
        {
            return Err("politique sensible non autorisée pour ce nœud".into());
        }
        Ok(())
    }

    pub fn get(&self, node_id: &str) -> Option<&LanNode> {
        self.nodes.get(node_id)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &LanNode> {
        self.nodes.values()
    }

    pub fn paired_nodes(&self) -> impl Iterator<Item = &LanNode> {
        self.nodes
            .values()
            .filter(|node| node.trust == NodeTrust::Paired)
    }
}

/// In-process LAN scheduler policy.
///
/// The coordinator deliberately has no socket or discovery side effect. A
/// future authenticated transport must call `plan`, `recover_node_loss` and
/// `cancel`, and must refuse to transmit anything that these methods did not
/// authorize.
#[derive(Debug, Clone, Default)]
pub struct LanCluster {
    registry: LanPairingRegistry,
    jobs: HashMap<String, LanWorkPlan>,
}

impl LanCluster {
    pub fn new(registry: LanPairingRegistry) -> Self {
        Self {
            registry,
            jobs: HashMap::new(),
        }
    }

    pub fn registry(&self) -> &LanPairingRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut LanPairingRegistry {
        &mut self.registry
    }

    /// Partition a model's declared shards across paired LAN nodes.
    pub fn plan(&mut self, work: &DistributedWork, kv_tokens: u32) -> Result<LanWorkPlan, String> {
        if work.work_id.trim().is_empty() {
            return Err("identifiant de travail LAN vide".into());
        }
        if work.shard_ids.is_empty() {
            return Err("aucun shard déclaré pour le travail LAN".into());
        }
        let nodes: Vec<String> = self
            .registry
            .paired_nodes()
            .map(|node| node.node_id.clone())
            .collect();
        if nodes.is_empty() {
            return Err("aucun nœud LAN appairé".into());
        }
        for node_id in &nodes {
            self.registry.authorize(node_id, work)?;
        }

        let mut assignments: Vec<LanShardAssignment> = nodes
            .iter()
            .map(|node_id| LanShardAssignment {
                node_id: node_id.clone(),
                shard_ids: Vec::new(),
                kv_tokens,
                encrypted_transport: work.encrypted_transport,
            })
            .collect();
        let assignment_count = assignments.len();
        for (index, shard_id) in work.shard_ids.iter().copied().enumerate() {
            assignments[index % assignment_count]
                .shard_ids
                .push(shard_id);
        }
        assignments.retain(|assignment| !assignment.shard_ids.is_empty());
        let plan = LanWorkPlan {
            work_id: work.work_id.clone(),
            state: LanJobState::Planned,
            assignments,
            unassigned_shards: Vec::new(),
        };
        self.jobs.insert(work.work_id.clone(), plan.clone());
        Ok(plan)
    }

    pub fn set_running(&mut self, work_id: &str) -> Result<(), String> {
        let plan = self.jobs.get_mut(work_id).ok_or("travail LAN inconnu")?;
        if plan.state == LanJobState::Cancelled || plan.state == LanJobState::Failed {
            return Err("travail LAN terminé".into());
        }
        plan.state = LanJobState::Running;
        Ok(())
    }

    /// Remove a lost node and redistribute its shards to surviving paired nodes.
    pub fn recover_node_loss(
        &mut self,
        work_id: &str,
        lost_node_id: &str,
    ) -> Result<LanRecovery, String> {
        let plan = self.jobs.get_mut(work_id).ok_or("travail LAN inconnu")?;
        let lost = plan
            .assignments
            .iter()
            .position(|assignment| assignment.node_id == lost_node_id)
            .ok_or("nœud absent du travail LAN")?;
        let shards = plan.assignments.remove(lost).shard_ids;
        let survivors: Vec<String> = plan
            .assignments
            .iter()
            .filter(|assignment| {
                self.registry
                    .get(&assignment.node_id)
                    .is_some_and(|node| node.trust == NodeTrust::Paired)
            })
            .map(|assignment| assignment.node_id.clone())
            .collect();
        if survivors.is_empty() {
            plan.state = LanJobState::Failed;
            plan.unassigned_shards = shards.clone();
            return Ok(LanRecovery {
                work_id: work_id.into(),
                lost_node_id: lost_node_id.into(),
                state: plan.state,
                reassigned_shards: Vec::new(),
                cancelled_nodes: Vec::new(),
            });
        }
        for (index, shard_id) in shards.iter().copied().enumerate() {
            let node_id = &survivors[index % survivors.len()];
            if let Some(assignment) = plan
                .assignments
                .iter_mut()
                .find(|assignment| &assignment.node_id == node_id)
            {
                assignment.shard_ids.push(shard_id);
            }
        }
        plan.state = LanJobState::Degraded;
        Ok(LanRecovery {
            work_id: work_id.into(),
            lost_node_id: lost_node_id.into(),
            state: plan.state,
            reassigned_shards: shards,
            cancelled_nodes: Vec::new(),
        })
    }

    /// Mark the job cancelled and return every node that must receive a cancel.
    pub fn cancel(&mut self, work_id: &str) -> Result<LanRecovery, String> {
        let plan = self.jobs.get_mut(work_id).ok_or("travail LAN inconnu")?;
        plan.state = LanJobState::Cancelled;
        Ok(LanRecovery {
            work_id: work_id.into(),
            lost_node_id: String::new(),
            state: plan.state,
            reassigned_shards: Vec::new(),
            cancelled_nodes: plan
                .assignments
                .iter()
                .map(|assignment| assignment.node_id.clone())
                .collect(),
        })
    }

    pub fn job(&self, work_id: &str) -> Option<&LanWorkPlan> {
        self.jobs.get(work_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> LanNode {
        LanNode {
            node_id: "n1".into(),
            display_name: "laptop".into(),
            address: "192.168.1.2:9000".into(),
            public_key_fingerprint: "abc".into(),
            trust: NodeTrust::Unpaired,
            capabilities: vec![],
        }
    }

    #[test]
    fn discovery_ne_authorise_pas_le_routage() {
        let mut registry = LanPairingRegistry::default();
        registry.discover(node());
        let work = DistributedWork {
            work_id: "w".into(),
            model_id: "m".into(),
            shard_ids: vec![1],
            allow_sensitive_data: false,
            encrypted_transport: true,
        };
        assert!(registry.authorize("n1", &work).is_err());
        registry.pair("n1", "abc").unwrap();
        assert!(registry.authorize("n1", &work).is_ok());
        registry.revoke("n1");
        assert!(registry.authorize("n1", &work).is_err());
    }

    #[test]
    fn cluster_repartit_reprend_et_annule_un_travail() {
        let mut registry = LanPairingRegistry::default();
        for (id, ip) in [("n1", "192.168.1.2:9000"), ("n2", "192.168.1.3:9000")] {
            registry.discover(LanNode {
                node_id: id.into(),
                display_name: id.into(),
                address: ip.into(),
                public_key_fingerprint: format!("{id}-key"),
                trust: NodeTrust::Unpaired,
                capabilities: vec![],
            });
            registry.pair(id, &format!("{id}-key")).unwrap();
        }
        let mut cluster = LanCluster::new(registry);
        let work = DistributedWork {
            work_id: "w1".into(),
            model_id: "m".into(),
            shard_ids: vec![1, 2, 3, 4],
            allow_sensitive_data: false,
            encrypted_transport: true,
        };
        let plan = cluster.plan(&work, 4096).unwrap();
        assert_eq!(plan.assignments.len(), 2);
        assert_eq!(
            plan.assignments
                .iter()
                .map(|a| a.shard_ids.len())
                .sum::<usize>(),
            4
        );
        cluster.set_running("w1").unwrap();
        let recovery = cluster.recover_node_loss("w1", "n1").unwrap();
        assert_eq!(recovery.reassigned_shards.len(), 2);
        assert_eq!(cluster.job("w1").unwrap().state, LanJobState::Degraded);
        let cancelled = cluster.cancel("w1").unwrap();
        assert_eq!(cancelled.state, LanJobState::Cancelled);
        assert_eq!(cancelled.cancelled_nodes, vec!["n2"]);
    }
}
