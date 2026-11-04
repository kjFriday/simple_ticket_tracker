// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/host/install.rs
// About: Install or uninstall the program for the current user, with no admin rights.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Install or uninstall the program for the current user, with no admin rights.
//!
//! The exe, its `www` page and icon go to %LOCALAPPDATA%\Programs\Generic Company Inc\<name>\ with an install.json marker; a Start-menu shortcut
//! (and optionally a desktop one) and an uninstall entry under HKCU\...\Uninstall\<app id> make it show
//! in the Start menu and Apps & features. Uninstall removes all of it and never the shared database. An
//! install that a setup made by Generic Company Wizard Maker left (its marker names the installer), here or in the folder
//! its uninstall entry names, is removed by its own uninstall.exe instead, which keeps the files changed
//! since: see `located` and `wizard`.
//!
//! Same three layers as the other Generic Company programs: `plan` (pure), `script` (pure PowerShell, fed on stdin)
//! and `install`/`uninstall` with a replaceable runner. A sandbox root keeps everything under it, with
//! the registry as root\registry.json.
//!
//! host/mod.rs's `run_installer` is the only caller. Install runs `plan`, then `install`. Uninstall runs
//! `plan`, then `located`; then either `run_uninstaller` (a wizard-made install) or `uninstall`, and
//! finally `delete_after_exit` when the running exe sits in the folder it removed.
//!
//! Two Windows mechanisms do the visible work, both from PowerShell:
//! - A shortcut (.lnk) is made through the WScript.Shell COM object: `CreateShortcut` returns a link
//! object, its properties are set, and `Save` writes the file.
//! - Apps & features lists every subkey of the per-user Uninstall key; DisplayName, UninstallString
//! and the other values there are what it shows and runs.
//!
//! Types: `Program` (what to install), `Plan` (every path and value, computed up front), `Shortcut`,
//! `Reg`, and the runner aliases `Runner`, `Reader` and `Exec`. Start reading at `plan`.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The per-user Uninstall key, in PowerShell's registry-drive form; each program has a subkey named
/// after its app id. Per-user (HKCU) needs no admin rights.
pub const UNINSTALL_KEY: &str = r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall";
/// The marker file in the install folder: it names the app id that owns the folder (nothing is deleted
/// without it), its files and its shortcuts.
pub const MARKER: &str = "install.json";
/// What a setup made by Generic Company Wizard Maker leaves in the install folder.
pub const WIZARD_UNINSTALLER: &str = "uninstall.exe";
/// That uninstaller's exit code when the user cancels it in its window.
///
/// The same value as the Wizard Maker's own cancel exit code; the two programs must agree on it.
pub const CANCELLED: i32 = 3;

/// The program to install, as host/mod.rs describes it.
pub struct Program<'a> {
    /// Stable id: the Uninstall subkey's name and the marker's owner.
    pub app_id: &'a str,
    /// Display name: the install folder's and the shortcuts' name.
    pub name: &'a str,
    /// Version for the uninstall entry and the marker.
    pub version: &'a str,
    /// Publisher shown in Apps & features.
    pub publisher: &'a str,
    /// The running exe, which is copied.
    pub exe: PathBuf,
    /// The exe's name once installed.
    pub exe_name: &'a str,
    /// The www folder the running exe serves, which is copied beside the installed exe.
    pub www: PathBuf,
    /// The .ico bytes written as app.ico.
    pub icon: &'a [u8],
}

/// One .lnk file to create or remove.
#[derive(Debug, Clone, PartialEq)]
pub struct Shortcut {
    /// Where the .lnk file goes.
    pub path: PathBuf,
    /// The exe it starts.
    pub target: PathBuf,
    /// The folder it starts in (the install folder).
    pub workdir: PathBuf,
    /// The .ico file its icon comes from.
    pub icon: PathBuf,
    /// Its tooltip: name and version.
    pub description: String,
}

/// A registry value of the uninstall entry: REG_SZ text or a REG_DWORD number.
#[derive(Debug, Clone, PartialEq)]
pub enum Reg {
    /// A string value (REG_SZ).
    Text(String),
    /// A 32-bit number (REG_DWORD), used for flags and sizes.
    DWord(u32),
}

