// ==== Generic Company Inc. portfolio ====================================
// Project: simple_ticket_tracker | File: build.rs
// About: ==== Generic Company Inc. portfolio ===============================================...
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

// ==== Generic Company Inc. portfolio ===============================================
// Project: simple_ticket_tracker | File: build.rs
// About: Links icon/app.res (the pixel icon as resource 1, plus a DPI-aware manifest) into the...
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

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

// ==== stamp v1 | author=kjFriday | hmac-sha256=112991bf78d0e6f24ad20822897a86b6f27f406390527e302f2315a08cd89c53 ====
