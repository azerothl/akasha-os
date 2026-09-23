//! Fixed-source, user-initiated Blender and TRELLIS installation for Illustration Studio.
//! The WASM guest can request a known dependency, but cannot choose a URL or path.

use crate::cmd::Evt;
use reqwest::blocking::Client;
use reqwest::header::RANGE;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const BLENDER_VERSION: &str = "4.5.14";
const TRELLIS_VERSION: &str = "v0.6.0";
const HF_REVISION: &str = "a57397bd3d351599d9729fc144b3f87c3f87d65b";

struct Artifact {
    name: String,
    url: String,
    bytes: u64,
    sha256: &'static str,
}

#[derive(Clone, Copy)]
struct Weight {
    name: &'static str,
    bytes: u64,
    sha256: &'static str,
}

const Q4: &[Weight] = &[
    Weight {
        name: "birefnet.gguf",
        bytes: 882_749_024,
        sha256: "10c5dd4dcac904cf81c9a16180eb66f167dd52ab55867f54a44567b0b2babbc1",
    },
    Weight {
        name: "dinov3.gguf",
        bytes: 172_662_976,
        sha256: "6473cf96fd275bf84f5cc0556975a2abaa10b641e4a07101dcad561df1917ef2",
    },
    Weight {
        name: "shape_dec.gguf",
        bytes: 845_423_552,
        sha256: "79a52ddfed3454f724683940c11cfbfcf76c427ab3b7fdeb4c3cfc6d26647de4",
    },
    Weight {
        name: "shape_flow_1024.gguf",
        bytes: 730_520_576,
        sha256: "54e49e3408b9f77bdc85c3f5a400a58a1d9fc091e111986eda7caeaea621b305",
    },
    Weight {
        name: "shape_flow_512.gguf",
        bytes: 730_520_576,
        sha256: "de7b87a92280035258c94314258e3b8314de39e5f001eac1ada0b4087785fb63",
    },
    Weight {
        name: "ss_dec.gguf",
        bytes: 147_379_392,
        sha256: "2790b5eecb261cc877d9bf175ce2bd6dd48cd65be8c042c5f5bc023dfca01cf7",
    },
    Weight {
        name: "ss_flow.gguf",
        bytes: 730_496_672,
        sha256: "a43c6393ee4a763a03e382de750d4f62752bacf969940442fd856230e62b88c2",
    },
    Weight {
        name: "tex_dec.gguf",
        bytes: 845_414_272,
        sha256: "20b208e402db907c6800dd4ad486a0d6e927e4511a6ff87fb51049794988927b",
    },
    Weight {
        name: "tex_flow_1024.gguf",
        bytes: 730_548_224,
        sha256: "028d3be82075f8a4d8b4bd08985e1215ac3783ed332dbf7e8fd6535fcdd3d5a8",
    },
    Weight {
        name: "tex_flow_512.gguf",
        bytes: 730_548_224,
        sha256: "fff7bca6418ad607b8b9ff24034ff63f591d0a9b72f308960bffd6f5cd17390b",
    },
];

