//! The desktop program: a native window (tao) with Edge WebView2 (wry) showing the wasm page from
//! `www/`. The page's requests to `/api/...` are answered in-process from the shared SQLite file through
//! the custom protocol `acme://` (no network port at all); `/host/...` covers what only the program can
//! do (show or change the database path). Command line: see `main`.

pub mod api;
pub mod assets;
pub mod install;
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

/// Everything that differs between the ACME Rust programs.
pub struct Spec {
    pub app_id: &'static str,
    pub name: &'static str,
    pub exe_name: &'static str,
    pub version: &'static str,
    pub width: f64,
    pub height: f64,
    pub tables: api::Tables,
    pub read_only: bool,
    pub icon_ico: &'static [u8],
    pub icon_rgba: &'static [u8],
    /// JavaScript that resolves to a number once the page shows its data (for --smoke).
    pub smoke_count: &'static str,
}

pub const PUBLISHER: &str = "ACME Inc. (fictional)";
pub const CREDIT: &str = "Icon: Silk by Mark James (famfamfam.com/lab/icons/silk), CC BY 2.5 (creativecommons.org/licenses/by/2.5), enlarged by nearest-neighbour.";

pub fn title(spec: &Spec) -> String {
    format!("ACME Inc. — {} {}", spec.name, spec.version)
}

#[derive(Default)]
pub struct Options {
    pub db: Option<String>,
    pub mock: bool,
    pub version: bool,
    pub install: bool,
    pub desktop_shortcut: bool,
    pub uninstall: bool,
    pub quiet: bool,
    pub purge: bool,
    pub root: Option<PathBuf>,
    pub smoke: Option<PathBuf>,
}

/// `--db PATH --mock --version --install [--desktop-shortcut] --uninstall [--quiet] [--purge] --root DIR --smoke FILE`
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
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
        .creation_flags(0x0800_0000)
        .status();
}

fn tell(o: &Options, spec: &Spec, msg: &str) {
    println!("{msg}");
    if !o.quiet {
        message_box(spec.name, msg);
    }
}

fn today() -> String {
    // days since 1970 -> civil date (Howard Hinnant's algorithm); no chrono needed
    let days = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}{m:02}{d:02}", yoe + era * 400 + i64::from(m <= 2))
}

/// Installs or uninstalls -> the exit code, and a folder to remove once this exe has exited.
fn run_installer(o: &Options, spec: &Spec) -> Result<(i32, Option<PathBuf>), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
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
    let plan = install::plan(&program, &paths::real_env, o.root.as_deref(), o.desktop_shortcut, size, &today());
    if o.uninstall {
        let plan = install::located(&plan, &install::read_powershell)?;
        if install::wizard(&plan).is_some() {
            // a setup made by ACME Wizard Maker installed it: its uninstaller keeps the files changed since
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
        tell(o, spec, &if removed {
            format!("{} uninstalled. The database is kept.", spec.name)
        } else {
            format!("{} was not installed; shortcuts and the uninstall entry are cleared.", spec.name)
        });
        return Ok((0, later));
    }
    install::install(&plan, &install::powershell)?;
    tell(o, spec, &format!("{} {} installed in {}. Find it in the Start menu under ACME Inc.", spec.name, spec.version, plan.install_dir.display()));
    Ok((0, None))
}

/// The database in use (None: the page runs on its mock snapshot) and why not, if not.
struct State {
    db: Option<PathBuf>,
    why: String,
}

fn choose(path: &Path, spec: &Spec, force_mock: bool) -> State {
    if force_mock {
        return State { db: None, why: "--mock".into() };
    }
    match api::open(path, true).and_then(|c| api::check_schema(&c, &spec.tables)) {
        Ok(()) => State { db: Some(path.to_path_buf()), why: String::new() },
        Err(e) => State { db: None, why: e },
    }
}

fn reply(r: api::Reply) -> Response<Cow<'static, [u8]>> {
    let mut b = Response::builder().status(r.status).header("Cache-Control", "no-store");
    for (k, v) in r.headers {
        b = b.header(k, v);
    }
    b.body(Cow::Owned(r.body)).unwrap()
}

