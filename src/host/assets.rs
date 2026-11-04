// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/host/assets.rs
// About: The page's files (index.html, the wasm build, the icon), served from a `www` folder.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! The page's files (index.html, the wasm build, the icon), served from a `www` folder.
//!
//! That folder sits beside the exe, or is the project's own `www` when run from the source tree.
//!
//! host/mod.rs's `serve` answers every `generic-company://localhost/...` request that is not an API call
//! with a file found here: `www_dir` finds the folder once at start-up, `resolve` maps the URL path to a
//! file inside it, and `mime` names its Content-Type. The files are read from disk on each request, not
//! embedded in the exe, so the installer copies the whole www folder beside the exe.

use std::path::{Component, Path, PathBuf};

/// The Content-Type for a file, chosen by its extension (any case); unknown types are
/// application/octet-stream.
///
/// The wasm-pack glue compiles the .wasm with `WebAssembly.instantiateStreaming`, which insists on
/// application/wasm, and the browser runs an ES module only when it comes with a JavaScript type.
pub fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// The file under root for a URL path, or None for anything that would leave root.
///
/// The query and fragment are dropped and "/" means index.html. The path is then rebuilt one component
/// at a time, and any component that is not a plain name (`..`, `.`, a drive such as `C:`, a `\\server`
/// share or a root) refuses the whole path, so a request can never read outside the www folder.
/// The file is not checked to exist. Percent-escapes are not decoded.
pub fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    let rel = url_path.split(['?', '#']).next()?.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    let mut out = root.to_path_buf();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(part) => out.push(part),
            _ => return None, // .., ., C:\, \\server, a leading \ ...
        }
    }
    Some(out)
}

/// The www folder: beside the exe when installed, else the source tree's (cargo run, target\release).
pub fn www_dir() -> PathBuf {
    www_for(&std::env::current_exe().unwrap_or_default())
}

/// The www beside exe, else the one in the project above the `target` folder holding exe. Found from the
/// exe's path at run time, so no build compiles in the folder it was built in.
///
/// Steps:
/// 1. Use the www beside the exe when it holds index.html (an installed copy).
/// 2. Else find the nearest ancestor folder named `target` (any case) and use the www beside it
/// (target\debug, target\release and target\debug\deps all lead to the project).
/// 3. Else fall back to the www beside the exe (the installed layout); the page then answers 404.
fn www_for(exe: &Path) -> PathBuf {
    let beside = exe.with_file_name("www");
    if beside.join("index.html").is_file() {
        return beside;
    }
    let target = exe
        .ancestors()
        .find(|d| d.file_name().is_some_and(|n| n.eq_ignore_ascii_case("target")));
    match target.and_then(Path::parent).map(|project| project.join("www")) {
        Some(d) if d.join("index.html").is_file() => d,
        _ => beside,
    }
}

/// MIME types, the traversal guard and finding www.
#[cfg(test)]
mod tests {
    use super::*;

    /// Extension lookup ignores case and the folder, and a name without a dot is binary.
    #[test]
    fn mime_types() {
        assert_eq!(mime("/pkg/ticket_tracker_bg.wasm"), "application/wasm");
        assert_eq!(mime("main.JS"), "text/javascript; charset=utf-8");
        assert_eq!(mime("favicon.png"), "image/png");
        assert_eq!(mime("x"), "application/octet-stream");
    }

    /// "/" is index.html, a query is dropped, and `..` (even after a real folder) or a drive is refused.
    #[test]
    fn stays_inside_root() {
        let root = Path::new("C:/app/www");
        assert_eq!(resolve(root, "/"), Some(root.join("index.html")));
        assert_eq!(resolve(root, "/pkg/a.js?v=1"), Some(root.join("pkg").join("a.js")));
        assert_eq!(resolve(root, "/../secret.txt"), None);
        assert_eq!(resolve(root, "/pkg/../../x"), None);
        assert_eq!(resolve(root, "/C:/Windows/win.ini"), None);
    }

    /// The project's own www has the page and the icon, and the test exe finds it from inside `target`.
    /// Known limitation: fails when CARGO_TARGET_DIR puts the build outside the project.
    #[test]
    fn source_tree_has_the_page() {
        let www = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("www");
        assert!(www.join("index.html").is_file() && www.join("favicon.png").is_file());
        assert_eq!(www_dir(), www); // this test exe runs from target\debug\deps
    }

    /// Each of the three cases of `www_for` in a temp tree: installed, built in a project, neither.
    /// The www under target\release has no index.html, so it must be passed over.
    #[test]
    fn www_beside_the_exe_or_above_target() {
        let tmp = std::env::temp_dir().join(format!("generic-company-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        for dir in ["installed", "project", "project/target/release"] {
            std::fs::create_dir_all(tmp.join(dir).join("www")).unwrap();
        }
        for dir in ["installed", "project"] {
            std::fs::write(tmp.join(dir).join("www").join("index.html"), "").unwrap();
        }
        let exe = "generic_company.exe";
        assert_eq!(
            www_for(&tmp.join("installed").join(exe)),
            tmp.join("installed").join("www")
        );
        let built = tmp.join("project").join("target").join("release");
        assert_eq!(www_for(&built.join(exe)), tmp.join("project").join("www"));
        // no page anywhere: the folder beside the exe, as when installed
        assert_eq!(www_for(&tmp.join("other").join(exe)), tmp.join("other").join("www"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=5adacc3a65de63a17959d4393bc18002cbe66d1489809f9d4c9226d0e2077d07 ====
