// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/core/validate.rs
// About: Form input → hub JSON bodies.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Form input → hub JSON bodies. Mirrors the hub's textsafe rules so mock mode stores what the hub would.
//!
//! src/web.rs reads the ticket editor (or a card's arrow button) into a `Draft`. In database mode it
//! sends `create_body` as a POST or `patch_body` as a PATCH; in mock mode, and after the program accepts
//! a write, `apply` changes the ticket in the page's Store. `parse_id` reads a `<select>`'s value.
//!
//! `clean` and `LIMIT` are a port of the CRUD hub's textsafe.py. src/host/api.rs runs every string it
//! stores through them too, so text is cleaned the same way in the page, the desktop program and the hub.

use super::model::{Status, Ticket};
use serde_json::{json, Map, Value};

/// textsafe.LIMIT in the hub: string columns are capped at this many characters.
///
/// index.html's title and details fields carry the same number as their maxlength.
pub const LIMIT: usize = 240;

/// Drop the characters textsafe refuses (C0 except tab/newline, DEL, C1, U+2028/9, bidi controls), then cap.
///
/// `limit` counts characters (Unicode scalar values), not bytes, as Python's slicing does in the hub.
/// Ordinary non-ASCII text passes unchanged.
pub fn clean(text: &str, limit: usize) -> String {
    text.chars().filter(|&c| !refused(c)).take(limit).collect()
}

/// Whether textsafe drops `c`: characters that are invisible yet change how text displays or is parsed.
fn refused(c: char) -> bool {
    let u = c as u32;
    // in order:
    // - C0 controls (NUL, ESC, CR, ...) except tab and newline, which a details field may hold
    // - DEL and the whole C1 block U+0080-U+009F; U+009B alone starts a terminal escape sequence (CSI)
    // - LINE SEPARATOR and PARAGRAPH SEPARATOR, which some viewers and older JavaScript treat as line ends
    // - bidi embeddings and overrides U+202A-U+202E (RLO can make text display backwards)
    // - bidi isolates U+2066-U+2069 (LRI, RLI, FSI, PDI)
    (u < 0x20 && c != '\t' && c != '\n')
        || (0x7F..=0x9F).contains(&u)
        || u == 0x2028
        || u == 0x2029
        || (0x202A..=0x202E).contains(&u)
        || (0x2066..=0x2069).contains(&u)
}

/// The ticket editor's fields, as typed and not yet checked.
///
/// web.rs fills it from the editor form, or from an existing ticket with a new status when an arrow
/// button moves a card.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Draft {
    /// Title as typed; `checked` trims and cleans it and refuses it when nothing is left.
    pub title: String,
    /// Details as typed; empty means no body (NULL in the database).
    pub body: String,
    /// The chosen customer; None clears it.
    pub customer_id: Option<i64>,
    /// The chosen assignee; None clears it.
    pub assignee_id: Option<i64>,
    /// The chosen status; None means Open for a new ticket and "keep it" for an existing one.
    pub status: Option<Status>,
}

// Checking a draft and turning it into a POST body, a PATCH body or an in-place change.
impl Draft {
    /// Cleaned and checked; the title must have something left after trimming.
    ///
    /// Returns a copy with the title and body trimmed, then cleaned and capped at `LIMIT`; the ids and the
    /// status are copied as they are. Trimming comes first, so a space next to a removed control character
    /// survives: "a \u{0}" becomes "a ".
    /// Errors: "A ticket needs a title." when nothing is left of the title.
    pub fn checked(&self) -> Result<Draft, String> {
        let title = clean(self.title.trim(), LIMIT);
        if title.is_empty() {
            return Err("A ticket needs a title.".into());
        }
        Ok(Draft {
            title,
            body: clean(self.body.trim(), LIMIT),
            ..self.clone()
        })
    }

    /// POST body for a new ticket; `now` is the created_at stamp (yyyy-mm-ddThh:mm).
    ///
    /// Every column is present: an empty body and missing ids are sent as null, and a missing status as
    /// "open". Errors as `checked`.
    pub fn create_body(&self, now: &str) -> Result<Value, String> {
        let d = self.checked()?;
        Ok(json!({
            "title": d.title,
            "body": if d.body.is_empty() { Value::Null } else { Value::String(d.body) },
            "customer_id": d.customer_id,
            "assignee_id": d.assignee_id,
            "status": d.status.unwrap_or(Status::Open).as_str(),
            "created_at": now,
        }))
    }

