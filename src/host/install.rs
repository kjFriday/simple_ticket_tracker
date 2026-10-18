//! Install or uninstall the program for the current user (no admin rights): the exe, its `www` page and
//! icon go to %LOCALAPPDATA%\Programs\ACME Inc\<name>\ with an install.json marker; a Start-menu shortcut
//! (and optionally a desktop one) and an uninstall entry under HKCU\...\Uninstall\<app id> make it show
//! in the Start menu and Apps & features. Uninstall removes all of it and never the shared database.
//!
//! Same three layers as the other ACME programs: `plan` (pure), `script` (pure PowerShell, fed on stdin)
//! and `install`/`uninstall` with a replaceable runner. A sandbox root keeps everything under it, with
//! the registry as root\registry.json.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const UNINSTALL_KEY: &str = r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall";
pub const MARKER: &str = "install.json";

pub struct Program<'a> {
    pub app_id: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub publisher: &'a str,
    pub exe: PathBuf,
    pub exe_name: &'a str,
    pub www: PathBuf,
    pub icon: &'a [u8],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shortcut {
    pub path: PathBuf,
    pub target: PathBuf,
    pub workdir: PathBuf,
    pub icon: PathBuf,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reg {
    Text(String),
    DWord(u32),
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub app_id: String,
    pub name: String,
    pub version: String,
    pub install_dir: PathBuf,
    pub programs_dir: PathBuf,
    pub exe_source: PathBuf,
    pub exe_target: PathBuf,
    pub www_source: PathBuf,
    pub icon_target: PathBuf,
    pub icon: Vec<u8>,
    pub shortcuts: Vec<Shortcut>,
    pub registry_key: String,
    pub registry_file: Option<PathBuf>,
    pub registry: Vec<(&'static str, Reg)>,
}

/// (programs, Start menu, desktop, sandbox registry file).
pub fn locations(env: &dyn Fn(&str) -> Option<String>, root: Option<&Path>) -> (PathBuf, PathBuf, PathBuf, Option<PathBuf>) {
    if let Some(root) = root {
        let root = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
        return (root.join("Programs").join("ACME Inc"), root.join("Start Menu").join("ACME Inc"), root.join("Desktop"), Some(root.join("registry.json")));
    }
    let home = env("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    let local = env("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Local"));
    let roaming = env("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Roaming"));
    (
        local.join("Programs").join("ACME Inc"),
        roaming.join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("ACME Inc"),
        home.join("Desktop"),
        None,
    )
}

fn quote(p: &Path) -> String {
    let s = p.display().to_string();
    if s.contains(' ') { format!("\"{s}\"") } else { s }
}

pub fn plan(p: &Program, env: &dyn Fn(&str) -> Option<String>, root: Option<&Path>, desktop: bool, size: u64, today: &str) -> Plan {
    let (programs, start_menu, desk, registry_file) = locations(env, root);
    let dir = programs.join(p.name);
    let exe = dir.join(p.exe_name);
    let icon = dir.join("app.ico");
    let mut uninstall = format!("{} --uninstall", quote(&exe));
    if let Some(root) = root {
        uninstall += &format!(" --root {}", quote(&std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf())));
    }
    let mut links = vec![start_menu.join(format!("{}.lnk", p.name))];
    if desktop {
        links.push(desk.join(format!("{}.lnk", p.name)));
    }
    let shortcuts = links
        .into_iter()
        .map(|path| Shortcut { path, target: exe.clone(), workdir: dir.clone(), icon: icon.clone(), description: format!("{} {}", p.name, p.version) })
        .collect();
    let text = |s: String| Reg::Text(s);
    Plan {
        app_id: p.app_id.into(),
        name: p.name.into(),
        version: p.version.into(),
        install_dir: dir.clone(),
        programs_dir: programs,
        exe_source: p.exe.clone(),
        exe_target: exe,
        www_source: p.www.clone(),
        icon_target: icon.clone(),
        icon: p.icon.to_vec(),
        shortcuts,
        registry_key: format!(r"{UNINSTALL_KEY}\{}", p.app_id),
        registry_file,
        registry: vec![
            ("DisplayName", text(format!("{} {}", p.name, p.version))),
            ("DisplayVersion", text(p.version.into())),
            ("Publisher", text(p.publisher.into())),
            ("DisplayIcon", text(icon.display().to_string())),
            ("InstallLocation", text(dir.display().to_string())),
            ("UninstallString", text(uninstall.clone())),
            ("QuietUninstallString", text(format!("{uninstall} --quiet"))),
            ("InstallDate", text(today.into())),
            ("NoModify", Reg::DWord(1)),
            ("NoRepair", Reg::DWord(1)),
            ("EstimatedSize", Reg::DWord((size / 1024 + 1) as u32)),
        ],
    }
}

