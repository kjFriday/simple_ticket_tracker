// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/main.rs
// About: generic-company-tickets: the Generic Company Ticket Tracker as a desktop program.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! generic-company-tickets: the Generic Company Ticket Tracker as a desktop program.
//!
//! Built only with `cargo build --features desktop` (Cargo.toml lists it with required-features).
//!
//! The whole program is `host::main` (src/host/mod.rs), shared by the Generic Company Rust programs;
//! this file only fills in the `Spec` that makes it the ticket tracker: its ids and names, window
//! size, which shared-database tables the page may read and change, its icons and its smoke check.
//!
//! `windows_subsystem = "windows"` marks the exe as a GUI program, so starting it opens no console
//! window; host/mod.rs reports through message boxes, and also prints to stdout for scripts that
//! capture it (only stdout with `--quiet`).
#![windows_subsystem = "windows"]

use ticket_tracker::host::{self, api::Tables, Spec};

/// Starts the ticket tracker: hands its `Spec` to `host::main`, which parses the command line and
/// either installs, uninstalls or opens the window. Never returns normally once the window runs.
fn main() {
    host::main(Spec {
        // the app id names the uninstall registry key and the per-app data folder: changing it
        // orphans existing installs and settings
        app_id: "GenericCompany.Tickets",
        name: "Generic Company Ticket Tracker",
        exe_name: "generic-company-tickets.exe",
        version: env!("CARGO_PKG_VERSION"),
        // logical pixels; three 260 px minimum columns plus gaps fit with room to spare
        width: 1200.0,
        height: 800.0,
        tables: Tables {
            // `id` is implied for each table; these are the other columns the page reads, and the
            // program checks they all exist before it uses a database
            read: &[
                (
                    "tickets",
                    &["customer_id", "assignee_id", "title", "body", "status", "created_at"],
                ),
                ("customers", &["name", "email", "city"]),
                ("employees", &["name", "role", "email"]),
            ],
            // the board edits tickets only; customers and employees just fill the pickers
            write: Some("tickets"),
        },
        read_only: false,
        // the pixel icon (16/32/48/256 px) for shortcuts and Apps & features; tests/icon.rs checks it
        icon_ico: include_bytes!("../icon/app.ico"),
        // the 32 px window icon as raw RGBA, used when the exe lacks its linked icon resource
        icon_rgba: include_bytes!("../icon/app-32.rgba"),
        // --smoke counts the cards on the board: above 0 means the page loaded and drew tickets
        smoke_count: "document.querySelectorAll('.card').length",
    });
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=7329a262c216c345e6f9dfdbf8bc7bfd07e9251c819583e49313c04cc13a3d2f ====
