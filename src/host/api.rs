// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/host/api.rs
// About: The page's data API, answered in-process from the shared SQLite file.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! The page's data API, answered in-process from the shared SQLite file.
//!
//! It keeps the CRUD hub's contract: `GET /api/{table}?limit&offset` (newest first, `X-Total-Count`),
//! `GET /api/{table}/{id}`, and for the one writable table `POST`, `PATCH /{id}`, `DELETE /{id}` with 201/200, 400, 404 and 409. Every string
//! is cleaned with the hub's textsafe rules before it is stored. No database: 503, and the page falls
//! back to its mock snapshot.
//!
//! host/mod.rs's `serve` passes every `/api/...` request here with a connection it opens for that one
//! request, and calls `open` and `check_schema` when it picks the database. Nothing here knows about
//! HTTP libraries or the window: a request is a method, a target string and body bytes, and the answer
//! is a `Reply`, so the tests drive it against an in-memory database.
//!
//! Status codes from `handle`:
//! - 200 a row, a list (with `X-Total-Count`), or `{"updated": n}` / `{"deleted": n}`
//! - 201 `{"id": n}` for a created row
//! - 400 bad paging, a body that is not a JSON object, no known column, or an unsupported value
//! - 404 an unknown table, a bad path or no such row
//! - 405 a write to a read-only table, or another method
//! - 409 a constraint the database refused (NOT NULL, foreign key, unique, check)
//! - 500 any other database error
//! - 503 no database
//!
//! SQL text is built with format! from table and column names. That is safe only because the names
//! come from the program's static `Tables` list and never from the request; every value from the
//! request is bound as a `?` parameter.

use crate::core::validate::{clean, LIMIT};
// `Sql` is rusqlite's owned value, kept apart from serde_json's `Value`
use rusqlite::types::{Value as Sql, ValueRef};
use rusqlite::{params_from_iter, Connection, ErrorCode};
use serde_json::{json, Map, Value};

/// Which tables the page may read, their columns, and the one it may change.
///
/// Filled in by main.rs. Every listed column must exist (`check_schema`), and only these columns are
/// ever read or written; `id` is implied for every table.
pub struct Tables {
    /// (table, its columns other than id) for each readable table.
    pub read: &'static [(&'static str, &'static [&'static str])],
    /// The one table POST, PATCH and DELETE may change; None makes every table read-only.
    pub write: Option<&'static str>,
}

/// An HTTP answer before it is handed to wry: status, extra headers and body bytes.
pub struct Reply {
    /// HTTP status code.
    pub status: u16,
    /// Headers besides the ones `serve` adds (it adds Cache-Control).
    pub headers: Vec<(&'static str, String)>,
    /// The body, JSON text for every reply made here.
    pub body: Vec<u8>,
}

// Building JSON replies.
impl Reply {
    /// `v` as a JSON reply with that status.
    pub fn json(status: u16, v: &Value) -> Reply {
        Reply {
            status,
            headers: vec![("Content-Type", "application/json".into())],
            body: v.to_string().into_bytes(),
        }
    }
    /// `{"error": msg}` with that status, the hub's error shape.
    fn error(status: u16, msg: &str) -> Reply {
        Reply::json(status, &json!({ "error": msg }))
    }
}

/// Splits `/api/x/y?a=1&b=2` into path segments after /api and query pairs.
///
/// Empty segments are dropped, so `/api//x/` is `["x"]`. A query part without `=` is skipped. Nothing is
/// percent-decoded; the page only sends plain numbers in queries.
pub fn parse_target(target: &str) -> (Vec<String>, Vec<(String, String)>) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let segs = path
        .trim_start_matches("/api")
        .split('/')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let pairs = query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    (segs, pairs)
}

/// One SQLite cell as JSON: integers and reals as numbers, text as a string (invalid UTF-8 replaced),
/// and NULL as null. A BLOB also gives null, since JSON has no bytes type and no listed column holds one.
fn json_of(v: ValueRef) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
        ValueRef::Blob(_) => Value::Null,
    }
}

