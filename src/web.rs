// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/web.rs
// About: Page wiring.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Page wiring. Two sources: the shared database, served to this page by the desktop program at the same
//! origin (`/api/...`; every edit is a real POST/PATCH/DELETE, logged in the status bar), or the embedded
//! mock snapshot (edits stay in memory), used when there is no database or the page runs in a browser.
//!
//! Compiled only for wasm32 (see lib.rs). wasm-bindgen runs `start` as soon as www/main.js has loaded
//! the module; after that everything is driven by DOM events registered in `wire`. The board logic
//! itself lives in `core` (filters, columns, validation, HTML); this file only moves data between that
//! logic, the DOM of www/index.html and the `/api` endpoints.
//!
//! Main flow:
//! 1. `start` builds the `App` on the mock snapshot, wires the events, then tries the shared database
//!    (`load_db`) unless the URL says `?source=mock`.
//! 2. `load_db` asks `/api/source` whether a database is there; with one it pages every ticket,
//!    customer and employee in through `fetch_all`, else it falls back to `use_mock`.
//! 3. A user action (editor form, card button) becomes a `Change`, which `apply` sends to
//!    `apply_mock` or `apply_db`; on success the board is redrawn, and either way the status bar says
//!    what happened.
//! 4. `draw` re-renders the whole board from the `App` after every change or filter edit.
//!
//! In the desktop program the page's origin is `generic-company://localhost`, so a relative URL such
//! as `/api/tickets` goes to the program's custom protocol handler (src/host/mod.rs `serve`), not to
//! the network. In a plain browser the same requests reach whatever static server serves `www/`,
//! which has no `/api`, so the page stays on the mock snapshot.
//!
//! Types: `Source` (where the data comes from), `App` (all page state), `Change` (one edit). Start
//! reading at `start`, then `load_db` and `apply`.

use crate::core::board::{Filter, Store};
use crate::core::model::{parse_people, parse_tickets, Person, Status, Ticket};
use crate::core::render;
use crate::core::validate::{parse_id, Draft};
use serde_json::Value;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    Document, Element, Event, HtmlDialogElement, HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement, Request,
    RequestInit, Response,
};

/// The mock snapshot's tickets: a JSON array of rows exported from the hub, built into the wasm.
const MOCK_TICKETS: &str = include_str!("../mock/tickets.json");
/// The mock snapshot's customers (JSON rows), for names in the cards and the customer pickers.
const MOCK_CUSTOMERS: &str = include_str!("../mock/customers.json");
/// The mock snapshot's employees (JSON rows), for assignee names and pickers.
const MOCK_EMPLOYEES: &str = include_str!("../mock/employees.json");
/// Prefix for every `/api` URL; empty, so requests are relative to the page.
const API_ORIGIN: &str = ""; // same origin: the desktop program answers /api/... from the shared database
/// Rows asked for per `/api` request when paging (`?limit`).
const PAGE: usize = 500; // the largest ?limit
/// `#source`'s option for the shared database in index.html (its value dates from when the CRUD hub served /api).
const DB_OPTION: &str = "hub";

/// Where the tickets come from: the embedded mock snapshot, or the shared database through `/api`.
///
/// Decides where `apply` sends an edit and what the mode badge says.
#[derive(Clone, Copy, PartialEq)]
enum Source {
    Mock,
    Db,
}

/// Everything the page knows: the data source, the tickets and people on show, and the filters.
///
/// One instance lives in `APP` for the life of the page; reach it through `with`.
struct App {
    /// Where `store` came from, and so where edits go.
    source: Source,
    /// Prefix for `/api` URLs in database mode (always `API_ORIGIN` today).
    origin: String,
    /// The tickets on the board: the snapshot's or a copy of the database's, kept in step with each edit.
    store: Store,
    /// Customers, for names on cards, search by customer name and the customer pickers.
    customers: Vec<Person>,
    /// Employees, for assignee names and the assignee pickers.
    employees: Vec<Person>,
    /// The search box and the two filter pickers.
    filter: Filter,
}

