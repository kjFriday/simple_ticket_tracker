<img src="icon/app-256.png" width="64" height="64" alt="" style="image-rendering: pixelated">

# Generic Company Ticket Tracker

A kanban board for Generic Company Inc. support tickets, as a desktop program: a native window (Edge WebView2)
running a Rust → WebAssembly page over the shared Generic Company database. Generic Company Inc. and everyone in it is
fictional.

- **Shared database:** the window opens the Generic Company database **by path**: `--db PATH`, the `GENERIC_COMPANY_DB`
  environment variable, the file chosen with **Database…**, or `%LOCALAPPDATA%\Generic Company Inc\generic_company.db`. Every
  change is a real insert/update/delete on its `tickets` table, answered in-process with the CRUD hub's
  API contract (paging, `X-Total-Count`, 400/404/409) through the window's own `generic-company://` protocol: no
  network port, no other program needed. The status bar shows each call and its answer.
- **Mock snapshot:** with no database (or `--mock`, or *Data → Mock snapshot*) the page runs on the
  bundled `mock/` snapshot (400 tickets); edits stay in the page.

See [SPEC.md](SPEC.md) for what the board does.

## How it works

One crate, built two ways (`src/lib.rs`): `wasm-pack` compiles `core` + `web` into the page's
WebAssembly in `www/pkg/`; `cargo build --features desktop` compiles `core` + `host` into the exe.

1. `src/main.rs` fills a `Spec` (app id, name, window size, the tables and columns the page may read,
   `tickets` as the one writable table, icons, the smoke expression) and calls `host::main`.
2. `host::main` (`src/host/mod.rs`) parses the flags. `--version`, `--install` and `--uninstall` run and
   exit (`src/host/install.rs`).
3. Otherwise `paths::resolve_db` picks the database (`--db`, `GENERIC_COMPANY_DB`, the saved setting,
   then the default file) and `choose` checks it with `api::open` and `api::check_schema`. A missing
   file or column means no database: the page runs on the mock snapshot.
4. The window opens `generic-company://localhost/index.html`. `serve` answers every request:
   `/api/source`, `POST /host/db` (the **Database…** button), other `/api/...` through `api::handle`
   on a fresh SQLite connection, and anything else as a file from `www/` (`src/host/assets.rs`).
5. `www/index.html` loads `www/main.js`, which loads `www/pkg/ticket_tracker.js`; that starts
   `web::start` (`src/web.rs`). It builds the page state on the mock snapshot, wires the events, then
   `load_db` asks `/api/source` and, with a database, pages every ticket, customer and employee in
   (`?limit=500&offset=N`).
6. Each edit becomes a `Change`; `apply` sends it to `apply_db` (POST/PATCH/DELETE) or `apply_mock`,
   then `draw` renders the board again from `core` (`board::columns`, `render::board`). The status bar
   reports every call.

Where state lives:

