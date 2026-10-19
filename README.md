# modhub

A registry for [Stormlight](https://github.com/AestroFidelium/stormlight-sdk) WASM mods:
publish a mod, version it, validate it against the mod ABI, and serve it to game servers.

> Status: early. Registration, publishing and listing work end to end over HTTP;
> mod uploads and WASM validation come next.

## Run

```sh
tools/pg start                      # local PostgreSQL (or point DATABASE_URL elsewhere)
nix run                             # listens on 127.0.0.1:8080
curl -X POST localhost:8080/mods -H 'content-type: application/json' -d '{"name":"gpu-cloth"}'
curl -X PUT  localhost:8080/mods/gpu-cloth/versions/1.0.0
curl         localhost:8080/mods/gpu-cloth/versions
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
| `PUT` | `/mods/{name}/versions/{version}` | publish a release → `201` |
| `GET` | `/mods/{name}/versions` | versions, lowest precedence first → `200` |

Errors are JSON, `{"error": "version_exists", "message": "…"}`: branch on `error`.

## Layout

| Crate | Holds |
| --- | --- |
| `crates/core` (`modhub-core`) | Pure domain: names, versions, validation rules. No IO. |
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