/// One edit the user asked for, before it is applied to the mock store or sent to `/api`.
enum Change {
    /// A new ticket from the editor form.
    Create(Draft),
    /// New values for the ticket with this id (the editor form, or a ← / → move).
    Update(i64, Draft),
    /// Remove the ticket with this id (after the user confirmed).
    Delete(i64),
}

// The page's single App. A wasm page runs on one thread, so a thread-local RefCell is enough to share
// it between event callbacks; it is None only until `start` fills it.
thread_local! {
    /// The page state, filled by `start`; use `with` to reach it.
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the page's App and returns its result.
///
/// Panics before `start` has run, or if `f` itself calls `with` (the RefCell is already borrowed), so
/// callers keep DOM work that might re-enter it (`draw`, `status`) outside the closure. Never hold
/// the borrow across an `.await`: another callback could run meanwhile and find it borrowed.
fn with<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with(|a| f(a.borrow_mut().as_mut().expect("app started")))
}

/// The embedded mock snapshot as (tickets, customers, employees), parsed afresh each call.
///
/// A table that fails to parse comes back empty instead of stopping the page. Known limitation: that
/// failure is silent (the board just shows 0 tickets), and no test parses the snapshot files.
fn mock_data() -> (Vec<Ticket>, Vec<Person>, Vec<Person>) {
    (
        parse_tickets(MOCK_TICKETS).unwrap_or_default(),
        parse_people(MOCK_CUSTOMERS).unwrap_or_default(),
        parse_people(MOCK_EMPLOYEES).unwrap_or_default(),
    )
}

/// Starts the page: called once by wasm-bindgen when www/main.js initialises the module.
///
/// Errors: a failed event registration in `wire`. That error, or a panic from an element missing in
/// index.html (see `el`), rejects init() in www/main.js, which shows it in the status bar.
///
/// Steps:
/// 1. Fill `APP` with the mock snapshot, so the board can draw before any request returns.
/// 2. Show the version in the footer and register every event handler.
/// 3. With `?source=mock` in the URL, stay on the snapshot; otherwise select the database option and
///    start `load_db` in the background (it falls back to the snapshot itself).
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let origin = API_ORIGIN.to_string();
    let (tickets, customers, employees) = mock_data();
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            source: Source::Mock,
            origin: origin.clone(),
            store: Store::new(tickets),
            customers,
            employees,
            filter: Filter::default(),
        })
    });
    el("version").set_text_content(Some(concat!(
        "Generic Company Ticket Tracker ",
        env!("CARGO_PKG_VERSION")
    )));
    wire()?;
    // ?source=mock opens straight on the snapshot without even asking /api (handy in a plain browser)
    if query_param("source").as_deref() == Some("mock") {
        use_mock("Mock snapshot");
    } else {
        select("source").set_value(DB_OPTION);
        spawn_local(load_db());
    }
    Ok(())
}

// ---- sources ----

/// Switches the page to a fresh copy of the mock snapshot and says so in the status bar.
///
/// `why` starts the status message ("Mock snapshot", "No shared database here, so the mock
/// snapshot"). Edits made earlier in mock mode are discarded; the filters are kept (pickers drop an
/// id that is no longer listed).
fn use_mock(why: &str) {
    let (tickets, customers, employees) = mock_data();
    let n = tickets.len();
    with(|a| {
        a.source = Source::Mock;
        a.store = Store::new(tickets);
        a.customers = customers;
        a.employees = employees;
    });
    select("source").set_value("mock");
    refill_pickers();
    draw();
    status(&format!("{why}: {n} tickets. Edits stay in this page."), false);
}

