//! Background indexing on platformd boot — must not block bus `serve`.

use crate::subsystem::PlatformSubsystem;
use std::path::Path;
use std::sync::Arc;

/// Spawn product-doc and user-library indexing without blocking the caller.
///
/// Intended to run after bus handlers are registered and before or alongside
/// `BusService::serve`, so session healthcheck can reach `module.list` while
/// embeddings run.
pub fn spawn_background_indexing(
    sub: Arc<PlatformSubsystem>,
    memory_dir: String,
    version: String,
) {
    tokio::spawn(async move {
        let s = sub.clone();
        let version = version.clone();
        match tokio::task::spawn_blocking(move || {
            crate::product_rag::ensure_indexed(&s, &version)
        })
        .await
        {
            Ok(Ok(n)) => eprintln!("[aos-platformd] product RAG : {n} chunks indexés"),
            Ok(Err(e)) => eprintln!("[aos-platformd] product RAG skip : {e}"),
            Err(e) => eprintln!("[aos-platformd] product RAG panic : {e}"),
        }

        let s = sub.clone();
        match tokio::task::spawn_blocking(move || {
            crate::user_docs::ensure_indexed(&s, Path::new(&memory_dir))
        })
        .await
        {
            Ok(n) => eprintln!("[aos-platformd] user library : {n} chunks indexés"),
            Err(e) => eprintln!("[aos-platformd] user library panic : {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsystem::PlatformConfig;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn temp_path(label: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "aos-boot-index-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&p);
        p.display().to_string()
    }

    fn test_config() -> PlatformConfig {
        PlatformConfig {
            bus: "ipc://test".into(),
            audit_dir: temp_path("audit"),
            storage_dir: temp_path("storage"),
            memory_dir: temp_path("memory"),
            modules_dir: temp_path("modules"),
            catalogue_file: "/dev/null".into(),
            skills_dir: temp_path("skills"),
            sessions_dir: temp_path("sessions"),
            embed_model: None,
            policies_file: None,
            confirm_timeout_sec: 60,
            secrets_file: PathBuf::from(temp_path("secrets"))
                .join("secrets")
                .display()
                .to_string(),
            net_mode: "online".into(),
        }
    }

    #[tokio::test]
    async fn spawn_background_indexing_does_not_block() {
        let sub = PlatformSubsystem::open(&test_config()).expect("platform open");
        let memory_dir = test_config().memory_dir;
        let start = Instant::now();
        spawn_background_indexing(sub, memory_dir, "test-0.0.0".into());
        assert!(
            start.elapsed() < Duration::from_millis(200),
            "background indexing spawn blocked for {:?}",
            start.elapsed()
        );
    }
}