/// A PowerShell single-quoted string: nothing inside is expanded; ' is doubled.
pub fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub fn script(p: &Plan, remove: bool) -> String {
    let mut out = vec!["$ErrorActionPreference = 'Stop'".to_string()];
    let lit = |p: &Path| literal(&p.display().to_string());
    if remove {
        for s in &p.shortcuts {
            out.push(format!("Remove-Item -LiteralPath {} -Force -ErrorAction SilentlyContinue", lit(&s.path)));
        }
        out.push(format!(
            "$f = {}; if ((Test-Path -LiteralPath $f) -and -not (Get-ChildItem -LiteralPath $f)) {{ Remove-Item -LiteralPath $f }}",
            lit(p.shortcuts[0].path.parent().unwrap())
        ));
        if p.registry_file.is_none() {
            out.push(format!("Remove-Item -LiteralPath {} -Recurse -ErrorAction SilentlyContinue", literal(&p.registry_key)));
        }
    } else {
        out.push("$shell = New-Object -ComObject WScript.Shell".into());
        for s in &p.shortcuts {
            out.push(format!("New-Item -ItemType Directory -Force -Path {} | Out-Null", lit(s.path.parent().unwrap())));
            out.push(format!("$l = $shell.CreateShortcut({})", lit(&s.path)));
            out.push(format!("$l.TargetPath = {}", lit(&s.target)));
            out.push("$l.Arguments = ''".into());
            out.push(format!("$l.WorkingDirectory = {}", lit(&s.workdir)));
            out.push(format!("$l.IconLocation = {}", literal(&format!("{},0", s.icon.display()))));
            out.push(format!("$l.Description = {}", literal(&s.description)));
            out.push("$l.Save()".into());
        }
        if p.registry_file.is_none() {
            let key = literal(&p.registry_key);
            out.push(format!("New-Item -Path {key} -Force | Out-Null"));
            for (name, v) in &p.registry {
                let (kind, val) = match v {
                    Reg::Text(s) => ("String", s.clone()),
                    Reg::DWord(n) => ("DWord", n.to_string()),
                };
                out.push(format!("New-ItemProperty -Path {key} -Name {} -Value {} -PropertyType {kind} -Force | Out-Null", literal(name), literal(&val)));
            }
        }
    }
    out.join("\n") + "\n"
}

pub type Runner<'a> = &'a dyn Fn(&str) -> Result<(), String>;

/// The real runner: the script goes on stdin, so no quoting reaches the command line.
pub fn powershell(script: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| e.to_string())?;
    child.stdin.take().unwrap().write_all(script.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(format!("PowerShell: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

fn read_marker(dir: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(MARKER)).ok()?).ok()
}

fn edit_registry_file(path: &Path, edit: impl FnOnce(&mut Map<String, Value>)) -> Result<(), String> {
    let mut data: Map<String, Value> = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    edit(&mut data);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(path, serde_json::to_string_pretty(&Value::Object(data)).unwrap()).map_err(|e| e.to_string())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".gitignore" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to.join(&name))?;
        } else {
            std::fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
}

fn same(a: &Path, b: &Path) -> bool {
    a.display().to_string().eq_ignore_ascii_case(&b.display().to_string())
}