/// Loads the board from the shared database, or falls back to the mock snapshot.
///
/// Runs in the background (`spawn_local`) at start, on Reload, on choosing the database in the Data
/// picker and after Database... picks a new file. Nothing in `App` changes until all three tables
/// have arrived, so a failure part-way leaves no half-loaded board.
///
/// Steps:
/// 1. `GET /api/source`. Only the desktop program answers it with a `db` path; a 404 (plain static
///    server), a network error or `"db": null` (no usable database, or --mock) means there is none, and
///    the page goes to the snapshot.
/// 2. Page in every ticket, customer and employee; the first failure stops the load.
/// 3. On success, replace the App's data, refill the pickers, redraw and name the database in the
///    status bar. On failure, go to the snapshot and show the error in red.
async fn load_db() {
    let origin = API_ORIGIN.to_string();
    status("Opening the shared database ...", false);
    let db = match call("GET", "/api/source", None).await {
        Ok((200, text)) => serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["db"].as_str().map(String::from)),
        _ => None,
    };
    let Some(db) = db else {
        use_mock("No shared database here, so the mock snapshot");
        return;
    };
    // an async block so `?` can stop at the first failing table and hand back one error
    let loaded = async {
        let t: Vec<Ticket> =
            serde_json::from_value(Value::Array(fetch_all(&origin, "tickets").await?)).map_err(|e| e.to_string())?;
        let c: Vec<Person> =
            serde_json::from_value(Value::Array(fetch_all(&origin, "customers").await?)).map_err(|e| e.to_string())?;
        let e: Vec<Person> =
            serde_json::from_value(Value::Array(fetch_all(&origin, "employees").await?)).map_err(|e| e.to_string())?;
        Ok::<_, String>((t, c, e))
    }
    .await;
    match loaded {
        Ok((t, c, e)) => {
            let n = t.len();
            with(|a| {
                a.source = Source::Db;
                a.origin = origin.clone();
                a.store = Store::new(t);
                a.customers = c;
                a.employees = e;
            });
            refill_pickers();
            draw();
            status(
                &format!("Shared database {db}: {n} tickets. Edits are saved to it."),
                false,
            );
        }
        Err(err) => {
            // use_mock writes its own status line; this one replaces it with the reason, in red
            use_mock("Database unavailable, back on the mock snapshot");
            status(
                &format!("Could not read the shared database ({err}). Showing the mock snapshot."),
                true,
            );
        }
    }
}

/// Every row of a shared-database table, paging with ?limit/&offset.
///
/// Asks for `PAGE` rows at a time, each request starting where the rows so far end, and stops at the
/// first page shorter than `PAGE` (an exactly full last page costs one more, empty, request). Errors:
/// a network failure, a reply other than 200 (as `reply_error` text) or a body that is not a JSON array.
///
/// Known limitation: the pages are separate reads, so a row added or deleted by another program
/// between two of them shifts the offsets and can repeat or skip a row; `X-Total-Count` is not used.
async fn fetch_all(origin: &str, table: &str) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    loop {
        let url = format!("{origin}/api/{table}?limit={PAGE}&offset={}", rows.len());
        let (code, text) = call("GET", &url, None).await?;
        if code != 200 {
            return Err(reply_error(code, &text));
        }
        let page: Vec<Value> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let done = page.len() < PAGE;
        rows.extend(page);
        if done {
            return Ok(rows);
        }
    }
}

