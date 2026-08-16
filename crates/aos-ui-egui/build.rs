fn main() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    embed_windows_icon(&icon);
}

#[cfg(windows)]
fn embed_windows_icon(icon: &std::path::Path) {
    let mut res = winres::WindowsResource::new();
    res.set_icon(icon.to_str().expect("icon path"));
    res.compile().expect("embed Windows icon");
}

#[cfg(not(windows))]
fn embed_windows_icon(_icon: &std::path::Path) {}
