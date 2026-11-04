// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/host/mod.rs
// About: The desktop program: a native tao window whose Edge WebView2 (wry) shows the wasm page.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! The desktop program: a native tao window whose Edge WebView2 (wry) shows the wasm page.
//!
//! The page comes from `www/`. The page's requests to `/api/...` are answered in-process from the shared SQLite file through
//! the custom protocol `generic-company://` (no network port at all); `/host/...` covers what only the program can
//! do (show or change the database path). Command line: see `main`.
//!
//! Compiled only with the `desktop` feature on Windows. src/main.rs fills a `Spec` and calls `main`,
//! which runs one of two paths:
//! 1. `--version`, `--install` or `--uninstall`: report or run install.rs, then exit.
//! 2. Otherwise pick the database (`paths::resolve_db`, then `choose`), open the window and run its
//! event loop until it closes.
//!
//! A custom protocol is a URL scheme the WebView hands to this program instead of the network: WebView2
//! calls `serve` with each request for `generic-company://localhost/...` and shows what it returns, so
//! the page, its wasm and its data all arrive without a socket. The page reaches the program the other
//! way with `window.ipc.postMessage`, used only by the `--smoke` check.
//!
//! Types: `Spec` (what differs per program), `Options` (the flags), `State` (the database in use) and
//! `UserEvent` (messages into the event loop). Start reading at `main`, then `serve`.

/// The page's data API over the shared SQLite file.
pub mod api;
/// Finding and serving the page's files from `www`.
pub mod assets;
/// Per-user install and uninstall.
pub mod install;
/// Per-user folders, the settings file and which database to open.
pub mod paths;

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Icon, WindowBuilder};
use wry::http::{Request, Response};
use wry::{WebContext, WebViewBuilder};

/// Everything that differs between the Generic Company Rust programs.
///
/// Filled in once by src/main.rs; the rest of host is the same code in each program.
pub struct Spec {
    /// Stable id ("GenericCompany.Tickets"): the uninstall registry key, the install.json marker and the
    /// per-app data folder all use it.
    pub app_id: &'static str,
    /// Display name: window title, Start-menu shortcut, install folder and message boxes.
    pub name: &'static str,
    /// The exe's file name once installed.
    pub exe_name: &'static str,
    /// Version shown in the title and written to the uninstall entry (the crate version).
    pub version: &'static str,
    /// Initial window width in logical (DPI-independent) pixels.
    pub width: f64,
    /// Initial window height in logical pixels.
    pub height: f64,
    /// The tables and columns the page may read, and the one it may change.
    pub tables: api::Tables,
    /// Open the database read-only for API requests, so no write can reach it.
    pub read_only: bool,
    /// The .ico file the installer writes as app.ico for the shortcuts and Apps & features.
    pub icon_ico: &'static [u8],
    /// Fallback window icon: width and height as little-endian u32s, then RGBA pixels.
    pub icon_rgba: &'static [u8],
    /// JavaScript that resolves to a number once the page shows its data (for --smoke).
    pub smoke_count: &'static str,
}

/// The publisher named in the uninstall entry and in `/api/source`.
pub const PUBLISHER: &str = "Generic Company Inc. (fictional)";
/// Attribution the icon's CC BY licence asks for, sent in `/api/source`. The page reads only `db` from
/// that reply; its About box carries its own copy of the credit in index.html.
pub const CREDIT: &str = "Icon: Silk by Mark James (famfamfam.com/lab/icons/silk), CC BY 2.5 (creativecommons.org/licenses/by/2.5), enlarged by nearest-neighbour.";
/// The window's own protocol. A URI scheme may only hold letters, digits, `+`, `-` and `.`: wry turns each
/// request back into a `SCHEME://` URI and answers 400, before `serve` runs, when that does not parse.
pub const SCHEME: &str = "generic-company";

/// The window title: company, program name and version.
pub fn title(spec: &Spec) -> String {
    format!("Generic Company Inc. — {} {}", spec.name, spec.version)
}

