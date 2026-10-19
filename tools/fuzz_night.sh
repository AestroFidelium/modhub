#!/usr/bin/env bash
# Real fuzz campaign: splits a fuzzing budget across the bolero targets of the whole workspace,
# shows live progress, and writes one report an agent fixes from (crashes/LATEST.md).
# nextest runs are property checks; this is fuzzing.
#
#   tools/fuzz_night.sh 8h                          # every target, 1 thread: the overnight run
#   tools/fuzz_night.sh 10m 'test(/mod_name|version/)' # nextest-style filter on target names
#   tools/fuzz_night.sh 5m 'test(=mod_name::parsing_is_total_and_accepts_only_rule_abiding_text)'
#   tools/fuzz_night.sh 10m mod_name 2               # bare regex, 2 threads
#
# The budget is fuzzing time only. Builds come on top: listing the targets and the first run of
# each crate compile it with sanitizers (cold cache: tens of minutes, later only what changed).
#
# THREADS: keep 1, at most 2. Each "thread" is a whole target whose cargo-bolero spawns libFuzzer
# workers on every core; three at once once drove the load average to 67 and hung this machine.
#
# Output, all under crashes/ (gitignored):
#   REPORT_<time>.md, LATEST.md → newest report   what failed, how to reproduce, what to do
#   logs/<time>/<target>.log                      full cargo-bolero / libFuzzer output
#   inputs/<target>/                              crash / timeout / oom inputs (raw bytes)
#   corpus/<target>/                              corpus, kept: each night resumes the last one
set -uo pipefail

cd "$(git rev-parse --show-toplevel)" || exit 2
if [[ -z "${IN_NIX_SHELL:-}" ]]; then
  exec nix develop --quiet -c bash "$0" "$@"
fi
# shellcheck source=tools/lib/night.sh
source tools/lib/night.sh

dur_arg="${1:-}"
filter="${2:-}"
threads="${3:-1}"

if [[ -z "$dur_arg" ]]; then
  echo "usage: $0 <fuzz_budget> [filter] [threads]   (e.g. 8h, 10m 'test(/a|b/)' 1)" >&2
  exit 2
fi
if ! [[ "$threads" =~ ^[12]$ ]]; then
  echo "refusing threads '$threads': use 1 (at most 2); more hangs this machine" >&2
  exit 2
fi
total_budget="$(parse_dur "$dur_arg")"
if ((total_budget <= 0)); then
  echo "bad duration: '$dur_arg'" >&2
  exit 2
fi

ts="$(date +%Y%m%d_%H%M%S)"
logs="crashes/logs/${ts}"
mkdir -p "$logs" crashes/inputs crashes/corpus
tmp="$(mktemp -d)"
interrupted=0
trap 'interrupted=1' INT TERM
trap 'rm -rf "$tmp"' EXIT
started=$SECONDS

# ── targets ──────────────────────────────────────────────────────────────────────────────────
# `cargo bolero list` builds every test binary first, which is the long silent part; its cargo
# output goes to a log whose last line is shown live. `--workspace`: without it only the default
# member is listed.
cargo bolero list --workspace >"$tmp/targets.jsonl" 2>"$logs/_list.log" &
list_pid=$!
while kill -0 "$list_pid" 2>/dev/null; do
  if ((interrupted)); then
    kill -INT "$list_pid" 2>/dev/null
    break
  fi
  last="$(tail -c 4096 "$logs/_list.log" | tr '\r' '\n' | grep -a -E '^ *(Compiling|Finished|Executable|Building)' | tail -1)"
  block_draw "${night_bold}listing fuzz targets${night_reset} $(fmt_dur $((SECONDS - started)))  ${last# *}"
  sleep 1
done
wait "$list_pid"
list_rc=$?
block_clear
if ((interrupted)); then
  echo "interrupted while listing targets" >&2
  exit 130
fi
if ((list_rc != 0)); then
  say "${night_red}cargo bolero list failed (exit $list_rc):${night_reset}"
  tail -n 30 "$logs/_list.log" >&2
  echo "full log: $logs/_list.log" >&2
  exit 1
