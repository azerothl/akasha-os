//! # aos-module-sdk — SDK côté guest pour les modules Agent OS (WASM).
//!
//! Un module ne peut communiquer avec le système **que** via `host_call`
//! (pas de WASI, aucun accès ambiant, §7.4). Ce SDK encapsule la plomberie
//! FFI : sérialisation JSON, allocation mémoire, dispatch des outils.
//!
//! Usage :
//! ```ignore
//! fn handle(tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
//!     match tool {
//!         "notes.create" => { /* ... */ }
//!         _ => Err(format!("outil inconnu: {tool}")),
//!     }
//! }
//! aos_module_sdk::export_module!(handle);
//! ```

use serde::de::DeserializeOwned;
use serde::Serialize;

#[link(wasm_import_module = "env")]
extern "C" {
    fn host_call(svc_ptr: u32, svc_len: u32, args_ptr: u32, args_len: u32) -> u64;
}

/// Appelle un service système (`fs.read`, `fs.write`, `fs.list`,
/// `mem.episodic_write`, `mem.episodic_query`).
pub fn call(service: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let args_str = serde_json::to_string(args).map_err(|e| e.to_string())?;
    let packed = unsafe {
        host_call(
            service.as_ptr() as u32,
            service.len() as u32,
            args_str.as_ptr() as u32,
            args_str.len() as u32,
        )
    };
    let ptr = (packed >> 32) as u32;
    let len = (packed & 0xFFFF_FFFF) as u32;
    if ptr == 0 {
        return Err("host_call: réponse vide".into());
    }
    let buf = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let resp: serde_json::Value = serde_json::from_slice(buf).map_err(|e| e.to_string())?;
    if resp["ok"].as_bool() == Some(true) {
        Ok(resp["result"].clone())
    } else {
        Err(resp["error"]
            .as_str()
            .unwrap_or("erreur inconnue")
            .to_string())
    }
}

/// `fs.read` → contenu du fichier.
pub fn fs_read(path: &str) -> Result<String, String> {
    let r = call("fs.read", &serde_json::json!({"path": path}))?;
    r["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "fs.read: réponse invalide".into())
}

/// `fs.write` → version créée.
pub fn fs_write(path: &str, content: &str) -> Result<u64, String> {
    let r = call(
        "fs.write",
        &serde_json::json!({"path": path, "content": content}),
    )?;
    r["version"]
        .as_u64()
        .ok_or_else(|| "fs.write: réponse invalide".into())
}

/// `fs.list` → chemins sous un préfixe.
pub fn fs_list(prefix: &str) -> Result<Vec<String>, String> {
    let r = call("fs.list", &serde_json::json!({"prefix": prefix}))?;
    Ok(r["entries"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e["path"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

/// `mem.episodic_write` → id du souvenir.
pub fn mem_write(namespace: &str, text: &str, metadata: serde_json::Value) -> Result<u64, String> {
    let r = call(
        "mem.episodic_write",
        &serde_json::json!({"namespace": namespace, "text": text, "metadata": metadata}),
    )?;
    r["id"].as_u64().ok_or_else(|| "mem.write: réponse invalide".into())
}

/// `mem.episodic_query` → hits (JSON).
pub fn mem_query(namespace: &str, query: &str, k: usize) -> Result<serde_json::Value, String> {
    call(
        "mem.episodic_query",
        &serde_json::json!({"namespace": namespace, "query": query, "k": k}),
    )
}

/// Point d'entrée généré par la macro [`export_module!`].
#[doc(hidden)]
pub fn invoke_internal<H>(handler: &H, ptr: *const u8, len: u32) -> u64
where
    H: Fn(&str, &serde_json::Value) -> Result<serde_json::Value, String>,
{
    let req_bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let parsed: Result<GuestRequest, _> = serde_json::from_slice(req_bytes);
        match parsed {
            Ok(req) => match handler(req.tool, &req.args) {
                Ok(result) => serde_json::json!({"ok": true, "result": result}),
                Err(e) => serde_json::json!({"ok": false, "error": e}),
            },
            Err(e) => serde_json::json!({"ok": false, "error": format!("requête invalide: {e}")}),
        }
    }));
    let json = match response {
        Ok(v) => v,
        Err(_) => serde_json::json!({"ok": false, "error": "panic dans le module"}),
    };
    let bytes = json.to_string().into_bytes();
    let leaked: &'static mut [u8] = Box::leak(bytes.into_boxed_slice());
    ((leaked.as_ptr() as u64) << 32) | leaked.len() as u64
}

#[derive(serde::Deserialize)]
struct GuestRequest<'a> {
    tool: &'a str,
    #[serde(default)]
    args: serde_json::Value,
}

/// Génère les exports WASM du module (`alloc`, `dealloc`, `invoke`).
#[macro_export]
macro_rules! export_module {
    ($handler:expr) => {
        #[no_mangle]
        pub extern "C" fn alloc(size: u32) -> *mut u8 {
            let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
            unsafe { std::alloc::alloc(layout) }
        }

        #[no_mangle]
        pub extern "C" fn dealloc(ptr: *mut u8, size: u32) {
            let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
            unsafe { std::alloc::dealloc(ptr, layout) }
        }

        #[no_mangle]
        pub extern "C" fn invoke(ptr: *const u8, len: u32) -> u64 {
            let handler: fn(&str, &serde_json::Value) -> Result<serde_json::Value, String> =
                $handler;
            $crate::invoke_internal(&handler, ptr, len)
        }
    };
}

/// Sérialise une valeur de retour d'outil (helper).
pub fn json_ok<T: Serialize>(v: &T) -> Result<serde_json::Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// Parse les args d'un outil (helper).
pub fn parse_args<T: DeserializeOwned>(args: &serde_json::Value) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|e| format!("args invalides: {e}"))
}