pub fn install(p: &Plan, run: Runner) -> Result<(), String> {
    if let Some(m) = read_marker(&p.install_dir) {
        if m["app_id"].as_str() != Some(&p.app_id) {
            return Err(format!("{} belongs to {}", p.install_dir.display(), m["app_id"]));
        }
    }
    std::fs::create_dir_all(&p.install_dir).map_err(|e| e.to_string())?;
    if !same(&p.exe_source, &p.exe_target) {
        std::fs::copy(&p.exe_source, &p.exe_target).map_err(|e| format!("copy {}: {e} (is it running?)", p.exe_target.display()))?;
    }
    let www = p.install_dir.join("www");
    if !same(&p.www_source, &www) {
        copy_dir(&p.www_source, &www).map_err(|e| format!("copy www: {e}"))?;
    }
    std::fs::write(&p.icon_target, &p.icon).map_err(|e| e.to_string())?;
    let marker = json!({
        "app_id": p.app_id, "name": p.name, "version": p.version,
        "files": [p.exe_target.file_name().unwrap().to_string_lossy(), "www", "app.ico"],
        "shortcuts": p.shortcuts.iter().map(|s| s.path.display().to_string()).collect::<Vec<_>>(),
    });
    std::fs::write(p.install_dir.join(MARKER), serde_json::to_string_pretty(&marker).unwrap()).map_err(|e| e.to_string())?;
    if let Some(file) = &p.registry_file {
        let values: Map<String, Value> = p
            .registry
            .iter()
            .map(|(k, v)| (k.to_string(), match v { Reg::Text(s) => Value::from(s.clone()), Reg::DWord(n) => Value::from(*n) }))
            .collect();
        edit_registry_file(file, |d| {
            d.insert(p.registry_key.clone(), Value::Object(values));
        })?;
    }
    run(&script(p, false))
}

/// Only a folder under the programs dir whose marker names this program.
pub fn safe_to_remove(p: &Plan) -> bool {
    let inside = p.install_dir.starts_with(&p.programs_dir) && p.install_dir != p.programs_dir;
    inside && read_marker(&p.install_dir).is_some_and(|m| m["app_id"].as_str() == Some(&p.app_id))
}

/// Removes shortcuts, registry entry and install folder; when `running` is inside that folder it removes
/// the rest and returns the folder as later: the caller passes it to `delete_after_exit` as its very last
/// action, after any message box. -> (whether a folder was removed, later).
pub fn uninstall(p: &Plan, run: Runner, running: &Path, purge: Option<&Path>) -> Result<(bool, Option<PathBuf>), String> {
    let mut p = p.clone();
    if let Some(m) = read_marker(&p.install_dir) {
        for s in m["shortcuts"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if !p.shortcuts.iter().any(|x| x.path == Path::new(s)) {
                let mut extra = p.shortcuts[0].clone();
                extra.path = PathBuf::from(s);
                p.shortcuts.push(extra);
            }
        }
    }
    run(&script(&p, true))?;
    if let Some(file) = &p.registry_file {
        edit_registry_file(file, |d| {
            d.remove(&p.registry_key);
        })?;
    }
    if let Some(dir) = purge {
        let _ = std::fs::remove_dir_all(dir);
    }
    if !p.install_dir.exists() {
        return Ok((false, None));
    }
    if !safe_to_remove(&p) {
        return Err(format!("refusing to delete {}: no matching {MARKER}", p.install_dir.display()));
    }
    if running.parent().is_some_and(|d| same(d, &p.install_dir)) {
        for entry in std::fs::read_dir(&p.install_dir).map_err(|e| e.to_string())?.flatten() {
            if !same(&entry.path(), running) {
                let path = entry.path();
                let _ = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
            }
        }
        return Ok((true, Some(p.install_dir.clone())));
    }
    std::fs::remove_dir_all(&p.install_dir).map_err(|e| e.to_string())?;
    Ok((true, None))
}