/// The command-line flags; the default is no flags at all (open the window on the usual database).
#[derive(Default)]
pub struct Options {
    /// `--db PATH`: the database file to use, before any saved setting.
    pub db: Option<String>,
    /// `--mock`: run on the page's mock snapshot even when a database is there.
    pub mock: bool,
    /// `--version`: report the name and version, then exit.
    pub version: bool,
    /// `--install`: install for the current user, then exit.
    pub install: bool,
    /// `--desktop-shortcut`: with --install, also put a shortcut on the desktop.
    pub desktop_shortcut: bool,
    /// `--uninstall`: remove the install, then exit.
    pub uninstall: bool,
    /// `--quiet`: print messages only, without a message box.
    pub quiet: bool,
    /// `--purge`: with --uninstall, also delete the per-app data folder (settings, WebView2 profile).
    pub purge: bool,
    /// `--root DIR`: install into a sandbox folder instead of the real user folders and registry.
    pub root: Option<PathBuf>,
    /// `--smoke FILE`: self-check; write a JSON report of what the page showed to FILE, then exit.
    pub smoke: Option<PathBuf>,
}

/// `--db PATH --mock --version --install [--desktop-shortcut] --uninstall [--quiet] [--purge] --root DIR --smoke FILE`
///
/// Any number of leading dashes is accepted (`-install`, `--install`), and a value is always the next
/// argument; there is no `--db=PATH` form. Errors name an unknown flag or a flag missing its value.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        // takes the following argument; called only by the flags that need a value
        let mut value = || it.next().cloned().ok_or(format!("{a} needs a value"));
        match a.trim_start_matches('-') {
            "db" => o.db = Some(value()?),
            "root" => o.root = Some(PathBuf::from(value()?)),
            "smoke" => o.smoke = Some(PathBuf::from(value()?)),
            "mock" => o.mock = true,
            "version" => o.version = true,
            "install" => o.install = true,
            "desktop-shortcut" => o.desktop_shortcut = true,
            "uninstall" => o.uninstall = true,
            "quiet" => o.quiet = true,
            "purge" => o.purge = true,
            _ => return Err(format!("unknown option {a}")),
        }
    }
    Ok(o)
}

/// Shows `text` in a modal message box titled `title` and waits until it is closed.
///
/// Runs PowerShell to call WPF's MessageBox; both strings go in as PowerShell literals, so nothing in
/// them is expanded. Any failure is ignored: the message is lost, nothing else breaks.
fn message_box(title: &str, text: &str) {
    // a windows-subsystem exe has no console; PowerShell's own message box keeps this dependency-free
    use std::os::windows::process::CommandExt;
    let script = format!(
        "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show({}, {}) | Out-Null",
        install::literal(text),
        install::literal(title)
    );
    let _ = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW: no console window flashes up behind the box
        .status();
}

/// Reports `msg`: always on stdout (for scripts and --quiet runs), and in a message box unless --quiet.
fn tell(o: &Options, spec: &Spec, msg: &str) {
    println!("{msg}");
    if !o.quiet {
        message_box(spec.name, msg);
    }
}

/// Today's UTC date as "yyyymmdd", the form Windows expects in an uninstall entry's InstallDate.
///
/// The trick of the algorithm is to count years from 1 March: then the leap day is the last day of the
/// year, and the month lengths from March on repeat in a 31-30-31-30-31 pattern. Steps:
/// 1. Whole days since 1970-01-01 (86 400 seconds a day; leap seconds do not exist in Unix time).
/// 2. Shift the count to start at 0000-03-01 and split it into 400-year eras of 146 097 days, after
/// which the Gregorian calendar repeats exactly.
/// 3. Find the year within the era, then the day within that March-based year.
/// 4. Turn the day of the year into a month and a day with the 153-day five-month pattern.
/// 5. January and February belong to the next civil year, so add 1 to the year for them.
fn today() -> String {
    // days since 1970 -> civil date (Howard Hinnant's algorithm); no chrono needed
    let days = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 86_400) as i64;
    // 719 468 days run from 0000-03-01 to 1970-01-01
    let z = days + 719_468;
    // 146 097 = days in 400 Gregorian years (97 of them leap); div_euclid floors for dates before year 0
    let era = z.div_euclid(146_097);
    // day of era, 0..=146 096
    let doe = z - era * 146_097;
    // year of era, 0..=399: take out one day per 4 years (1460 = 4 * 365), put back one per 100 years
    // (36 524 days) and take one out again for the 400th (146 096), then divide by 365
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    // day of the March-based year, 0..=365
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    // month from March = 0: March to July is 153 days, and (5 * doy + 2) / 153 rounds onto that pattern
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1; // 1..=31
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // March..December are 3..12, January and February 1 and 2
    format!("{:04}{m:02}{d:02}", yoe + era * 400 + i64::from(m <= 2))
}

