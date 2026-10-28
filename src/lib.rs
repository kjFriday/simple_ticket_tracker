// ==== Generic Company Inc. portfolio ====================================
// Project: simple_ticket_tracker | File: src/lib.rs
// About: ==== Generic Company Inc. portfolio ===============================================...
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

// ==== Generic Company Inc. portfolio ===============================================
// Project: simple_ticket_tracker | File: src/lib.rs
// About: Generic Company Inc. ticket tracker.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Generic Company Inc. ticket tracker. `core` is plain Rust (tested on the host); `web` binds it to the page;
//! `host` (feature `desktop`) is the native window that shows that page over the shared database.

pub mod core;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(all(feature = "desktop", windows))]
pub mod host;

// ==== stamp v1 | author=kjFriday | hmac-sha256=e582da5579fd5b7c76dd425ef570ec09942e424f8aff29f652633b4b844fdfd7 ====