/// Sends one HTTP request with the browser's `fetch` and returns (status code, body text).
///
/// `body`, when given, is sent as JSON (`Content-Type: application/json`). Any status counts as a
/// reply; only a request that never got one (network error, refused URL) or a body that could not be
/// read is an Err, as the JavaScript error's message.
///
/// Steps:
/// 1. Build a `RequestInit` with the method and body, then the `Request` itself.
/// 2. Add the JSON content type when there is a body.
/// 3. Await `window.fetch` for the `Response`, then await its text.
async fn call(method: &str, url: &str, body: Option<String>) -> Result<(u16, String), String> {
    let init = RequestInit::new();
    init.set_method(method);
    if let Some(b) = &body {
        init.set_body(&JsValue::from_str(b));
    }
    let req = Request::new_with_str_and_init(url, &init).map_err(js_err)?;
    if body.is_some() {
        req.headers().set("Content-Type", "application/json").map_err(js_err)?;
    }
    let win = web_sys::window().ok_or("no window")?;
    // fetch hands back a JS Promise: JsFuture awaits it, and dyn_into checks the result is a Response
    let resp: Response = JsFuture::from(win.fetch_with_request(&req))
        .await
        .map_err(js_err)?
        .dyn_into()
        .map_err(js_err)?;
    let text = JsFuture::from(resp.text().map_err(js_err)?)
        .await
        .map_err(js_err)?
        .as_string()
        .unwrap_or_default();
    Ok((resp.status(), text))
}

/// An error reply as `code: message`: its JSON `error`, else the first 120 characters of the body.
///
/// The `/api` contract (src/host/api.rs, like the hub) answers errors as `{"error": "..."}`; any
/// other body, such as a server's HTML error page, is cut short so the status bar stays one line.
fn reply_error(code: u16, text: &str) -> String {
    let msg = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v["error"].as_str().map(String::from))
        .unwrap_or_else(|| text.chars().take(120).collect());
    format!("{code}: {msg}")
}

/// A JavaScript exception as text: an Error's message, a thrown string as is, else "request failed".
fn js_err(e: JsValue) -> String {
    if let Some(err) = e.dyn_ref::<js_sys::Error>() {
        return err.message().into();
    }
    e.as_string().unwrap_or_else(|| "request failed".into())
}

/// The current time as `yyyy-mm-ddThh:mm`, the `created_at` format of the tickets table.
///
/// Known limitation: `toISOString` is always UTC, so a ticket made in the evening west of Greenwich
/// is dated the next day.
fn now_stamp() -> String {
    let iso: String = js_sys::Date::new_0().to_iso_string().into();
    // "2026-11-03T09:41:27.123Z": the first 16 characters stop at the minutes
    iso.chars().take(16).collect()
}

// ---- the desktop program ----

/// Asks for another database file; the program checks it, saves the choice and the board reloads.
///
/// Uses the browser's `prompt` box; Cancel or a blank answer does nothing. `POST /host/db` exists only
/// in the desktop program, which answers 200 after checking the file and saving it as the program's
/// setting, or 400 with the reason it refused it. On 200 the board reloads from the new file
/// (`load_db`); the page itself is not reloaded.
///
/// Known limitation: a plain static server may answer the POST with another code than 404 (Python's
/// http.server answers 501), which shows as "Not used: 501: ..." instead of the desktop-program hint.
async fn change_db() {
    let win = web_sys::window().unwrap();
    let Ok(Some(path)) = win.prompt_with_message("Path of the shared Generic Company database (generic_company.db):")
    else {
        return;
    };
    if path.trim().is_empty() {
        return;
    }
    let body = serde_json::json!({ "path": path.trim() }).to_string();
    match call("POST", "/host/db", Some(body)).await {
        Ok((200, _)) => spawn_local(load_db()),
        Ok((404, _)) | Err(_) => status("Choosing a database needs the desktop program.", true),
        Ok((code, text)) => status(&format!("Not used: {}", reply_error(code, &text)), true),
    }
}

/// Opens the About box, naming the database the program is using right now.
///
/// Asks `/api/source` afresh rather than trusting `App::source`, so it shows the program's current
/// file; with no program or no database it says "none (mock snapshot)". Note that this follows the
/// program, not the page: after choosing *Mock snapshot* in the Data picker the box still names the
/// program's database.
async fn about() {
    let info = match call("GET", "/api/source", None).await {
        Ok((200, text)) => serde_json::from_str::<Value>(&text).unwrap_or(Value::Null),
        _ => Value::Null,
    };
    let db = info["db"]
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| "none (mock snapshot)".into());
    el("about-db").set_text_content(Some(&format!("Database: {db}")));
    let _ = about_dialog().show_modal();
}

