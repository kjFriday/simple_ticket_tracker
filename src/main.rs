// ==== ACME Inc. portfolio ===============================================
// Project: simple_ticket_tracker | File: src/main.rs
// About: acme-tickets: ACME Ticket Tracker as a desktop program (build with `--features desktop`).
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! acme-tickets: ACME Ticket Tracker as a desktop program (build with `--features desktop`).
#![windows_subsystem = "windows"]

use ticket_tracker::host::{self, api::Tables, Spec};

fn main() {
    host::main(Spec {
        app_id: "ACME.Tickets",
        name: "ACME Ticket Tracker",
        exe_name: "acme-tickets.exe",
        version: env!("CARGO_PKG_VERSION"),
        width: 1200.0,
        height: 800.0,
        tables: Tables {
            read: &[
                ("tickets", &["customer_id", "assignee_id", "title", "body", "status", "created_at"]),
                ("customers", &["name", "email", "city"]),
                ("employees", &["name", "role", "email"]),
            ],
            write: Some("tickets"),
        },
        read_only: false,
        icon_ico: include_bytes!("../icon/app.ico"),
        icon_rgba: include_bytes!("../icon/app-32.rgba"),
        smoke_count: "document.querySelectorAll('.card').length",
    });
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=2fcb893d9693471d565f6076a130ecf0d6ecbf6dd39f9d5770daada3281c8c8a ====
