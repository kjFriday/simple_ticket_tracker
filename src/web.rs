//! Page wiring. Two sources: the shared database, served to this page by the desktop program at the same
//! origin (`/api/...`; every edit is a real POST/PATCH/DELETE, logged in the status bar), or the embedded
//! mock snapshot (edits stay in memory), used when there is no database or the page runs in a browser.

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

const MOCK_TICKETS: &str = include_str!("../mock/tickets.json");
const MOCK_CUSTOMERS: &str = include_str!("../mock/customers.json");
const MOCK_EMPLOYEES: &str = include_str!("../mock/employees.json");
const API: &str = ""; // same origin: the desktop program answers /api/... from the shared database
const PAGE: usize = 500; // the largest ?limit

#[derive(Clone, Copy, PartialEq)]
enum Source {
    Mock,
    Hub,
}

struct App {
    source: Source,
    hub: String,
    store: Store,
    customers: Vec<Person>,
    employees: Vec<Person>,
    filter: Filter,
}

enum Change {
    Create(Draft),
    Update(i64, Draft),
    Delete(i64),
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with(|a| f(a.borrow_mut().as_mut().expect("app started")))
}

fn mock_data() -> (Vec<Ticket>, Vec<Person>, Vec<Person>) {
    (
        parse_tickets(MOCK_TICKETS).unwrap_or_default(),
        parse_people(MOCK_CUSTOMERS).unwrap_or_default(),
        parse_people(MOCK_EMPLOYEES).unwrap_or_default(),
    )
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let hub = API.to_string();
    let (tickets, customers, employees) = mock_data();
    APP.with(|a| {
        *a.borrow_mut() = Some(App { source: Source::Mock, hub: hub.clone(), store: Store::new(tickets), customers, employees, filter: Filter::default() })
    });
    el("version").set_text_content(Some(concat!("ACME Ticket Tracker ", env!("CARGO_PKG_VERSION"))));
    wire()?;
    if query_param("source").as_deref() == Some("mock") {
        use_mock("Mock snapshot");
    } else {
        select("source").set_value("hub");
        spawn_local(load_hub());
    }
    Ok(())
}

// ---- sources ----

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

async fn load_hub() {
    let hub = API.to_string();
    status("Opening the shared database ...", false);
    let db = match call("GET", "/api/source", None).await {
        Ok((200, text)) => serde_json::from_str::<Value>(&text).ok().and_then(|v| v["db"].as_str().map(String::from)),
        _ => None,
    };
    let Some(db) = db else {
        use_mock("No shared database here, so the mock snapshot");
        return;
    };
    let loaded = async {
        let t: Vec<Ticket> = serde_json::from_value(Value::Array(fetch_all(&hub, "tickets").await?)).map_err(|e| e.to_string())?;
        let c: Vec<Person> = serde_json::from_value(Value::Array(fetch_all(&hub, "customers").await?)).map_err(|e| e.to_string())?;
        let e: Vec<Person> = serde_json::from_value(Value::Array(fetch_all(&hub, "employees").await?)).map_err(|e| e.to_string())?;
        Ok::<_, String>((t, c, e))
    }
    .await;
    match loaded {
        Ok((t, c, e)) => {
            let n = t.len();
            with(|a| {
                a.source = Source::Hub;
                a.hub = hub.clone();
                a.store = Store::new(t);
                a.customers = c;
                a.employees = e;
            });
            refill_pickers();
            draw();
            status(&format!("Shared database {db}: {n} tickets. Edits are saved to it."), false);
        }
        Err(err) => {
            use_mock("Database unavailable, back on the mock snapshot");
            status(&format!("Could not read the shared database ({err}). Showing the mock snapshot."), true);
        }
    }
}

/// Every row of a hub table, paging with ?limit/&offset.
async fn fetch_all(hub: &str, table: &str) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    loop {
        let url = format!("{hub}/api/{table}?limit={PAGE}&offset={}", rows.len());
        let (code, text) = call("GET", &url, None).await?;
        if code != 200 {
            return Err(hub_error(code, &text));
        }
        let page: Vec<Value> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let done = page.len() < PAGE;
        rows.extend(page);
        if done {
            return Ok(rows);
        }
    }
}

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
    let resp: Response = JsFuture::from(win.fetch_with_request(&req)).await.map_err(js_err)?.dyn_into().map_err(js_err)?;
    let text = JsFuture::from(resp.text().map_err(js_err)?).await.map_err(js_err)?.as_string().unwrap_or_default();
    Ok((resp.status(), text))
}

