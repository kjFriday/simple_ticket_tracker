//! The page's files (index.html, the wasm build, the icon), served from the `www` folder beside the
//! exe, or the project's own `www` when run from the source tree.

use std::path::{Component, Path, PathBuf};

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
pub fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    let rel = url_path.split(['?', '#']).next()?.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    let mut out = root.to_path_buf();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(part) => out.push(part),
            _ => return None, // .., C:\, \\server, ...
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
fn www_for(exe: &Path) -> PathBuf {
    let beside = exe.with_file_name("www");
    if beside.join("index.html").is_file() {
        return beside;
    }
    let target = exe.ancestors().find(|d| d.file_name().is_some_and(|n| n.eq_ignore_ascii_case("target")));
    match target.and_then(Path::parent).map(|project| project.join("www")) {
        Some(d) if d.join("index.html").is_file() => d,
        _ => beside,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_types() {
        assert_eq!(mime("/pkg/ticket_tracker_bg.wasm"), "application/wasm");
        assert_eq!(mime("main.JS"), "text/javascript; charset=utf-8");
        assert_eq!(mime("favicon.png"), "image/png");
        assert_eq!(mime("x"), "application/octet-stream");
    }

    #[test]
    fn stays_inside_root() {
        let root = Path::new("C:/app/www");
        assert_eq!(resolve(root, "/"), Some(root.join("index.html")));
        assert_eq!(resolve(root, "/pkg/a.js?v=1"), Some(root.join("pkg").join("a.js")));
        assert_eq!(resolve(root, "/../secret.txt"), None);
        assert_eq!(resolve(root, "/pkg/../../x"), None);
        assert_eq!(resolve(root, "/C:/Windows/win.ini"), None);
    }

    #[test]
    fn source_tree_has_the_page() {
        let www = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("www");
        assert!(www.join("index.html").is_file() && www.join("favicon.png").is_file());
        assert_eq!(www_dir(), www); // this test exe runs from target\debug\deps
    }

    #[test]
    fn www_beside_the_exe_or_above_target() {
        let tmp = std::env::temp_dir().join(format!("acme-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        for dir in ["installed", "project", "project/target/release"] {
            std::fs::create_dir_all(tmp.join(dir).join("www")).unwrap();
        }
        for dir in ["installed", "project"] {
            std::fs::write(tmp.join(dir).join("www").join("index.html"), "").unwrap();
        }
        let exe = "acme.exe";
        assert_eq!(www_for(&tmp.join("installed").join(exe)), tmp.join("installed").join("www"));
        let built = tmp.join("project").join("target").join("release");
        assert_eq!(www_for(&built.join(exe)), tmp.join("project").join("www"));
        // no page anywhere: the folder beside the exe, as when installed
        assert_eq!(www_for(&tmp.join("other").join(exe)), tmp.join("other").join("www"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
