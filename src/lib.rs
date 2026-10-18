// ==== ACME Inc. portfolio ===============================================
// Project: simple_ticket_tracker | File: src/lib.rs
// About: ACME Inc. ticket tracker.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! ACME Inc. ticket tracker. `core` is plain Rust (tested on the host); `web` binds it to the page;
//! `host` (feature `desktop`) is the native window that shows that page over the shared database.

pub mod core;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(all(feature = "desktop", windows))]
pub mod host;

// ==== stamp v1 | author=kjFriday | hmac-sha256=53ccf00cf10d0565b8bce67e7b43a1565b3a78e2b2740ed1346b08674e3b4ef6 ====
