//! Download/cache compatibility tests for #445. Fetch is deterministic; archive
//! extraction, hashes, manifests and filesystem activation are the real code.
use super::*;
use std::io::Write;

fn package(version: &str, files: &[(&str, &[u8])]) -> (CatalogueEntry, Vec<u8>) {
    let wasm = b"\0asm\x01\0\0\0";
    let manifest = format!(
        "name: fixture\nversion: {version}\nhash: {}\nmin_os_api: 1\n",
        sha256_hex(wasm)
    );
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in [
        ("manifest.yaml", manifest.as_bytes()),
        ("module.wasm", wasm.as_slice()),
    ]
    .into_iter()
    .chain(files.iter().copied())
    {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    let data = writer.finish().unwrap().into_inner();
    let entry=serde_json::from_value(serde_json::json!({"name":"fixture","version":version,"kind":"module",
        "path":"share/modules/fixture.aospkg","hash":format!("sha256:{}",sha256_hex(wasm)),"minimum_host_api":1,
        "platforms":[host_platform_id()],"package":{"url":"https://github.com/azerothl/akasha-os/releases/download/v-test/fixture.zip",
        "sha256":sha256_hex(&data),"size_bytes":data.len(),"input_sha256":"a".repeat(64)}})).unwrap();
    (entry, data)
}

fn temp() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "aos-pack-install-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
fn cleanup(path: &Path) {
    assert!(path.starts_with(std::env::temp_dir()));
    assert!(path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("aos-pack-install-"));
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn downloaded_pack_is_validated_and_usable_after_cache_restart() {
    let home = temp();
    let extra = ExtraCatalogueSource::open(&home, "cache");
    let (entry, data) = package("1.0.0", &[]);
    let dir =
        resolve_downloaded_module_archive(&entry, entry.package.as_ref().unwrap(), &extra, &|_| {
            Ok(data.clone())
        })
        .unwrap();
    assert!(dir.join("manifest.yaml").is_file());
    let reopened = ExtraCatalogueSource::open(&home, "cache");
    let again = resolve_downloaded_module_archive(
        &entry,
        entry.package.as_ref().unwrap(),
        &reopened,
        &|_| panic!("valid cached archive should work offline"),
    )
    .unwrap();
    assert_eq!(dir, again);
    cleanup(&home);
}

#[test]
fn invalid_upgrade_does_not_replace_the_previous_valid_pack_or_user_files() {
    let home = temp();
    let extra = ExtraCatalogueSource::open(&home, "cache");
    let (entry, data) = package("1.0.0", &[]);
    let dir =
        resolve_downloaded_module_archive(&entry, entry.package.as_ref().unwrap(), &extra, &|_| {
            Ok(data.clone())
        })
        .unwrap();
    let user = home.join("user-document.md");
    std::fs::write(&user, "retained").unwrap();
    let (new, bad) = package("2.0.0", &[("../outside", b"invalid")]);
    assert!(resolve_downloaded_module_archive(
        &new,
        new.package.as_ref().unwrap(),
        &extra,
        &|_| Ok(bad.clone())
    )
    .is_err());
    assert!(dir.join("module.wasm").is_file());
    assert_eq!(std::fs::read_to_string(&user).unwrap(), "retained");
    assert!(!home.join("outside").exists());
    cleanup(&home);
}

#[test]
fn hash_size_platform_and_api_mismatches_refuse_before_activation() {
    let home = temp();
    let extra = ExtraCatalogueSource::open(&home, "cache");
    let (entry, data) = package("1.0.0", &[]);
    let mut wrong = entry.clone();
    wrong.minimum_host_api = aos_proto::OS_API_VERSION + 1;
    assert!(validate_package_artifact_metadata(&wrong, wrong.package.as_ref().unwrap()).is_err());
    wrong = entry.clone();
    wrong.platforms = vec!["unsupported-platform".into()];
    assert!(validate_package_artifact_metadata(&wrong, wrong.package.as_ref().unwrap()).is_err());
    let mut artifact = entry.package.clone().unwrap();
    artifact.size_bytes += 1;
    assert!(
        resolve_downloaded_module_archive(&entry, &artifact, &extra, &|_| Ok(data.clone()))
            .is_err()
    );
    artifact = entry.package.clone().unwrap();
    artifact.sha256 = "0".repeat(64);
    assert!(
        resolve_downloaded_module_archive(&entry, &artifact, &extra, &|_| Ok(data.clone()))
            .is_err()
    );
    cleanup(&home);
}
