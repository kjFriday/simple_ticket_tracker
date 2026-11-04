// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: src/host/paths.rs
// About: Where the program keeps its files.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Where the program keeps its files. The shared database is the only link between the Generic Company
//! programs; its path is, in order: --db, GENERIC_COMPANY_DB, the program's saved setting, then
//! %LOCALAPPDATA%\Generic Company Inc\generic_company.db. The setting lives in the program's own
//! folder (see the layout below).
//!
//! Called by host/mod.rs: `resolve_db` at start-up, `save_settings` when the page's Database... button
//! (`POST /host/db`) picks another file, and `data_dir` for the WebView2 profile and `--purge`. host/mod.rs
//! also hands `real_env` to the installer (install.rs), which reads its own folders through it.
//!
//! Every function that reads the environment takes it as an `Env` closure instead of calling
//! `std::env::var`, so the tests can point LOCALAPPDATA at a temp folder without touching the real
//! process environment (which other tests running in parallel share).
//!
//! Layout under %LOCALAPPDATA%:
//! - `Generic Company Inc\generic_company.db`: the default shared database.
//! - `Generic Company Inc\<app id>\settings.json`: this program's settings (only `db` and `version`
//!   today).
//! - `Generic Company Inc\<app id>\WebView2\`: the window's browser profile (made by host/mod.rs).

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// A lookup of environment variables: Some(value) for a set, non-empty variable, else None.
///
/// The program passes `real_env`; tests pass a closure over a map.
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The process environment as an `Env`: an unset, empty or non-Unicode variable reads as None.
///
/// Treating an empty value as unset means `GENERIC_COMPANY_DB=` (set to nothing) falls through to the
/// next choice instead of naming the current folder.
pub fn real_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// The user's profile folder (USERPROFILE), or "." when it is not set.
///
/// Known limitation: it reads the real environment, not the `Env` given to `local_appdata`, so a test
/// that leaves LOCALAPPDATA unset still lands in the real profile.
fn home() -> PathBuf {
    real_env("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The per-user local application data folder: LOCALAPPDATA, else `<profile>\AppData\Local`.
///
/// The fallback is where Windows puts that folder by default, for a process started with a stripped
/// environment.
pub fn local_appdata(env: Env) -> PathBuf {
    env("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("AppData").join("Local"))
}

/// The company folder every Generic Company program shares: `%LOCALAPPDATA%\Generic Company Inc`.
pub fn base_dir(env: Env) -> PathBuf {
    local_appdata(env).join("Generic Company Inc")
}

/// The shared database used when nothing else names one: `generic_company.db` in the company folder.
///
/// Only the path: the file may not exist yet (host/mod.rs `choose` checks it).
pub fn default_db(env: Env) -> PathBuf {
    base_dir(env).join("generic_company.db")
}

/// This program's own folder inside the company folder, named by its app id
/// ("GenericCompany.Tickets"), for its settings and WebView2 profile.
///
/// Not created here; `save_settings` and WebView2 create it on first use.
pub fn data_dir(app_id: &str, env: Env) -> PathBuf {
    base_dir(env).join(app_id)
}

/// The program's settings file: `settings.json` in its data folder.
fn settings_path(app_id: &str, env: Env) -> PathBuf {
    data_dir(app_id, env).join("settings.json")
}

/// The program's saved settings as a JSON object; empty when the file is missing, unreadable, not
/// JSON, or JSON that is not an object.
///
/// Never fails: a damaged settings file must not stop the program from starting, it only loses the
/// saved choice (and the next `save_settings` rewrites it whole).
pub fn load_settings(app_id: &str, env: Env) -> Map<String, Value> {
    std::fs::read_to_string(settings_path(app_id, env))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Merges changes into settings.json (unknown keys kept), via a temp file and a rename.
///
/// `changes` overwrite keys of the same name; every other key already in the file stays, so a setting
/// written by a newer version survives an older one. The file always carries `"version": 1`, the
/// settings format number.
///
/// Side effects: creates the data folder if needed, writes `settings.json.tmp` and renames it over
/// `settings.json`. Errors: any I/O error from those steps (the old file is then left as it was).
///
/// Steps:
/// 1. Load what is there now (empty when missing or damaged) and lay `changes` over it.
/// 2. Stamp the format version.
/// 3. Write the whole object, pretty-printed with a final newline, to a temp file beside the real one.
/// 4. Rename the temp file over the real one.
pub fn save_settings(app_id: &str, env: Env, changes: Map<String, Value>) -> std::io::Result<()> {
    let mut data = load_settings(app_id, env);
    data.extend(changes);
    data.insert("version".into(), Value::from(1));
    let path = settings_path(app_id, env);
    // settings_path always ends in a file name under data_dir, so there is a parent
    std::fs::create_dir_all(path.parent().unwrap())?;
    // write-then-rename: a crash mid-write leaves the old settings whole instead of a cut-off file;
    // on Windows std::fs::rename replaces an existing target
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&Value::Object(data)).unwrap() + "\n")?;
    std::fs::rename(tmp, path)
}

/// `p` as an absolute path: kept when already absolute, else joined to the current folder.
///
/// Does not touch the disk beyond reading the current folder (no symlink or `..` resolution); if even
/// that fails, `p` comes back as given.
fn absolute(p: &str) -> PathBuf {
    let p = Path::new(p);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|d| d.join(p))
            .unwrap_or_else(|_| p.to_path_buf())
    }
}