/// Installs or uninstalls -> the exit code, and a folder to remove once this exe has exited.
///
/// Steps:
/// 1. Describe this program and its exe (and the size, for Apps & features) as an `install::Plan`.
/// 2. For --uninstall, follow the uninstall entry to the install it names (`install::located`).
/// 3. When a setup made by Generic Company Wizard Maker made that install, run its own uninstall.exe
/// with the same flags and pass its exit code on; it reports for itself, and only a failure other than
/// a cancel is repeated here.
/// 4. Otherwise uninstall it here (keeping the shared database; --purge also removes the per-app data
/// folder), or for --install, install it.
/// 5. Tell the user how it went (`tell`).
///
/// Errors come back as text for `main` to show; the exit code is then 1.
fn run_installer(o: &Options, spec: &Spec) -> Result<(i32, Option<PathBuf>), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // the exe's size in bytes becomes the entry's EstimatedSize; 0 when it cannot be read
    let size = std::fs::metadata(&exe).map(|m| m.len()).unwrap_or(0);
    let program = install::Program {
        app_id: spec.app_id,
        name: spec.name,
        version: spec.version,
        publisher: PUBLISHER,
        exe: exe.clone(),
        exe_name: spec.exe_name,
        www: assets::www_dir(),
        icon: spec.icon_ico,
    };
    let plan = install::plan(
        &program,
        &paths::real_env,
        o.root.as_deref(),
        o.desktop_shortcut,
        size,
        &today(),
    );
    if o.uninstall {
        let plan = install::located(&plan, &install::read_powershell)?;
        if install::wizard(&plan).is_some() {
            // a setup made by Generic Company Wizard Maker installed it: its uninstaller keeps the files changed since
            let args = install::uninstall_args(o.quiet, o.purge, o.root.as_deref());
            let (code, said) = install::run_uninstaller(&plan, &exe, &args, &install::run_and_wait)?;
            if code != 0 && code != install::CANCELLED {
                tell(o, spec, &format!("{}: {said}", spec.name));
            } else if !said.is_empty() {
                println!("{said}"); // its window, or its quiet output, has said how it went
            }
            return Ok((code, None));
        }
        let purge = o.purge.then(|| paths::data_dir(spec.app_id, &paths::real_env));
        let (removed, later) = install::uninstall(&plan, &install::powershell, &exe, purge.as_deref())?;
        tell(
            o,
            spec,
            &if removed {
                format!("{} uninstalled. The database is kept.", spec.name)
            } else {
                format!(
                    "{} was not installed; shortcuts and the uninstall entry are cleared.",
                    spec.name
                )
            },
        );
        return Ok((0, later));
    }
    install::install(&plan, &install::powershell)?;
    tell(
        o,
        spec,
        &format!(
            "{} {} installed in {}. Find it in the Start menu under Generic Company Inc.",
            spec.name,
            spec.version,
            plan.install_dir.display()
        ),
    );
    Ok((0, None))
}

/// The database in use (None: the page runs on its mock snapshot) and why not, if not.
///
/// Shared, behind a Mutex, by the protocol handler (which may switch it on `POST /host/db`) and the
/// event loop (which reads it for the smoke report).
struct State {
    /// The checked database file; None runs the page on its mock snapshot.
    db: Option<PathBuf>,
    /// Why there is no database ("--mock", "no database at ...", "tickets lacks ..."); empty when there is.
    why: String,
}