| What | Where |
|---|---|
| Tickets, customers, employees | The shared SQLite file, default `%LOCALAPPDATA%\Generic Company Inc\generic_company.db` (never deleted by this program) |
| Chosen database path | `%LOCALAPPDATA%\Generic Company Inc\GenericCompany.Tickets\settings.json` (`src/host/paths.rs`) |
| Window browser profile | `%LOCALAPPDATA%\Generic Company Inc\GenericCompany.Tickets\WebView2\` |
| Install | `%LOCALAPPDATA%\Programs\Generic Company Inc\Generic Company Ticket Tracker\` with an `install.json` marker; the uninstall entry under `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\GenericCompany.Tickets` |
| Mock snapshot | `mock/*.json`, compiled into the wasm by `include_str!` in `src/web.rs`; edits live only in the page |
| Page state | The `App` in `src/web.rs` (a `Store` of tickets and a `Filter`); reset on reload |

## Code map

In reading order:

| File | What |
|---|---|
| `src/main.rs` | Entry point: the ticket tracker's `Spec`, handed to `host::main` |
| `src/lib.rs` | The crate's three modules and which build compiles which |
| `src/core/mod.rs` | Module list for the plain-Rust board logic |
| `src/core/model.rs` | `Status`, `Ticket`, `Person`; lenient parsing of `/api` rows |
| `src/core/board.rs` | `Filter`, `columns`, and `Store`, the page's ticket list |
| `src/core/render.rs` | Board and picker HTML, every value escaped |
| `src/core/validate.rs` | `clean` (the hub's textsafe rules) and `Draft`, the editor form as request bodies |
| `src/web.rs` | Page wiring: `start`, `load_db`, `apply`, `draw`, DOM events (wasm only) |
| `www/index.html` | The static page frame that `web.rs` fills by element id |
| `www/main.js` | Loads the wasm build |
| `www/style.css` | The board's look |
| `src/host/mod.rs` | The desktop program: flags, database choice, window, `serve`, `--smoke` |
| `src/host/api.rs` | `/api` over SQLite with the CRUD hub's contract |
| `src/host/assets.rs` | Finds `www/` and serves its files safely |
| `src/host/paths.rs` | Per-user folders, `settings.json`, which database to open |
| `src/host/install.rs` | Per-user install and uninstall (shortcuts, uninstall entry, marker) |
| `build.rs` | Links `icon/app.res` (icon and DPI manifest) into the exe |
| Tests in `src/core/*.rs` | Models, filters, columns, escaping, cleaning, request bodies (`cargo test`) |
| Tests in `src/host/*.rs` | API against an in-memory database from `mock/schema.sql`, file serving, paths, installer (`--features desktop`) |
| `tests/icon.rs` | The icon files agree with each other and keep their credit |

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
target\release\generic-company-tickets.exe
```

The first command keeps the build machine's paths out of the wasm and the exe: both builds write the
user folder, which holds cargo's crate sources and rustup's library sources, as `~` (one flag, so a
folder name with spaces works). The exe serves the page from the `www` folder beside it (or this project's `www` when run from here).
`build.rs` links `icon/app.res` into it: the pixel icon and a DPI-aware manifest. The page still runs in
a plain browser on the mock snapshot (`python -m http.server 8710 --directory www`).

## Install

```bash
target\release\generic-company-tickets.exe --install
```

Installs for the current user: copies the exe and `www` to
`%LOCALAPPDATA%\Programs\Generic Company Inc\Generic Company Ticket Tracker\`, adds **Generic Company Inc → Generic Company Ticket Tracker** to the
Start menu (`--desktop-shortcut` for the desktop too) and lists it in **Apps & features**. `--uninstall`
(or Apps & features) removes it; the database is never deleted (`--purge` also removes the program's
settings and WebView2 profile). `--root DIR` does all of it inside DIR. Other flags: `--version`,
`--quiet`, `--smoke FILE`.

`..\wizard_maker\wizard-maker.exe build setup.json` (Generic Company Wizard Maker) makes the setup,
`dist\Generic Company Ticket Tracker-0.2.0-setup.exe`, from the release exe and `www`.

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
Both run from this folder.

## Developing

- **Run:** after both builds, `target\release\generic-company-tickets.exe` (add `--mock` for the
  snapshot, `--db PATH` for another file). Rebuild with `wasm-pack` after any change to `src/core` or
  `src/web.rs`; the exe serves whatever is in `www/pkg/`.
- **Test:** `cargo test`, then `cargo test --features desktop`, from this folder.
- **Smoke check:** from this folder, `Start-Process -Wait target\release\generic-company-tickets.exe
  -ArgumentList '--mock','--smoke','smoke.json','--quiet'` opens the window, waits for the cards and
  writes a JSON report (`counts.shown` is 400 on the snapshot). The portfolio's `tools/verify.ps1` runs
  the same check against a sandbox copy of the shared database.
- **Package:** the release build, then the Wizard Maker command under Install.

Where to add things:

- **A new ticket field** (say a due date): the column must first exist in the shared schema (the CRUD
  hub). Then add it to the `tickets` read list in `src/main.rs`, to `Ticket` in `src/core/model.rs`, to
  `Draft` and its bodies in `src/core/validate.rs`, to the card in `src/core/render.rs`, and an editor
  field in `www/index.html` read by `open_editor`, `read_editor` and `draft_of` in `src/web.rs`.
- **A new filter:** a field on `Filter` and a test in `matches` (`src/core/board.rs`), a control in
  `www/index.html`, and its handler in `wire` (`src/web.rs`).

Twins (kept identical with the copy in `company/data_dash`; change both): `src/host/install.rs`,
`src/host/paths.rs` and `rustfmt.toml`. `src/host/assets.rs` matches too, except for the wasm file name
in its test. `clean` in `src/core/validate.rs` is a port of the CRUD hub's `textsafe.py`: change them
together.

Traps the comments warn about:

- **Version:** `version` in `Cargo.toml` (shown in the title, footer and uninstall entry) and in
  `setup.json` (the setup's file name) move together.
- **App id:** `GenericCompany.Tickets` in `src/main.rs` names the uninstall key and the data folder;
  changing it orphans existing installs and settings. `setup.json` repeats it.
- **Status text:** `open`, `in_progress`, `closed` are stored in `tickets.status`, used as the
  `#e-status` options in `www/index.html` and written as `data-status` by `render.rs`; rename all of them.
- **Ids and markup:** `src/web.rs` finds `www/index.html` elements by id, and reads `data-act` and
  `data-id` from `render.rs` markup; `.card` is what `--smoke` counts.
- **Text limit:** `LIMIT` (240) in `validate.rs` equals the `maxlength` of the title and details fields
  in `www/index.html`.
- **SQL names:** `src/host/api.rs` builds SQL from table and column names, safe only because they come
  from the static `Tables` in `src/main.rs`; never take a name from a request.
- **Generated files, do not edit:** `www/pkg/` (wasm-pack), `mock/` (the hub's `export_mock.py`),
  `icon/` (the portfolio's `tools/pixel_icons.py`), `dist/` (Wizard Maker).
- **Stamps:** every source file ends with a `stamp v1` line checked by the portfolio's
  `tools/stamp.py --check`; leave that line alone.

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