/// The shared database to open: the first of `--db` (`flag`), GENERIC_COMPANY_DB and the saved `db`
/// setting that is not blank after trimming, made absolute; else `default_db`.
///
/// A blank choice (such as `--db "  "`) is skipped, not an error. The result is only a path: whether
/// the file exists and is a Generic Company database is checked later by host/mod.rs `choose`. A
/// relative choice is taken from the current folder at start-up.
///
/// Steps:
/// 1. Read the saved setting's `db` string, if any.
/// 2. Walk the three candidates in precedence order, dropping unset ones and trimming the rest.
/// 3. Take the first non-empty one and make it absolute; with none, use the default path.
pub fn resolve_db(flag: Option<&str>, app_id: &str, env: Env) -> PathBuf {
    let saved = load_settings(app_id, env)
        .get("db")
        .and_then(Value::as_str)
        .map(String::from);
    // array order is the precedence: command line, then environment, then saved setting
    [flag.map(String::from), env("GENERIC_COMPANY_DB"), saved]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .map(|s| absolute(&s))
        .unwrap_or_else(|| default_db(env))
}

/// Tests for the database precedence and the settings file, each in its own temp folder.
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // a fresh, empty temp folder for one test; the process id keeps parallel test runs apart
    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("generic-company-paths-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    // pins the order default < saved setting < GENERIC_COMPANY_DB < --db, adding one source at a time,
    // and that a blank --db falls through instead of winning
    #[test]
    fn precedence_and_default() {
        let dir = temp("prec");
        let mut vars = HashMap::from([("LOCALAPPDATA".to_string(), dir.display().to_string())]);
        // an Env over a snapshot of the map, so later inserts need a fresh one
        let env_of = |vars: &HashMap<String, String>| {
            let vars = vars.clone();
            move |k: &str| vars.get(k).cloned().filter(|v| !v.is_empty())
        };
        let e = env_of(&vars);
        assert_eq!(
            resolve_db(None, "GenericCompany.T", &e),
            dir.join("Generic Company Inc").join("generic_company.db")
        );
        let saved = dir.join("saved.db").display().to_string();
        save_settings(
            "GenericCompany.T",
            &e,
            Map::from_iter([("db".to_string(), Value::from(saved.clone()))]),
        )
        .unwrap();
        assert_eq!(resolve_db(None, "GenericCompany.T", &e), PathBuf::from(&saved));
        vars.insert("GENERIC_COMPANY_DB".into(), dir.join("env.db").display().to_string());
        let e = env_of(&vars);
        assert!(resolve_db(None, "GenericCompany.T", &e).ends_with("env.db"));
        assert!(resolve_db(Some("  "), "GenericCompany.T", &e).ends_with("env.db"));
        assert!(
            resolve_db(Some(&dir.join("flag.db").display().to_string()), "GenericCompany.T", &e).ends_with("flag.db")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    // pins that a damaged settings file reads as empty, that a second save keeps keys it does not
    // name ("theme"), that "version" is stamped, and that no temp file is left beside settings.json
    #[test]
    fn settings_keep_unknown_keys_and_tolerate_junk() {
        let dir = temp("junk");
        let p = dir.display().to_string();
        let e = move |k: &str| (k == "LOCALAPPDATA").then(|| p.clone());
        std::fs::create_dir_all(data_dir("GenericCompany.T", &e)).unwrap();
        std::fs::write(settings_path("GenericCompany.T", &e), "{not json").unwrap();
        assert!(load_settings("GenericCompany.T", &e).is_empty());
        save_settings(
            "GenericCompany.T",
            &e,
            Map::from_iter([("db".into(), "a".into()), ("theme".into(), "dark".into())]),
        )
        .unwrap();
        save_settings("GenericCompany.T", &e, Map::from_iter([("db".into(), "b".into())])).unwrap();
        let s = load_settings("GenericCompany.T", &e);
        assert_eq!(
            (s["db"].as_str(), s["theme"].as_str(), s["version"].as_i64()),
            (Some("b"), Some("dark"), Some(1))
        );
        assert_eq!(std::fs::read_dir(data_dir("GenericCompany.T", &e)).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=70f5a1a28b602adbb21220f7b5da111b96edf491a2717876006007cfd57967a7 ====
