// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/lib.rs
// About: Generic Company Inc. ticket tracker.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Generic Company Inc. ticket tracker. `core` is plain Rust (tested on the host); `web` binds it to the page;
//! `host` (feature `desktop`) is the native window that shows that page over the shared database.
//!
//! One crate, built two ways:
//! 1. `wasm-pack build` (target wasm32) compiles `core` and `web` into `www/pkg/`, the page's
//!    WebAssembly; www/main.js loads it and `web::start` takes over the page.
//! 2. `cargo build --features desktop` on Windows compiles `core` and `host`; src/main.rs (the exe)
//!    calls `host::main`, which opens a window that loads that same page from `www/`.
//!
//! `cargo test` runs `core`'s tests on the host; `--features desktop` adds `host`'s. Start reading at
//! `core` for what the board does, `web` for how the page drives it.

/// The board logic shared by both builds: models, filters, form validation and HTML rendering.
pub mod core;

/// The page wiring: DOM events, drawing and `fetch` calls (WebAssembly builds only).
#[cfg(target_arch = "wasm32")]
mod web;

/// The desktop program: window, WebView2, `/api` over SQLite, installer and settings (Windows,
/// feature `desktop`).
#[cfg(all(feature = "desktop", windows))]
pub mod host;

// ==== stamp v1 | author=kjFriday | hmac-sha256=51186273d663253aaa889e42574d0138d7863844e44aaace3f5d8426843f8eaf ====
