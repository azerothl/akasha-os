fn main() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    // build.rs is compiled for the host; CARGO_CFG_TARGET_OS is the crate target.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon(icon.to_str().expect("icon path"));
        let ver = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
        res.set("FileVersion", &ver);
        res.set("ProductVersion", &ver);
        res.set("ProductName", "Akasha OS Preview");
        res.set("FileDescription", "Akasha OS Preview");
        res.compile().expect("embed Windows icon");
    }
}