/// Every path, shortcut and registry value an install or uninstall touches, computed by `plan` before
/// anything changes.
#[derive(Debug, Clone)]
pub struct Plan {
    /// The owner id written to and checked in the marker.
    pub app_id: String,
    /// Display name.
    pub name: String,
    /// Version installed.
    pub version: String,
    /// The program's own folder: programs_dir\name (or wherever `located` points it).
    pub install_dir: PathBuf,
    /// The folder shared by all Generic Company programs; never removed itself.
    pub programs_dir: PathBuf,
    /// The running exe, copied from.
    pub exe_source: PathBuf,
    /// The installed exe.
    pub exe_target: PathBuf,
    /// The www folder copied from.
    pub www_source: PathBuf,
    /// The installed app.ico.
    pub icon_target: PathBuf,
    /// The bytes of app.ico.
    pub icon: Vec<u8>,
    /// The Start-menu shortcut first, then the desktop one if asked for. `script` takes the first
    /// shortcut's folder as the Start-menu group, so the list is never empty.
    pub shortcuts: Vec<Shortcut>,
    /// The uninstall entry's key: UNINSTALL_KEY\app id.
    pub registry_key: String,
    /// In a sandbox (--root), the JSON file that stands in for the registry; None for the real one.
    pub registry_file: Option<PathBuf>,
    /// The uninstall entry's values, in the order they are written.
    pub registry: Vec<(&'static str, Reg)>,
}

/// (programs, Start menu, desktop, sandbox registry file).
///
/// Without `root`: %LOCALAPPDATA%\Programs\Generic Company Inc, the per-user Start-menu folder
/// (%APPDATA%\Microsoft\Windows\Start Menu\Programs\Generic Company Inc), %USERPROFILE%\Desktop and
/// no registry file. A missing variable falls back to the usual place under the user profile.
/// With `root` (made absolute), the same folders sit under it and the registry is root\registry.json.
/// `env` looks up environment variables, so tests can supply their own.
pub fn locations(
    env: &dyn Fn(&str) -> Option<String>,
    root: Option<&Path>,
) -> (PathBuf, PathBuf, PathBuf, Option<PathBuf>) {
    if let Some(root) = root {
        let root = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
        return (
            root.join("Programs").join("Generic Company Inc"),
            root.join("Start Menu").join("Generic Company Inc"),
            root.join("Desktop"),
            Some(root.join("registry.json")),
        );
    }
    let home = env("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    let local = env("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData").join("Local"));
    let roaming = env("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData").join("Roaming"));
    (
        local.join("Programs").join("Generic Company Inc"),
        roaming
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Generic Company Inc"),
        home.join("Desktop"),
        None,
    )
}

/// `p` for a command line: in double quotes when it holds a space, so Windows splits the arguments
/// right. Used for the UninstallString that Apps & features runs.
fn quote(p: &Path) -> String {
    let s = p.display().to_string();
    if s.contains(' ') {
        format!("\"{s}\"")
    } else {
        s
    }
}

/// The `Plan` for installing `p`; pure, it reads and changes nothing.
///
/// `root` is the --root sandbox, `desktop` adds a desktop shortcut, `size` is the exe's size in bytes
/// and `today` the install date as yyyymmdd.
///
/// Steps:
/// 1. Find the folders (`locations`) and put the install in programs\name.
/// 2. Build the uninstall command: the installed exe with --uninstall, plus --root for a sandbox;
/// QuietUninstallString adds --quiet.
/// 3. List the shortcuts: the Start menu always, the desktop when asked.
/// 4. List the uninstall entry's values. NoModify and NoRepair hide those buttons in Apps & features,
/// and EstimatedSize is in KB, rounded up.
pub fn plan(
    p: &Program,
    env: &dyn Fn(&str) -> Option<String>,
    root: Option<&Path>,
    desktop: bool,
    size: u64,
    today: &str,
) -> Plan {
    let (programs, start_menu, desk, registry_file) = locations(env, root);
    let dir = programs.join(p.name);
    let exe = dir.join(p.exe_name);
    let icon = dir.join("app.ico");
    let mut uninstall = format!("{} --uninstall", quote(&exe));
    if let Some(root) = root {
        uninstall += &format!(
            " --root {}",
            quote(&std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf()))
        );
    }
    let mut links = vec![start_menu.join(format!("{}.lnk", p.name))];
    if desktop {
        links.push(desk.join(format!("{}.lnk", p.name)));
    }
    let shortcuts = links
        .into_iter()
        .map(|path| Shortcut {
            path,
            target: exe.clone(),
            workdir: dir.clone(),
            icon: icon.clone(),
            description: format!("{} {}", p.name, p.version),
        })
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
        // the tests read UninstallString by its position (index 5), so the order matters
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
            // in KB; + 1 rounds up (an exact multiple of 1024 bytes also gains one)
            ("EstimatedSize", Reg::DWord((size / 1024 + 1) as u32)),
        ],
    }
}

