//! Host-managed Illustration Studio asset import and selected Poly Haven downloads.

use crate::cmd::Evt;
use md5::{Digest as _, Md5};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

const MAX_ASSET_BYTES: u64 = 200_000_000;
const MAX_CATALOGUE_BYTES: u64 = 100_000_000;

#[derive(Clone, Copy)]
struct Curated {
    id: &'static str,
    name: &'static str,
    category: &'static str,
    tags: &'static [&'static str],
    dimensions_m: [f32; 3],
    author: &'static str,
    thumbnail: &'static str,
}

const CURATED: &[Curated] = &[
    Curated {
        id: "WoodenChair_01",
        name: "Wooden Chair 01",
        category: "Furniture / chairs",
        tags: &["chair", "wood", "vintage"],
        dimensions_m: [0.688, 0.658, 2.274],
        author: "Jake Mobley",
        thumbnail: "/assets/illustration/catalogue/thumbs/WoodenChair_01.png",
    },
    Curated {
        id: "SchoolDesk_01",
        name: "School Desk 01",
        category: "Furniture / desks",
        tags: &["desk", "wood", "classroom"],
        dimensions_m: [0.712, 0.546, 0.883],
        author: "Ethan Place",
        thumbnail: "/assets/illustration/catalogue/thumbs/SchoolDesk_01.png",
    },
    Curated {
        id: "binder_notebook",
        name: "Binder Notebook",
        category: "Office / notebooks",
        tags: &["notebook", "leather", "paper"],
        dimensions_m: [0.584, 0.200, 0.025],
        author: "DaDrood",
        thumbnail: "/assets/illustration/catalogue/thumbs/binder_notebook.png",
    },
];

#[derive(Deserialize, Serialize)]
struct AssetMetadata {
    id: String,
    name: String,
    category: String,
    tags: Vec<String>,
    dimensions_m: [f32; 3],
    pivot_m: [f32; 3],
    orientation: String,
    thumbnail: Option<String>,
    provenance: String,
    license: String,
    author: Option<String>,
    source_url: Option<String>,
}

pub(crate) fn dispatch(
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    input: &Value,
    refresh_binds: Vec<String>,
) {
    if module != "illustration-studio" {
        send_done(
            evt_tx,
            module,
            action_id,
            Err("Asset import reserved for Illustration Studio".into()),
            vec![],
        );
        return;
    }
    let tx = evt_tx.clone();
    let module = module.to_owned();
    let action_id = action_id.to_owned();
    let input = input.clone();
    std::thread::spawn(move || {
        let result = import_and_insert(&input);
        send_done(&tx, &module, &action_id, result, refresh_binds);
    });
}

fn send_done(
    tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    result: Result<Value, String>,
    refresh_binds: Vec<String>,
) {
    let (ok, result, error) = match result {
        Ok(value) => (true, value, None),
        Err(message) => (false, Value::Null, Some(message)),
    };
    let _ = tx.send(Evt::ModuleUiServiceDone {
        module: module.into(),
        action_id: action_id.into(),
        ok,
        result,
        error,
        refresh_binds: if ok { refresh_binds } else { vec![] },
    });
}

