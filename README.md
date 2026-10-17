# Ticket tracker

A kanban board for ACME Inc. support tickets. Rust compiled to WebAssembly, with a plain HTML page and a
four-line JavaScript loader. ACME Inc. and everyone in it is fictional.

- **Standalone:** opens on the bundled `mock/` snapshot (400 tickets). Edits stay in the page.
- **Connected:** switch *Data* to *Live CRUD hub* (or open `?source=hub`) and every change is a real
  `POST`/`PATCH`/`DELETE` against the hub's `tickets` table. The status bar shows each call and its answer.
  If the hub is down, the page says so and falls back to the snapshot.

See [SPEC.md](SPEC.md) for what the board does.

## Build and run

Needs Rust with the `wasm32-unknown-unknown` target and `wasm-pack` (`cargo install wasm-pack`).

```bash
wasm-pack build --target web --out-dir www/pkg --no-typescript
```

```bash
python -m http.server 8710 --bind 127.0.0.1 --directory www
```

Open `http://127.0.0.1:8710/`. For live mode, start the hub first (`python api.py` in
`create_read_update_delete_simple_io_data_base`); `?hub=http://host:port` points at another hub.

## Tests

```bash
cargo test
```

All logic (models, filters, columns, form validation, request bodies, HTML escaping) lives in `src/core`
and runs on the host. `src/web.rs` only wires it to the page.

## Layout

| Path | What |
|---|---|
| `src/core/model.rs` | Ticket, person, status (lenient about unknown statuses) |
| `src/core/board.rs` | Filters, columns, the in-memory store for standalone mode |
| `src/core/validate.rs` | Form → hub JSON; the same text cleaning the hub's `textsafe` applies |
| `src/core/render.rs` | Board and picker HTML, escaped |
| `src/web.rs` | DOM, events, `fetch` to the hub (paged with `?limit=500&offset=N`) |
| `www/` | `index.html`, `style.css`, `main.js`; the build writes `www/pkg/` |
| `mock/` | Snapshot from the hub's `export_mock.py`; do not edit |

## Dependencies

| Crate | Why |
|---|---|
| `serde`, `serde_json` | Parse hub rows and build request bodies; hand-rolled JSON would be most of the code |
| `wasm-bindgen`, `wasm-bindgen-futures`, `js-sys`, `web-sys` | The standard Rust ↔ browser bindings: DOM, events, `fetch`, promises |

No UI framework: the board is a few HTML strings from `render.rs`.

## License

AGPL-3.0, as the rest of the portfolio.
