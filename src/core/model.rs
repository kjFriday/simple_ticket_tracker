//! Rows as the hub serves them (`GET /api/{table}`); unknown columns are ignored.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Open,
    InProgress,
    Closed,
}

impl Status {
    /// Board order, left to right.
    pub const ALL: [Status; 3] = [Status::Open, Status::InProgress, Status::Closed];

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::InProgress => "in_progress",
            Status::Closed => "closed",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Open => "Open",
            Status::InProgress => "In progress",
            Status::Closed => "Closed",
        }
    }

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

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A status the hub stores as free text: anything unrecognised lands in Open rather than failing the load.
fn lenient_status<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Status, D::Error> {
    let s: Option<String> = Option::deserialize(d)?;
    Ok(s.and_then(|s| Status::parse(&s).ok()).unwrap_or(Status::Open))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ticket {
    pub id: i64,
    #[serde(default)]
    pub customer_id: Option<i64>,
    #[serde(default)]
    pub assignee_id: Option<i64>,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default = "default_status", deserialize_with = "lenient_status")]
    pub status: Status,
    #[serde(default)]
    pub created_at: Option<String>,
}

fn default_status() -> Status {
    Status::Open
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
}

pub fn parse_tickets(json: &str) -> Result<Vec<Ticket>, String> {
    serde_json::from_str(json).map_err(|e| format!("tickets: {e}"))
}

pub fn parse_people(json: &str) -> Result<Vec<Person>, String> {
    serde_json::from_str(json).map_err(|e| format!("people: {e}"))
}

pub fn name_of(people: &[Person], id: Option<i64>) -> &str {
    id.and_then(|id| people.iter().find(|p| p.id == id))
        .map(|p| p.name.as_str())
        .unwrap_or("Unassigned")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips() {
        for s in Status::ALL {
            assert_eq!(Status::parse(s.as_str()), Ok(s));
        }
        assert!(Status::parse("done").is_err());
    }

    #[test]
    fn next_and_prev_stop_at_the_edges() {
        assert_eq!(Status::Open.prev(), None);
        assert_eq!(Status::Open.next(), Some(Status::InProgress));
        assert_eq!(Status::InProgress.prev(), Some(Status::Open));
        assert_eq!(Status::Closed.next(), None);
    }

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

    #[test]
    fn name_of_falls_back() {
        let p = parse_people(r#"[{"id":1,"name":"Maria Garcia"}]"#).unwrap();
        assert_eq!(name_of(&p, Some(1)), "Maria Garcia");
        assert_eq!(name_of(&p, Some(9)), "Unassigned");
        assert_eq!(name_of(&p, None), "Unassigned");
    }
}
