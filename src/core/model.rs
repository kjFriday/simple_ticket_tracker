// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/core/model.rs
// About: Row types as `GET /api/{table}` returns them; unknown columns are ignored.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Row types as `GET /api/{table}` returns them; unknown columns are ignored.
//!
//! The rows come from the desktop program's `host::api`, which follows the CRUD hub's contract, or from
//! the mock snapshot in mock/*.json. Parsing is lenient so that one odd row never fails the whole load:
//! columns the types do not name are skipped, and a missing, null or unknown status reads as Open.
//!
//! Types: `Status` (a ticket's state and its board column), `Ticket` (a row of tickets) and `Person`
//! (a row of customers or of employees). Helpers: `parse_tickets`, `parse_people` and `name_of`.
//! Used by board.rs, render.rs, validate.rs and src/web.rs.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A ticket's state, which is also the board column it sits in.
///
/// Serialised in snake_case: "open", "in_progress", "closed". The same text is stored in the database's
/// tickets.status, used as the values of index.html's `#e-status` options and written by render.rs as a
/// column's `data-status`, so a rename has to change all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// New and not picked up yet; the leftmost column and the default.
    Open,
    /// Someone works on it.
    InProgress,
    /// Done; the rightmost column.
    Closed,
}

// The wire text, the label, parsing, and moving one column left or right.
impl Status {
    /// Board order, left to right.
    pub const ALL: [Status; 3] = [Status::Open, Status::InProgress, Status::Closed];

    /// The stored and wire text ("in_progress"), the same text serde writes.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::InProgress => "in_progress",
            Status::Closed => "closed",
        }
    }

    /// The column heading the board shows ("In progress").
    pub fn label(self) -> &'static str {
        match self {
            Status::Open => "Open",
            Status::InProgress => "In progress",
            Status::Closed => "Closed",
        }
    }

    /// The status for its wire text, ignoring surrounding whitespace; any other text is an error naming it.
    /// Exact match only: "Open" or "done" are refused.
    pub fn parse(s: &str) -> Result<Status, String> {
        match s.trim() {
            "open" => Ok(Status::Open),
            "in_progress" => Ok(Status::InProgress),
            "closed" => Ok(Status::Closed),
            other => Err(format!("unknown status {other:?}")),
        }
    }

    /// One column right, or None at the last column.
    pub fn next(self) -> Option<Status> {
        let i = Status::ALL.iter().position(|&s| s == self)?;
        Status::ALL.get(i + 1).copied()
    }

    /// One column left, or None at the first column.
    pub fn prev(self) -> Option<Status> {
        let i = Status::ALL.iter().position(|&s| s == self)?;
        i.checked_sub(1).map(|j| Status::ALL[j])
    }
}

// Displays as the wire text, so `format!("{s}")` gives "in_progress".
impl fmt::Display for Status {
    /// Writes `as_str()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A status the hub stores as free text: anything unrecognised lands in Open rather than failing the load.
///
/// Reads the field as an optional string, so a JSON null also lands in Open. A missing key never reaches
/// this function; `default_status` covers that case.
fn lenient_status<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Status, D::Error> {
    let s: Option<String> = Option::deserialize(d)?;
    Ok(s.and_then(|s| Status::parse(&s).ok()).unwrap_or(Status::Open))
}

/// One row of the tickets table.
///
/// Only `id` and `title` are required; every other column may be missing or null. `created_at` is kept
/// as the text the database holds ("yyyy-mm-ddThh:mm" as the page writes it): board.rs sorts it as text
/// and render.rs shows its first 10 characters as the day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ticket {
    /// The row id the database assigned (or `Store::create` in mock mode).
    pub id: i64,
    /// The customer who raised it (customers.id), if any.
    #[serde(default)]
    pub customer_id: Option<i64>,
    /// The employee working on it (employees.id), if any.
    #[serde(default)]
    pub assignee_id: Option<i64>,
    /// One-line summary; never empty once `Draft::checked` has passed it.
    pub title: String,
    /// Longer details; None when blank.
    #[serde(default)]
    pub body: Option<String>,
    // `default` covers a missing key, `deserialize_with` covers null and unknown text: both give Open
    /// Its column on the board.
    #[serde(default = "default_status", deserialize_with = "lenient_status")]
    pub status: Status,
    /// When it was created, as stored text; None for rows that never had one.
    #[serde(default)]
    pub created_at: Option<String>,
}

