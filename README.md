<img src="icon/app-256.png" width="64" height="64" alt="" style="image-rendering: pixelated">

# ACME Ticket Tracker

A kanban board for ACME Inc. support tickets, as a desktop program: a native window (Edge WebView2)
running a Rust → WebAssembly page over the shared ACME database. ACME Inc. and everyone in it is
fictional.

- **Shared database:** the window opens the ACME database **by path**: `--db PATH`, the `ACME_DB`
  environment variable, the file chosen with **Database…**, or `%LOCALAPPDATA%\ACME Inc\acme.db`. Every
  change is a real insert/update/delete on its `tickets` table, answered in-process with the CRUD hub's
  API contract (paging, `X-Total-Count`, 400/404/409) through the window's own `acme://` protocol: no
  network port, no other program needed. The status bar shows each call and its answer.
- **Mock snapshot:** with no database (or `--mock`, or *Data → Mock snapshot*) the page runs on the
  bundled `mock/` snapshot (400 tickets); edits stay in the page.

See [SPEC.md](SPEC.md) for what the board does.

## Build and run

Needs Rust (MSVC toolchain) with the `wasm32-unknown-unknown` target and `wasm-pack`. In PowerShell:

```bash
$env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~"
```

```bash
wasm-pack build --target web --out-dir www/pkg --no-typescript
```

```bash
cargo build --release --features desktop
```

```bash
target\release\acme-tickets.exe
```

The first command keeps the build machine's paths out of the wasm and the exe: both builds write the
user folder, which holds cargo's crate sources and rustup's library sources, as `~` (one flag, so a
folder name with spaces works). The exe serves the page from the `www` folder beside it (or this project's `www` when run from here).
`build.rs` links `icon/app.res` into it: the pixel icon and a DPI-aware manifest. The page still runs in
a plain browser on the mock snapshot (`python -m http.server 8710 --directory www`).

## Install

```bash
target\release\acme-tickets.exe --install
```

Installs for the current user: copies the exe and `www` to
`%LOCALAPPDATA%\Programs\ACME Inc\ACME Ticket Tracker\`, adds **ACME Inc → ACME Ticket Tracker** to the
Start menu (`--desktop-shortcut` for the desktop too) and lists it in **Apps & features**. `--uninstall`
(or Apps & features) removes it; the database is never deleted (`--purge` also removes the program's
settings and WebView2 profile). `--root DIR` does all of it inside DIR. Other flags: `--version`,
`--quiet`, `--smoke FILE`.

## Tests

```bash
cargo test
```

```bash
cargo test --features desktop
```

All page logic (models, filters, columns, form validation, request bodies, HTML escaping) lives in
`src/core` and runs on the host. The second command adds the desktop program's tests: the API against an
in-memory database built from `mock/schema.sql`, file serving, the installer and the path settings.

## Layout

| Path | What |
|---|---|
| `src/core/model.rs` | Ticket, person, status (lenient about unknown statuses) |
| `src/core/board.rs` | Filters, columns, the in-memory store for mock mode |
| `src/core/validate.rs` | Form → JSON; the same text cleaning as the hub's `textsafe` |
| `src/core/render.rs` | Board and picker HTML, escaped |
| `src/web.rs` | DOM, events, `fetch` to `/api` (paged with `?limit=500&offset=N`) |
| `src/host/` | The desktop program: window, `acme://` protocol, API over SQLite, installer, settings |
| `src/main.rs` | `acme-tickets.exe` |
| `www/` | `index.html`, `style.css`, `main.js`, `favicon.png`; the wasm build writes `www/pkg/` |
| `icon/` | The pixel icon and its credits |
| `mock/` | Snapshot and schema from the hub's `export_mock.py`; do not edit |

## Dependencies

| Crate | Why |
|---|---|
| `serde`, `serde_json` | Rows and request bodies; hand-rolled JSON would be most of the code |
| `wasm-bindgen`, `wasm-bindgen-futures`, `js-sys`, `web-sys` | The standard Rust ↔ browser bindings |
| `tao`, `wry` (feature `desktop`) | The native window and its WebView2, with a custom protocol for the page and API |
| `rusqlite` with `bundled` (feature `desktop`) | The shared SQLite file; bundled so no SQLite install is needed |

No UI framework: the board is a few HTML strings from `render.rs`.

## Icon and license

Icon: Silk "note" by Mark James, CC BY 2.5, enlarged by nearest-neighbour (`icon/README.md`).
Code: AGPL-3.0, as the rest of the portfolio.
