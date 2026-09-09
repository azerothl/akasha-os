// SPDX-License-Identifier: Apache-2.0
//! Gallery demo — rich UI contract v2 sample (layout, image_view, job subscription).

use serde_json::json;

const SAMPLE_PATH: &str = "/documents/gallery-demo/sample.png";
const MINIMAL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

fn handle(tool: &str, _args: &serde_json::Value) -> Result<serde_json::Value, String> {
    match tool {
        "gallery-demo.preview.ensure" => ensure_sample(),
        "gallery-demo.preview.get" => preview_get(),
        _ => Err(format!("outil inconnu: {tool}")),
    }
}

fn ensure_sample() -> Result<serde_json::Value, String> {
    if aos_module_sdk::fs_read(SAMPLE_PATH).is_err() {
        let b64 = base64_encode(MINIMAL_PNG);
        let _ = aos_module_sdk::call(
            "fs.write",
            &json!({"path": SAMPLE_PATH, "content": b64, "encoding": "base64"}),
        )?;
    }
    Ok(json!({"path": SAMPLE_PATH}))
}

fn preview_get() -> Result<serde_json::Value, String> {
    let _ = ensure_sample()?;
    Ok(json!({"path": SAMPLE_PATH}))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

aos_module_sdk::export_module!(handle);