/// Runs `sql` with `args` bound to its `?`s and returns each row as a JSON object.
///
/// The SELECT must list `id` first and then `cols` in order: column 0 is read as "id" and column i + 1
/// under `cols[i]`. `handle` builds its SELECT that way.
fn rows(conn: &Connection, sql: &str, args: &[Sql], cols: &[&str]) -> rusqlite::Result<Vec<Value>> {
    let mut stmt = conn.prepare(sql)?;
    let mut out = Vec::new();
    let mut q = stmt.query(params_from_iter(args.iter()))?;
    while let Some(r) = q.next()? {
        let mut m = Map::new();
        m.insert("id".into(), json_of(r.get_ref(0)?));
        for (i, c) in cols.iter().enumerate() {
            m.insert((*c).into(), json_of(r.get_ref(i + 1)?));
        }
        out.push(Value::Object(m));
    }
    Ok(out)
}

/// The writable columns present in body, cleaned; an empty list means nothing known was sent.
///
/// Keys that are not in `cols` (including "id") are skipped without an error. Values map to SQLite
/// types: null to NULL, a string to TEXT after `clean` (so capped at `LIMIT`), a whole number to
/// INTEGER, any other number to REAL, and a bool to 0 or 1. An array or an object is an error naming
/// the column.
fn known(cols: &[&str], body: &Map<String, Value>) -> Result<Vec<(String, Sql)>, String> {
    let mut out = Vec::new();
    for c in cols {
        let Some(v) = body.get(*c) else { continue };
        let sql = match v {
            Value::Null => Sql::Null,
            Value::String(s) => Sql::Text(clean(s, LIMIT)),
            Value::Number(n) if n.is_i64() => Sql::Integer(n.as_i64().unwrap()),
            Value::Number(n) => Sql::Real(n.as_f64().unwrap_or(0.0)),
            Value::Bool(b) => Sql::Integer(*b as i64),
            _ => return Err(format!("{c}: unsupported value")),
        };
        out.push((c.to_string(), sql));
    }
    Ok(out)
}

/// A database error as a reply: 409 for a refused constraint (the caller's data is at fault), else 500.
/// The message is SQLite's own text.
fn db_error(e: rusqlite::Error) -> Reply {
    match e.sqlite_error_code() {
        Some(ErrorCode::ConstraintViolation) => Reply::error(409, &e.to_string()),
        _ => Reply::error(500, &e.to_string()),
    }
}