const Q8: &[Weight] = &[
    Weight {
        name: "birefnet.gguf",
        bytes: 882_749_024,
        sha256: "10c5dd4dcac904cf81c9a16180eb66f167dd52ab55867f54a44567b0b2babbc1",
    },
    Weight {
        name: "dinov3.gguf",
        bytes: 323_657_920,
        sha256: "0dd4ffd4b46a248f5b7d49c35275d68461fbf73f57ddb4c1fa8afb4f7bb45a0d",
    },
    Weight {
        name: "shape_dec.gguf",
        bytes: 881_361_568,
        sha256: "0de7c7a675022dd8696d526a9279e5a50b2a35c8a79452f434a72dc53d40f169",
    },
    Weight {
        name: "shape_flow_1024.gguf",
        bytes: 1_376_125_952,
        sha256: "997e9fc10ab95fda11c4cd1cbaf425101980ac36a2ef80e9c83e0b9c4bfc9680",
    },
    Weight {
        name: "shape_flow_512.gguf",
        bytes: 1_376_125_952,
        sha256: "29b639f4ff22ded8f91b619376a835f64b9874b0ebcadb9ac6c305195bf5d1f9",
    },
    Weight {
        name: "ss_dec.gguf",
        bytes: 147_379_392,
        sha256: "2790b5eecb261cc877d9bf175ce2bd6dd48cd65be8c042c5f5bc023dfca01cf7",
    },
    Weight {
        name: "ss_flow.gguf",
        bytes: 1_376_059_040,
        sha256: "ea6d8a42b20661a5c6a52e5ffbdc1df7aca9b212792193873b418c69f871422c",
    },
    Weight {
        name: "tex_dec.gguf",
        bytes: 881_344_576,
        sha256: "88b4fced46455e02f316664d5c43584a311921dd9a1cdc1b7b7d981cca9214d4",
    },
    Weight {
        name: "tex_flow_1024.gguf",
        bytes: 1_376_178_176,
        sha256: "cb2b3aee74ba09c018f918ed8c146bc7e4f96335b61fa0d1fc1ff1a7811e6da",
    },
    Weight {
        name: "tex_flow_512.gguf",
        bytes: 1_376_178_176,
        sha256: "389a2cbdda59d53b21e5989650d9d36b7ac603266eaef06712cd07a9fc377210",
    },
];

static ACTIVE: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();

fn active() -> &'static Mutex<Option<Arc<AtomicBool>>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}

pub(crate) fn dispatch(
    evt_tx: &std::sync::mpsc::Sender<Evt>,
    module: &str,
    action_id: &str,
    input: &Value,
    refresh_binds: Vec<String>,
) {
    let kind = input.get("kind").and_then(Value::as_str).unwrap_or("");
    if module != "illustration-studio" {
        done(
            evt_tx,
            module,
            action_id,
            Err("Installer réservé à Illustration Studio".into()),
            vec![],
        );
        return;
    }
    if kind == "cancel" {
        let guard = active().lock().unwrap_or_else(|e| e.into_inner());
        let found = guard.as_ref().is_some_and(|flag| {
            flag.store(true, Ordering::Relaxed);
            true
        });
        drop(guard);
        done(
            evt_tx,
            module,
            action_id,
            if found {
                Ok(json!({"cancelling": true}))
            } else {
                Err("Aucun téléchargement actif".into())
            },
            vec![],
        );
        return;
    }
    let quant = input.get("quant").and_then(Value::as_str).unwrap_or("q4");
    if kind != "blender" && kind != "trellis" || kind == "trellis" && quant != "q4" && quant != "q8"
    {
        done(
            evt_tx,
            module,
            action_id,
            Err("Dépendance ou variante inconnue".into()),
            vec![],
        );
        return;
    }
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut guard = active().lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_some() {
            drop(guard);
            done(
                evt_tx,
                module,
                action_id,
                Err("Un téléchargement est déjà en cours".into()),
                vec![],
            );
            return;
        }
        *guard = Some(flag.clone());
    }
    // A long download must not keep DeclUI's global pending state locked: the
    // cancellation action and the rest of the editor remain usable.
    done(
        evt_tx,
        module,
        action_id,
        Ok(json!({"started": true})),
        vec![],
    );
    progress(evt_tx, module, "Téléchargement lancé…".into(), true);
    let tx = evt_tx.clone();
    let module = module.to_owned();
    let action_id = action_id.to_owned();
    let quant = quant.to_owned();
    let kind = kind.to_owned();
    std::thread::spawn(move || {
        let result = install(&tx, &module, &kind, &quant, &flag);
        *active().lock().unwrap_or_else(|e| e.into_inner()) = None;
        let summary = match &result {
            Ok(_) => {
                if kind == "blender" {
                    "Blender est prêt.".to_owned()
                } else {
                    format!("TRELLIS {quant} est prêt.")
                }
            }
            Err(e) => e.clone(),
        };
        done(
            &tx,
            &module,
            &action_id,
            result.map(|p| json!({"path": p})),
            refresh_binds,
        );
        progress(&tx, &module, summary, false);
    });
}

