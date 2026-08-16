fn main() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon(icon.to_str().expect("icon path"));
        res.compile().expect("embed Windows icon");
    }
}
