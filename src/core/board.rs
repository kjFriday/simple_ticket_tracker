//! Board state: filtering, columns, and the in-memory store used in mock (standalone) mode.

use super::model::{Person, Status, Ticket};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    pub assignee: Option<i64>,
    pub customer: Option<i64>,
    pub text: String,
}

impl Filter {
    pub fn matches(&self, t: &Ticket, people: &[Person]) -> bool {
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
        let customer = t
            .customer_id
            .and_then(|id| people.iter().find(|p| p.id == id))
            .map(|p| p.name.to_lowercase())
            .unwrap_or_default();
        t.title.to_lowercase().contains(&q)
            || t.body.as_deref().unwrap_or("").to_lowercase().contains(&q)
            || customer.contains(&q)
            || format!("#{}", t.id) == q
    }
}

/// Tickets per board column, newest first (created_at, then id).
pub fn columns<'a>(tickets: &'a [Ticket], filter: &Filter, customers: &[Person]) -> Vec<(Status, Vec<&'a Ticket>)> {
    Status::ALL
        .iter()
        .map(|&s| {
            let mut col: Vec<&Ticket> = tickets.iter().filter(|t| t.status == s && filter.matches(t, customers)).collect();
            col.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
            (s, col)
        })
        .collect()
}

/// Mock-mode store: the same create/update/delete the hub offers, kept in memory.
#[derive(Debug, Clone, Default)]
pub struct Store {
    pub tickets: Vec<Ticket>,
}

impl Store {
    pub fn new(tickets: Vec<Ticket>) -> Store {
        Store { tickets }
    }

    pub fn create(&mut self, mut t: Ticket) -> i64 {
        t.id = self.tickets.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        let id = t.id;
        self.tickets.push(t);
        id
    }

    pub fn get_mut(&mut self, id: i64) -> Option<&mut Ticket> {
        self.tickets.iter_mut().find(|t| t.id == id)
    }

    pub fn delete(&mut self, id: i64) -> bool {
        let before = self.tickets.len();
        self.tickets.retain(|t| t.id != id);
        self.tickets.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: i64, status: Status, assignee: Option<i64>, created: &str, title: &str) -> Ticket {
        Ticket { id, customer_id: Some(1), assignee_id: assignee, title: title.into(), body: None, status, created_at: Some(created.into()) }
    }

    fn sample() -> Vec<Ticket> {
        vec![
            t(1, Status::Open, Some(10), "2026-01-01T09:00", "Login fails"),
            t(2, Status::Open, Some(11), "2026-03-01T09:00", "Refund"),
            t(3, Status::Closed, Some(10), "2026-02-01T09:00", "Invoice"),
            t(4, Status::InProgress, None, "2026-02-01T09:00", "Shipping"),
        ]
    }

    #[test]
    fn groups_into_three_columns_newest_first() {
        let ts = sample();
        let cols = columns(&ts, &Filter::default(), &[]);
        let counts: Vec<usize> = cols.iter().map(|(_, c)| c.len()).collect();
        assert_eq!(counts, vec![2, 1, 1]);
        assert_eq!(cols[0].1[0].id, 2);
    }

    #[test]
    fn filters_by_assignee_text_and_customer_name() {
        let ts = sample();
        let people = vec![Person { id: 1, name: "Maria Garcia".into(), role: None, email: None, city: None }];
        let f = Filter { assignee: Some(10), ..Default::default() };
        assert_eq!(ts.iter().filter(|t| f.matches(t, &people)).count(), 2);
        let f = Filter { text: "  REFUND ".into(), ..Default::default() };
        assert_eq!(ts.iter().filter(|t| f.matches(t, &people)).count(), 1);
        let f = Filter { text: "garcia".into(), ..Default::default() };
        assert_eq!(ts.iter().filter(|t| f.matches(t, &people)).count(), 4);
        let f = Filter { text: "#3".into(), ..Default::default() };
        assert_eq!(ts.iter().filter(|t| f.matches(t, &people)).count(), 1);
    }

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
