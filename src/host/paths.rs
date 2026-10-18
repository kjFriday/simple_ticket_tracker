// ==== ACME Inc. portfolio ===============================================
// Project: simple_ticket_tracker | File: src/host/paths.rs
// About: Where the program keeps its files.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Where the program keeps its files. The shared database is the only link between the ACME
//! programs; its path is, in order: --db, ACME_DB, the program's saved setting, then
//! %LOCALAPPDATA%\ACME Inc\acme.db. The setting is %LOCALAPPDATA%\ACME Inc\<app id>\settings.json.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

pub fn real_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn home() -> PathBuf {
    real_env("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn local_appdata(env: Env) -> PathBuf {
    env("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| home().join("AppData").join("Local"))
}

pub fn base_dir(env: Env) -> PathBuf {
    local_appdata(env).join("ACME Inc")
}

pub fn default_db(env: Env) -> PathBuf {
    base_dir(env).join("acme.db")
}

pub fn data_dir(app_id: &str, env: Env) -> PathBuf {
    base_dir(env).join(app_id)
}

fn settings_path(app_id: &str, env: Env) -> PathBuf {
    data_dir(app_id, env).join("settings.json")
}

pub fn load_settings(app_id: &str, env: Env) -> Map<String, Value> {
    std::fs::read_to_string(settings_path(app_id, env))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Merges changes into settings.json (unknown keys kept), via a temp file and a rename.
pub fn save_settings(app_id: &str, env: Env, changes: Map<String, Value>) -> std::io::Result<()> {
    let mut data = load_settings(app_id, env);
    data.extend(changes);
    data.insert("version".into(), Value::from(1));
    let path = settings_path(app_id, env);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&Value::Object(data)).unwrap() + "\n")?;
    std::fs::rename(tmp, path)
}

fn absolute(p: &str) -> PathBuf {
    let p = Path::new(p);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf())
    }
}

pub fn resolve_db(flag: Option<&str>, app_id: &str, env: Env) -> PathBuf {
    let saved = load_settings(app_id, env).get("db").and_then(Value::as_str).map(String::from);
    [flag.map(String::from), env("ACME_DB"), saved]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .map(|s| absolute(&s))
        .unwrap_or_else(|| default_db(env))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("acme-paths-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn precedence_and_default() {
        let dir = temp("prec");
        let mut vars = HashMap::from([("LOCALAPPDATA".to_string(), dir.display().to_string())]);
        let env_of = |vars: &HashMap<String, String>| {
            let vars = vars.clone();
            move |k: &str| vars.get(k).cloned().filter(|v| !v.is_empty())
        };
        let e = env_of(&vars);
        assert_eq!(resolve_db(None, "ACME.T", &e), dir.join("ACME Inc").join("acme.db"));
        let saved = dir.join("saved.db").display().to_string();
        save_settings("ACME.T", &e, Map::from_iter([("db".to_string(), Value::from(saved.clone()))])).unwrap();
        assert_eq!(resolve_db(None, "ACME.T", &e), PathBuf::from(&saved));
        vars.insert("ACME_DB".into(), dir.join("env.db").display().to_string());
        let e = env_of(&vars);
        assert!(resolve_db(None, "ACME.T", &e).ends_with("env.db"));
        assert!(resolve_db(Some("  "), "ACME.T", &e).ends_with("env.db"));
        assert!(resolve_db(Some(&dir.join("flag.db").display().to_string()), "ACME.T", &e).ends_with("flag.db"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_keep_unknown_keys_and_tolerate_junk() {
        let dir = temp("junk");
        let p = dir.display().to_string();
        let e = move |k: &str| (k == "LOCALAPPDATA").then(|| p.clone());
        std::fs::create_dir_all(data_dir("ACME.T", &e)).unwrap();
        std::fs::write(settings_path("ACME.T", &e), "{not json").unwrap();
        assert!(load_settings("ACME.T", &e).is_empty());
        save_settings("ACME.T", &e, Map::from_iter([("db".into(), "a".into()), ("theme".into(), "dark".into())])).unwrap();
        save_settings("ACME.T", &e, Map::from_iter([("db".into(), "b".into())])).unwrap();
        let s = load_settings("ACME.T", &e);
        assert_eq!((s["db"].as_str(), s["theme"].as_str(), s["version"].as_i64()), (Some("b"), Some("dark"), Some(1)));
        assert_eq!(std::fs::read_dir(data_dir("ACME.T", &e)).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=fe11108cddd0813da9c24d227890b8b4361eddc1f59a81f00e9621483db762e7 ====