// ---- changes ----

/// Applies one edit to the current source, then redraws and reports it in the status bar.
///
/// On failure the board is not redrawn and the error shows in red; the App's data is left as it was,
/// so the board keeps its last good state (as SPEC.md asks).
async fn apply(change: Change) {
    // copy what is needed out first: the App must not stay borrowed across the await below
    let (source, origin) = with(|a| (a.source, a.origin.clone()));
    let result = match source {
        Source::Mock => apply_mock(change),
        Source::Db => apply_db(&origin, change).await,
    };
    match result {
        Ok(note) => {
            draw();
            status(&note, false);
        }
        Err(e) => status(&e, true),
    }
}

/// A new open ticket made from the draft, dated now, with id 0 until the store or database gives one.
///
/// Errors as `Draft::apply` (no title left after cleaning).
fn new_ticket(d: &Draft) -> Result<Ticket, String> {
    let mut t = Ticket {
        id: 0,
        customer_id: None,
        assignee_id: None,
        title: String::new(),
        body: None,
        status: Status::Open,
        created_at: Some(now_stamp()),
    };
    d.apply(&mut t)?;
    Ok(t)
}

/// Applies an edit to the in-memory snapshot; returns the status-bar note.
///
/// Errors: the draft's validation message, or "no such ticket" for an id the store lacks. Deleting
/// an id that is not there still reports success.
fn apply_mock(change: Change) -> Result<String, String> {
    with(|a| match change {
        Change::Create(d) => {
            let id = a.store.create(new_ticket(&d)?);
            Ok(format!("Mock: created #{id} (in memory)"))
        }
        Change::Update(id, d) => {
            let t = a.store.get_mut(id).ok_or("no such ticket")?;
            d.apply(t)?;
            Ok(format!("Mock: updated #{id} (in memory)"))
        }
        Change::Delete(id) => {
            a.store.delete(id);
            Ok(format!("Mock: deleted #{id} (in memory)"))
        }
    })
}

/// Sends an edit to the shared database through `/api/tickets`, then mirrors it in the page's copy.
///
/// The page's copy changes only after the program accepted the request, so a refused edit leaves the
/// board as it was. Returns the request and its answer for the status bar ("PATCH /api/tickets/7
/// {...} -> 200"); errors carry the method, path and the reply's error text.
///
/// - Create: `POST` with every column (`Draft::create_body`); expects 201 and the new `id` in the
///   reply, then adds the ticket as the page built it.
/// - Update: `PATCH` with only the changed columns (`Draft::patch_body`); when nothing changed no
///   request is sent at all. Expects 200, then applies the draft to the page's copy.
/// - Delete: `DELETE`; expects 200, then drops the ticket from the page's copy.
///
/// Known limitation: the page keeps its own version of a created or patched ticket instead of
/// reading back what was stored; the program cleans text by the same rules, so they agree.
async fn apply_db(origin: &str, change: Change) -> Result<String, String> {
    match change {
        Change::Create(d) => {
            // build the ticket locally first: it validates the draft and fixes created_at for both copies
            let mut t = new_ticket(&d)?;
            let body = d.create_body(t.created_at.as_deref().unwrap_or(""))?;
            let (code, text) = call("POST", &format!("{origin}/api/tickets"), Some(body.to_string())).await?;
            if code != 201 {
                return Err(format!("POST /api/tickets failed {}", reply_error(code, &text)));
            }
            t.id = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v["id"].as_i64())
                .ok_or("no id in the reply")?;
            let id = t.id;
            with(|a| a.store.tickets.push(t));
            Ok(format!("POST /api/tickets -> 201, id {id}"))
        }
        Change::Update(id, d) => {
            let old = with(|a| a.store.tickets.iter().find(|t| t.id == id).cloned()).ok_or("no such ticket")?;
            let Some(body) = d.patch_body(&old)? else {
                return Ok(format!("#{id}: nothing changed"));
            };
            let (code, text) = call("PATCH", &format!("{origin}/api/tickets/{id}"), Some(body.to_string())).await?;
            if code != 200 {
                return Err(format!("PATCH /api/tickets/{id} failed {}", reply_error(code, &text)));
            }
            // Option<Result> to Result<Option>: a ticket gone meanwhile is fine, a draft error is not
            with(|a| a.store.get_mut(id).map(|t| d.apply(t))).transpose()?;
            Ok(format!("PATCH /api/tickets/{id} {body} -> 200"))
        }
        Change::Delete(id) => {
            let (code, text) = call("DELETE", &format!("{origin}/api/tickets/{id}"), None).await?;
            if code != 200 {
                return Err(format!("DELETE /api/tickets/{id} failed {}", reply_error(code, &text)));
            }
            with(|a| a.store.delete(id));
            Ok(format!("DELETE /api/tickets/{id} -> 200"))
        }
    }
}