fi

# Same filter forms as nextest: test(/regex/), test(=exact), test(substring); a bare string is a
# regex.
matches() {
  local name="$1"
  case "$filter" in
    '') return 0 ;;
    test\(/*/\)) [[ "$name" =~ ${filter:6:-2} ]] ;;
    test\(=*\)) [[ "$name" == "${filter:6:-1}" ]] ;;
    test\(*\)) [[ "$name" == *"${filter:5:-1}"* ]] ;;
    *) [[ "$name" =~ $filter ]] ;;
  esac
}

pkgs=() tests=()
while IFS=$'\t' read -r pkg test; do
  matches "$test" || continue
  pkgs+=("$pkg")
  tests+=("$test")
done < <(sed -n 's/.*"package":"\([^"]*\)".*"test":"\([^"]*\)".*/\1\t\2/p' "$tmp/targets.jsonl" | sort -u)
n="${#tests[@]}"
if ((n == 0)); then
  echo "no fuzz targets match '$filter' (cargo bolero list: $logs/_list.log)" >&2
  exit 1
fi

# fuzzing time per target = total budget × threads / number of targets
per=$(((total_budget * threads) / n))
((per < 1)) && per=1

safe=() result=() headline=() fuzzed=() execs=() cov=() fuzz_from=() launched_at=()
for i in "${!tests[@]}"; do
  s="${pkgs[i]}__${tests[i]}"
  safe[i]="${s//[^A-Za-z0-9_]/_}"
  result[i]="NOT RUN"
done

# ── per-target helpers ───────────────────────────────────────────────────────────────────────
# Last libFuzzer status line ("#123456 pulse cov: 812 ft: … corp: 40/2Kb exec/s: 9100 …").
fuzz_status() {
  tail -c 8192 "$1" 2>/dev/null | grep -a -E '^#[0-9]+' | tail -1
}

# Fills execs / cov of target $1 from its log.
read_stats() {
  local i="$1" line
  line="$(fuzz_status "$logs/${safe[i]}.log")"
  [[ "$line" =~ ^#([0-9]+) ]] && execs[i]="${BASH_REMATCH[1]}"
  [[ "$line" =~ cov:\ ([0-9]+) ]] && cov[i]="${BASH_REMATCH[1]}"
}

# The failure kind, from the exit code and what the log says.
classify() {
  local rc="$1" log="$2"
  if grep -q -a 'Test Failure' "$log"; then
    echo FAIL
  elif grep -q -a 'libFuzzer: timeout' "$log"; then
    echo TIMEOUT
  elif grep -q -a -E 'out-of-memory|libFuzzer: out-of-memory' "$log"; then
    echo OOM
  elif grep -q -a 'LeakSanitizer' "$log"; then
    echo LEAK
  elif grep -q -a 'AddressSanitizer' "$log"; then
    echo ASAN
  elif ((interrupted)); then
    echo INTERRUPTED
  elif ((rc == 0)); then
    echo PASS
  elif ! grep -q -a -E '^#[0-9]+' "$log" && grep -q -a -E '^error(\[E[0-9]+\])?:' "$log"; then
    echo BUILD
  else
    echo ERROR
  fi
}

# One line saying why a target failed.
failure_headline() {
  local kind="$1" log="$2"
  case "$kind" in
    FAIL)
      sed -n '/^Error: *$/,/^=====/p' "$log" | sed '1d' | grep -a -v -E '^(note:|=+$|$)' | head -2 | paste -sd ' '
      ;;
    TIMEOUT) grep -a -m1 'libFuzzer: timeout' "$log" ;;
    BUILD) grep -a -m1 -E '^error' "$log" ;;
    *) grep -a -m1 -E 'SUMMARY|ERROR|error' "$log" ;;
  esac
}

declare -A running=() # pid → target index

launch() {
  local i="$1"
  local log="$logs/${safe[i]}.log"
  # setsid: its own process group, so an interrupt stops the whole libFuzzer tree cleanly.
  setsid cargo bolero test -p "${pkgs[i]}" "${tests[i]}" \
    -T "${per}s" --timeout 30s \
    --corpus-dir "crashes/corpus/${safe[i]}" \
    --crashes-dir "crashes/inputs/${safe[i]}" \
    >"$log" 2>&1 &
  running[$!]="$i"
  fuzz_from[i]=""
}

