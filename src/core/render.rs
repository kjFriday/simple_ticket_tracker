// ==== ACME Inc. portfolio ===============================================
// Project: simple_ticket_tracker | File: src/core/render.rs
// About: HTML for the board.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! HTML for the board. Every value from data passes `escape` before it reaches the page.

use super::board::{columns, Filter};
use super::model::{name_of, Person, Ticket};

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn card(t: &Ticket, customers: &[Person], employees: &[Person]) -> String {
    let left = if t.status.prev().is_some() { r#"<button data-act="prev" title="Move left">&larr;</button>"# } else { "" };
    let right = if t.status.next().is_some() { r#"<button data-act="next" title="Move right">&rarr;</button>"# } else { "" };
    let body = t.body.as_deref().map(|b| format!(r#"<p class="body">{}</p>"#, escape(b))).unwrap_or_default();
    let day = t.created_at.as_deref().map(|d| d.get(..10).unwrap_or(d)).unwrap_or("");
    format!(
        concat!(
            r#"<article class="card" data-id="{id}">"#,
            r#"<header><span class="num">#{id}</span><time>{day}</time></header>"#,
            r#"<h3>{title}</h3>{body}"#,
            r#"<dl><dt>Customer</dt><dd>{cust}</dd><dt>Assignee</dt><dd>{who}</dd></dl>"#,
            r#"<footer>{left}<button data-act="edit">Edit</button><button data-act="delete" class="danger">Delete</button>{right}</footer>"#,
            "</article>"
        ),
        id = t.id,
        day = escape(day),
        title = escape(&t.title),
        body = body,
        cust = escape(name_of(customers, t.customer_id)),
        who = escape(name_of(employees, t.assignee_id)),
        left = left,
        right = right,
    )
}

pub fn board(tickets: &[Ticket], filter: &Filter, customers: &[Person], employees: &[Person]) -> String {
    columns(tickets, filter, customers)
        .into_iter()
        .map(|(status, col)| {
            let cards: String = col.iter().map(|t| card(t, customers, employees)).collect();
            format!(
                r#"<section class="column" data-status="{s}"><h2>{label} <span class="count">{n}</span></h2>{cards}</section>"#,
                s = status.as_str(),
                label = status.label(),
                n = col.len(),
                cards = cards,
            )
        })
        .collect()
}

/// `<option>`s for a person picker; `blank` labels the empty choice.
pub fn options(people: &[Person], selected: Option<i64>, blank: &str) -> String {
    let mut out = format!(r#"<option value="">{}</option>"#, escape(blank));
    let mut sorted: Vec<&Person> = people.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    for p in sorted {
        let sel = if Some(p.id) == selected { " selected" } else { "" };
        let extra = p.role.as_deref().or(p.city.as_deref()).map(|r| format!(" ({})", escape(r))).unwrap_or_default();
        out.push_str(&format!(r#"<option value="{}"{}>{}{}</option>"#, p.id, sel, escape(&p.name), extra));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Status;

    #[test]
    fn escape_covers_html_specials() {
        assert_eq!(escape(r#"<a href="x">'&'</a>"#), "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
    }

    #[test]
    fn board_escapes_titles_and_counts_columns() {
        let ts = vec![Ticket { id: 7, customer_id: None, assignee_id: None, title: "<script>".into(), body: Some("a&b".into()), status: Status::Open, created_at: Some("2026-05-06T07:08".into()) }];
        let html = board(&ts, &Filter::default(), &[], &[]);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("a&amp;b"));
        assert!(html.contains("2026-05-06</time>"));
        assert_eq!(html.matches(r#"class="column""#).count(), 3);
        // first column: no left arrow, has right arrow
        assert!(!html.contains(r#"data-act="prev""#));
        assert!(html.contains(r#"data-act="next""#));
    }

    #[test]
    fn options_sort_and_select() {
        let p = vec![
            Person { id: 2, name: "Zoe Adams".into(), role: Some("Sales".into()), email: None, city: None },
            Person { id: 1, name: "Aiden <B>".into(), role: None, email: None, city: Some("Lyon".into()) },
        ];
        let html = options(&p, Some(2), "Anyone");
        assert!(html.starts_with(r#"<option value="">Anyone</option><option value="1">Aiden &lt;B&gt; (Lyon)"#));
        assert!(html.contains(r#"<option value="2" selected>Zoe Adams (Sales)"#));
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=773ae9a6f546d7eeb6349359c66a639a991933b2149098abdc96d92f589169a2 ====