/// A draft holding a ticket's current values: fills the editor, and is the base of an arrow move.
fn draft_of(t: &Ticket) -> Draft {
    Draft {
        title: t.title.clone(),
        body: t.body.clone().unwrap_or_default(),
        customer_id: t.customer_id,
        assignee_id: t.assignee_id,
        status: Some(t.status),
    }
}

// ---- DOM ----
// The element ids below are the ones in www/index.html; a missing element is a broken page, so the
// lookups panic instead of returning errors.

/// The page's document. Panics outside a browser window (a worker has none).
fn doc() -> Document {
    web_sys::window().unwrap().document().unwrap()
}

/// The element with this id. Panics with "#id missing" when index.html lacks it.
fn el(id: &str) -> Element {
    doc().get_element_by_id(id).unwrap_or_else(|| panic!("#{id} missing"))
}

/// The `<input>` with this id. Panics when it is missing or another kind of element.
fn input(id: &str) -> HtmlInputElement {
    el(id).dyn_into().unwrap()
}

/// The `<select>` with this id. Panics when it is missing or another kind of element.
fn select(id: &str) -> HtmlSelectElement {
    el(id).dyn_into().unwrap()
}

/// The About box (`<dialog id="about-box">`).
fn about_dialog() -> HtmlDialogElement {
    el("about-box").dyn_into().unwrap()
}

/// The new/edit ticket form's dialog (`<dialog id="editor">`).
fn dialog() -> HtmlDialogElement {
    el("editor").dyn_into().unwrap()
}

/// Shows `msg` in the status bar; `error` turns it red (the `error` class in style.css).
///
/// Each message replaces the last; there is no history.
fn status(msg: &str, error: bool) {
    let s = el("status");
    s.set_text_content(Some(msg));
    s.set_class_name(if error { "error" } else { "" });
}

/// The value of `key` in the page URL's query string (`?source=mock`), decoded; None when absent.
///
/// A form-style `+` reads as a space, then `%xx` escapes are decoded. A key given without `=` is
/// skipped, and a value that does not decode reads as None. The first match wins.
fn query_param(key: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    search.trim_start_matches('?').split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key)
            .then(|| {
                js_sys::decode_uri_component(&v.replace('+', " "))
                    .ok()
                    .map(String::from)
            })
            .flatten()
    })
}

/// Redraws the whole board, the "shown of total" count and the mode badge from the App.
///
/// The board is replaced as one HTML string from `core::render::board` (every value escaped there);
/// with a few hundred cards this is fast enough and keeps no per-card state in the DOM. The HTML is
/// built inside `with` and written to the DOM after it.
fn draw() {
    let (html, shown, total, source) = with(|a| {
        let html = render::board(&a.store.tickets, &a.filter, &a.customers, &a.employees);
        let shown = a
            .store
            .tickets
            .iter()
            .filter(|t| a.filter.matches(t, &a.customers))
            .count();
        (html, shown, a.store.tickets.len(), a.source)
    });
    el("board").set_inner_html(&html);
    el("count").set_text_content(Some(&format!("{shown} of {total} tickets")));
    el("mode").set_text_content(Some(if source == Source::Db {
        "shared database"
    } else {
        "mock"
    }));
    el("mode").set_class_name(if source == Source::Db { "badge live" } else { "badge" });
}