fn hub_error(code: u16, text: &str) -> String {
    let msg = serde_json::from_str::<Value>(text).ok().and_then(|v| v["error"].as_str().map(String::from)).unwrap_or_else(|| text.chars().take(120).collect());
    format!("{code}: {msg}")
}

fn js_err(e: JsValue) -> String {
    if let Some(err) = e.dyn_ref::<js_sys::Error>() {
        return err.message().into();
    }
    e.as_string().unwrap_or_else(|| "request failed".into())
}

fn now_stamp() -> String {
    let iso: String = js_sys::Date::new_0().to_iso_string().into();
    iso.chars().take(16).collect()
}

// ---- the desktop program ----

/// Ask for another database file; the program checks it, saves the choice and the page reloads.
async fn change_db() {
    let win = web_sys::window().unwrap();
    let Ok(Some(path)) = win.prompt_with_message("Path of the shared ACME database (acme.db):") else { return };
    if path.trim().is_empty() {
        return;
    }
    let body = serde_json::json!({ "path": path.trim() }).to_string();
    match call("POST", "/host/db", Some(body)).await {
        Ok((200, _)) => spawn_local(load_hub()),
        Ok((404, _)) | Err(_) => status("Choosing a database needs the desktop program.", true),
        Ok((code, text)) => status(&format!("Not used: {}", hub_error(code, &text)), true),
    }
}

async fn about() {
    let info = match call("GET", "/api/source", None).await {
        Ok((200, text)) => serde_json::from_str::<Value>(&text).unwrap_or(Value::Null),
        _ => Value::Null,
    };
    let db = info["db"].as_str().map(String::from).unwrap_or_else(|| "none (mock snapshot)".into());
    el("about-db").set_text_content(Some(&format!("Database: {db}")));
    let _ = about_dialog().show_modal();
}

// ---- changes ----

async fn apply(change: Change) {
    let (source, hub) = with(|a| (a.source, a.hub.clone()));
    let result = match source {
        Source::Mock => apply_mock(change),
        Source::Hub => apply_hub(&hub, change).await,
    };
    match result {
        Ok(note) => {
            draw();
            status(&note, false);
        }
        Err(e) => status(&e, true),
    }
}

fn new_ticket(d: &Draft) -> Result<Ticket, String> {
    let mut t = Ticket { id: 0, customer_id: None, assignee_id: None, title: String::new(), body: None, status: Status::Open, created_at: Some(now_stamp()) };
    d.apply(&mut t)?;
    Ok(t)
}

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

async fn apply_hub(hub: &str, change: Change) -> Result<String, String> {
    match change {
        Change::Create(d) => {
            let mut t = new_ticket(&d)?;
            let body = d.create_body(t.created_at.as_deref().unwrap_or(""))?;
            let (code, text) = call("POST", &format!("{hub}/api/tickets"), Some(body.to_string())).await?;
            if code != 201 {
                return Err(format!("POST /api/tickets failed {}", hub_error(code, &text)));
            }
            t.id = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["id"].as_i64()).ok_or("no id in the reply")?;
            let id = t.id;
            with(|a| a.store.tickets.push(t));
            Ok(format!("POST /api/tickets -> 201, id {id}"))
        }
        Change::Update(id, d) => {
            let old = with(|a| a.store.tickets.iter().find(|t| t.id == id).cloned()).ok_or("no such ticket")?;
            let Some(body) = d.patch_body(&old)? else { return Ok(format!("#{id}: nothing changed")) };
            let (code, text) = call("PATCH", &format!("{hub}/api/tickets/{id}"), Some(body.to_string())).await?;
            if code != 200 {
                return Err(format!("PATCH /api/tickets/{id} failed {}", hub_error(code, &text)));
            }
            with(|a| a.store.get_mut(id).map(|t| d.apply(t))).transpose()?;
            Ok(format!("PATCH /api/tickets/{id} {body} -> 200"))
        }
        Change::Delete(id) => {
            let (code, text) = call("DELETE", &format!("{hub}/api/tickets/{id}"), None).await?;
            if code != 200 {
                return Err(format!("DELETE /api/tickets/{id} failed {}", hub_error(code, &text)));
            }
            with(|a| a.store.delete(id));
            Ok(format!("DELETE /api/tickets/{id} -> 200"))
        }
    }
}

fn draft_of(t: &Ticket) -> Draft {
    Draft { title: t.title.clone(), body: t.body.clone().unwrap_or_default(), customer_id: t.customer_id, assignee_id: t.assignee_id, status: Some(t.status) }
}

// ---- DOM ----

fn doc() -> Document {
    web_sys::window().unwrap().document().unwrap()
}

fn el(id: &str) -> Element {
    doc().get_element_by_id(id).unwrap_or_else(|| panic!("#{id} missing"))
}