/// The `State` for `path`: the database when it opens read-only and has every listed table and
/// column, else no database and the reason. `force_mock` (--mock) skips the check.
///
/// The check connection is dropped at once; requests open their own.
fn choose(path: &Path, spec: &Spec, force_mock: bool) -> State {
    if force_mock {
        return State {
            db: None,
            why: "--mock".into(),
        };
    }
    match api::open(path, true).and_then(|c| api::check_schema(&c, &spec.tables)) {
        Ok(()) => State {
            db: Some(path.to_path_buf()),
            why: String::new(),
        },
        Err(e) => State { db: None, why: e },
    }
}

/// An `api::Reply` as the response type wry expects.
fn reply(r: api::Reply) -> Response<Cow<'static, [u8]>> {
    // no-store: WebView2 must never answer a data request from its cache, so lists show the file as it is now
    let mut b = Response::builder().status(r.status).header("Cache-Control", "no-store");
    for (k, v) in r.headers {
        b = b.header(k, v);
    }
    b.body(Cow::Owned(r.body)).unwrap()
}

/// Answers one request the page made to `generic-company://localhost/...`.
///
/// Routes, tried in this order:
/// 1. `/api/source` (any method): the database in use (or null), why not, and the program's name,
/// version, credit and publisher. web.rs asks it first to choose between database and mock mode.
/// 2. `POST /host/db` with `{"path": ...}`: check that file as `choose` does; when it passes, switch to
/// it and save it as the setting. 400 with the reason when it fails.
/// 3. Any other `/api/...`: `api::handle` on a connection opened for this request alone, so no handle
/// on the shared file stays open between requests. With no database it answers 503.
/// 4. Anything else: the file under www (`assets::resolve`), or 404.
///
/// wry calls it once per request and sends back what it returns; the answer is built synchronously.
fn serve(spec: &Spec, state: &Mutex<State>, www: &Path, req: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let path = req.uri().path().to_string();
    let target = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| path.clone());
    let method = req.method().as_str().to_string();
    if path == "/api/source" {
        let s = state.lock().unwrap();
        return reply(api::Reply::json(
            200,
            &json!({
                "db": s.db.as_ref().map(|p| p.display().to_string()), "why": s.why, "app": spec.name, "version": spec.version,
                "credit": CREDIT, "publisher": PUBLISHER,
            }),
        ));
    }
    if path == "/host/db" && method == "POST" {
        let asked = serde_json::from_slice::<Value>(req.body())
            .ok()
            .and_then(|v| v["path"].as_str().map(PathBuf::from));
        let Some(asked) = asked else {
            return reply(api::Reply::json(400, &json!({"error": "send {\"path\": ...}"})));
        };
        // a relative path is taken from the program's current folder, and saved absolute
        let asked = std::path::absolute(&asked).unwrap_or(asked);
        let next = choose(&asked, spec, false);
        if next.db.is_none() {
            return reply(api::Reply::json(400, &json!({ "error": next.why })));
        }
        let saved = paths::save_settings(
            spec.app_id,
            &paths::real_env,
            Map::from_iter([("db".to_string(), Value::from(asked.display().to_string()))]),
        );
        // the switch holds for this run even when saving the setting fails, which answers 500 below
        *state.lock().unwrap() = next;
        return reply(match saved {
            Ok(()) => api::Reply::json(200, &json!({ "db": asked.display().to_string() })),
            Err(e) => api::Reply::json(500, &json!({ "error": e.to_string() })),
        });
    }
    if path.starts_with("/api/") {
        // copy the path out so the lock is not held while SQLite works
        let db = state.lock().unwrap().db.clone();
        let conn = db.as_deref().and_then(|p| api::open(p, spec.read_only).ok());
        return reply(api::handle(&spec.tables, conn.as_ref(), &method, &target, req.body()));
    }
    match assets::resolve(www, &path).and_then(|f| std::fs::read(&f).ok().map(|b| (f, b))) {
        Some((file, body)) => Response::builder()
            .header("Content-Type", assets::mime(&file.display().to_string()))
            .body(Cow::Owned(body))
            .unwrap(),
        None => Response::builder()
            .status(404)
            .header("Content-Type", "text/plain")
            .body(Cow::Borrowed(&b"not found"[..]))
            .unwrap(),
    }
}