fn done(
    tx: &std::sync::mpsc::Sender<Evt>,
    module: &str,
    action_id: &str,
    result: Result<Value, String>,
    refresh_binds: Vec<String>,
) {
    let (ok, value, error) = match result {
        Ok(v) => (true, v, None),
        Err(e) => (false, Value::Null, Some(e)),
    };
    let _ = tx.send(Evt::ModuleUiServiceDone {
        module: module.into(),
        action_id: action_id.into(),
        ok,
        result: value,
        error,
        refresh_binds: if ok { refresh_binds } else { vec![] },
    });
}

fn progress(tx: &std::sync::mpsc::Sender<Evt>, module: &str, message: String, active: bool) {
    let _ = tx.send(Evt::ModuleUiServiceProgress {
        module: module.into(),
        message,
        active,
    });
}

fn install(
    tx: &std::sync::mpsc::Sender<Evt>,
    module: &str,
    kind: &str,
    quant: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let root = crate::os_open::aos_home().join("var/illustration-studio/integrations");
    fs::create_dir_all(&root).map_err(|e| format!("Création du dossier d'intégration : {e}"))?;
    let client = Client::builder()
        .user_agent("AkashaOS-IllustrationStudio/0.7.8")
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Client de téléchargement : {e}"))?;
    if kind == "blender" {
        install_blender(&client, &root, tx, module, cancel)
    } else {
        install_trellis(&client, &root, quant, tx, module, cancel)
    }
}

fn blender_artifact() -> Result<Artifact, String> {
    let (name, bytes, sha) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => (
            "blender-4.5.14-windows-x64.zip",
            398_661_046,
            "b9533d2397ac1984db4466fb23a7a4649391cca93f6e84209f9bcc60d071c8b9",
        ),
        ("windows", "aarch64") => (
            "blender-4.5.14-windows-arm64.zip",
            254_747_440,
            "0153ecefc96a0e23985e6edcdbe04e909ba2078258f4ff6a05c21d4b7c756911",
        ),
        ("linux", "x86_64") => (
            "blender-4.5.14-linux-x64.tar.xz",
            378_045_212,
            "9ba871ff2ecd36526b77432745980b7e6664ecd0c7ca11c48849073dcfe06da3",
        ),
        ("macos", "aarch64") => (
            "blender-4.5.14-macos-arm64.dmg",
            311_909_763,
            "65134d9b07b20e2fa8d3c9e44f6f44ffb5c9774dd521b95f50387310241ca170",
        ),
        ("macos", "x86_64") => (
            "blender-4.5.14-macos-x64.dmg",
            340_090_796,
            "613e73339e97bd113adeeb28d9155beeacf2df7119ee4dbd6bdce234843645f6",
        ),
        _ => return Err("Blender automatique indisponible sur cette architecture".into()),
    };
    Ok(Artifact {
        name: name.into(),
        url: format!("https://download.blender.org/release/Blender4.5/{name}"),
        bytes,
        sha256: sha,
    })
}

fn trellis_artifact() -> Result<Artifact, String> {
    let (name, bytes, sha) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => ("trellis-vulkan-windows-x64.zip", 22_782_550, "bfe437e4c222b37141b3bcb0600eabee6f39823a5fafa0b3913f50f575e424d0"),
        ("linux", "x86_64") => ("trellis-vulkan-linux-x64.tar.gz", 25_341_680, "6bc453c1e3a94a4b7bda15cd7f93fdf05f240869c44bf2b102a5db4a5fde19b6"),
        ("macos", _) => return Err("TRELLIS.cpp ne publie pas de binaire macOS. Configurez un runner installé manuellement via AOS_NEURAL_MESH_BIN.".into()),
        _ => return Err("TRELLIS automatique indisponible sur cette architecture".into()),
    };
    Ok(Artifact {
        name: name.into(),
        url: format!(
            "https://github.com/pwilkin/trellis.cpp/releases/download/{TRELLIS_VERSION}/{name}"
        ),
        bytes,
        sha256: sha,
    })
}

