# modhub

A registry for [Stormlight](https://github.com/AestroFidelium/stormlight-sdk) WASM mods:
publish a mod, version it, validate it against the mod ABI, and serve it to game servers.

> Status: early. The domain core is being built test-first; the HTTP service comes next.

## Layout

| Crate | Holds |
| --- | --- |
| `crates/core` (`modhub-core`) | Pure domain: names, versions, validation rules. No IO. |
| `crates/server` (`modhub-server`) | PostgreSQL store (sqlx, compile-time checked queries); the HTTP API comes next. |

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