/// Rebuilds the assignee and customer filter pickers for the people just loaded.
///
/// A filter id that the new people list lacks (after switching source) is cleared. The options are
/// built before that check, so a stale id simply matches no option and the picker shows its "Any"
/// entry, in step with the cleared filter.
fn refill_pickers() {
    with(|a| {
        select("f-assignee").set_inner_html(&render::options(&a.employees, a.filter.assignee, "Any assignee"));
        select("f-customer").set_inner_html(&render::options(&a.customers, a.filter.customer, "Any customer"));
        a.filter.assignee = a.filter.assignee.filter(|id| a.employees.iter().any(|p| p.id == *id));
        a.filter.customer = a.filter.customer.filter(|id| a.customers.iter().any(|p| p.id == *id));
    });
}

/// Opens the editor dialog: empty for a new ticket (None), or filled from `t` to edit it.
///
/// The hidden `e-id` field carries the ticket's id (empty for a new one); `read_editor` reads it back
/// to tell Create from Update. The pickers are rebuilt each time so they list the current people.
fn open_editor(t: Option<&Ticket>) {
    let d = t.map(draft_of).unwrap_or_default();
    el("editor-title").set_text_content(Some(&t.map_or("New ticket".to_string(), |t| format!("Edit #{}", t.id))));
    input("e-id").set_value(&t.map_or(String::new(), |t| t.id.to_string()));
    input("e-title").set_value(&d.title);
    el("e-body")
        .dyn_into::<HtmlTextAreaElement>()
        .unwrap()
        .set_value(&d.body);
    with(|a| {
        select("e-customer").set_inner_html(&render::options(&a.customers, d.customer_id, "No customer"));
        select("e-assignee").set_inner_html(&render::options(&a.employees, d.assignee_id, "Unassigned"));
    });
    select("e-status").set_value(d.status.unwrap_or(Status::Open).as_str());
    // show_modal throws for a dialog in the wrong state (open but not modal); the page never does that
    let _ = dialog().show_modal();
    let _ = input("e-title").focus();
}

/// The editor form as (ticket id, draft): the id is None for a new ticket.
///
/// The values are taken as typed; cleaning and the title check happen later (`Draft::checked`). An
/// empty picker choice reads as no customer / unassigned, and an unknown status as None.
fn read_editor() -> (Option<i64>, Draft) {
    let draft = Draft {
        title: input("e-title").value(),
        body: el("e-body").dyn_into::<HtmlTextAreaElement>().unwrap().value(),
        customer_id: parse_id(&select("e-customer").value()),
        assignee_id: parse_id(&select("e-assignee").value()),
        status: Status::parse(&select("e-status").value()).ok(),
    };
    (parse_id(&input("e-id").value()), draft)
}

/// Registers `f` as the `kind` event listener ("click", "change", ...) of the element with this id.
///
/// The Rust closure is wrapped in a wasm-bindgen `Closure` that JavaScript can call. `forget` hands
/// its ownership to JavaScript: otherwise Rust would free it when this function returns, and the next
/// event would call freed memory. The listeners are never removed, so nothing leaks beyond the page.
fn on(id: &str, kind: &str, f: impl FnMut(Event) + 'static) -> Result<(), JsValue> {
    let cb = Closure::<dyn FnMut(Event)>::new(f);
    el(id).add_event_listener_with_callback(kind, cb.as_ref().unchecked_ref())?;
    cb.forget(); // listeners live as long as the page
    Ok(())
}