fn serve(spec: &Spec, state: &Mutex<State>, www: &Path, req: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let path = req.uri().path().to_string();
    let target = req.uri().path_and_query().map(|p| p.as_str().to_string()).unwrap_or_else(|| path.clone());
    let method = req.method().as_str().to_string();
    if path == "/api/source" {
        let s = state.lock().unwrap();
        return reply(api::Reply::json(200, &json!({
            "db": s.db.as_ref().map(|p| p.display().to_string()), "why": s.why, "app": spec.name, "version": spec.version,
            "credit": CREDIT, "publisher": PUBLISHER,
        })));
    }
    if path == "/host/db" && method == "POST" {
        let asked = serde_json::from_slice::<Value>(req.body()).ok().and_then(|v| v["path"].as_str().map(PathBuf::from));
        let Some(asked) = asked else { return reply(api::Reply::json(400, &json!({"error": "send {\"path\": ...}"}))) };
        let asked = std::path::absolute(&asked).unwrap_or(asked);
        let next = choose(&asked, spec, false);
        if next.db.is_none() {
            return reply(api::Reply::json(400, &json!({ "error": next.why })));
        }
        let saved = paths::save_settings(spec.app_id, &paths::real_env, Map::from_iter([("db".to_string(), Value::from(asked.display().to_string()))]));
        *state.lock().unwrap() = next;
        return reply(match saved {
            Ok(()) => api::Reply::json(200, &json!({ "db": asked.display().to_string() })),
            Err(e) => api::Reply::json(500, &json!({ "error": e.to_string() })),
        });
    }
    if path.starts_with("/api/") {
        let db = state.lock().unwrap().db.clone();
        let conn = db.as_deref().and_then(|p| api::open(p, spec.read_only).ok());
        return reply(api::handle(&spec.tables, conn.as_ref(), &method, &target, req.body()));
    }
    match assets::resolve(www, &path).and_then(|f| std::fs::read(&f).ok().map(|b| (f, b))) {
        Some((file, body)) => Response::builder()
            .header("Content-Type", assets::mime(&file.display().to_string()))
            .body(Cow::Owned(body))
            .unwrap(),
        None => Response::builder().status(404).header("Content-Type", "text/plain").body(Cow::Borrowed(&b"not found"[..])).unwrap(),
    }
}

enum UserEvent {
    Smoke,
    Report(String),
}

fn window_icon(spec: &Spec) -> Option<Icon> {
    use tao::platform::windows::IconExtWindows;
    Icon::from_resource(1, None).ok().or_else(|| {
        let raw = spec.icon_rgba;
        let (w, h) = (u32::from_le_bytes(raw[0..4].try_into().ok()?), u32::from_le_bytes(raw[4..8].try_into().ok()?));
        Icon::from_rgba(raw[8..].to_vec(), w, h).ok()
    })
}

/// Parses the command line and runs: installer, or the window.
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
        message_box("ACME", &e);
        std::process::exit(1);
    }
}

fn open_window(spec: Arc<Spec>, state: Arc<Mutex<State>>, smoke: Option<PathBuf>) -> Result<(), String> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title(title(&spec))
        .with_inner_size(tao::dpi::LogicalSize::new(spec.width, spec.height))
        .with_window_icon(window_icon(&spec))
        .build(&event_loop)
        .map_err(|e| e.to_string())?;
    let icon_set = window_icon(&spec).is_some();
    let www = assets::www_dir();
    let mut context = WebContext::new(Some(paths::data_dir(spec.app_id, &paths::real_env).join("WebView2")));
    let proxy = event_loop.create_proxy();
    let (spec2, state2) = (spec.clone(), state.clone());
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_custom_protocol("acme".into(), move |_id, req| serve(&spec2, &state2, &www, req))
        .with_ipc_handler(move |req: Request<String>| {
            let _ = proxy.send_event(UserEvent::Report(req.into_body()));
        })
        .with_url("acme://localhost/index.html")
        .build(&window)
        .map_err(|e| e.to_string())?;
    if smoke.is_some() {
        let proxy = event_loop.create_proxy();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let _ = proxy.send_event(UserEvent::Smoke);
        });
    }
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *flow = ControlFlow::Exit,
            Event::UserEvent(UserEvent::Smoke) => {
                let js = format!(
                    "(async () => {{ let n = 0; for (let i = 0; i < 80; i++) {{ n = {}; if (n > 0) break; await new Promise(r => setTimeout(r, 100)); }} \
                     window.ipc.postMessage(JSON.stringify({{title: document.title, count: n, status: (document.getElementById('status') || {{}}).textContent || ''}})); }})()",
                    spec.smoke_count
                );
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::Report(body)) => {
                if let Some(file) = &smoke {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options() {
        let args: Vec<String> = ["--db", "x.db", "-install", "--root", "sb", "--quiet"].iter().map(|s| s.to_string()).collect();
        let o = parse(&args).unwrap();
        assert_eq!((o.db.as_deref(), o.install, o.quiet), (Some("x.db"), true, true));
        assert_eq!(o.root.as_deref(), Some(Path::new("sb")));
        assert!(parse(&["--nope".to_string()]).is_err());
        assert!(parse(&["--db".to_string()]).is_err());
    }

    #[test]
    fn today_is_a_date() {
        let d = today();
        assert_eq!(d.len(), 8);
        assert!(d.starts_with("20") && d[4..6].parse::<u32>().unwrap() <= 12);
    }
}
