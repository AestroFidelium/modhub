# modhub

A registry for [Stormlight](https://github.com/AestroFidelium/stormlight-sdk) WASM mods:
publish a mod, version it, validate it against the mod ABI, and serve it to game servers.

Every upload is checked against the game's own mod contract before it is stored, so
a server never downloads a mod its engine would refuse to load.

## Run

```sh
tools/pg start                      # local PostgreSQL (or point DATABASE_URL elsewhere)
nix run                             # listens on 127.0.0.1:8080
curl -X POST localhost:8080/mods -H 'content-type: application/json' -d '{"name":"gpu-cloth"}'
curl -X PUT  localhost:8080/mods/gpu-cloth/versions/1.0.0 \
     -H 'content-type: application/wasm' --data-binary @gpu_cloth.wasm
curl         localhost:8080/mods/gpu-cloth/versions
curl -o gpu_cloth.wasm localhost:8080/mods/gpu-cloth/versions/1.0.0
```

| Variable | Default | |
| --- | --- | --- |
| `DATABASE_URL` | required | `postgres://` or `postgresql://` URL |
| `MODHUB_LISTEN` | `127.0.0.1:8080` | `IP:port` |
| `RUST_LOG` | `info,sqlx=warn` | log filter; logs go to stderr |

Stops on SIGTERM or Ctrl-C after in-flight requests finish.

## API

| Method | Path | Does |
| --- | --- | --- |
| `POST` | `/mods` | register `{"name": "gpu-cloth"}` → `201` |
| `PUT` | `/mods/{name}/versions/{version}` | publish the `application/wasm` body → `201` with its `sha256`, size and entry points |
| `GET` | `/mods/{name}/versions/{version}` | download → `200 application/wasm`, `ETag` = SHA-256, cached as immutable |
| `GET` | `/mods/{name}/versions` | versions, lowest precedence first → `200` |

Releases are immutable: a version is published once (`409 version_exists` after) and
always serves the same bytes. The SHA-256 stored with a module is checked on every
download; a mismatch is a `500`, never a corrupt module.

Errors are JSON, `{"error": "version_exists", "message": "…"}`: branch on `error`.

## The mod contract

A module is accepted when the game's host could load it:

- valid WebAssembly 2.0 (no threads, no shared memory, no components), at most 8 MiB;
- **no imports at all**: mods run with no WASI and no host calls;
- exports `memory` (initial size within the host's 64 MiB) and `mod_register: () -> i64`;
- any of `mod_handle`, `mod_trigger`, `mod_tick` it exports have the host's signatures,
  and come with `mod_alloc: (i32) -> i32`.

Export names come from the game's own ABI crate (`stormlight_mod_abi`), and the real
example mod is part of the test suite.

## Layout

| Crate | Holds |
| --- | --- |
| `crates/core` (`modhub-core`) | Pure domain: names, versions, and `ModModule`, the check of a `.wasm` against the Stormlight mod contract. No IO. |
| `crates/server` (`modhub-server`) | HTTP API (axum) over a PostgreSQL store (sqlx, compile-time checked queries). |

## Development

```sh
direnv allow        # or: nix develop
tools/pg start      # local PostgreSQL on a unix socket under .pg/ (the gate starts it too)
cargo t             # tests (nextest)
tools/gate all      # everything CI checks
tools/fuzz_night.sh 10m     # fuzz campaign → crashes/LATEST.md
tools/mutants_night.sh 10m  # mutation testing → mutants/LATEST.md
```

Tests are property and fuzz tests on [bolero](https://github.com/camshaft/bolero),
checked for holes with [cargo-mutants](https://mutants.rs).

## License

MIT OR Apache-2.0.