finished=0 failures=0
finish() {
  local i="$1" rc="$2"
  local log="$logs/${safe[i]}.log" now=$SECONDS
  read_stats "$i"
  if [[ -n "${fuzz_from[i]}" ]]; then
    fuzzed[i]=$((now - fuzz_from[i]))
  else
    fuzzed[i]=0
  fi
  result[i]="$(classify "$rc" "$log")"
  finished=$((finished + 1))
  local where="[$((i + 1))/$n] ${pkgs[i]} ${tests[i]}"
  if [[ "${result[i]}" == PASS ]]; then
    say "${night_green}✓ PASS${night_reset} ${night_dim}${where}  $(fmt_dur "${fuzzed[i]}") · cov ${cov[i]:-?} · $(fmt_count "${execs[i]:-0}") execs${night_reset}"
  elif [[ "${result[i]}" == INTERRUPTED ]]; then
    say "${night_yellow}■ STOP${night_reset} $where"
  else
    failures=$((failures + 1))
    headline[i]="$(failure_headline "${result[i]}" "$log")"
    say "${night_red}✗ ${result[i]}${night_reset} ${night_bold}${where}${night_reset}"
    say "    ${headline[i]:-see $log}"
  fi
}

draw_status() {
  local now=$SECONDS lines=() left=0 partial=0 i pid
  for pid in "${!running[@]}"; do
    i="${running[$pid]}"
    local log="$logs/${safe[i]}.log" line
    line="$(fuzz_status "$log")"
    lines+=("▸ ${pkgs[i]}  ${tests[i]}")
    if [[ -n "$line" ]]; then
      [[ -z "${fuzz_from[i]}" ]] && fuzz_from[i]=$now
      local spent=$((now - fuzz_from[i]))
      ((spent > per)) && spent=$per
      partial=$((partial + spent))
      left=$((left + per - spent))
      read_stats "$i"
      local corp="" rate=""
      [[ "$line" =~ corp:\ ([0-9]+)/ ]] && corp="${BASH_REMATCH[1]}"
      [[ "$line" =~ exec/s:\ ([0-9]+) ]] && rate="${BASH_REMATCH[1]}"
      lines+=("    fuzzing $(bar "$spent" "$per" 20) $(fmt_dur "$spent")/$(fmt_dur "$per") · cov ${cov[i]:-?} · corp ${corp:-?} · $(fmt_count "${rate:-0}") exec/s · $(fmt_count "${execs[i]:-0}") execs")
    else
      local build
      build="$(tail -c 4096 "$log" | tr '\r' '\n' | grep -a -E '^ *(Compiling|Finished|Running|Executable|Building)' | tail -1)"
      left=$((left + per))
      lines+=("    building $(fmt_dur $((now - ${launched_at[i]:-now})))  ${build# *}")
    fi
  done
  left=$((left + (n - next) * per))
  left=$((left / threads))
  # Overall progress in fuzzing seconds: finished targets count whole.
  local done_s=$((finished * per + partial)) all_s=$((n * per))
  local fail_note="${night_green}0 failed${night_reset}"
  ((failures > 0)) && fail_note="${night_red}${failures} failed${night_reset}"
  block_draw "" \
    "${night_bold}fuzz${night_reset} $(bar "$done_s" "$all_s" 30) ${finished}/${n} targets · ${fail_note} · elapsed $(fmt_dur $((now - started))) · fuzzing left ~$(fmt_dur "$left") + builds" \
    "${lines[@]}"
}