/// Connects every control in index.html to its handler; called once by `start`.
///
/// - Data picker and Reload: load from the chosen source again.
/// - Search box and the two filter pickers: update the filter and redraw (no request).
/// - New ticket, Database..., About and the dialogs' close buttons.
/// - The editor form's submit: validate, close the dialog, then Create or Update.
/// - One click handler on the whole board for every card button (`data-act` = prev, next, edit or
///   delete), so the buttons need no listeners of their own after each redraw.
///
/// Errors: a failed `addEventListener` (not expected in practice).
fn wire() -> Result<(), JsValue> {
    on("source", "change", |_| {
        if select("source").value() == DB_OPTION {
            spawn_local(load_db());
        } else {
            use_mock("Mock snapshot");
        }
    })?;
    on("reload", "click", |_| {
        if select("source").value() == DB_OPTION {
            spawn_local(load_db());
        } else {
            use_mock("Mock snapshot reloaded");
        }
    })?;
    on("q", "input", |_| {
        with(|a| a.filter.text = input("q").value());
        draw();
    })?;
    on("f-assignee", "change", |_| {
        with(|a| a.filter.assignee = parse_id(&select("f-assignee").value()));
        draw();
    })?;
    on("f-customer", "change", |_| {
        with(|a| a.filter.customer = parse_id(&select("f-customer").value()));
        draw();
    })?;
    on("new", "click", |_| open_editor(None))?;
    on("change-db", "click", |_| spawn_local(change_db()))?;
    on("about", "click", |_| spawn_local(about()))?;
    on("about-close", "click", |_| about_dialog().close())?;
    on("e-cancel", "click", |_| dialog().close())?;
    on("editor-form", "submit", |ev| {
        // the page handles the save itself; the browser's own submit would reload the page
        ev.prevent_default();
        let (id, draft) = read_editor();
        // the dialog stays open on a validation error so the user can fix the field
        if let Err(e) = draft.checked() {
            status(&e, true);
            return;
        }
        dialog().close();
        spawn_local(apply(match id {
            Some(id) => Change::Update(id, draft),
            None => Change::Create(draft),
        }));
    })?;
    // event delegation: one listener on the board finds which card button was clicked
    // 1. walk up from the clicked node to the nearest [data-act] element, then its .card and data-id
    // 2. look the ticket up in the App (a copy, so no borrow is held while dialogs are open)
    // 3. turn the action into a Change: arrows move one column, edit opens the editor, delete asks first
    on("board", "click", |ev| {
        let Some(target) = ev.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
            return;
        };
        let Ok(Some(button)) = target.closest("[data-act]") else {
            return;
        };
        let Some(id) = button
            .closest(".card")
            .ok()
            .flatten()
            .and_then(|c| c.get_attribute("data-id"))
            .and_then(|s| parse_id(&s))
        else {
            return;
        };
        let Some(t) = with(|a| a.store.tickets.iter().find(|t| t.id == id).cloned()) else {
            return;
        };
        let act = button.get_attribute("data-act").unwrap_or_default();
        // the ticket as it is with only the status changed; None (no column that way) makes no change
        let moved = |s: Option<Status>| {
            s.map(|s| {
                Change::Update(
                    id,
                    Draft {
                        status: Some(s),
                        ..draft_of(&t)
                    },
                )
            })
        };
        let change = match act.as_str() {
            "prev" => moved(t.status.prev()),
            "next" => moved(t.status.next()),
            "edit" => {
                open_editor(Some(&t));
                None
            }
            "delete" => {
                // the browser's confirm box blocks until answered; a failure to show it counts as No
                let ok = web_sys::window()
                    .unwrap()
                    .confirm_with_message(&format!("Delete ticket #{id}?"))
                    .unwrap_or(false);
                ok.then_some(Change::Delete(id))
            }
            _ => None,
        };
        if let Some(c) = change {
            spawn_local(apply(c));
        }
    })?;
    Ok(())
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=e96d31e64754a115c0bb0ddadbe69b52ce8365789451f5e8d74a1a278250523f ====
