# Provenance

Code in this project that came from elsewhere in the portfolio (all AGPL-3.0, same author).

| File | From | What |
|---|---|---|
| `src/core/validate.rs` (`clean`) | the CRUD hub's `textsafe.py` | The same text-cleaning rules, in Rust. |
| `src/host/install.rs`, `src/host/paths.rs` | the CRUD hub's `installer.py` and `paths.py` | Rust ports: same plan, PowerShell script, sandbox and settings. |
| `src/host/api.rs` | the CRUD hub's REST API contract (retired from the hub in 0.2.0) | Answered from the shared SQLite file in-process. |

`src/host/mod.rs`, `src/host/assets.rs`, `src/main.rs` and `build.rs` (the desktop window) are new.