fn input(id: &str) -> HtmlInputElement {
    el(id).dyn_into().unwrap()
}

fn select(id: &str) -> HtmlSelectElement {
    el(id).dyn_into().unwrap()
}

fn about_dialog() -> HtmlDialogElement {
    el("about-box").dyn_into().unwrap()
}

fn dialog() -> HtmlDialogElement {
    el("editor").dyn_into().unwrap()
}

fn status(msg: &str, error: bool) {
    let s = el("status");
    s.set_text_content(Some(msg));
    s.set_class_name(if error { "error" } else { "" });
}

fn query_param(key: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    search.trim_start_matches('?').split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| js_sys::decode_uri_component(&v.replace('+', " ")).ok().map(String::from)).flatten()
    })
}

fn draw() {
    let (html, shown, total, source) = with(|a| {
        let html = render::board(&a.store.tickets, &a.filter, &a.customers, &a.employees);
        let shown = a.store.tickets.iter().filter(|t| a.filter.matches(t, &a.customers)).count();
        (html, shown, a.store.tickets.len(), a.source)
    });
    el("board").set_inner_html(&html);
    el("count").set_text_content(Some(&format!("{shown} of {total} tickets")));
    el("mode").set_text_content(Some(if source == Source::Hub { "shared database" } else { "mock" }));
    el("mode").set_class_name(if source == Source::Hub { "badge live" } else { "badge" });
}

fn refill_pickers() {
    with(|a| {
        select("f-assignee").set_inner_html(&render::options(&a.employees, a.filter.assignee, "Any assignee"));
        select("f-customer").set_inner_html(&render::options(&a.customers, a.filter.customer, "Any customer"));
        a.filter.assignee = a.filter.assignee.filter(|id| a.employees.iter().any(|p| p.id == *id));
        a.filter.customer = a.filter.customer.filter(|id| a.customers.iter().any(|p| p.id == *id));
    });
}

fn open_editor(t: Option<&Ticket>) {
    let d = t.map(draft_of).unwrap_or_default();
    el("editor-title").set_text_content(Some(&t.map_or("New ticket".to_string(), |t| format!("Edit #{}", t.id))));
    input("e-id").set_value(&t.map_or(String::new(), |t| t.id.to_string()));
    input("e-title").set_value(&d.title);
    el("e-body").dyn_into::<HtmlTextAreaElement>().unwrap().set_value(&d.body);
    with(|a| {
        select("e-customer").set_inner_html(&render::options(&a.customers, d.customer_id, "No customer"));
        select("e-assignee").set_inner_html(&render::options(&a.employees, d.assignee_id, "Unassigned"));
    });
    select("e-status").set_value(d.status.unwrap_or(Status::Open).as_str());
    let _ = dialog().show_modal();
    let _ = input("e-title").focus();
}

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

fn on(id: &str, kind: &str, f: impl FnMut(Event) + 'static) -> Result<(), JsValue> {
    let cb = Closure::<dyn FnMut(Event)>::new(f);
    el(id).add_event_listener_with_callback(kind, cb.as_ref().unchecked_ref())?;
    cb.forget(); // listeners live as long as the page
    Ok(())
}

fn wire() -> Result<(), JsValue> {
    on("source", "change", |_| {
        if select("source").value() == "hub" {
            spawn_local(load_hub());
        } else {
            use_mock("Mock snapshot");
        }
    })?;
    on("reload", "click", |_| {
        if select("source").value() == "hub" {
            spawn_local(load_hub());
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
        ev.prevent_default();
        let (id, draft) = read_editor();
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
    on("board", "click", |ev| {
        let Some(target) = ev.target().and_then(|t| t.dyn_into::<Element>().ok()) else { return };
        let Ok(Some(button)) = target.closest("[data-act]") else { return };
        let Some(id) = button.closest(".card").ok().flatten().and_then(|c| c.get_attribute("data-id")).and_then(|s| parse_id(&s)) else { return };
        let Some(t) = with(|a| a.store.tickets.iter().find(|t| t.id == id).cloned()) else { return };
        let act = button.get_attribute("data-act").unwrap_or_default();
        let moved = |s: Option<Status>| s.map(|s| Change::Update(id, Draft { status: Some(s), ..draft_of(&t) }));
        let change = match act.as_str() {
            "prev" => moved(t.status.prev()),
            "next" => moved(t.status.next()),
            "edit" => {
                open_editor(Some(&t));
                None
            }
            "delete" => {
                let ok = web_sys::window().unwrap().confirm_with_message(&format!("Delete ticket #{id}?")).unwrap_or(false);
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