/// A PowerShell single-quoted string: nothing inside is expanded; ' is doubled.
pub fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The PowerShell that makes (`remove` false) or removes (`remove` true) the shortcuts and, outside a
/// sandbox, the uninstall entry; pure, it only builds text.
///
/// Every path and value goes in through `literal`, so no character in them can change the script.
/// Making: for each shortcut, create its folder, then a link through WScript.Shell with target, working
/// folder, icon (`path,0`: the first icon in the file) and description. The registry key is then
/// created and each value written with its type (String or DWord). Removing: delete each shortcut,
/// then the Start-menu group folder only when it is empty (other Generic Company programs may share
/// it), then the whole registry key. In a sandbox the registry part is left out; `install` and
/// `uninstall` edit root\registry.json instead.
pub fn script(p: &Plan, remove: bool) -> String {
    // stop at the first failing command, so the caller sees an error instead of a half-done job
    let mut out = vec!["$ErrorActionPreference = 'Stop'".to_string()];
    let lit = |p: &Path| literal(&p.display().to_string());
    if remove {
        for s in &p.shortcuts {
            out.push(format!(
                "Remove-Item -LiteralPath {} -Force -ErrorAction SilentlyContinue",
                lit(&s.path)
            ));
        }
        out.push(format!(
            "$f = {}; if ((Test-Path -LiteralPath $f) -and -not (Get-ChildItem -LiteralPath $f)) {{ Remove-Item -LiteralPath $f }}",
            lit(p.shortcuts[0].path.parent().unwrap())
        ));
        if p.registry_file.is_none() {
            out.push(format!(
                "Remove-Item -LiteralPath {} -Recurse -ErrorAction SilentlyContinue",
                literal(&p.registry_key)
            ));
        }
    } else {
        out.push("$shell = New-Object -ComObject WScript.Shell".into());
        for s in &p.shortcuts {
            out.push(format!(
                "New-Item -ItemType Directory -Force -Path {} | Out-Null",
                lit(s.path.parent().unwrap())
            ));
            out.push(format!("$l = $shell.CreateShortcut({})", lit(&s.path)));
            out.push(format!("$l.TargetPath = {}", lit(&s.target)));
            out.push("$l.Arguments = ''".into());
            out.push(format!("$l.WorkingDirectory = {}", lit(&s.workdir)));
            out.push(format!(
                "$l.IconLocation = {}",
                literal(&format!("{},0", s.icon.display()))
            ));
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
                out.push(format!(
                    "New-ItemProperty -Path {key} -Name {} -Value {} -PropertyType {kind} -Force | Out-Null",
                    literal(name),
                    literal(&val)
                ));
            }
        }
    }
    out.join("\n") + "\n"
}

/// Runs a PowerShell script; `powershell` is the real one, tests pass closures that record or ignore it.
pub type Runner<'a> = &'a dyn Fn(&str) -> Result<(), String>;

/// The real runner: the script goes on stdin, so no quoting reaches the command line.
pub fn powershell(script: &str) -> Result<(), String> {
    read_powershell(script).map(|_| ())
}

/// Runs a PowerShell script and returns what it printed; `read_powershell` is the real one.
pub type Reader<'a> = &'a dyn Fn(&str) -> Result<String, String>;

/// The real `Reader`: runs a script like `powershell` -> what it printed.
///
/// The output is trimmed. A script that prints non-ASCII text must set `[Console]::OutputEncoding` to
/// UTF-8 itself; otherwise PowerShell writes in the console code page and the text is garbled.
/// Errors: PowerShell could not start, or the script failed (its error output is in the message).
/// Blocks until PowerShell exits.
pub fn read_powershell(script: &str) -> Result<String, String> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            // "-" reads the script from stdin
            "-Command",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .map_err(|e| e.to_string())?;
    // take() moved stdin out and dropped it after the write, closing the pipe: PowerShell runs the
    // script once its input ends
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!("PowerShell: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// The install.json in `dir` as JSON, or None when it is missing or not JSON.
fn read_marker(dir: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(MARKER)).ok()?).ok()
}

/// Changes the sandbox registry file: reads it (a missing or broken file is empty), lets `edit` change
/// the map of key -> values, and writes it back, creating its folder.
fn edit_registry_file(path: &Path, edit: impl FnOnce(&mut Map<String, Value>)) -> Result<(), String> {
    let mut data: Map<String, Value> = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    edit(&mut data);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(path, serde_json::to_string_pretty(&Value::Object(data)).unwrap()).map_err(|e| e.to_string())
}

/// Copies the folder `from` into `to` recursively, overwriting files of the same name.
///
/// .gitignore files are skipped: wasm-pack leaves one in www\pkg, and it means nothing once installed.
/// Files already in `to` that `from` lacks are kept.
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

/// Whether two paths name the same file, compared as text without regard to ASCII case, as Windows
/// file names are. Not canonicalised: `a\..\b` and `b` differ.
fn same(a: &Path, b: &Path) -> bool {
    a.display().to_string().eq_ignore_ascii_case(&b.display().to_string())
}

