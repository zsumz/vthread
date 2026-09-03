#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_dir=$(CDPATH= cd -- "$script_dir/../.." && pwd)

vthread_bin=${VTHREAD_HTTP_BIN:-$repository_dir/benchmarks/target/release/vthread-http}
may_bin=${MAY_HTTP_BIN:-}
wrk_bin=${WRK_BIN:-wrk}
server_cpus=${SERVER_CPUS:-0-3}
client_cpus=${CLIENT_CPUS:-4-7}
workers=${WORKERS:-4}
client_threads=${CLIENT_THREADS:-4}
connections=${CONNECTIONS:-256}
server_capacity=${SERVER_CAPACITY:-4096}
warm_seconds=${WARM_SECONDS:-3}
duration=${DURATION_SECONDS:-10}
samples=${SAMPLES:-5}
mode=${1:-pipeline}

fail() {
    printf 'error: %s\n' "$1" >&2
    exit 2
}

(( $# <= 1 )) || fail "usage: compare.sh [pipeline|keepalive]"
[[ $mode == pipeline || $mode == keepalive ]] || fail "mode must be pipeline or keepalive"
[[ $samples =~ ^[1-9][0-9]*$ ]] || fail "SAMPLES must be a positive odd integer"
((samples % 2 == 1)) || fail "SAMPLES must be a positive odd integer"
[[ -x $vthread_bin ]] || fail "build vthread-http first: $vthread_bin"
[[ -n $may_bin && -x $may_bin ]] || fail "set MAY_HTTP_BIN to techempower_no_db"
command -v "$wrk_bin" >/dev/null || fail "wrk is not executable: $wrk_bin"
command -v taskset >/dev/null || fail "taskset is required for CPU isolation"

run_dir=$(mktemp -d)
active_pid=
cleanup() {
    if [[ -n $active_pid ]]; then
        kill "$active_pid" 2>/dev/null || true
        wait "$active_pid" 2>/dev/null || true
    fi
    rm -r -- "$run_dir"
}
trap cleanup EXIT INT TERM

declare -a vthread_rates=()
declare -a may_rates=()

run_engine() {
    local engine=$1
    local sample=$2
    local port binary output rate
    if [[ $engine == vthread ]]; then
        port=8080
        binary=$vthread_bin
        taskset -c "$server_cpus" "$binary" "$workers" "$port" "$server_capacity" \
            >"$run_dir/$engine.out" 2>"$run_dir/$engine.err" &
    else
        port=8081
        binary=$may_bin
        taskset -c "$server_cpus" "$binary" \
            >"$run_dir/$engine.out" 2>"$run_dir/$engine.err" &
    fi
    active_pid=$!
    sleep 1
    kill -0 "$active_pid" 2>/dev/null || fail "$engine exited before the warm-up"

    local -a wrk_args=(
        taskset -c "$client_cpus" "$wrk_bin"
        -t"$client_threads" -c"$connections"
    )
    local verify_script=$script_dir/verify.lua
    if [[ $mode == pipeline ]]; then
        wrk_args+=(-s "$script_dir/pipeline.lua")
        verify_script=$script_dir/pipeline_verify.lua
    fi
    taskset -c "$client_cpus" "$wrk_bin" -t1 -c1 -d1s -s "$verify_script" \
        "http://127.0.0.1:$port/plaintext" >/dev/null
    "${wrk_args[@]}" -d"${warm_seconds}s" "http://127.0.0.1:$port/plaintext" >/dev/null
    output=$("${wrk_args[@]}" -d"${duration}s" "http://127.0.0.1:$port/plaintext")
    rate=$(awk '/^Requests\/sec:/ { print $2 }' <<<"$output")
    [[ -n $rate ]] || fail "wrk did not report throughput for $engine"

    kill "$active_pid"
    wait "$active_pid" 2>/dev/null || true
    active_pid=
    printf 'sample %d %-7s %s req/s\n' "$sample" "$engine" "$rate"
    if [[ $engine == vthread ]]; then
        vthread_rates+=("$rate")
    else
        may_rates+=("$rate")
    fi
}

for ((sample = 1; sample <= samples; sample++)); do
    if ((sample % 2 == 1)); then
        run_engine vthread "$sample"
        run_engine may "$sample"
    else
        run_engine may "$sample"
        run_engine vthread "$sample"
    fi
done

median() {
    local -a sorted
    mapfile -t sorted < <(printf '%s\n' "$@" | sort -n)
    printf '%s' "${sorted[${#sorted[@]} / 2]}"
}

vthread_median=$(median "${vthread_rates[@]}")
may_median=$(median "${may_rates[@]}")
delta=$(awk -v vthread="$vthread_median" -v may="$may_median" \
    'BEGIN { printf "%+.2f", (vthread / may - 1) * 100 }')
printf 'median  vthread %s req/s\n' "$vthread_median"
printf 'median  may     %s req/s\n' "$may_median"
printf 'delta   vthread %s%%\n' "$delta"