fn install_blender(
    client: &Client,
    root: &Path,
    tx: &std::sync::mpsc::Sender<Evt>,
    module: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let artifact = blender_artifact()?;
    let archive = download(client, root, &artifact, tx, module, cancel, "Blender")?;
    let dest = root.join("blender").join(BLENDER_VERSION);
    if find_binary(&dest, blender_binary(), 5).is_some() {
        return Ok(dest.display().to_string());
    }
    let stage = root
        .join("blender")
        .join(format!("{BLENDER_VERSION}.staging"));
    reset_stage(&stage)?;
    progress(tx, module, "Extraction de Blender…".into(), true);
    extract_archive(&archive, &stage, cancel)?;
    if find_binary(&stage, blender_binary(), 5).is_none() {
        return Err("Archive Blender vérifiée, mais binaire introuvable".into());
    }
    replace_stage(&stage, &dest)?;
    Ok(dest.display().to_string())
}

fn install_trellis(
    client: &Client,
    root: &Path,
    quant: &str,
    tx: &std::sync::mpsc::Sender<Evt>,
    module: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let artifact = trellis_artifact()?;
    let archive = download(
        client,
        root,
        &artifact,
        tx,
        module,
        cancel,
        "TRELLIS runtime",
    )?;
    let dest = root.join("trellis/runtime").join(TRELLIS_VERSION);
    if find_binary(&dest, trellis_binary(), 5).is_none() {
        let stage = root
            .join("trellis/runtime")
            .join(format!("{TRELLIS_VERSION}.staging"));
        reset_stage(&stage)?;
        progress(tx, module, "Extraction de TRELLIS…".into(), true);
        extract_archive(&archive, &stage, cancel)?;
        if find_binary(&stage, trellis_binary(), 5).is_none() {
            return Err("Archive TRELLIS vérifiée, mais trellis-cli introuvable".into());
        }
        replace_stage(&stage, &dest)?;
    }
    let weights = if quant == "q4" { Q4 } else { Q8 };
    let weights_dir = root.join("trellis/weights").join(quant);
    fs::create_dir_all(&weights_dir).map_err(|e| e.to_string())?;
    let ready_marker = weights_dir.join(".aos-weights-ready");
    if ready_marker.exists() {
        fs::remove_file(&ready_marker).map_err(|e| e.to_string())?;
    }
    for (i, weight) in weights.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(
                "Téléchargement annulé. Les fichiers partiels seront repris au prochain essai."
                    .into(),
            );
        }
        let artifact = Artifact {
            name: weight.name.into(),
            url: format!("https://huggingface.co/ilintar/trellis2-gguf/resolve/{HF_REVISION}/{quant}/{}?download=true", weight.name),
            bytes: weight.bytes,
            sha256: weight.sha256,
        };
        let label = format!("TRELLIS {quant} ({}/{})", i + 1, weights.len());
        let _ = download_to(
            client,
            &artifact,
            &weights_dir.join(weight.name),
            tx,
            module,
            cancel,
            &label,
        )?;
    }
    fs::write(ready_marker, HF_REVISION).map_err(|e| e.to_string())?;
    fs::write(root.join("trellis/weights/current.txt"), quant).map_err(|e| e.to_string())?;
    Ok(weights_dir.display().to_string())
}

fn download(
    client: &Client,
    root: &Path,
    artifact: &Artifact,
    tx: &std::sync::mpsc::Sender<Evt>,
    module: &str,
    cancel: &AtomicBool,
    label: &str,
) -> Result<PathBuf, String> {
    let cache = root.join("cache");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    download_to(
        client,
        artifact,
        &cache.join(&artifact.name),
        tx,
        module,
        cancel,
        label,
    )
}