/// Carries out `p`: files, marker, sandbox registry, then the PowerShell `script` through `run`.
///
/// Installing over an earlier install of the same program updates it in place.
///
/// Steps:
/// 1. Refuse a folder whose marker names another app id.
/// 2. Create the folder and copy the exe and www into it, unless this already is the installed copy.
/// 3. Write app.ico and the marker (app id, name, version, files, shortcut paths).
/// 4. In a sandbox, write the uninstall entry into root\registry.json.
/// 5. Run the script that makes the shortcuts and, outside a sandbox, the registry entry.
///
/// Errors: the folder belongs to another program, a copy failed (most often because the target exe is
/// running), or the script failed.
pub fn install(p: &Plan, run: Runner) -> Result<(), String> {
    if let Some(m) = read_marker(&p.install_dir) {
        if m["app_id"].as_str() != Some(&p.app_id) {
            return Err(format!("{} belongs to {}", p.install_dir.display(), m["app_id"]));
        }
    }
    std::fs::create_dir_all(&p.install_dir).map_err(|e| e.to_string())?;
    // the installed exe run with --install would otherwise copy onto itself
    if !same(&p.exe_source, &p.exe_target) {
        std::fs::copy(&p.exe_source, &p.exe_target)
            .map_err(|e| format!("copy {}: {e} (is it running?)", p.exe_target.display()))?;
    }
    let www = p.install_dir.join("www");
    if !same(&p.www_source, &www) {
        copy_dir(&p.www_source, &www).map_err(|e| format!("copy www: {e}"))?;
    }
    std::fs::write(&p.icon_target, &p.icon).map_err(|e| e.to_string())?;
    // no "installer" key: that key marks a Wizard Maker install (see `wizard`)
    let marker = json!({
        "app_id": p.app_id, "name": p.name, "version": p.version,
        "files": [p.exe_target.file_name().unwrap().to_string_lossy(), "www", "app.ico"],
        "shortcuts": p.shortcuts.iter().map(|s| s.path.display().to_string()).collect::<Vec<_>>(),
    });
    std::fs::write(
        p.install_dir.join(MARKER),
        serde_json::to_string_pretty(&marker).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    if let Some(file) = &p.registry_file {
        let values: Map<String, Value> = p
            .registry
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    match v {
                        Reg::Text(s) => Value::from(s.clone()),
                        Reg::DWord(n) => Value::from(*n),
                    },
                )
            })
            .collect();
        edit_registry_file(file, |d| {
            d.insert(p.registry_key.clone(), Value::Object(values));
        })?;
    }
    run(&script(p, false))
}

/// Only a folder under the programs dir whose marker names this program.
///
/// The guard before any recursive delete: a wrong path, or a folder some other program owns, is
/// never removed. `programs_dir` itself never qualifies.
pub fn safe_to_remove(p: &Plan) -> bool {
    let inside = p.install_dir.starts_with(&p.programs_dir) && p.install_dir != p.programs_dir;
    inside && read_marker(&p.install_dir).is_some_and(|m| m["app_id"].as_str() == Some(&p.app_id))
}