# ── run ──────────────────────────────────────────────────────────────────────────────────────
say "${night_bold}fuzz campaign ${ts}${night_reset}: $n targets, budget $dur_arg, $threads thread(s), $(fmt_dur "$per") each"
say "${night_dim}logs: $logs/   report at the end: crashes/LATEST.md   Ctrl-C stops and still writes it${night_reset}"
next=0
while :; do
  for pid in "${!running[@]}"; do
    kill -0 "$pid" 2>/dev/null && continue
    wait "$pid"
    rc=$?
    i="${running[$pid]}"
    unset 'running[$pid]'
    finish "$i" "$rc"
  done
  ((interrupted)) && break
  while ((next < n && ${#running[@]} < threads)); do
    launched_at[next]=$SECONDS
    launch "$next"
    next=$((next + 1))
  done
  ((finished == n)) && break
  draw_status
  sleep 1
done

if ((interrupted)); then
  for pid in "${!running[@]}"; do
    kill -INT -- "-$pid" 2>/dev/null
  done
  for pid in "${!running[@]}"; do
    wait "$pid"
    rc=$?
    finish "${running[$pid]}" "$rc"
  done
fi
block_clear

# ── report ───────────────────────────────────────────────────────────────────────────────────
# Where a target's test function lives: the `__fuzz__` replay directory sits next to that file.
test_file() {
  local pkg="$1" test="$2" fn="${2##*::}" module dir hits
  module="${test%::*}"
  module="${module##*::}"
  dir="crates/${pkg#modhub-}/tests"
  mapfile -t hits < <(grep -rlE "fn ${fn}\(" "$dir" 2>/dev/null)
  local hit
  for hit in "${hits[@]}"; do
    [[ "$(basename "$hit" .rs)" == "$module" ]] && echo "$hit" && return
  done
  [[ ${#hits[@]} -gt 0 ]] && echo "${hits[0]}"
}

# bolero replays every file in `<test file dir>/__fuzz__/<name>/crashes/` on each `cargo t`.
# <name>: the module path with `::` → `__`, a `test_`/`fuzz_` prefix or `_test`/`_fuzz` suffix
# of the function dropped (bolero-engine TargetLocation::fuzz_dir).
fuzz_dir_name() {
  local fn="${1##*::}" prefix=""
  [[ "$1" == *::* ]] && prefix="${1%::*}::"
  fn="${fn#test_}" fn="${fn#fuzz_}" fn="${fn%_test}" fn="${fn%_fuzz}"
  local path="${prefix}${fn}"
  echo "${path//::/__}"
}

# The useful part of a log: bolero's shrunk input + error, or the sanitizer / libFuzzer summary
# without the raw stack addresses. bolero closes its block with exactly 62 `=`; a sanitizer
# report inside the error (a child process) opens with 65, so the length matters. Sanitizer
# verdicts are added at the end: the block is cut at 80 lines and may not reach them.
failure_excerpt() {
  local log="$1"
  if grep -q -a 'Test Failure' "$log"; then
    sed -n -E '/=== Test Failure ===/,/^={62}$/p' "$log" | head -n 80
    grep -a -E '(ERROR|SUMMARY): (Leak|Address)Sanitizer' "$log" | head -n 4
  else
    grep -a -v -E '^ *#[0-9]+ 0x|^INFO:|^#[0-9]+' "$log" | tail -n 40
  fi
}

report="crashes/REPORT_${ts}.md"
sha="$(git rev-parse --short HEAD)"
branch="$(git branch --show-current)"
dirty=""
[[ -n "$(git status --porcelain)" ]] && dirty=" (working tree had uncommitted changes)"
{
  echo "# Fuzz campaign $ts"
  echo
  echo "- git: \`$sha\` on \`$branch\`$dirty"
  echo "- budget: \`$dur_arg\` fuzzing, $threads thread(s), $(fmt_dur "$per") per target; wall time $(fmt_dur $((SECONDS - started)))"
  [[ -n "$filter" ]] && echo "- filter: \`$filter\`"
  ((interrupted)) && echo "- **interrupted** after starting $next of $n targets"
  echo "- targets: $n, finished: $finished, **failures: $failures**"
  echo
  if ((failures > 0)); then
    cat <<'EOF'
## How to work this report (agent)

Each failure below is a real bug until proven otherwise. For every one, in its own `fix/` branch:

1. **Red.** Copy the saved inputs into the target's `__fuzz__/<name>/crashes/` directory (the
   commands are under each failure) and run the nextest command: it must fail with the same
   error. bolero replays every file there on each `cargo t`, so the input is now a regression
   test. The files are raw libFuzzer bytes; `Input:` in the excerpt is the shrunk scenario.
2. **Fix the cause** in `src/`, not the symptom. If the test itself is wrong (an invariant that
   does not hold, a generator outside the domain), fix the test and say why in the commit.
3. **Green.** The nextest command passes; then
   `tools/fuzz_night.sh 5m 'test(=<target>)'` passes on the same target; then `tools/gate quick`.
4. Commit the fix together with the `__fuzz__/…/crashes/` input (it stays as the regression).

Kinds: FAIL = panic or failed assertion (bolero shrank the input); TIMEOUT = one input ran over
30 s (unbounded loop or blow-up); OOM = memory blow-up; LEAK / ASAN = sanitizer finding;
BUILD = the target did not compile under the fuzz profile; ERROR = anything else, read the log.

## Failures

EOF
    k=0
    for i in "${!tests[@]}"; do
      case "${result[i]}" in PASS | "NOT RUN" | INTERRUPTED) continue ;; esac
      k=$((k + 1))
      log="$logs/${safe[i]}.log"
      file="$(test_file "${pkgs[i]}" "${tests[i]}")"
      echo "### $k. ${result[i]} · \`${pkgs[i]}\` · \`${tests[i]}\`"
      echo
      echo "- ${headline[i]:-see the log}"
      echo "- test: \`${file:-not found, search crates/${pkgs[i]#modhub-}/tests for fn ${tests[i]##*::}}\`"
      echo "- log: \`$log\`"
      inputs=()
      [[ -d "crashes/inputs/${safe[i]}" ]] && mapfile -t inputs < <(find "crashes/inputs/${safe[i]}" -type f | sort)
      if ((${#inputs[@]} > 0)); then
        echo "- inputs: $(printf "\`%s\` " "${inputs[@]}")"
      fi
      echo
      echo '```text'
      failure_excerpt "$log"
      echo '```'
      echo
      if ((${#inputs[@]} > 0)) && [[ -n "$file" ]]; then
        replay="$(dirname "$file")/__fuzz__/$(fuzz_dir_name "${tests[i]}")/crashes"
        echo "Reproduce:"
        echo
        echo '```sh'
        echo "mkdir -p '$replay'"
        echo "cp crashes/inputs/${safe[i]}/* '$replay/'"
        echo "cargo nextest run -p ${pkgs[i]} -E 'test(=${tests[i]})'"
        echo '```'
        echo
      fi
    done
  else
    echo "## Result"
    echo
    echo "No failures. Nothing to fix."
    echo
  fi
  echo "## All targets"
  echo
  echo "| # | package | target | result | fuzzed | execs | cov |"
  echo "|--:|---------|--------|--------|-------:|------:|----:|"
  for i in "${!tests[@]}"; do
    r="${result[i]}"
    case "$r" in PASS | "NOT RUN" | INTERRUPTED) ;; *) r="**$r**" ;; esac
    echo "| $((i + 1)) | \`${pkgs[i]}\` | \`${tests[i]}\` | $r | $(fmt_dur "${fuzzed[i]:-0}") | $(fmt_count "${execs[i]:-0}") | ${cov[i]:--} |"
  done
} >"$report"
ln -sfn "REPORT_${ts}.md" crashes/LATEST.md

if ((failures > 0)); then
  say "${night_red}${night_bold}done: $finished/$n targets, $failures failure(s)${night_reset} in $(fmt_dur $((SECONDS - started)))"
  say "report: $report  (crashes/LATEST.md)  → hand it to the agent: \"fix the fuzz failures in crashes/LATEST.md\""
else
  say "${night_green}${night_bold}done: $finished/$n targets, no failures${night_reset} in $(fmt_dur $((SECONDS - started)))"
  say "report: $report  (crashes/LATEST.md)"
fi
((interrupted)) && exit 130
((failures > 0)) && exit 1
exit 0
