//! Insert mix/speech mods when GitHub Contents cannot rewrite the large lib.rs.
fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    let path = std::path::Path::new("src/lib.rs");
    let src = std::fs::read_to_string(path).expect("aos-proto lib.rs");
    if src.contains("pub mod mix;") {
        return;
    }
    let Some(idx) = src.find("pub mod host_folder;") else {
        panic!("aos-proto lib.rs: host_folder mod not found");
    };
    let insert_at = idx + "pub mod host_folder;".len();
    let mut out = String::with_capacity(src.len() + 32);
    out.push_str(&src[..insert_at]);
    out.push_str("\npub mod mix;\npub mod speech;");
    out.push_str(&src[insert_at..]);
    std::fs::write(path, out).expect("patch aos-proto lib.rs");
}