fn download_to(
    client: &Client,
    artifact: &Artifact,
    dest: &Path,
    tx: &std::sync::mpsc::Sender<Evt>,
    module: &str,
    cancel: &AtomicBool,
    label: &str,
) -> Result<PathBuf, String> {
    if dest.is_file() && verify(dest, artifact.bytes, artifact.sha256)? {
        progress(tx, module, format!("{label} : fichier déjà vérifié"), true);
        return Ok(dest.into());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let partial = dest.with_extension(format!(
        "{}partial",
        dest.extension()
            .and_then(|s| s.to_str())
            .map(|e| format!("{e}."))
            .unwrap_or_default()
    ));
    let mut offset = fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
    if offset > artifact.bytes {
        fs::remove_file(&partial).map_err(|e| e.to_string())?;
        offset = 0;
    }
    if offset == artifact.bytes {
        if verify(&partial, artifact.bytes, artifact.sha256)? {
            if dest.exists() {
                fs::remove_file(dest).map_err(|e| e.to_string())?;
            }
            fs::rename(&partial, dest).map_err(|e| e.to_string())?;
            return Ok(dest.into());
        }
        fs::remove_file(&partial).map_err(|e| e.to_string())?;
        offset = 0;
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Téléchargement annulé. La reprise est disponible au prochain essai.".into());
    }
    let mut request = client.get(&artifact.url);
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
    }
    let mut response = request
        .send()
        .map_err(|e| format!("{label} : connexion : {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{label} : serveur HTTP {}", response.status()));
    }
    let append = offset > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if !append {
        offset = 0;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&partial)
        .map_err(|e| e.to_string())?;
    let mut buffer = [0u8; 256 * 1024];
    let mut last_update = Instant::now() - Duration::from_secs(1);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(
                "Téléchargement annulé. La reprise est disponible au prochain essai.".into(),
            );
        }
        let count = response
            .read(&mut buffer)
            .map_err(|e| format!("{label} : lecture interrompue : {e}"))?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])
            .map_err(|e| format!("{label} : disque : {e}"))?;
        offset += count as u64;
        if offset > artifact.bytes {
            return Err(format!("{label} : taille supérieure au manifeste vérifié"));
        }
        if last_update.elapsed() >= Duration::from_millis(500) {
            progress(
                tx,
                module,
                format!(
                    "{label} : {:.1} / {:.1} Go",
                    offset as f64 / 1e9,
                    artifact.bytes as f64 / 1e9
                ),
                true,
            );
            last_update = Instant::now();
        }
    }
    drop(file);
    progress(tx, module, format!("{label} : vérification SHA-256…"), true);
    if !verify(&partial, artifact.bytes, artifact.sha256)? {
        let _ = fs::remove_file(&partial);
        return Err(format!(
            "{label} : taille ou empreinte SHA-256 incorrecte ; téléchargement à reprendre"
        ));
    }
    if dest.exists() {
        fs::remove_file(dest).map_err(|e| e.to_string())?;
    }
    fs::rename(&partial, dest).map_err(|e| e.to_string())?;
    Ok(dest.into())
}

fn verify(path: &Path, expected_bytes: u64, expected_sha: &str) -> Result<bool, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != expected_bytes {
        return Ok(false);
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected_sha))
}

fn reset_stage(stage: &Path) -> Result<(), String> {
    if stage.exists() {
        fs::remove_dir_all(stage).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(stage).map_err(|e| e.to_string())
}

fn replace_stage(stage: &Path, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
    }
    fs::rename(stage, dest).map_err(|e| e.to_string())
}

fn extract_archive(archive: &Path, stage: &Path, cancel: &AtomicBool) -> Result<(), String> {
    match archive.extension().and_then(|e| e.to_str()) {
        Some("zip") => extract_zip(archive, stage, cancel),
        Some("gz" | "xz") => extract_tar(archive, stage, cancel),
        Some("dmg") => extract_dmg(archive, stage, cancel),
        _ => Err("Format d'archive inconnu".into()),
    }
}

