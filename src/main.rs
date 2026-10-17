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