/// Messages other threads and the page send into the window's event loop.
enum UserEvent {
    /// The --smoke timer has run out: inject the script that waits for the page's data.
    Smoke,
    /// The page posted this text with `window.ipc.postMessage` (the smoke script's JSON).
    Report(String),
}

/// The window's icon: resource 1 linked into the exe by build.rs, else `Spec::icon_rgba`.
///
/// The fallback covers an exe built without the resource (for example by `cargo test` binaries).
/// The raw format is two little-endian u32s, width then height, followed by width * height RGBA pixels.
fn window_icon(spec: &Spec) -> Option<Icon> {
    use tao::platform::windows::IconExtWindows;
    // None asks for the icon's default size
    Icon::from_resource(1, None).ok().or_else(|| {
        let raw = spec.icon_rgba;
        let (w, h) = (
            u32::from_le_bytes(raw[0..4].try_into().ok()?),
            u32::from_le_bytes(raw[4..8].try_into().ok()?),
        );
        Icon::from_rgba(raw[8..].to_vec(), w, h).ok()
    })
}

/// Parses the command line and runs: installer, or the window.
///
/// Exit codes: 0 done, 1 an install or window error (after telling the user), 2 bad flags, or the exit
/// code of a wizard-made uninstaller. Never returns once the window is open: the event loop ends the
/// process when the window closes.
///
/// Steps:
/// 1. Parse the flags; a bad flag shows a message box and exits with 2.
/// 2. --version: report and return.
/// 3. --install or --uninstall: run the installer, start `delete_after_exit` when this exe sits in the
/// folder it removed, and exit with its code.
/// 4. Otherwise resolve and check the database, then open the window.
pub fn main(spec: Spec) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let o = match parse(&args) {
        Ok(o) => o,
        Err(e) => {
            message_box(spec.name, &e);
            std::process::exit(2);
        }
    };
    if o.version {
        tell(&o, &spec, &format!("{} {}", spec.name, spec.version));
        return;
    }
    if o.install || o.uninstall {
        let done = run_installer(&o, &spec).and_then(|(code, later)| match later {
            // the very last action, after the message box: the cmd removes the folder once this exe has exited
            Some(dir) => install::delete_after_exit(&dir).map(|()| code),
            None => Ok(code),
        });
        match done {
            Ok(code) => std::process::exit(code),
            Err(e) => {
                tell(&o, &spec, &format!("{}: {e}", spec.name));
                std::process::exit(1);
            }
        }
    }
    let db = paths::resolve_db(o.db.as_deref(), spec.app_id, &paths::real_env);
    let state = Arc::new(Mutex::new(choose(&db, &spec, o.mock)));
    if let Err(e) = open_window(Arc::new(spec), state, o.smoke) {
        message_box("Generic Company", &e);
        std::process::exit(1);
    }
}

