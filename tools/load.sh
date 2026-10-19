#!/usr/bin/env bash
# HTTP load numbers for README.md: the release binary against the local cluster.
#
#   nix build && tools/load.sh
#
# Uses a throwaway `loadtest` database on the tools/pg cluster (fsync off: dev data),
# port 18080 on loopback. Numbers depend on the machine; README states which.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
if [[ -z "${IN_NIX_SHELL:-}" ]]; then
  exec nix develop --quiet -c "$0" "$@"
fi

module=crates/core/tests/fixtures/example_mod.wasm
base=http://localhost:18080
tools/pg start
psql "$DATABASE_URL" -qc "DROP DATABASE IF EXISTS loadtest WITH (FORCE)" -qc "CREATE DATABASE loadtest"
mkdir -p target
DATABASE_URL="${DATABASE_URL/\/modhub\?//loadtest?}" MODHUB_LISTEN=127.0.0.1:18080 RUST_LOG=warn \
  ./result/bin/modhub >target/load-server.log 2>&1 &
server=$!
trap 'kill -TERM $server; wait $server 2>/dev/null || true' EXIT
for _ in $(seq 50); do curl -s -o /dev/null "$base/mods/x/versions" && break; sleep 0.1; done

curl -s -X POST "$base/mods" -H 'content-type: application/json' -d '{"name":"example"}' >/dev/null
for version in 0.1.0 0.2.0 0.9.0 1.0.0-rc.1 1.0.0; do
  curl -s -o /dev/null -X PUT "$base/mods/example/versions/$version" \
    -H 'content-type: application/wasm' --data-binary @"$module"
done

echo "== download $(stat -c %s "$module") B module, 16 connections, 10 s"
oha -c 16 -z 10s --no-tui "$base/mods/example/versions/1.0.0" |
  grep -E 'Requests/sec|50.00%|99.00%|^ +\[[0-9]{3}\]'
echo "== list versions, 16 connections, 10 s"
oha -c 16 -z 10s --no-tui "$base/mods/example/versions" |
  grep -E 'Requests/sec|50.00%|99.00%|^ +\[[0-9]{3}\]'

echo "== publish (check + store), 300 distinct versions, one at a time"
times=$(mktemp)
for minor in $(seq 1 300); do
  curl -s -o /dev/null -w '%{http_code} %{time_total}\n' -X PUT "$base/mods/example/versions/2.$minor.0" \
    -H 'content-type: application/wasm' --data-binary @"$module"
done >"$times"
awk '{print $1}' "$times" | sort | uniq -c
awk '{print $2 * 1000}' "$times" | sort -n |
  awk '{a[NR] = $1; s += $1} END {printf "  mean %.2f ms, p50 %.2f ms, p99 %.2f ms\n", s / NR, a[int(NR * 0.5)], a[int(NR * 0.99)]}'
rm -f "$times"
