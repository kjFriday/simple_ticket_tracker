// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/core/mod.rs
// About: The page's logic in plain Rust: ticket rows, board columns, HTML and form checks.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! The page's logic in plain Rust: ticket rows, board columns, HTML and form checks.
//!
//! Nothing here touches the DOM, the network or the disk; the only dependencies are serde and
//! serde_json. So the same code compiles for the wasm page (src/web.rs) and for the host, and plain
//! `cargo test` covers all of it. src/web.rs binds it to the page; src/host/api.rs borrows
//! `validate::clean` so the desktop program stores text by the same rules.
//!
//! Start reading at model.rs, then board.rs, render.rs and validate.rs.

/// Filtering, grouping into status columns, and the in-memory ticket list (`Store`).
pub mod board;
/// Row types `Ticket`, `Person` and `Status`, parsed leniently from /api JSON.
pub mod model;
/// HTML strings for the board, the cards and the person pickers, with every value escaped.
pub mod render;
/// Text cleaning (the hub's textsafe rules) and `Draft`, the editor form turned into request bodies.
pub mod validate;

// ==== stamp v1 | author=kjFriday | hmac-sha256=c12889c58b632e0955d828d12408b54faa165782f4d073249ffbfe15bdf1012c ====
