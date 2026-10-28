// ==== Generic Company Inc. portfolio ====================================
// Project: simple_ticket_tracker | File: src/main.rs
// About: ==== Generic Company Inc. portfolio ===============================================...
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

// ==== Generic Company Inc. portfolio ===============================================
// Project: simple_ticket_tracker | File: src/main.rs
// About: generic-company-tickets: Generic Company Ticket Tracker as a desktop program (build with `--features desktop`).
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! generic-company-tickets: Generic Company Ticket Tracker as a desktop program (build with `--features desktop`).
#![windows_subsystem = "windows"]

use ticket_tracker::host::{self, api::Tables, Spec};

fn main() {
    host::main(Spec {
        app_id: "GenericCompany.Tickets",
        name: "Generic Company Ticket Tracker",
        exe_name: "generic-company-tickets.exe",
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

// ==== stamp v1 | author=kjFriday | hmac-sha256=698970afb2ecb6732346f0ff4792381d91c0de1fc9e5f946e21188f8d54afe3a ====
