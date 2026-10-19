# CLAUDE.md

modhub: a registry for Stormlight WASM mods (publish, version, validate against the
mod ABI, serve). Portfolio project for Rust backend roles: tokio, axum, PostgreSQL
(sqlx), object storage, background jobs, observability, deployment.

## Shape

- `crates/core`: pure domain, no IO. Every rule a request is checked against lives here.
- `crates/server`: thin shell over core. `api` (axum router; handlers parse into core
  types, `ApiError` maps every failure to a status and a stable JSON `error` code;
  store failures log and answer `500 internal`), `store` (PostgreSQL via sqlx `query!`
  macros, checked at compile time against `DATABASE_URL`), `config` (environment → `Config`,
  pure, password-free `Debug`), `serve` (graceful shutdown). `src/main.rs` is the `modhub`
  binary: config, logs, pool, migrations, signal handlers, serve.
- `nix build` / `nix run` build the binary with `SQLX_OFFLINE`; after `Cargo.lock` changes,
  set `cargoHash` in `flake.nix` to `lib.fakeHash`, build, copy the reported hash.
- API tests drive the router in-process (`tests/support/http.rs`, `TestApi`), model-based
  like the store tests (`api_model.rs`). `binary.rs` runs the built binary as a process
  (startup failures, SIGTERM/SIGINT, no password in logs) with deadlines, never hangs.
- Check what a test actually reaches: `cargo llvm-cov report --text` shows per-line counts.
  A rule hit a handful of times means the generator rarely gets there; fix the generator.
- Migrations: `crates/server/migrations/`, embedded with `sqlx::migrate!`. After changing a
  query or a migration run `cargo sqlx prepare --workspace` and commit `.sqlx/`
  (`tools/gate all` checks it is current).

## Database

- `tools/pg start|stop|status|psql`: one dev cluster under `.pg/`, unix socket only,
  fsync off (disposable data). The dev shell sets `DATABASE_URL` to it.
- Integration tests (`crates/server/tests/integration/`) each create a throwaway database
  (`tests/support/database.rs`) and `reset()` it between bolero iterations.
- Database tests are model-based where possible: random operation sequences against an
  in-memory model (`registry_model.rs`).

## Rules

- Global `~/.claude/CLAUDE.md` applies in full: TDD, bolero-only tests in `tests/`,
  nextest, no inline tests, no `mod.rs`.
- Tests: `tests/property/` (laws), `tests/fuzz/` (parsers on arbitrary input),
  `tests/support/` (generators; each domain range is generated in one place).
- Gate: `tools/gate quick` (pre-commit hook), `tools/gate all` before merging to main,
  `tools/gate cov` for branch coverage.
- Night runs (copied from atelier, reports for an agent to work from):
  - `tools/fuzz_night.sh <budget> [filter] [threads]` → `crashes/LATEST.md`
  - `tools/mutants_night.sh <budget> [all|core,…] [jobs]` → `mutants/LATEST.md`
  - Threads/jobs: 1, at most 2; more hangs this machine.