/// Answers one request. `conn` is None when there is no usable database.
///
/// `target` is the path and query (`/api/tickets?limit=50`), `body` the request body. The status codes
/// are listed in the module doc.
///
/// Steps:
/// 1. Answer 503 when there is no database, before looking at anything else.
/// 2. Find the table among `tables.read` (else 404) and the optional numeric id (a third segment or a
/// non-number is 404).
/// 3. Dispatch on method and id: list, get one, refuse writes to a read-only table (405), insert,
/// update or delete.
///
/// Lists are ordered by id descending (newest first, as AUTOINCREMENT ids only grow) and paged with
/// `limit` (1-500, default 500) and `offset`; `X-Total-Count` gives the row count of the whole table.
pub fn handle(tables: &Tables, conn: Option<&Connection>, method: &str, target: &str, body: &[u8]) -> Reply {
    let (segs, query) = parse_target(target);
    let Some(conn) = conn else {
        return Reply::error(503, "no database");
    };
    let Some(table) = segs.first() else {
        return Reply::error(404, "not found");
    };
    let Some(&(name, cols)) = tables.read.iter().find(|(t, _)| t == table) else {
        return Reply::error(404, "unknown table");
    };
    let id = match segs.get(1).map(|s| s.parse::<i64>()) {
        None => None,
        Some(Ok(id)) if segs.len() == 2 => Some(id),
        _ => return Reply::error(404, "not found"),
    };
    let writable = tables.write == Some(name);
    // id first, then cols in order: the layout `rows` reads
    let select = format!("SELECT id, {} FROM {name}", cols.join(", "));
    // arms are tried in order: the read-only guard has to come before the POST and PATCH arms
    match (method, id) {
        ("GET", None) => {
            let get = |k: &str| query.iter().find(|(q, _)| q == k).map(|(_, v)| v.as_str());
            // 500 is the hub's cap too; the page asks for pages of 500 until one comes back short
            let limit = match get("limit").map(str::parse::<i64>) {
                None => 500,
                Some(Ok(n)) if (1..=500).contains(&n) => n,
                _ => return Reply::error(400, "limit must be 1-500"),
            };
            let offset = match get("offset").map(str::parse::<i64>) {
                None => 0,
                Some(Ok(n)) if n >= 0 => n,
                _ => return Reply::error(400, "offset must be 0 or more"),
            };
            let list = rows(
                conn,
                &format!("{select} ORDER BY id DESC LIMIT ? OFFSET ?"),
                &[Sql::Integer(limit), Sql::Integer(offset)],
                cols,
            );
            let total: rusqlite::Result<i64> =
                conn.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r.get(0));
            match (list, total) {
                (Ok(list), Ok(total)) => {
                    let mut r = Reply::json(200, &Value::Array(list));
                    r.headers.push(("X-Total-Count", total.to_string()));
                    r
                }
                (Err(e), _) | (_, Err(e)) => db_error(e),
            }
        }
        ("GET", Some(id)) => match rows(conn, &format!("{select} WHERE id = ?"), &[Sql::Integer(id)], cols) {
            Ok(mut found) if !found.is_empty() => Reply::json(200, &found.remove(0)),
            Ok(_) => Reply::error(404, "not found"),
            Err(e) => db_error(e),
        },
        ("POST" | "PATCH" | "DELETE", _) if !writable => Reply::error(405, "read-only"),
        // POST to the table creates; PATCH to a row changes only the columns the body names
        ("POST", None) | ("PATCH", Some(_)) => {
            let Ok(Value::Object(obj)) = serde_json::from_slice::<Value>(body) else {
                return Reply::error(400, "body must be a JSON object");
            };
            let fields = match known(cols, &obj) {
                Ok(f) if !f.is_empty() => f,
                Ok(_) => return Reply::error(400, "no known columns"),
                Err(e) => return Reply::error(400, &e),
            };
            let names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            let mut values: Vec<Sql> = fields.into_iter().map(|(_, v)| v).collect();
            if let Some(id) = id {
                // the id binds to the WHERE's ?, after the SET values
                values.push(Sql::Integer(id));
                let sets: Vec<String> = names.iter().map(|n| format!("{n} = ?")).collect();
                match conn.execute(
                    &format!("UPDATE {name} SET {} WHERE id = ?", sets.join(", ")),
                    params_from_iter(values.iter()),
                ) {
                    Ok(0) => Reply::error(404, "not found"),
                    Ok(n) => Reply::json(200, &json!({ "updated": n })),
                    Err(e) => db_error(e),
                }
            } else {
                // columns the body leaves out get the schema's defaults (status 'open')
                let marks = vec!["?"; names.len()].join(", ");
                match conn.execute(
                    &format!("INSERT INTO {name} ({}) VALUES ({marks})", names.join(", ")),
                    params_from_iter(values.iter()),
                ) {
                    Ok(_) => Reply::json(201, &json!({ "id": conn.last_insert_rowid() })),
                    Err(e) => db_error(e),
                }
            }
        }
        ("DELETE", Some(id)) => match conn.execute(&format!("DELETE FROM {name} WHERE id = ?"), [id]) {
            Ok(0) => Reply::error(404, "not found"),
            Ok(n) => Reply::json(200, &json!({ "deleted": n })),
            Err(e) => db_error(e),
        },
        _ => Reply::error(405, "method not allowed"),
    }
}

