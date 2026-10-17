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

/// The www folder: beside the exe when installed, else the source tree's (cargo run).
pub fn www_dir() -> PathBuf {
    let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("www")));
    match beside {
        Some(d) if d.join("index.html").is_file() => d,
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("www"),
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
    }
}
