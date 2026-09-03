# HTTP benchmark

This harness compares a bounded vthread HTTP/1.1 server with
[`may_minihttp`](https://github.com/Xudong-Huang/may_minihttp)'s
`techempower_no_db` example. Both expose `/plaintext` and `/json`, accept persistent connections,
batch pipelined responses, use 16 header slots and 32 KiB input buffers, and emit equal-length
plaintext responses. The vthread implementation caps each connection's output buffer at 64 KiB
and rejects ambiguous request framing.

This is a focused localhost comparison, not an official TechEmpower submission. In particular,
it does not implement the database-backed Fortunes test for which `may-minihttp` led TechEmpower
Round 23.

## Build

Build the vthread server from the repository root:

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml --bin vthread-http
```

The comparison below pins `may_minihttp` to the measured upstream revision:

```sh
git clone https://github.com/Xudong-Huang/may_minihttp.git
git -C may_minihttp checkout 826a761933c53c7927d55e92443691065dcc5b7e
cargo build --release --manifest-path may_minihttp/Cargo.toml \
  --example techempower_no_db
```

Install `wrk` 4.2.0, then run five alternating samples. The defaults reserve CPUs 0-3 for the
server and CPUs 4-7 for four client threads, use 256 connections, warm each fresh process for
three seconds, and measure for ten seconds:

```sh
MAY_HTTP_BIN="$PWD/may_minihttp/target/release/examples/techempower_no_db" \
  WRK_BIN="$PWD/wrk/wrk" \
  benchmarks/http/compare.sh pipeline

MAY_HTTP_BIN="$PWD/may_minihttp/target/release/examples/techempower_no_db" \
  WRK_BIN="$PWD/wrk/wrk" \
  benchmarks/http/compare.sh keepalive
```

`pipeline` sends 16 requests at a time. Before each warm-up, the runner uses a separate low-load
script to validate response status and body. The timed interval omits that expensive Lua callback
so client-side validation does not cap server throughput.

Override `SERVER_CPUS`, `CLIENT_CPUS`, `WORKERS`, `CLIENT_THREADS`, `CONNECTIONS`,
`SERVER_CAPACITY`, `WARM_SECONDS`, `DURATION_SECONDS`, or odd `SAMPLES` values explicitly when
needed. For example, a one-carrier comparison uses `SERVER_CPUS=0 WORKERS=1`.

## Local result

On 2026-09-03, an eight-core AMD EPYC 9555P host running Linux 5.15, Rust 1.96.1, and `wrk` 4.2.0
produced these pipeline medians with the server and client on disjoint CPU sets. The headline used
five alternating fresh-process samples, three-second warm-ups, and ten-second measurements. The
single-carrier control used three alternating samples, two-second warm-ups, and seven-second
measurements.

| Server CPUs | Load | vthread req/s | May req/s | vthread delta |
|---:|---|---:|---:|---:|
| 1 | pipeline 16 | 807,622 | 827,488 | -2.40% |
| 4 | pipeline 16 | 2,513,241 | 2,321,228 | +8.27% |

In this control, vthread scaled 3.11 times from one to four carriers while May scaled 2.81 times.

These numbers describe this host and configuration, not a portable ranking. `may_minihttp` asks
May for 4 KiB coroutine stacks; vthread's validated minimum is 64 KiB. The vthread server uses a
500-stack cache per carrier, the same numeric setting as May's configured pool capacity, while
keeping total live connections explicitly bounded.
