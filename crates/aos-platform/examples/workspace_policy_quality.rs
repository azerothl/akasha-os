//! Synthetic, non-sensitive regression corpus. Never prints source or matched values.
use aos_platform::workspace_policy::secret_findings;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    category: String,
    text: String,
    sensitive: bool,
    expected_detected: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let check = std::env::args().skip(1).any(|a| a == "--check");
    let bytes = include_bytes!("../../../fixtures/workspace-policy-quality.json");
    let cases: Vec<Case> = serde_json::from_slice(bytes)?;
    let mut ids = BTreeSet::new();
    let (mut tp, mut fp, mut tn, mut fn_) = (0, 0, 0, 0);
    let mut regressions = Vec::new();
    let mut results = Vec::new();
    for case in cases {
        if !ids.insert(case.id.clone()) || case.id.is_empty() || case.text.len() > 65536 {
            return Err("invalid corpus identity or size".into());
        }
        let findings = secret_findings(&case.text);
        let detected = !findings.is_empty();
        match (case.sensitive, detected) {
            (true, true) => tp += 1,
            (true, false) => fn_ += 1,
            (false, true) => fp += 1,
            (false, false) => tn += 1,
        }
        if detected != case.expected_detected {
            regressions.push(case.id.clone());
        }
        results.push(json!({"id":case.id,"category":case.category,
            "sensitive":case.sensitive,"detected":detected,"findings":findings}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "corpus_sha256":format!("{:x}", Sha256::digest(bytes)),
            "synthetic_only":true,"cases":results.len(),
            "true_positive":tp,"false_positive":fp,"true_negative":tn,"false_negative":fn_,
            "precision":if tp+fp == 0 {None} else {Some(tp as f64/(tp+fp) as f64)},
            "recall":if tp+fn_ == 0 {None} else {Some(tp as f64/(tp+fn_) as f64)},
            "regressions":regressions,"results":results
        }))?
    );
    if check && !regressions.is_empty() {
        return Err("synthetic corpus expectations changed".into());
    }
    Ok(())
}
