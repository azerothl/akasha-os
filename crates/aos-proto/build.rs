//! Insert mix/speech mods when GitHub Contents cannot rewrite the large lib.rs.
fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    let path = std::path::Path::new("src/lib.rs");
    let src = std::fs::read_to_string(path).expect("aos-proto lib.rs");
    if src.contains("pub mod mix;") {
        return;
    }
    let needle = "pub mod host_folder;\n";
    let repl = "pub mod host_folder;\npub mod mix;\npub mod speech;\n";
    if !src.contains(needle) {
        panic!("aos-proto lib.rs: host_folder mod not found");
    }
    std::fs::write(path, src.replacen(needle, repl, 1)).expect("patch aos-proto lib.rs");
}