/// A hidden cmd that waits a moment, then removes dir (three tries, for about 4 seconds). Start it as this
/// process's very last action: it gives up while the exe still runs.
pub fn delete_after_exit(dir: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let rd = format!("rd /s /q \"{}\"", dir.display());
    let line = format!("/d /c \"ping -n 3 127.0.0.1 >nul & {rd} & ping -n 2 127.0.0.1 >nul & {rd} & ping -n 2 127.0.0.1 >nul & {rd}\"");
    Command::new("cmd.exe").raw_arg(line).current_dir(std::env::temp_dir()).creation_flags(0x0800_0000).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn setup(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("acme-install-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let build = dir.join("build");
        std::fs::create_dir_all(build.join("www").join("pkg")).unwrap();
        std::fs::write(build.join("acme-test.exe"), b"MZ fake").unwrap();
        std::fs::write(build.join("www").join("index.html"), b"<html>").unwrap();
        std::fs::write(build.join("www").join("pkg").join("a.wasm"), b"\0asm").unwrap();
        std::fs::write(build.join("www").join("pkg").join(".gitignore"), b"*").unwrap();
        (dir.clone(), build, dir.join("sandbox"))
    }

    fn program(build: &Path) -> Program<'static> {
        Program {
            app_id: "ACME.Test",
            name: "ACME Test App",
            version: "0.2.0",
            publisher: "ACME Inc. (fictional)",
            exe: build.join("acme-test.exe"),
            exe_name: "acme-test.exe",
            www: build.join("www"),
            icon: &[0, 0, 1, 0],
        }
    }

    fn none(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn plan_and_script() {
        let (dir, build, root) = setup("plan");
        let p = plan(&program(&build), &none, Some(&root), true, 4096, "20261017");
        assert_eq!(p.install_dir, std::path::absolute(&root).unwrap().join("Programs").join("ACME Inc").join("ACME Test App"));
        assert_eq!(p.shortcuts.len(), 2);
        assert!(p.registry.contains(&("DisplayVersion", Reg::Text("0.2.0".into()))));
        assert!(p.registry.contains(&("EstimatedSize", Reg::DWord(5))));
        assert!(matches!(&p.registry[5].1, Reg::Text(s) if s.contains("--uninstall --root")));
        assert!(!script(&p, false).contains("HKCU") && !script(&p, true).contains("HKCU"));
        let vars = |k: &str| (k == "LOCALAPPDATA").then(|| r"C:\O'Brien".to_string());
        let real = plan(&program(&build), &vars, None, false, 1, "20261017");
        let s = script(&real, false);
        assert!(s.contains(r"'C:\O''Brien\Programs\ACME Inc\ACME Test App\app.ico,0'"));
        assert!(s.contains("-Name 'NoModify' -Value '1' -PropertyType DWord"));
        assert!(script(&real, true).contains(r"Remove-Item -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ACME.Test'"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn round_trip_keeps_the_database() {
        let (dir, build, root) = setup("trip");
        let p = plan(&program(&build), &none, Some(&root), false, 1, "20261017");
        let scripts = RefCell::new(0);
        let run = |_: &str| -> Result<(), String> {
            *scripts.borrow_mut() += 1;
            Ok(())
        };
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("acme.db"), b"").unwrap();
        install(&p, &run).unwrap();
        install(&p, &run).unwrap();
        assert!(p.install_dir.join("www").join("pkg").join("a.wasm").is_file());
        assert!(!p.install_dir.join("www").join("pkg").join(".gitignore").exists());
        let reg: Value = serde_json::from_str(&std::fs::read_to_string(p.registry_file.as_ref().unwrap()).unwrap()).unwrap();
        assert_eq!(reg[&p.registry_key]["DisplayVersion"], "0.2.0");
        let (removed, later) = uninstall(&p, &run, &build.join("acme-test.exe"), None).unwrap();
        assert!(removed && later.is_none() && !p.install_dir.exists());
        assert!(root.join("acme.db").exists());
        assert_eq!(*scripts.borrow(), 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn running_copy_is_deleted_later_and_foreign_folders_are_refused() {
        let (dir, build, root) = setup("guard");
        let p = plan(&program(&build), &none, Some(&root), false, 1, "20261017");
        let ok = |_: &str| -> Result<(), String> { Ok(()) };
        install(&p, &ok).unwrap();
        // uninstall only names the folder: the caller starts delete_after_exit once its message box is gone
        let (removed, later) = uninstall(&p, &ok, &p.exe_target, None).unwrap();
        assert!(removed);
        assert_eq!(later.as_deref(), Some(p.install_dir.as_path()));
        let left: Vec<_> = std::fs::read_dir(&p.install_dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("acme-test.exe")]);
        std::fs::write(p.install_dir.join(MARKER), r#"{"app_id":"ACME.Other"}"#).unwrap();
        assert!(install(&p, &ok).is_err());
        assert!(uninstall(&p, &ok, &build.join("acme-test.exe"), None).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