/// Open: the status of a row that has no status key at all.
fn default_status() -> Status {
    Status::Open
}

/// One row of customers or of employees; the two tables share this shape.
///
/// Customers carry `city` and employees carry `role`; the column the table lacks stays None.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    /// The row id that tickets refer to (customer_id or assignee_id).
    pub id: i64,
    /// Full name, shown on cards and in pickers.
    pub name: String,
    /// Job title (employees only).
    #[serde(default)]
    pub role: Option<String>,
    /// Email address; read but not shown on the board.
    #[serde(default)]
    pub email: Option<String>,
    /// Home city (customers only).
    #[serde(default)]
    pub city: Option<String>,
}

/// Tickets from a JSON array of rows; the error names the table and serde's reason.
pub fn parse_tickets(json: &str) -> Result<Vec<Ticket>, String> {
    serde_json::from_str(json).map_err(|e| format!("tickets: {e}"))
}

/// Customers or employees from a JSON array of rows; the error starts with "people:".
pub fn parse_people(json: &str) -> Result<Vec<Person>, String> {
    serde_json::from_str(json).map_err(|e| format!("people: {e}"))
}

/// The name of the person with `id`, or "Unassigned" when `id` is None or not in `people`.
///
/// Used for customers too, so a ticket with no customer shows "Customer: Unassigned".
pub fn name_of(people: &[Person], id: Option<i64>) -> &str {
    id.and_then(|id| people.iter().find(|p| p.id == id))
        .map(|p| p.name.as_str())
        .unwrap_or("Unassigned")
}

/// Parsing, the wire text and column moves.
#[cfg(test)]
mod tests {
    use super::*;

    /// Every status parses back from its own wire text, and an unknown word is an error.
    #[test]
    fn status_round_trips() {
        for s in Status::ALL {
            assert_eq!(Status::parse(s.as_str()), Ok(s));
        }
        assert!(Status::parse("done").is_err());
    }

    /// Moving stops at the board's edges, which is what hides the outer arrow buttons.
    #[test]
    fn next_and_prev_stop_at_the_edges() {
        assert_eq!(Status::Open.prev(), None);
        assert_eq!(Status::Open.next(), Some(Status::InProgress));
        assert_eq!(Status::InProgress.prev(), Some(Status::Open));
        assert_eq!(Status::Closed.next(), None);
    }

    /// An unknown status, a missing status, null ids and an extra column all load instead of failing.
    #[test]
    fn parses_hub_rows_and_tolerates_odd_status() {
        let json = r#"[{"id":3,"customer_id":1,"assignee_id":null,"title":"Hi","status":"weird","created_at":"2026-01-02T03:04","extra":1},
                       {"id":4,"title":"No status"}]"#;
        let t = parse_tickets(json).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].status, Status::Open);
        assert_eq!(t[0].assignee_id, None);
        assert_eq!(t[1].status, Status::Open);
        assert_eq!(t[1].body, None);
    }

    /// A known id gives the name; an unknown id and no id both give "Unassigned".
    #[test]
    fn name_of_falls_back() {
        let p = parse_people(r#"[{"id":1,"name":"Maria Garcia"}]"#).unwrap();
        assert_eq!(name_of(&p, Some(1)), "Maria Garcia");
        assert_eq!(name_of(&p, Some(9)), "Unassigned");
        assert_eq!(name_of(&p, None), "Unassigned");
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=e9ae0b34f3bd6b5c346bce2537cf56891c577fd7c9c4d3cb0c96f8eb6f463a3b ====