    /// PATCH body holding only the fields that differ from `old`; None when nothing changed.
    ///
    /// The comparison uses the checked values, so an edit that only adds spaces or control characters
    /// sends nothing. A cleared body or id is sent as null; a status of None is never sent. Columns the
    /// user did not change are left out, so the PATCH leaves them as the database holds them.
    /// Errors as `checked`.
    pub fn patch_body(&self, old: &Ticket) -> Result<Option<Value>, String> {
        let d = self.checked()?;
        let mut m = Map::new();
        if d.title != old.title {
            m.insert("title".into(), d.title.into());
        }
        let body = if d.body.is_empty() { None } else { Some(d.body) };
        if body != old.body {
            m.insert("body".into(), body.map_or(Value::Null, Value::String));
        }
        if d.customer_id != old.customer_id {
            m.insert("customer_id".into(), d.customer_id.into());
        }
        if d.assignee_id != old.assignee_id {
            m.insert("assignee_id".into(), d.assignee_id.into());
        }
        if let Some(s) = d.status.filter(|&s| s != old.status) {
            m.insert("status".into(), s.as_str().into());
        }
        Ok(if m.is_empty() { None } else { Some(Value::Object(m)) })
    }

    /// Apply a checked draft to a ticket (mock mode's PATCH).
    ///
    /// Title, body and both ids are overwritten (None clears an id); the status changes only when the
    /// draft names one. web.rs also uses it to build a new ticket, and in database mode to mirror a
    /// PATCH the program accepted. Errors as `checked`, and then `t` is left unchanged.
    pub fn apply(&self, t: &mut Ticket) -> Result<(), String> {
        let d = self.checked()?;
        t.title = d.title;
        t.body = if d.body.is_empty() { None } else { Some(d.body) };
        t.customer_id = d.customer_id;
        t.assignee_id = d.assignee_id;
        if let Some(s) = d.status {
            t.status = s;
        }
        Ok(())
    }
}

/// Parse an optional id from a <select> value ("" = none).
///
/// Anything that is not a whole number after trimming is None as well. Also reads a card's `data-id`
/// and the editor's hidden id field.
pub fn parse_id(s: &str) -> Option<i64> {
    s.trim().parse().ok()
}

/// Cleaning, the request bodies and `apply`.
#[cfg(test)]
mod tests {
    use super::*;

    /// Ticket 1: "Old", customer 2, assignee 3, open, no body.
    fn ticket() -> Ticket {
        Ticket {
            id: 1,
            customer_id: Some(2),
            assignee_id: Some(3),
            title: "Old".into(),
            body: None,
            status: Status::Open,
            created_at: None,
        }
    }

    /// One character from each refused class is dropped while tab and newline stay, and the cap counts
    /// characters: five of a string with two-byte letters.
    #[test]
    fn clean_matches_textsafe() {
        assert_eq!(
            clean("a\u{0}b\tc\nd\u{7f}e\u{9b}f\u{202e}g\u{2066}h", 240),
            "ab\tc\ndefgh"
        );
        assert_eq!(clean("héllo wörld", 5), "héllo");
    }

    /// A title of spaces around a NUL is empty once trimmed and cleaned.
    #[test]
    fn empty_title_is_refused() {
        let d = Draft {
            title: "  \u{0} ".into(),
            ..Default::default()
        };
        assert!(d.create_body("2026-01-01T00:00").is_err());
    }

    /// No status gives "open", the title is trimmed, and the empty body and missing assignee are null.
    #[test]
    fn create_body_defaults_to_open() {
        let d = Draft {
            title: " Lorem ipsum ".into(),
            customer_id: Some(4),
            ..Default::default()
        };
        let v = d.create_body("2026-01-01T00:00").unwrap();
        assert_eq!(v["title"], "Lorem ipsum");
        assert_eq!(v["status"], "open");
        assert_eq!(v["customer_id"], 4);
        assert!(v["body"].is_null());
        assert!(v["assignee_id"].is_null());
    }

    /// An unchanged draft gives None; a moved and unassigned one gives exactly status and a null assignee.
    #[test]
    fn patch_body_only_holds_changes() {
        let old = ticket();
        let same = Draft {
            title: "Old".into(),
            customer_id: Some(2),
            assignee_id: Some(3),
            ..Default::default()
        };
        assert_eq!(same.patch_body(&old).unwrap(), None);
        let moved = Draft {
            status: Some(Status::Closed),
            assignee_id: None,
            ..same.clone()
        };
        let v = moved.patch_body(&old).unwrap().unwrap();
        assert_eq!(v.as_object().unwrap().len(), 2);
        assert_eq!(v["status"], "closed");
        assert!(v["assignee_id"].is_null());
    }

    /// apply overwrites title, body and status, and a draft without a customer clears it.
    #[test]
    fn apply_updates_the_ticket() {
        let mut t = ticket();
        let d = Draft {
            title: "New".into(),
            body: "dolor".into(),
            status: Some(Status::InProgress),
            ..Default::default()
        };
        d.apply(&mut t).unwrap();
        assert_eq!(
            (t.title.as_str(), t.body.as_deref(), t.status, t.customer_id),
            ("New", Some("dolor"), Status::InProgress, None)
        );
    }

    /// A number is an id; the empty choice and junk are None.
    #[test]
    fn parse_id_reads_select_values() {
        assert_eq!(parse_id("12"), Some(12));
        assert_eq!(parse_id(""), None);
        assert_eq!(parse_id("x"), None);
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=fc02f9703d82ca0487b8e89bf98a369c55ded89b8f231a98c54dbbde819677e6 ====
