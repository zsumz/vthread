"""Bounded closed-loop clients; preserve every latency and actual response digest."""

import concurrent.futures
import hashlib
import json
import threading
import time

from application_client import Service, exchange
from application_verify import percentiles
import evidence


def run(binary, out, carriers, concurrency, rounds):
    with Service(binary, out, carriers, concurrency, concurrency, 10_000) as server:
        go, ready = threading.Event(), threading.Barrier(concurrency + 1, timeout=15)

        def client(index):
            try:
                admitted = time.perf_counter_ns()
                with server.connect() as stream:
                    admission_ns = time.perf_counter_ns() - admitted
                    ready.wait()
                    assert go.wait(15), "load start timed out"
                    latencies, digest = [], hashlib.sha256()
                    deadline = time.monotonic() + 120
                    for sequence in range(index * rounds, (index + 1) * rounds):
                        assert time.monotonic() < deadline, "client workload exceeded two minutes"
                        elapsed, response = exchange(stream, sequence)
                        latencies.append(elapsed)
                        digest.update(response)
                    return dict(client=index, admission_ns=admission_ns,
                                latencies_ns=latencies, response_sha256=digest.hexdigest())
            except BaseException:
                ready.abort()
                go.set()
                raise

        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as clients:
            futures = [clients.submit(client, index) for index in range(concurrency)]
            try:
                ready.wait()
                parked = server.until(lambda state: state["active"] == concurrency
                                      and state["io_readers"] == concurrency)
                parked_resource = evidence.resources(server.process.pid, server.start)
                start = time.perf_counter_ns()
                go.set()
                values = [future.result(timeout=120) for future in futures]
                elapsed = time.perf_counter_ns() - start
            finally:
                go.set()
        drained = server.recovered()
        assert drained["requests"] == concurrency * rounds
        assert drained["rejected"] == drained["deadlines"] == drained["malformed"] == drained["disconnected"] == 0
        stopped = server.stop()
    latencies = [value for client in values for value in client["latencies_ns"]]
    report = dict(schema=1, kind="load", carriers=carriers, concurrency=concurrency, rounds=rounds,
                  model="closed-loop", input_bytes=64, output_bytes=256, clients=values,
                  completed=len(latencies), elapsed_ns=elapsed,
                  requests_per_second=len(latencies) * 1e9 / elapsed,
                  latency_ns=percentiles(latencies), admission_ns=percentiles([c["admission_ns"] for c in values]), parked=parked,
                  parked_resource=parked_resource, drained=drained, stopped=stopped)
    (out / "load.json").write_text(json.dumps(report, indent=2) + "\n")
    return report
