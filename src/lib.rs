//! ACME Inc. ticket tracker. `core` is plain Rust (tested on the host); `web` binds it to the page;
//! `host` (feature `desktop`) is the native window that shows that page over the shared database.

pub mod core;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(all(feature = "desktop", windows))]
pub mod host;