/// Fails unless every table and column in `tables` exists (the hub owns the schema; this never changes it).
///
/// The error names the first table that is missing ("no tickets table") or lacks columns ("tickets lacks
/// body, status"); host/mod.rs shows it as the reason the page runs on its mock snapshot.
pub fn check_schema(conn: &Connection, tables: &Tables) -> Result<(), String> {
    for (t, cols) in tables.read {
        // pragma_table_info is the table-valued form of PRAGMA table_info: one row per column of the
        // named table, none when there is no such table; unlike the PRAGMA statement it takes a parameter
        let mut stmt = conn
            .prepare("SELECT name FROM pragma_table_info(?)")
            .map_err(|e| e.to_string())?;
        let have: Vec<String> = stmt
            .query_map([t], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        if have.is_empty() {
            return Err(format!("no {t} table"));
        }
        let missing: Vec<&str> = std::iter::once("id")
            .chain(cols.iter().copied())
            .filter(|c| !have.iter().any(|h| h == c))
            .collect();
        if !missing.is_empty() {
            return Err(format!("{t} lacks {}", missing.join(", ")));
        }
    }
    Ok(())
}

/// Opens the shared database (never creating it) with busy_timeout and foreign keys.
///
/// `read_only` opens it so that no write can succeed (used to check a file, and by read-only programs).
/// Errors: no file at `path`, or SQLite's own message.
///
/// Steps:
/// 1. Refuse a path that is not an existing file; neither open mode includes SQLITE_OPEN_CREATE either.
/// 2. Open it read-only or read-write, without SQLite's per-connection mutex.
/// 3. Wait up to 5 s on a lock another program holds, instead of failing at once with SQLITE_BUSY.
/// 4. Turn on foreign-key enforcement, which SQLite leaves off for every new connection.
pub fn open(path: &std::path::Path, read_only: bool) -> Result<Connection, String> {
    use rusqlite::OpenFlags;
    if !path.is_file() {
        return Err(format!("no database at {}", path.display()));
    }
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    // NO_MUTEX is safe: a rusqlite Connection is not Sync, so only one thread uses it at a time
    let conn = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|e| e.to_string())?;
    // the same 5 s wait the hub sets with PRAGMA busy_timeout=5000
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "foreign_keys", 1).map_err(|e| e.to_string())?;
    Ok(conn)
}

/// The REST contract against an in-memory copy of the hub's schema.
#[cfg(test)]
mod tests {
    use super::*;

    // tickets writable, customers read-only: the shape main.rs uses, minus employees
    const TABLES: Tables = Tables {
        read: &[
            (
                "tickets",
                &["customer_id", "assignee_id", "title", "body", "status", "created_at"],
            ),
            ("customers", &["name", "email", "city"]),
        ],
        write: Some("tickets"),
    };