/// Removes shortcuts, registry entry and install folder; when `running` is inside that folder it removes
/// the rest and returns the folder as later: the caller passes it to `delete_after_exit` as its very last
/// action, after any message box. -> (whether a folder was removed, later).
pub fn uninstall(
    p: &Plan,
    run: Runner,
    running: &Path,
    purge: Option<&Path>,
) -> Result<(bool, Option<PathBuf>), String> {
    let mut p = p.clone();
    // an earlier install may have made a desktop shortcut this run was not asked about: the marker
    // lists every shortcut it made, so those are removed too
    if let Some(m) = read_marker(&p.install_dir) {
        for s in m["shortcuts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
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
    // best effort: settings and the WebView2 profile; the shared database lives elsewhere
    if let Some(dir) = purge {
        let _ = std::fs::remove_dir_all(dir);
    }
    // shortcuts and entry are cleared even when the folder is already gone
    if !p.install_dir.exists() {
        return Ok((false, None));
    }
    if !safe_to_remove(&p) {
        return Err(format!(
            "refusing to delete {}: no matching {MARKER}",
            p.install_dir.display()
        ));
    }
    // Windows cannot delete a running exe: remove everything else now and leave the folder to
    // delete_after_exit
    if running.parent().is_some_and(|d| same(d, &p.install_dir)) {
        for entry in std::fs::read_dir(&p.install_dir).map_err(|e| e.to_string())?.flatten() {
            if !same(&entry.path(), running) {
                let path = entry.path();
                let _ = if path.is_dir() {
                    std::fs::remove_dir_all(path)
                } else {
                    std::fs::remove_file(path)
                };
            }
        }
        return Ok((true, Some(p.install_dir.clone())));
    }
    std::fs::remove_dir_all(&p.install_dir).map_err(|e| e.to_string())?;
    Ok((true, None))
}

/// The uninstaller of an install that a setup made by Generic Company Wizard Maker left in the install folder (its
/// marker names this program and an installer), or None for this program's own install. Such an install
/// is removed by that uninstaller, which takes away only the files still as installed.
///
/// The contract with the Wizard Maker: its marker has a non-empty "installer" key, and its uninstaller
/// is uninstall.exe in the same folder. The file is not checked to exist; running it reports that.
pub fn wizard(p: &Plan) -> Option<PathBuf> {
    let m = read_marker(&p.install_dir)?;
    let made = m["app_id"].as_str() == Some(&p.app_id) && m["installer"].as_str().is_some_and(|s| !s.is_empty());
    made.then(|| p.install_dir.join(WIZARD_UNINSTALLER))
}

/// The folder the uninstall entry names, or "": from root\registry.json in a sandbox, else read from the
/// registry through PowerShell.
pub fn install_location(p: &Plan, read: Reader) -> Result<String, String> {
    if let Some(file) = &p.registry_file {
        let data: Value = std::fs::read_to_string(file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        return Ok(data[&p.registry_key]["InstallLocation"]
            .as_str()
            .unwrap_or_default()
            .to_string());
    }
    // UTF-8 output keeps a non-ASCII folder name intact; a missing key or value prints nothing
    read(&format!(
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8\n(Get-ItemProperty -LiteralPath {} -Name InstallLocation -ErrorAction SilentlyContinue).InstallLocation\n",
        literal(&p.registry_key)
    ))
}

/// `p` aimed at the install its uninstall entry names when that lies outside its install folder: a setup
/// made by Generic Company Wizard Maker installs where the user picks. Only such an install (see `wizard`) is handed
/// on; any other folder still there is refused, so its entry and shortcuts are never cleared.
///
/// Errors: the entry could not be read, or it names a folder elsewhere that is not this program's
/// wizard-made install (the user is sent to Apps & features).
pub fn located(p: &Plan, read: Reader) -> Result<Plan, String> {
    let location = PathBuf::from(install_location(p, read)?);
    if !location.is_absolute() || same(&location, &p.install_dir) || !location.exists() {
        return Ok(p.clone()); // none, this folder, or a stale entry: clearing it orphans nothing
    }
    let mut there = p.clone();
    there.install_dir = location;
    match wizard(&there) {
        Some(_) => Ok(there),
        None => Err(format!(
            "{} is installed in {}, which this copy cannot uninstall: use Apps & features",
            p.name,
            there.install_dir.display()
        )),
    }
}

/// The flags --uninstall passes on to that uninstaller.
///
/// The Wizard Maker's flags take a single dash (-quiet, -purge, -root DIR); the root is made absolute
/// because the uninstaller starts in the temp folder.
pub fn uninstall_args(quiet: bool, purge: bool, root: Option<&Path>) -> Vec<String> {
    let mut args = Vec::new();
    if quiet {
        args.push("-quiet".to_string());
    }
    if purge {
        args.push("-purge".to_string());
    }
    if let Some(root) = root {
        args.push("-root".to_string());
        args.push(
            std::path::absolute(root)
                .unwrap_or_else(|_| root.to_path_buf())
                .display()
                .to_string(),
        );
    }
    args
}

/// Runs an exe with arguments and waits -> (exit code, its output); `run_and_wait` is the real one.
pub type Exec<'a> = &'a dyn Fn(&Path, &[String]) -> Result<(i32, String), String>;

/// The real `Exec`: runs exe and waits -> (exit code, what it printed). It starts in the temp folder, so
/// it never holds the folder it removes.
pub fn run_and_wait(exe: &Path, args: &[String]) -> Result<(i32, String), String> {
    let out = Command::new(exe)
        .args(args)
        .current_dir(std::env::temp_dir())
        .output()
        .map_err(|e| e.to_string())?;
    let said = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    // code() is None only when no exit code exists; count that as a failure
    Ok((out.status.code().unwrap_or(1), said.trim().to_string()))
}

/// Removes a wizard-made install (see `wizard`) through its uninstaller, run with `args` -> its exit code
/// (0 done, `CANCELLED`, ...) and how it went. Refuses while `running` lies in the install folder: Windows
/// cannot delete a running exe, so the uninstaller would stop half-way.
pub fn run_uninstaller(p: &Plan, running: &Path, args: &[String], run: Exec) -> Result<(i32, String), String> {
    let exe =
        wizard(p).ok_or_else(|| format!("{} holds no wizard-made install of {}", p.install_dir.display(), p.name))?;
    let running = std::path::absolute(running).unwrap_or_else(|_| running.to_path_buf());
    // skip(1): the exe's own path is not a folder; any folder above it counts, so a subfolder too
    if running.ancestors().skip(1).any(|d| same(d, &p.install_dir)) {
        return Err(format!(
            "this copy runs from {}, which its uninstaller removes: uninstall {} from Apps & features, or run {}",
            p.install_dir.display(),
            p.name,
            exe.display()
        ));
    }
    let (code, mut said) = run(&exe, args).map_err(|e| format!("run {}: {e}", exe.display()))?;
    // a silent uninstaller still gets a message, so the user learns how it ended
    if said.is_empty() && code == CANCELLED {
        said = "Uninstall cancelled.".into();
    } else if said.is_empty() && code != 0 {
        said = format!("{WIZARD_UNINSTALLER} exited with {code}");
    }
    Ok((code, said))
}

/// A hidden cmd that waits a moment, then removes dir (three tries, for about 4 seconds). Start it as this
/// process's very last action: it gives up while the exe still runs.
///
/// cmd has no sleep command, so `ping -n N 127.0.0.1` waits about N - 1 seconds (one echo a second to
/// this machine). `rd /s /q` removes the folder without asking; the later tries cover an exe that is
/// still closing. The line goes through `raw_arg` because cmd parses its own quotes, and Rust's usual
/// argument quoting would break them. The cmd starts in the temp folder, so it does not hold `dir`
/// itself, and has no window (CREATE_NO_WINDOW). It is not waited for.
pub fn delete_after_exit(dir: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let rd = format!("rd /s /q \"{}\"", dir.display());
    // /d skips cmd's AutoRun commands; /c runs the line, then exits
    let line = format!(
        "/d /c \"ping -n 3 127.0.0.1 >nul & {rd} & ping -n 2 127.0.0.1 >nul & {rd} & ping -n 2 127.0.0.1 >nul & {rd}\""
    );
    Command::new("cmd.exe")
        .raw_arg(line)
        .current_dir(std::env::temp_dir())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Plans, scripts and real file work in temp sandboxes; PowerShell runs only where a test needs it.
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A fresh temp folder -> (it, a fake build folder with an exe and a www holding a pkg\.gitignore,
    /// a sandbox root inside it that does not exist yet).
    fn setup(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("generic-company-install-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let build = dir.join("build");
        std::fs::create_dir_all(build.join("www").join("pkg")).unwrap();
        std::fs::write(build.join("generic-company-test.exe"), b"MZ fake").unwrap();
        std::fs::write(build.join("www").join("index.html"), b"<html>").unwrap();
        std::fs::write(build.join("www").join("pkg").join("a.wasm"), b"\0asm").unwrap();
        std::fs::write(build.join("www").join("pkg").join(".gitignore"), b"*").unwrap();
        (dir.clone(), build, dir.join("sandbox"))
    }

    /// A test program whose exe and www are in `build`; the icon is four bytes.
    fn program(build: &Path) -> Program<'static> {
        Program {
            app_id: "GenericCompany.Test",
            name: "Generic Company Test App",
            version: "0.2.0",
            publisher: "Generic Company Inc. (fictional)",
            exe: build.join("generic-company-test.exe"),
            exe_name: "generic-company-test.exe",
            www: build.join("www"),
            icon: &[0, 0, 1, 0],
        }
    }

    /// An environment with no variables set.
    fn none(_: &str) -> Option<String> {
        None
    }

    /// Sandbox paths, sizes (4096 bytes -> 5 KB) and the --root in the uninstall command; a sandbox
    /// script never touches HKCU; an apostrophe in a real path (O'Brien) is doubled inside the quotes.
    #[test]
    fn plan_and_script() {
        let (dir, build, root) = setup("plan");
        let p = plan(&program(&build), &none, Some(&root), true, 4096, "20261017");
        assert_eq!(
            p.install_dir,
            std::path::absolute(&root)
                .unwrap()
                .join("Programs")
                .join("Generic Company Inc")
                .join("Generic Company Test App")
        );
        assert_eq!(p.shortcuts.len(), 2);
        assert!(p.registry.contains(&("DisplayVersion", Reg::Text("0.2.0".into()))));
        assert!(p.registry.contains(&("EstimatedSize", Reg::DWord(5))));
        assert!(matches!(&p.registry[5].1, Reg::Text(s) if s.contains("--uninstall --root")));
        assert!(!script(&p, false).contains("HKCU") && !script(&p, true).contains("HKCU"));
        let vars = |k: &str| (k == "LOCALAPPDATA").then(|| r"C:\O'Brien".to_string());
        let real = plan(&program(&build), &vars, None, false, 1, "20261017");
        let s = script(&real, false);
        assert!(s.contains(r"'C:\O''Brien\Programs\Generic Company Inc\Generic Company Test App\app.ico,0'"));
        assert!(s.contains("-Name 'NoModify' -Value '1' -PropertyType DWord"));
        assert!(script(&real, true).contains(
            r"Remove-Item -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\GenericCompany.Test'"
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Installing twice then uninstalling: www copied without .gitignore, the sandbox entry written,
    /// the folder gone, a database in the root untouched, and one script per step (3).
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
        std::fs::write(root.join("generic_company.db"), b"").unwrap();
        install(&p, &run).unwrap();
        install(&p, &run).unwrap();
        assert!(p.install_dir.join("www").join("pkg").join("a.wasm").is_file());
        assert!(!p.install_dir.join("www").join("pkg").join(".gitignore").exists());
        let reg: Value =
            serde_json::from_str(&std::fs::read_to_string(p.registry_file.as_ref().unwrap()).unwrap()).unwrap();
        assert_eq!(reg[&p.registry_key]["DisplayVersion"], "0.2.0");
        let (removed, later) = uninstall(&p, &run, &build.join("generic-company-test.exe"), None).unwrap();
        assert!(removed && later.is_none() && !p.install_dir.exists());
        assert!(root.join("generic_company.db").exists());
        assert_eq!(*scripts.borrow(), 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Uninstalling from the installed exe leaves only that exe and returns the folder; a marker naming
    /// another app makes both install and uninstall refuse.
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
        let left: Vec<_> = std::fs::read_dir(&p.install_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("generic-company-test.exe")]);
        std::fs::write(p.install_dir.join(MARKER), r#"{"app_id":"GenericCompany.Other"}"#).unwrap();
        assert!(install(&p, &ok).is_err());
        assert!(uninstall(&p, &ok, &build.join("generic-company-test.exe"), None).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// What a setup made by Generic Company Wizard Maker leaves: its files, uninstall.exe and a marker with sha256 and
    /// installer; then notes.txt changes, as a user would change it.
    fn wizard_install(p: &Plan, app_id: &str) {
        std::fs::create_dir_all(&p.install_dir).unwrap();
        let files = ["generic-company-test.exe", "notes.txt", "app.ico", WIZARD_UNINSTALLER];
        for f in files {
            std::fs::write(p.install_dir.join(f), b"as installed").unwrap();
        }
        let marker = json!({"app_id": app_id, "name": p.name, "version": p.version, "files": files, "shortcuts": [],
            "sha256": {"notes.txt": "e3b0"}, "installer": "Generic Company Wizard Maker 0.1.0"});
        std::fs::write(p.install_dir.join(MARKER), marker.to_string()).unwrap();
        std::fs::write(p.install_dir.join("notes.txt"), b"changed by the user").unwrap();
    }

    /// `wizard` finds an uninstaller only for this app's wizard-made install, and the passed-on flags
    /// are single-dash with an absolute root.
    #[test]
    fn wizard_installs_are_their_uninstallers_to_remove() {
        let (dir, build, root) = setup("wizard");
        let p = plan(&program(&build), &none, Some(&root), false, 1, "20261017");
        let ok = |_: &str| -> Result<(), String> { Ok(()) };
        assert_eq!(wizard(&p), None);
        install(&p, &ok).unwrap();
        assert_eq!(wizard(&p), None, "the program's own install");
        wizard_install(&p, "GenericCompany.Other");
        assert_eq!(wizard(&p), None, "never another program's uninstaller");
        wizard_install(&p, "GenericCompany.Test");
        assert_eq!(wizard(&p), Some(p.install_dir.join("uninstall.exe")));
        assert!(uninstall_args(false, false, None).is_empty());
        let abs = std::path::absolute(&root).unwrap().display().to_string();
        assert_eq!(
            uninstall_args(true, true, Some(&root)),
            ["-quiet", "-purge", "-root", abs.as_str()]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The uninstaller's code and words come back as they are (4, "already running"), a silent cancel
    /// gets a message, and a copy running inside the folder (even in upper case) or a folder with no
    /// wizard install is refused without running anything.
    #[test]
    fn run_uninstaller_keeps_changed_files_and_its_exit_code() {
        let (dir, build, root) = setup("uninstaller");
        let p = plan(&program(&build), &none, Some(&root), false, 1, "20261017");
        wizard_install(&p, "GenericCompany.Test");
        let calls = RefCell::new(Vec::new());
        let busy = |exe: &Path, args: &[String]| -> Result<(i32, String), String> {
            calls.borrow_mut().push((exe.to_path_buf(), args.to_vec()));
            Ok((
                4,
                "Setup or uninstall of Generic Company Test App is already running.".into(),
            ))
        };
        let args = uninstall_args(true, false, Some(&root));
        let (code, said) = run_uninstaller(&p, &build.join("generic-company-test.exe"), &args, &busy).unwrap();
        assert!(code == 4 && said.contains("already running"));
        assert_eq!(
            *calls.borrow(),
            vec![(p.install_dir.join("uninstall.exe"), args.clone())]
        );
        assert_eq!(
            std::fs::read_to_string(p.install_dir.join("notes.txt")).unwrap(),
            "changed by the user"
        );
        let cancel = |_: &Path, _: &[String]| -> Result<(i32, String), String> { Ok((CANCELLED, String::new())) };
        assert_eq!(
            run_uninstaller(&p, &build.join("generic-company-test.exe"), &[], &cancel).unwrap(),
            (CANCELLED, "Uninstall cancelled.".to_string())
        );
        // run from inside the folder (in any case) it refuses, as it does with no wizard-made install
        let inside = PathBuf::from(p.exe_target.display().to_string().to_uppercase());
        assert!(run_uninstaller(&p, &inside, &[], &busy).is_err());
        let elsewhere = plan(&program(&build), &none, Some(&root.join("x")), false, 1, "20261017");
        assert!(run_uninstaller(&elsewhere, &build.join("generic-company-test.exe"), &[], &busy).is_err());
        assert_eq!(calls.borrow().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The sandbox uninstall entry naming `folder`, as a setup writes it.
    fn entry(p: &Plan, folder: &Path) {
        edit_registry_file(p.registry_file.as_ref().unwrap(), |d| {
            d.insert(
                p.registry_key.clone(),
                json!({"InstallLocation": folder.display().to_string()}),
            );
        })
        .unwrap();
    }

    /// No entry and a stale entry keep the default folder; an entry naming a wizard install elsewhere
    /// moves the plan there, and its uninstaller is the one run. The sandbox never reads the registry.
    #[test]
    fn located_finds_a_wizard_install_elsewhere() {
        let (dir, build, root) = setup("located");
        let p = plan(&program(&build), &none, Some(&root), false, 1, "20261017");
        let real = |_: &str| -> Result<String, String> { panic!("read the real registry in a sandbox") };
        assert_eq!(located(&p, &real).unwrap().install_dir, p.install_dir, "no entry");
        let mut elsewhere = p.clone();
        elsewhere.install_dir = std::path::absolute(&root).unwrap().join("Apps").join("Elsewhere");
        entry(&p, &elsewhere.install_dir);
        assert_eq!(located(&p, &real).unwrap().install_dir, p.install_dir, "a stale entry");
        wizard_install(&elsewhere, "GenericCompany.Test");
        let there = located(&p, &real).unwrap();
        assert_eq!(there.install_dir, elsewhere.install_dir);
        let calls = RefCell::new(Vec::new());
        let done = |exe: &Path, _: &[String]| -> Result<(i32, String), String> {
            calls.borrow_mut().push(exe.to_path_buf());
            Ok((0, String::new()))
        };
        assert_eq!(
            run_uninstaller(&there, &build.join("generic-company-test.exe"), &[], &done)
                .unwrap()
                .0,
            0
        );
        assert_eq!(*calls.borrow(), vec![elsewhere.install_dir.join("uninstall.exe")]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An existing folder elsewhere that is another app's, or has no marker, is refused and its entry kept.
    #[test]
    fn located_never_clears_an_install_elsewhere() {
        let (dir, build, root) = setup("elsewhere");
        let p = plan(&program(&build), &none, Some(&root), false, 1, "20261017");
        let mut elsewhere = p.clone();
        elsewhere.install_dir = std::path::absolute(&root).unwrap().join("Apps").join("Elsewhere");
        entry(&p, &elsewhere.install_dir);
        let empty = |_: &str| -> Result<String, String> { Ok(String::new()) };
        wizard_install(&elsewhere, "GenericCompany.Other");
        assert!(located(&p, &empty).is_err());
        std::fs::remove_file(elsewhere.install_dir.join(MARKER)).unwrap();
        assert!(located(&p, &empty).is_err());
        assert!(std::fs::read_to_string(p.registry_file.as_ref().unwrap())
            .unwrap()
            .contains("InstallLocation"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Outside a sandbox the entry is read with Get-ItemProperty on the real key; a relative answer is
    /// ignored and a failed read is an error. The reader is faked, so the registry is not touched.
    #[test]
    fn located_reads_the_real_registry() {
        let (dir, build, root) = setup("registry");
        let p = plan(&program(&build), &none, None, false, 1, "20261017");
        let folder = std::path::absolute(&root).unwrap().join("Apps").join("Elsewhere");
        let mut elsewhere = p.clone();
        elsewhere.install_dir = folder.clone();
        wizard_install(&elsewhere, "GenericCompany.Test");
        let scripts = RefCell::new(Vec::new());
        let read = |s: &str| -> Result<String, String> {
            scripts.borrow_mut().push(s.to_string());
            Ok(folder.display().to_string())
        };
        assert_eq!(located(&p, &read).unwrap().install_dir, folder);
        assert!(scripts.borrow()[0].contains(r"Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\GenericCompany.Test' -Name InstallLocation"));
        let relative = |_: &str| -> Result<String, String> { Ok("Apps".into()) };
        assert_eq!(
            located(&p, &relative).unwrap().install_dir,
            p.install_dir,
            "never a relative folder"
        );
        let failed = |_: &str| -> Result<String, String> { Err("denied".into()) };
        assert!(located(&p, &failed).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Runs real PowerShell: a non-ASCII letter comes back intact with UTF-8 output, and a key that does
    /// not exist reads as "".
    #[test]
    fn read_powershell_prints_utf8() {
        let out =
            read_powershell("[Console]::OutputEncoding = [Text.Encoding]::UTF8\n'M' + [char]0xF6 + 'tley'\n").unwrap();
        assert_eq!(out, "M\u{f6}tley");
        let (dir, build, _) = setup("noentry");
        let mut p = plan(&program(&build), &none, None, false, 1, "20261017");
        p.registry_key = format!(r"{UNINSTALL_KEY}\GenericCompany.NoSuchProgram.Test");
        assert_eq!(install_location(&p, &read_powershell).unwrap(), "");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// cmd's output and exit code 4 come back; an exe that does not exist is an error, not a code.
    #[test]
    fn run_and_wait_returns_the_exit_code() {
        let args = ["/d", "/c", "echo kept & exit 4"].map(String::from);
        assert_eq!(
            run_and_wait(Path::new("cmd.exe"), &args).unwrap(),
            (4, "kept".to_string())
        );
        assert!(run_and_wait(
            &std::env::temp_dir().join("generic-company-none").join("uninstall.exe"),
            &[]
        )
        .is_err());
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=2cdf67a176bc5e61178af652995f5316bbc2108e25ea5ceb344b12fbbc0ff382 ====
