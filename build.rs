// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: build.rs
// About: Build script that links the pixel icon and its manifest into the desktop exe.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Build script that links the pixel icon and its manifest into the desktop exe.
//!
//! icon/app.res is a compiled Windows resource file written by tools/pixel_icons.py. It holds the icon
//! group as resource 1, which `host::window_icon` loads for the title bar, and a DPI-aware application
//! manifest. MSVC's link.exe takes a .res file as an input like an object file, so no resource compiler
//! runs here.
//!
//! Cargo runs this before it compiles the crate. The wasm build (wasm-pack) and plain `cargo test` leave
//! the `desktop` feature off, so they link nothing extra.

/// Links icon/app.res (the pixel icon as resource 1, plus a DPI-aware manifest) into the desktop exe.
///
/// Steps:
/// 1. Ask cargo to rerun this script only when icon/app.res changes.
/// 2. Read the enabled features and the target OS from cargo's environment variables.
/// 3. For a `desktop` build that targets Windows, pass the .res file to the linker of the bin only.
fn main() {
    // without any rerun-if-changed line cargo reruns the script whenever any file in the package changes
    println!("cargo:rerun-if-changed=icon/app.res");
    // cargo tells a build script the package's enabled features as CARGO_FEATURE_<NAME> variables
    let desktop = std::env::var_os("CARGO_FEATURE_DESKTOP").is_some();
    // cfg!(windows) would describe the machine running this script; the target being built is in CARGO_CFG_TARGET_OS
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if desktop && windows {
        let res = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("icon")
            .join("app.res");
        // -bins: only the generic-company-tickets exe gets the resource, not the library (cdylib, rlib)
        println!("cargo:rustc-link-arg-bins={}", res.display());
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=b45b93af1dfbe7bd51df06dd5670c6ad886b402dfde749f6360759490c53b038 ====