fn extract_zip(archive: &Path, stage: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut expanded = 0u64;
    for index in 0..zip.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Extraction annulée".into());
        }
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.enclosed_name().ok_or("Chemin d'archive invalide")?;
        let dest = stage.join(name);
        expanded = expanded.saturating_add(entry.size());
        if expanded > 8_000_000_000 {
            return Err("Archive extraite trop volumineuse".into());
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Lien symbolique refusé dans l'archive".into());
        }
        if entry.is_dir() {
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut output = File::create(dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut output).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn extract_tar(archive: &Path, stage: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let listing = Command::new("tar")
        .arg("-tf")
        .arg(archive)
        .output()
        .map_err(|e| format!("tar absent : {e}"))?;
    if !listing.status.success() {
        return Err("Lecture de l'archive tar impossible".into());
    }
    for line in String::from_utf8_lossy(&listing.stdout).lines() {
        let path = Path::new(line);
        if path.is_absolute()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err("Chemin dangereux dans l'archive tar".into());
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Extraction annulée".into());
    }
    let status = Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(stage)
        .status()
        .map_err(|e| format!("tar absent : {e}"))?;
    if !status.success() {
        return Err("Extraction de l'archive tar impossible".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn extract_dmg(archive: &Path, stage: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let mount = stage.join("mounted");
    fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    let mounted = Command::new("hdiutil")
        .arg("attach")
        .arg("-readonly")
        .arg("-nobrowse")
        .arg("-mountpoint")
        .arg(&mount)
        .arg(archive)
        .status()
        .map_err(|e| e.to_string())?;
    if !mounted.success() {
        return Err("Montage de Blender.dmg impossible".into());
    }
    let result = if cancel.load(Ordering::Relaxed) {
        Err("Extraction annulée".into())
    } else {
        let source = mount.join("Blender.app");
        if !source.is_dir() {
            Err("Blender.app absent du DMG".into())
        } else {
            Command::new("ditto")
                .arg(&source)
                .arg(stage.join("Blender.app"))
                .status()
                .map_err(|e| e.to_string())
                .and_then(|copied| {
                    if copied.success() {
                        Ok(())
                    } else {
                        Err("Copie de Blender.app impossible".into())
                    }
                })
        }
    };
    let detached = Command::new("hdiutil").arg("detach").arg(&mount).status();
    if !detached.is_ok_and(|s| s.success()) {
        return Err("Démontage de Blender.dmg impossible".into());
    }
    fs::remove_dir(&mount).map_err(|e| e.to_string())?;
    result
}

#[cfg(not(target_os = "macos"))]
fn extract_dmg(_: &Path, _: &Path, _: &AtomicBool) -> Result<(), String> {
    Err("Archive DMG disponible uniquement sur macOS".into())
}

fn blender_binary() -> &'static str {
    if cfg!(windows) {
        "blender.exe"
    } else if cfg!(target_os = "macos") {
        "Blender"
    } else {
        "blender"
    }
}
fn trellis_binary() -> &'static str {
    if cfg!(windows) {
        "trellis-cli.exe"
    } else {
        "trellis-cli"
    }
}

fn find_binary(root: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 || !root.is_dir() {
        return None;
    }
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_binary(&path, name, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

pub(crate) fn managed_binary(
    root: &Path,
    dependency: &str,
    version: &str,
    name: &str,
) -> Option<PathBuf> {
    find_binary(&root.join(dependency).join(version), name, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_size_and_sha256() {
        let path = std::env::temp_dir().join(format!(
            "aos-installer-hash-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, b"abc").unwrap();
        assert!(verify(
            &path,
            3,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
        .unwrap());
        assert!(!verify(&path, 4, "").unwrap());
        assert!(!verify(&path, 3, "000000").unwrap());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn managed_binary_requires_installed_version() {
        let root = std::env::temp_dir().join(format!("aos-installer-bin-{}", std::process::id()));
        let binary = root.join("blender/4.5.14/bin/blender.exe");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(&binary, b"test").unwrap();
        assert_eq!(
            managed_binary(&root, "blender", "4.5.14", "blender.exe"),
            Some(binary)
        );
        assert!(managed_binary(&root, "blender", "4.5.13", "blender.exe").is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_prevents_network_request() {
        let root =
            std::env::temp_dir().join(format!("aos-installer-cancel-{}", std::process::id()));
        let artifact = Artifact {
            name: "sample.bin".into(),
            url: "http://127.0.0.1:1/unreachable".into(),
            bytes: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        };
        let (tx, _) = std::sync::mpsc::channel();
        let cancelled = AtomicBool::new(true);
        let result = download_to(
            &Client::new(),
            &artifact,
            &root.join(&artifact.name),
            &tx,
            "illustration-studio",
            &cancelled,
            "sample",
        );
        assert!(result.unwrap_err().contains("annulé"));
        assert!(!root.join("sample.bin").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
