"""Verify full application matrix, sample accounting, payload digests and failure recovery."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import struct

import evidence


def matrix(args):
    carriers, clients = args["carriers"], args["concurrency"]
    assert 1 <= len(carriers) <= 4 and 1 <= len(clients) <= 8
    assert len(set(carriers)) == len(carriers) and len(set(clients)) == len(clients)
    assert all(1 <= value <= 16 for value in carriers)
    assert all(1 <= value <= 256 for value in clients)
    assert 1 <= args["rounds"] <= 512 and 1 <= args["fault_rounds"] <= 10
    assert len(carriers) * sum(clients) * args["rounds"] <= 1_000_000
    full = ({1, 4}.issubset(carriers) and {1, 16, 64, 256}.issubset(clients)
            and args["rounds"] >= 128 and args["fault_rounds"] >= 3)
    return "full" if full else "smoke"


def inventory(root, files):
    assert files, "empty evidence inventory"
    for name, checksum in files.items():
        file = (root / name).resolve()
        assert file.is_relative_to(root), "evidence path escaped"
        assert evidence.digest(file) == checksum, name


def percentiles(values):
    assert values and all(isinstance(value, int) and value > 0 for value in values)
    ordered = sorted(values)
    return {f"p{percent}": ordered[math.ceil(len(ordered) * percent / 100) - 1]
            for percent in (50, 95, 99)}


def stopped(state):
    assert state["event"] == "stopped" and state["shutdown"] == "Complete"
    for field in ("active", "pending", "runtime_active", "runtime_parked", "timers", "readiness",
                  "registered", "native", "panicked", "io_readers", "io_writers", "stack_cached"):
        assert state[field] == 0, f"unreclaimed {field}"
    assert state["accepted"] == state["closed"]
    assert state["spawned"] == state["completed"] + state["aborted"]


def load(report):
    clients, rounds = report["concurrency"], report["rounds"]
    assert report["schema"] == 1 and report["kind"] == "load"
    assert report["model"] == "closed-loop"
    assert report["input_bytes"] == 64 and report["output_bytes"] == 256
    assert len(report["clients"]) == clients
    assert {client["client"] for client in report["clients"]} == set(range(clients))
    samples = []
    for client in report["clients"]:
        assert len(client["latencies_ns"]) == rounds
        samples.extend(client["latencies_ns"])
        digest = hashlib.sha256()
        for sequence in range(client["client"] * rounds, (client["client"] + 1) * rounds):
            payload = [(sequence + offset * 17) % 256 for offset in range(64)]
            digest.update(struct.pack("!Q", sequence))
            digest.update(bytes(payload[offset % 64] ^ ((sequence + offset) % 256) for offset in range(256)))
        assert digest.hexdigest() == client["response_sha256"], "payload digest mismatch"
    assert report["completed"] == clients * rounds == len(samples)
    assert report["latency_ns"] == percentiles(samples)
    assert report["admission_ns"] == percentiles([client["admission_ns"] for client in report["clients"]])
    assert report["elapsed_ns"] > 0
    assert report["requests_per_second"] == len(samples) * 1e9 / report["elapsed_ns"]
    assert report["parked"]["active"] == report["parked"]["io_readers"] == clients
    assert report["parked_resource"]["rss_kib"] is not None
    drained = report["drained"]
    assert drained["requests"] == len(samples)
    assert drained["accepted"] == drained["closed"] == clients
    assert all(drained[key] == 0 for key in ("active", "pending", "rejected", "deadlines", "malformed", "disconnected"))
    stopped(report["stopped"])


def faults(report):
    assert set(report) == {"overload", "clients", "slow_reader", "shutdown"}
    overload = report["overload"]
    assert overload["full"]["pending"] == overload["full"]["active"] == 2
    assert overload["saturated"]["rejected"] == 16
    assert overload["recovered"]["requests"] == 1
    clients = report["clients"]
    assert clients["reading"]["io_readers"] >= 1
    assert clients["expired"]["deadlines"] == 1
    assert clients["settled"]["malformed"] == clients["settled"]["disconnected"] == 1
    assert clients["recovered"]["requests"] == 1
    reader = report["slow_reader"]
    assert reader["writing"]["io_writers"] >= 1
    assert reader["expired"]["deadlines"] == 1
    assert reader["recovered"]["requests"] > reader["expired"]["requests"]
    for case in (overload, clients, reader):
        state = case["recovered"]
        assert state["active"] == state["pending"] == state["panicked"] == 0
        assert state["accepted"] == state["closed"]
    shutdown = report["shutdown"]
    assert shutdown["before"]["io_readers"] >= 2 and shutdown["before"]["io_writers"] >= 2
    assert shutdown["before"]["pending"] == 2 and shutdown["before"]["active"] == 4
    assert 0 < shutdown["seconds"] < 2
    stopped(shutdown["after"])


def server_record(record, config, linux, recovery):
    assert record["config"] == config
    assert record["exit"] == 0 and 0 < record["shutdown_seconds"] < 10
    stopped(record["stopped"])
    assert record["stopped"]["peak_active"] <= config["workers"]
    assert record["stopped"]["peak_pending"] <= config["workers"] + config["queue"] + 1
    assert record["resources"] and any(s["rss_kib"] is not None for s in record["resources"])
    assert record["baseline"]["rss_kib"] is not None
    if recovery:
        assert record["recovered_resources"], "missing recovery resource observation"
    if linux:
        assert record["baseline"]["fds"] is not None
        assert any(s["fds"] is not None for s in record["resources"])
        assert all(s["fds"] == record["baseline"]["fds"] for s in record["recovered_resources"])


def verify(path, current=False):
    receipt = json.loads(path.read_text())
    assert receipt["schema"] == 1 and receipt["status"] == "passed"
    root = path.parent.resolve()
    assert receipt["coverage"] == matrix(receipt["arguments"])
    assert 0 < receipt["wall_seconds"] <= 1800
    inventory(root, receipt["files"])
    observed_load, observed_faults = set(), set()
    expected_servers = {}
    for case in receipt["cases"]:
        relative = case["path"]
        assert relative in receipt["files"]
        report = json.loads((root / relative).read_text())
        if case["kind"] == "load":
            load(report)
            key = (report["carriers"], report["concurrency"])
            assert key not in observed_load
            observed_load.add(key)
            assert report["rounds"] == receipt["arguments"]["rounds"]
            name = str(Path(relative).parent / "server.json")
            expected_servers[name] = (dict(carriers=key[0], workers=key[1], queue=key[1], timeout_ms=10000), True)
        else:
            assert case["kind"] == "faults"
            faults(report)
            key = (case["carriers"], case["round"])
            assert key not in observed_faults
            observed_faults.add(key)
            for name, workers, timeout in [("overload", 2, 5000), ("clients", 2, 250),
                                            ("slow-reader", 2, 1000), ("shutdown", 4, 5000)]:
                path = str(Path(relative).parent / name / "server.json")
                expected_servers[path] = (dict(carriers=key[0], workers=workers, queue=2, timeout_ms=timeout), name != "shutdown")
    args = receipt["arguments"]
    assert observed_load == {(c, n) for c in args["carriers"] for n in args["concurrency"]}
    assert observed_faults == {(c, n) for c in args["carriers"] for n in range(args["fault_rounds"])}
    servers = {name for name in receipt["files"] if name.endswith("server.json")}
    assert servers == set(expected_servers)
    for name in servers:
        server = root / name
        record = json.loads(server.read_text())
        assert str(server.relative_to(root).with_name("server.stderr")) in receipt["files"]
        assert evidence.digest(server.parent / "server.stderr") == record["stderr_sha256"]
        config, recovery = expected_servers[name]
        server_record(record, config, receipt["environment"]["system"] == "Linux", recovery)
    if current:
        assert receipt["environment"]["source_sha256"] == evidence.source_digest()
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--current-source", action="store_true")
    args = parser.parse_args()
    verify(args.receipt, args.current_source)
    print(f"verified application evidence: {args.receipt}")
