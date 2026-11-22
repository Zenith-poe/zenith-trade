fn main() {
    println!("cargo:rerun-if-changed=assets/zenith.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    let icon = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets/zenith.ico");
    res.set_icon(&icon.to_string_lossy());
    res.set("ProductName", "Zenith Trade");
    res.set("FileDescription", "Price-check overlay for Path of Exile");
    res.set("LegalCopyright", "MIT License. Fan-made, not affiliated with Grinding Gear Games.");
    // ProductVersion and FileVersion come from the package version.
    if let Err(error) = res.compile() {
        panic!("could not embed the exe resources: {error}");
    }
}