fn import_and_insert(input: &Value) -> Result<Value, String> {
    let (uri, metadata) = import_uri_metadata(input)?;
    let mut scene = match input.get("scene_yaml").and_then(Value::as_str) {
        Some(yaml) if !yaml.trim().is_empty() => {
            aos_scene::load_project_yaml(yaml)
                .map_err(|e| format!("Scene project: {e}"))?
                .scene
        }
        _ => aos_scene::SceneGraph::demo_scene(),
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let id = format!("asset_{stamp}");
    let name = metadata.name.to_string();
    aos_scene::insert_mesh_asset(
        &mut scene,
        "root",
        &id,
        name,
        &uri,
        aos_scene::Transform::default(),
    )
    .map_err(|e| format!("Add to scene: {e}"))?;
    let scene_yaml = aos_scene::save_project_yaml(&aos_scene::ProjectFile::new(scene))
        .map_err(|e| e.to_string())?;
    Ok(json!({"scene_yaml": scene_yaml, "root_id": id, "mesh_uri": uri, "metadata": metadata}))
}

pub(crate) fn import_to_library(input: &Value) -> Result<Value, String> {
    let (uri, metadata) = import_uri_metadata(input)?;
    Ok(json!({ "uri": uri, "metadata": metadata }))
}

fn import_uri_metadata(input: &Value) -> Result<(String, AssetMetadata), String> {
    let mode = input.get("mode").and_then(Value::as_str).unwrap_or("");
    match mode {
        "file" => import_local_glb(input.get("path").and_then(Value::as_str).unwrap_or("")),
        "polyhaven" => download_curated(
            input
                .get("catalogue_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
        ),
        _ => Err("Unknown asset source".into()),
    }
}

fn storage_root() -> PathBuf {
    crate::os_open::aos_home().join("var/storage/data/documents/illustrations/assets")
}

fn import_local_glb(source: &str) -> Result<(String, AssetMetadata), String> {
    import_local_glb_into(source, &storage_root())
}

fn import_local_glb_into(source: &str, root: &Path) -> Result<(String, AssetMetadata), String> {
    let path = Path::new(source);
    if !path.is_file()
        || !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("glb"))
    {
        return Err("Select a local .glb file".into());
    }
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_ASSET_BYTES {
        return Err("GLB exceeds the 200 MB import limit".into());
    }
    let mesh = aos_scene::load_gltf_mesh(path).map_err(|e| format!("Invalid GLB: {e}"))?;
    let mut hasher = Sha256::new();
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let digest = format!("{:x}", hasher.finalize());
    let short = &digest[..16];
    let directory = root.join("imported");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let dest = directory.join(format!("{short}.glb"));
    if !dest.is_file() {
        fs::copy(path, &dest).map_err(|e| format!("Copy GLB: {e}"))?;
    }
    let uri = format!("/documents/illustrations/assets/imported/{short}.glb");
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported GLB")
        .to_owned();
    let center = mesh.aabb_center();
    let extent = mesh.aabb_half_extents();
    let metadata = AssetMetadata {
        id: format!("local_{short}"),
        name,
        category: "Local import".into(),
        tags: vec![],
        dimensions_m: [extent.x * 2.0, extent.y * 2.0, extent.z * 2.0],
        pivot_m: [center.x, center.y, center.z],
        orientation: "Y-up".into(),
        thumbnail: None,
        provenance: "User-supplied GLB".into(),
        license: "User supplied / unspecified".into(),
        author: None,
        source_url: None,
    };
    let meta_path = directory.join(format!("{short}.metadata.json"));
    fs::write(
        meta_path,
        serde_json::to_vec_pretty(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok((uri, metadata))
}

fn download_curated(id: &str) -> Result<(String, AssetMetadata), String> {
    let curated = CURATED
        .iter()
        .find(|asset| asset.id == id)
        .ok_or_else(|| "Unknown catalogue asset".to_string())?;
    let directory = storage_root().join("catalogue").join(id).join("1k");
    let uri = format!("/documents/illustrations/assets/catalogue/{id}/1k/source.gltf");
    if let Ok(raw) = fs::read(directory.join("metadata.json")) {
        if let Ok(metadata) = serde_json::from_slice::<AssetMetadata>(&raw) {
            if aos_scene::load_gltf_mesh(&directory.join("source.gltf")).is_ok() {
                return Ok((uri, metadata));
            }
        }
    }
    let client = Client::builder()
        .user_agent("AkashaOS-IllustrationStudio-AssetLibrary/0.7.10")
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let manifest: Value = client
        .get(format!("https://api.polyhaven.com/files/{id}"))
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|e| format!("Poly Haven API: {e}"))?
        .json()
        .map_err(|e| e.to_string())?;
    let gltf = &manifest["gltf"]["1k"]["gltf"];
    let source = gltf.as_object().ok_or("Poly Haven 1k glTF unavailable")?;
    let include = source
        .get("include")
        .and_then(Value::as_object)
        .ok_or("Poly Haven dependencies missing")?;
    if include.len() > 24 {
        return Err("Too many Poly Haven dependencies".into());
    }
    let mut entries = vec![("source.gltf".to_owned(), gltf)];
    entries.extend(include.iter().map(|(name, value)| (name.clone(), value)));
    let total: u64 = entries
        .iter()
        .map(|(_, v)| {
            v.get("size")
                .and_then(Value::as_u64)
                .unwrap_or(MAX_CATALOGUE_BYTES + 1)
        })
        .sum();
    if total > MAX_CATALOGUE_BYTES {
        return Err("Poly Haven asset exceeds 100 MB".into());
    }
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    for (name, entry) in &entries {
        let relative = Path::new(name);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("Unsafe Poly Haven file path".into());
        }
        let url = entry
            .get("url")
            .and_then(Value::as_str)
            .ok_or("Poly Haven URL missing")?;
        if !url.starts_with("https://dl.polyhaven.org/file/ph-assets/") {
            return Err("Unexpected Poly Haven download host".into());
        }
        let bytes = entry
            .get("size")
            .and_then(Value::as_u64)
            .ok_or("Poly Haven size missing")?;
        let md5 = entry
            .get("md5")
            .and_then(Value::as_str)
            .ok_or("Poly Haven checksum missing")?;
        download_verified(&client, url, &directory.join(relative), bytes, md5)?;
    }
    let path = directory.join("source.gltf");
    let mesh =
        aos_scene::load_gltf_mesh(&path).map_err(|e| format!("Downloaded model invalid: {e}"))?;
    let center = mesh.aabb_center();
    let metadata = AssetMetadata {
        id: curated.id.into(),
        name: curated.name.into(),
        category: curated.category.into(),
        tags: curated.tags.iter().map(|s| (*s).into()).collect(),
        dimensions_m: curated.dimensions_m,
        pivot_m: [center.x, center.y, center.z],
        orientation: "Y-up; source faces -Y".into(),
        thumbnail: Some(curated.thumbnail.into()),
        provenance: "Poly Haven".into(),
        license: "CC0 1.0".into(),
        author: Some(curated.author.into()),
        source_url: Some(format!("https://polyhaven.com/a/{id}")),
    };
    fs::write(
        directory.join("metadata.json"),
        serde_json::to_vec_pretty(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok((uri, metadata))
}

fn download_verified(
    client: &Client,
    url: &str,
    dest: &Path,
    expected_bytes: u64,
    expected_md5: &str,
) -> Result<(), String> {
    if dest.is_file() && verify_md5(dest, expected_bytes, expected_md5)? {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let partial = dest.with_extension("partial");
    let mut response = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Poly Haven download: {e}"))?;
    let mut out = File::create(&partial).map_err(|e| e.to_string())?;
    let mut received = 0u64;
    let mut buffer = [0u8; 256 * 1024];
    loop {
        let count = response.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        received += count as u64;
        if received > expected_bytes {
            return Err("Poly Haven file exceeds declared size".into());
        }
        out.write_all(&buffer[..count]).map_err(|e| e.to_string())?;
    }
    drop(out);
    if !verify_md5(&partial, expected_bytes, expected_md5)? {
        let _ = fs::remove_file(&partial);
        return Err("Poly Haven checksum mismatch".into());
    }
    if dest.exists() {
        fs::remove_file(dest).map_err(|e| e.to_string())?;
    }
    fs::rename(partial, dest).map_err(|e| e.to_string())
}

fn verify_md5(path: &Path, expected_bytes: u64, expected_md5: &str) -> Result<bool, String> {
    if expected_md5.len() != 32 || !expected_md5.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid Poly Haven checksum".into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != expected_bytes {
        return Ok(false);
    }
    let mut hash = Md5::new();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()).eq_ignore_ascii_case(expected_md5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_list_has_unique_ids_and_attribution() {
        let mut ids = std::collections::HashSet::new();
        for item in CURATED {
            assert!(ids.insert(item.id));
            assert!(!item.author.is_empty());
            assert!(item
                .thumbnail
                .starts_with("/assets/illustration/catalogue/thumbs/"));
            let thumb = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../share/assets/illustration/catalogue/thumbs")
                .join(format!("{}.png", item.id));
            assert!(thumb.is_file(), "missing curated thumbnail: {}", item.id);
        }
    }

    #[test]
    fn local_glb_import_writes_metadata_and_preserves_dimensions() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/aos-scene/tests/fixtures/hierarchy_textured.glb");
        let root =
            std::env::temp_dir().join(format!("aos-local-asset-test-{}", std::process::id()));
        let (uri, metadata) = import_local_glb_into(&fixture.to_string_lossy(), &root).unwrap();
        assert!(uri.ends_with(".glb"));
        assert_eq!(metadata.license, "User supplied / unspecified");
        assert!((metadata.dimensions_m[0] - 1.0).abs() < 1e-4);
        assert!(root.join("imported").read_dir().unwrap().count() >= 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn md5_verification_rejects_wrong_size_and_hash() {
        let path = std::env::temp_dir().join(format!("aos-polyhaven-test-{}", std::process::id()));
        fs::write(&path, b"abc").unwrap();
        assert!(verify_md5(&path, 3, "900150983cd24fb0d6963f7d28e17f72").unwrap());
        assert!(!verify_md5(&path, 4, "900150983cd24fb0d6963f7d28e17f72").unwrap());
        assert!(!verify_md5(&path, 3, "00000000000000000000000000000000").unwrap());
        fs::remove_file(path).unwrap();
    }

    #[test]
    #[ignore = "downloads a real CC0 Poly Haven model"]
    fn polyhaven_model_download_has_metadata_and_textures() {
        let (uri, metadata) = download_curated("SchoolDesk_01").expect("download Poly Haven model");
        assert_eq!(metadata.license, "CC0 1.0");
        assert_eq!(metadata.provenance, "Poly Haven");
        assert!(metadata.dimensions_m.iter().all(|d| *d > 0.0));
        assert!(uri.ends_with("SchoolDesk_01/1k/source.gltf"));
        assert!(storage_root()
            .join("catalogue/SchoolDesk_01/1k/metadata.json")
            .is_file());
    }
}