    /// An in-memory database with the hub's schema, customers Ada (1) and Bo (2), and open tickets T1-T7
    /// with ids 1-7, all of customer 1.
    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../mock/schema.sql")).unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON; INSERT INTO customers (name) VALUES ('Ada'), ('Bo');")
            .unwrap();
        for i in 1..=7 {
            conn.execute(
                "INSERT INTO tickets (customer_id, title, status) VALUES (1, ?, 'open')",
                [format!("T{i}")],
            )
            .unwrap();
        }
        conn
    }

    /// The reply body parsed as JSON.
    fn body(r: &Reply) -> Value {
        serde_json::from_slice(&r.body).unwrap()
    }

    /// limit 3 after skipping 2 of ids 7..1 gives 5, 4, 3 and a total of 7; limit 900 is over the cap;
    /// a missing row and an unlisted table are 404.
    #[test]
    fn lists_newest_first_with_paging_and_total() {
        let c = db();
        let r = handle(&TABLES, Some(&c), "GET", "/api/tickets?limit=3&offset=2", b"");
        assert_eq!(r.status, 200);
        let ids: Vec<i64> = body(&r)
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![5, 4, 3]);
        assert!(r.headers.contains(&("X-Total-Count", "7".into())));
        assert_eq!(
            handle(&TABLES, Some(&c), "GET", "/api/tickets?limit=900", b"").status,
            400
        );
        assert_eq!(handle(&TABLES, Some(&c), "GET", "/api/tickets/3", b"").status, 200);
        assert_eq!(handle(&TABLES, Some(&c), "GET", "/api/tickets/99", b"").status, 404);
        assert_eq!(handle(&TABLES, Some(&c), "GET", "/api/orders", b"").status, 404);
    }

    /// A POST with an RLO character and an unknown key stores the cleaned title and the default status;
    /// PATCH reports one row; a second DELETE of the same row is 404.
    #[test]
    fn create_patch_delete_round_trip_with_cleaning() {
        let c = db();
        let sent = format!(
            "{{\"title\":\"Hi{} there\",\"customer_id\":2,\"nope\":1}}",
            char::from_u32(0x202e).unwrap()
        );
        let r = handle(&TABLES, Some(&c), "POST", "/api/tickets", sent.as_bytes());
        assert_eq!(r.status, 201);
        let id = body(&r)["id"].as_i64().unwrap();
        let got = body(&handle(&TABLES, Some(&c), "GET", &format!("/api/tickets/{id}"), b""));
        assert_eq!(got["title"], "Hi there");
        assert_eq!(got["status"], "open"); // the schema's default
        let r = handle(
            &TABLES,
            Some(&c),
            "PATCH",
            &format!("/api/tickets/{id}"),
            br#"{"status":"closed"}"#,
        );
        assert_eq!((r.status, body(&r)["updated"].as_i64()), (200, Some(1)));
        assert_eq!(
            handle(&TABLES, Some(&c), "DELETE", &format!("/api/tickets/{id}"), b"").status,
            200
        );
        assert_eq!(
            handle(&TABLES, Some(&c), "DELETE", &format!("/api/tickets/{id}"), b"").status,
            404
        );
    }

    /// Each error code: a non-object body, no known column and an object value are 400; a missing NOT
    /// NULL title and an unknown customer are 409; a write to customers is 405; no database is 503.
    #[test]
    fn bad_input_and_constraints() {
        let c = db();
        assert_eq!(handle(&TABLES, Some(&c), "POST", "/api/tickets", b"[1]").status, 400);
        assert_eq!(
            handle(&TABLES, Some(&c), "POST", "/api/tickets", br#"{"nope":1}"#).status,
            400
        );
        assert_eq!(
            handle(&TABLES, Some(&c), "POST", "/api/tickets", br#"{"body":"no title"}"#).status,
            409
        ); // NOT NULL
        assert_eq!(
            handle(
                &TABLES,
                Some(&c),
                "POST",
                "/api/tickets",
                br#"{"title":"x","customer_id":999}"#
            )
            .status,
            409
        ); // FK
        assert_eq!(
            handle(&TABLES, Some(&c), "PATCH", "/api/tickets/1", br#"{"title":{"a":1}}"#).status,
            400
        );
        assert_eq!(
            handle(&TABLES, Some(&c), "POST", "/api/customers", br#"{"name":"x"}"#).status,
            405
        );
        assert_eq!(handle(&TABLES, None, "GET", "/api/tickets", b"").status, 503);
    }

    /// The real schema passes and a table missing columns is named; opening a missing file fails without
    /// creating it.
    #[test]
    fn schema_check_and_open() {
        let c = db();
        assert!(check_schema(&c, &TABLES).is_ok());
        let bad = Connection::open_in_memory().unwrap();
        bad.execute_batch("CREATE TABLE tickets (id INTEGER, title TEXT); CREATE TABLE customers (id INTEGER)")
            .unwrap();
        assert!(check_schema(&bad, &TABLES).unwrap_err().starts_with("tickets lacks"));
        let missing = std::env::temp_dir().join("generic-company-tickets-test-missing.db");
        assert!(open(&missing, true).is_err());
        assert!(!missing.exists());
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=8feb3a671667d8bb6e76fc7860a9438672d5939e207ecfba15244016cabe95be ====
