//! Form input → hub JSON bodies. Mirrors the hub's textsafe rules so mock mode stores what the hub would.

use super::model::{Status, Ticket};
use serde_json::{json, Map, Value};

/// textsafe.LIMIT in the hub: string columns are capped at this many characters.
pub const LIMIT: usize = 240;

/// Drop the characters textsafe refuses (C0 except tab/newline, DEL, C1, U+2028/9, bidi controls), then cap.
pub fn clean(text: &str, limit: usize) -> String {
    text.chars().filter(|&c| !refused(c)).take(limit).collect()
}

fn refused(c: char) -> bool {
    let u = c as u32;
    (u < 0x20 && c != '\t' && c != '\n')
        || (0x7F..=0x9F).contains(&u)
        || u == 0x2028
        || u == 0x2029
        || (0x202A..=0x202E).contains(&u)
        || (0x2066..=0x2069).contains(&u)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Draft {
    pub title: String,
    pub body: String,
    pub customer_id: Option<i64>,
    pub assignee_id: Option<i64>,
    pub status: Option<Status>,
}

impl Draft {
    /// Cleaned and checked; the title must have something left after trimming.
    pub fn checked(&self) -> Result<Draft, String> {
        let title = clean(self.title.trim(), LIMIT);
        if title.is_empty() {
            return Err("A ticket needs a title.".into());
        }
        Ok(Draft { title, body: clean(self.body.trim(), LIMIT), ..self.clone() })
    }

    /// POST body for a new ticket; `now` is the created_at stamp (yyyy-mm-ddThh:mm).
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
pub fn parse_id(s: &str) -> Option<i64> {
    s.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket() -> Ticket {
        Ticket { id: 1, customer_id: Some(2), assignee_id: Some(3), title: "Old".into(), body: None, status: Status::Open, created_at: None }
    }

    #[test]
    fn clean_matches_textsafe() {
        assert_eq!(clean("a\u{0}b\tc\nd\u{7f}e\u{9b}f\u{202e}g\u{2066}h", 240), "ab\tc\ndefgh");
        assert_eq!(clean("héllo wörld", 5), "héllo");
    }

    #[test]
    fn empty_title_is_refused() {
        let d = Draft { title: "  \u{0} ".into(), ..Default::default() };
        assert!(d.create_body("2026-01-01T00:00").is_err());
    }

    #[test]
    fn create_body_defaults_to_open() {
        let d = Draft { title: " Lorem ipsum ".into(), customer_id: Some(4), ..Default::default() };
        let v = d.create_body("2026-01-01T00:00").unwrap();
        assert_eq!(v["title"], "Lorem ipsum");
        assert_eq!(v["status"], "open");
        assert_eq!(v["customer_id"], 4);
        assert!(v["body"].is_null());
        assert!(v["assignee_id"].is_null());
    }

    #[test]
    fn patch_body_only_holds_changes() {
        let old = ticket();
        let same = Draft { title: "Old".into(), customer_id: Some(2), assignee_id: Some(3), ..Default::default() };
        assert_eq!(same.patch_body(&old).unwrap(), None);
        let moved = Draft { status: Some(Status::Closed), assignee_id: None, ..same.clone() };
        let v = moved.patch_body(&old).unwrap().unwrap();
        assert_eq!(v.as_object().unwrap().len(), 2);
        assert_eq!(v["status"], "closed");
        assert!(v["assignee_id"].is_null());
    }

    #[test]
    fn apply_updates_the_ticket() {
        let mut t = ticket();
        let d = Draft { title: "New".into(), body: "dolor".into(), status: Some(Status::InProgress), ..Default::default() };
        d.apply(&mut t).unwrap();
        assert_eq!((t.title.as_str(), t.body.as_deref(), t.status, t.customer_id), ("New", Some("dolor"), Status::InProgress, None));
    }

    #[test]
    fn parse_id_reads_select_values() {
        assert_eq!(parse_id("12"), Some(12));
        assert_eq!(parse_id(""), None);
        assert_eq!(parse_id("x"), None);
    }
}
