// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/core/board.rs
// About: Board state: filtering, columns, and the in-memory store used in mock (standalone) mode.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Board state: filtering, columns, and the in-memory store used in mock (standalone) mode.
//!
//! src/web.rs keeps one `Store` and one `Filter` in its page state. Every redraw passes them through
//! `columns`, and render.rs turns the result into HTML. The page keeps its tickets in a `Store` in
//! database mode too: after the program accepts a POST, PATCH or DELETE, the page repeats the change
//! in the Store instead of reloading the list.
//!
//! Types: `Filter` (the toolbar's pickers and search box) and `Store` (the ticket list).

use super::model::{Person, Status, Ticket};

/// The board's filter bar: an assignee picker, a customer picker and a search box.
///
/// The default lets every ticket through.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    /// Only tickets assigned to this employee id; None means anyone, including unassigned.
    pub assignee: Option<i64>,
    /// Only tickets of this customer id; None means any.
    pub customer: Option<i64>,
    /// Search text as typed; `matches` trims it and ignores case.
    pub text: String,
}

// The one test the board applies to each ticket.
impl Filter {
    /// Whether `t` passes: the set ids must match, and the text (trimmed, any case) must occur in the title, the
    /// body or the customer's name (looked up in `customers`), or equal `#id`.
    pub fn matches(&self, t: &Ticket, customers: &[Person]) -> bool {
        if self.assignee.is_some() && t.assignee_id != self.assignee {
            return false;
        }
        if self.customer.is_some() && t.customer_id != self.customer {
            return false;
        }
        let q = self.text.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        // a customer id with no matching row searches as an empty name
        let customer = t
            .customer_id
            .and_then(|id| customers.iter().find(|p| p.id == id))
            .map(|p| p.name.to_lowercase())
            .unwrap_or_default();
        // a substring of the title, body or customer name, or exactly "#id" (the id test is whole: "#3" does not find #30 by its id)
        t.title.to_lowercase().contains(&q)
            || t.body.as_deref().unwrap_or("").to_lowercase().contains(&q)
            || customer.contains(&q)
            || format!("#{}", t.id) == q
    }
}

/// Tickets per board column, newest first (created_at, then id).
///
/// Returns one entry per `Status::ALL` in board order, even when a column is empty, so the board always
/// has three sections. created_at is compared as text, which is time order for "yyyy-mm-ddThh:mm" stamps;
/// a ticket with no created_at (None) sorts after all dated ones. Equal stamps put the higher id first.
pub fn columns<'a>(tickets: &'a [Ticket], filter: &Filter, customers: &[Person]) -> Vec<(Status, Vec<&'a Ticket>)> {
    Status::ALL
        .iter()
        .map(|&s| {
            let mut col: Vec<&Ticket> = tickets
                .iter()
                .filter(|t| t.status == s && filter.matches(t, customers))
                .collect();
            // b before a: descending order
            col.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
            (s, col)
        })
        .collect()
}

/// Mock-mode store: the same create/update/delete the hub offers, kept in memory.
///
/// The page also keeps its database-mode copy of the tickets here, so the board draws from a Store in
/// both modes. Nothing is saved: a reload starts again from the snapshot or the database.
#[derive(Debug, Clone, Default)]
pub struct Store {
    /// Every ticket, in no particular order (`columns` sorts).
    pub tickets: Vec<Ticket>,
}

// Create, find and delete by id, the way the database would.
impl Store {
    /// A store holding `tickets` as loaded.
    pub fn new(tickets: Vec<Ticket>) -> Store {
        Store { tickets }
    }

    /// Adds `t` under a fresh id and returns that id; whatever id `t` carried is replaced.
    ///
    /// The fresh id is one more than the highest id present (1 for an empty store). Unlike SQLite's
    /// AUTOINCREMENT, an id freed by deleting the highest ticket is given out again.
    pub fn create(&mut self, mut t: Ticket) -> i64 {
        t.id = self.tickets.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        let id = t.id;
        self.tickets.push(t);
        id
    }

    /// The ticket with `id`, to change in place; None when there is none.
    pub fn get_mut(&mut self, id: i64) -> Option<&mut Ticket> {
        self.tickets.iter_mut().find(|t| t.id == id)
    }

    /// Removes the ticket with `id`; false when there was none.
    pub fn delete(&mut self, id: i64) -> bool {
        let before = self.tickets.len();
        self.tickets.retain(|t| t.id != id);
        self.tickets.len() != before
    }
}

/// Grouping, filtering and the store's id handling.
#[cfg(test)]
mod tests {
    use super::*;

    /// A ticket of customer 1 with no body.
    fn t(id: i64, status: Status, assignee: Option<i64>, created: &str, title: &str) -> Ticket {
        Ticket {
            id,
            customer_id: Some(1),
            assignee_id: assignee,
            title: title.into(),
            body: None,
            status,
            created_at: Some(created.into()),
        }
    }

    /// Four tickets: two open (the later one is #2), one in progress, one closed; #3 and #4 share a date.
    fn sample() -> Vec<Ticket> {
        vec![
            t(1, Status::Open, Some(10), "2026-01-01T09:00", "Login fails"),
            t(2, Status::Open, Some(11), "2026-03-01T09:00", "Refund"),
            t(3, Status::Closed, Some(10), "2026-02-01T09:00", "Invoice"),
            t(4, Status::InProgress, None, "2026-02-01T09:00", "Shipping"),
        ]
    }

    /// Column sizes follow Status::ALL order, and the open column starts with its newer ticket (#2).
    #[test]
    fn groups_into_three_columns_newest_first() {
        let ts = sample();
        let cols = columns(&ts, &Filter::default(), &[]);
        let counts: Vec<usize> = cols.iter().map(|(_, c)| c.len()).collect();
        assert_eq!(counts, vec![2, 1, 1]);
        assert_eq!(cols[0].1[0].id, 2);
    }

    /// Assignee id, padded upper-case text, a customer's surname (all four tickets are hers) and "#3" each filter.
    #[test]
    fn filters_by_assignee_text_and_customer_name() {
        let ts = sample();
        let customers = vec![Person {
            id: 1,
            name: "Maria Garcia".into(),
            role: None,
            email: None,
            city: None,
        }];
        let f = Filter {
            assignee: Some(10),
            ..Default::default()
        };
        assert_eq!(ts.iter().filter(|t| f.matches(t, &customers)).count(), 2);
        let f = Filter {
            text: "  REFUND ".into(),
            ..Default::default()
        };
        assert_eq!(ts.iter().filter(|t| f.matches(t, &customers)).count(), 1);
        let f = Filter {
            text: "garcia".into(),
            ..Default::default()
        };
        assert_eq!(ts.iter().filter(|t| f.matches(t, &customers)).count(), 4);
        let f = Filter {
            text: "#3".into(),
            ..Default::default()
        };
        assert_eq!(ts.iter().filter(|t| f.matches(t, &customers)).count(), 1);
    }

    /// create ignores the id it is given (0) and assigns max + 1; a second delete of the same id reports false.
    #[test]
    fn store_create_update_delete() {
        let mut s = Store::new(sample());
        let id = s.create(t(0, Status::Open, None, "2026-04-01T00:00", "New"));
        assert_eq!(id, 5);
        s.get_mut(id).unwrap().status = Status::Closed;
        assert_eq!(s.tickets.iter().find(|t| t.id == 5).unwrap().status, Status::Closed);
        assert!(s.delete(5));
        assert!(!s.delete(5));
        assert_eq!(s.tickets.len(), 4);
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=9031eb5ea34d21d3e1d53f859ef8937da65da76b5761698efa7e28f6ba0092a3 ====