/// Builds the window and its WebView, then runs the event loop until the window closes.
///
/// `spec` and `state` are shared (Arc) between the protocol handler and the event loop. With `smoke`
/// set, the program checks itself and exits instead of waiting for the user: see the `--smoke` steps
/// in the body. Errors (window or WebView2 could not be created) return before the loop starts; once
/// it runs, the process ends inside it.
///
/// Steps:
/// 1. Create the event loop and a window sized in logical pixels, titled and with the icon.
/// 2. Give WebView2 its own profile folder under the per-app data folder (cookies, cache, local storage).
/// 3. Register `serve` for the custom scheme and an IPC handler that forwards page messages as
/// `UserEvent::Report`, then load `generic-company://localhost/index.html`.
/// 4. For --smoke, start a timer thread.
/// 5. Run the loop: close on the window's close button, run the smoke script, write the smoke report.
fn open_window(spec: Arc<Spec>, state: Arc<Mutex<State>>, smoke: Option<PathBuf>) -> Result<(), String> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title(title(&spec))
        .with_inner_size(tao::dpi::LogicalSize::new(spec.width, spec.height))
        .with_window_icon(window_icon(&spec))
        .build(&event_loop)
        .map_err(|e| e.to_string())?;
    // asked again for the smoke report: the builder took the first Icon
    let icon_set = window_icon(&spec).is_some();
    let www = assets::www_dir();
    let mut context = WebContext::new(Some(paths::data_dir(spec.app_id, &paths::real_env).join("WebView2")));
    let proxy = event_loop.create_proxy();
    // the handler closure must own its copies; the originals stay with the event loop
    let (spec2, state2) = (spec.clone(), state.clone());
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_custom_protocol(SCHEME.into(), move |_id, req| serve(&spec2, &state2, &www, req))
        .with_ipc_handler(move |req: Request<String>| {
            let _ = proxy.send_event(UserEvent::Report(req.into_body()));
        })
        .with_url(format!("{SCHEME}://localhost/index.html"))
        .build(&window)
        .map_err(|e| e.to_string())?;
    // ---- --smoke: the self-check tools/verify.ps1 runs ----
    // 1. after 1.5 s (time for the page and its wasm to load) the timer sends UserEvent::Smoke
    // 2. the injected script polls smoke_count every 100 ms, up to 80 times (8 s), until it is above 0
    // 3. it posts the page title, the count and the #status text back over IPC (UserEvent::Report)
    // 4. the report adds what the program knows and goes to the file; then the program exits
    // Known limitation: if smoke_count throws, the script never posts and the window stays open
    if smoke.is_some() {
        let proxy = event_loop.create_proxy();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let _ = proxy.send_event(UserEvent::Smoke);
        });
    }
    event_loop.run(move |event, _, flow| {
        // sleep until the next event instead of spinning
        *flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *flow = ControlFlow::Exit,
            Event::UserEvent(UserEvent::Smoke) => {
                // {{ and }} are literal braces in format!; {} is the program's smoke_count expression
                let js = format!(
                    "(async () => {{ let n = 0; for (let i = 0; i < 80; i++) {{ n = {}; if (n > 0) break; await new Promise(r => setTimeout(r, 100)); }} \
                     window.ipc.postMessage(JSON.stringify({{title: document.title, count: n, status: (document.getElementById('status') || {{}}).textContent || ''}})); }})()",
                    spec.smoke_count
                );
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::Report(body)) => {
                // only the smoke script posts; without --smoke the message is ignored
                if let Some(file) = &smoke {
                    // verify.ps1 reads title, icon_set, source, version and counts.shown from this report
                    let page: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                    let s = state.lock().unwrap();
                    let report = json!({
                        "app_id": spec.app_id, "version": spec.version, "title": title(&spec), "icon_set": icon_set,
                        "source": if s.db.is_some() { "db" } else { "mock" }, "db": s.db.as_ref().map(|p| p.display().to_string()),
                        "page_title": page["title"], "counts": { "shown": page["count"] }, "status": page["status"],
                    });
                    let _ = std::fs::write(file, serde_json::to_string_pretty(&report).unwrap());
                    *flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}

/// Flags, the date and the scheme name.
#[cfg(test)]
mod tests {
    use super::*;

    /// One- and two-dash flags and values parse; an unknown flag and a value-less --db are errors.
    #[test]
    fn options() {
        let args: Vec<String> = ["--db", "x.db", "-install", "--root", "sb", "--quiet"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let o = parse(&args).unwrap();
        assert_eq!((o.db.as_deref(), o.install, o.quiet), (Some("x.db"), true, true));
        assert_eq!(o.root.as_deref(), Some(Path::new("sb")));
        assert!(parse(&["--nope".to_string()]).is_err());
        assert!(parse(&["--db".to_string()]).is_err());
    }

    /// The date has eight digits, a 21st-century year and a month no greater than 12 (a shape check only).
    #[test]
    fn today_is_a_date() {
        let d = today();
        assert_eq!(d.len(), 8);
        assert!(d.starts_with("20") && d[4..6].parse::<u32>().unwrap() <= 12);
    }

    /// The start URL parses, while an underscore spelling does not (wry would answer 400).
    #[test]
    fn scheme_makes_a_valid_uri() {
        let uri = format!("{SCHEME}://localhost/index.html");
        assert!(wry::http::Uri::try_from(uri.as_str()).is_ok(), "{uri}");
        assert!(wry::http::Uri::try_from("generic_company://localhost/index.html").is_err());
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=bd85a9287cf6bf0d685e8fafbc594e469f818df00f6fb5ed4df0bc45116deba9 ====
