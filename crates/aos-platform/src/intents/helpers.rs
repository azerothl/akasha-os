//! Shared intent helpers for aos-platformd.

use crate::subsystem::PlatformSubsystem;

/// Résout les caps FS : si l'enveloppe porte des `cap://kernel/<id>`,
/// le noyau `aos-capkd` est le seul juge (fail-closed). Sinon, caps
/// logiques P1-P3 du payload.
pub async fn resolve_fs_caps(
    s: &PlatformSubsystem,
    envelope: &[String],
    holder: &str,
    path: &str,
    kernel_right: &str,
    string_kind: &str,
    string_caps: Vec<String>,
) -> Result<Vec<String>, String> {
    match s
        .authorize_kernel(
            envelope,
            holder,
            &format!("fs:{path}"),
            &[kernel_right.to_string()],
        )
        .await
    {
        Some(Ok(())) => Ok(vec![format!("{string_kind}:{path}")]),
        Some(Err(e)) => Err(e),
        None => Ok(string_caps),
    }
}
