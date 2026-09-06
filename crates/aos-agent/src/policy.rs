//! Politique par agent, appliquée dans le worker (S6 phase 2).
//!
//! Refus explicites (fail-closed) : l'agent reçoit un message d'erreur
//! actionnable au lieu d'un contournement silencieux. Tout est permissif par
//! défaut (`AgentPolicy::default`) pour ne rien casser.

use aos_proto::{AgentNetPolicy, AgentPolicy};

/// Outils réseau soumis à `net:deny`.
pub const NET_TOOLS: &[&str] = &["web.search", "web.browse", "net.fetch"];

/// Retourne le motif de refus, ou `None` si l'outil passe.
pub fn policy_deny(policy: &AgentPolicy, tool: &str) -> Option<String> {
    if matches!(policy.net, AgentNetPolicy::Deny) && NET_TOOLS.contains(&tool) {
        return Some(format!(
            "outil {tool} bloqué par la politique réseau de l'agent (net:deny) — demandez à l'utilisateur de l'autoriser"
        ));
    }
    if !policy.fs_write && tool == "fs.write" {
        return Some(format!(
            "outil {tool} bloqué par la politique de l'agent (fs.write:deny) — demandez à l'utilisateur de l'autoriser"
        ));
    }
    if let Some(allow) = &policy.tool_allowlist {
        if !allow.iter().any(|t| t == tool) {
            return Some(format!(
                "outil {tool} hors allowlist de l'agent ({} outil(s) autorisé(s))",
                allow.len()
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_allows_everything() {
        let policy = AgentPolicy::default();
        assert_eq!(policy_deny(&policy, "web.search"), None);
        assert_eq!(policy_deny(&policy, "fs.write"), None);
        assert_eq!(policy_deny(&policy, "canvas.stroke"), None);
    }

    #[test]
    fn net_deny_blocks_only_net_tools() {
        let policy = AgentPolicy {
            net: AgentNetPolicy::Deny,
            ..Default::default()
        };
        for tool in NET_TOOLS {
            assert!(
                policy_deny(&policy, tool).is_some(),
                "{tool} devrait être refusé"
            );
        }
        assert_eq!(policy_deny(&policy, "fs.read"), None);
        assert_eq!(policy_deny(&policy, "notes.create"), None);
    }

    #[test]
    fn fs_write_deny_and_allowlist_compose() {
        let policy = AgentPolicy {
            fs_write: false,
            tool_allowlist: Some(vec!["fs.read".into(), "fs.write".into()]),
            ..Default::default()
        };
        // L'allowlist laisse passer fs.write, mais le verrou fs_write prime.
        assert!(policy_deny(&policy, "fs.write").is_some());
        assert_eq!(policy_deny(&policy, "fs.read"), None);
        assert!(policy_deny(&policy, "web.search").is_some());
    }
}
