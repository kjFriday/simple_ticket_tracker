// Links icon/app.res (the pixel icon as resource 1, plus a DPI-aware manifest) into the desktop exe.
fn main() {
    println!("cargo:rerun-if-changed=icon/app.res");
    let desktop = std::env::var_os("CARGO_FEATURE_DESKTOP").is_some();
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if desktop && windows {
        let res = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icon").join("app.res");
        println!("cargo:rustc-link-arg-bins={}", res.display());
    }
}
