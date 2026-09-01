"""Mutation tests for application evidence, independent of a live runtime."""

import copy
import json
from pathlib import Path
import tempfile
import unittest

import application_verify as verify
import evidence

FIXTURES = Path(__file__).parent / "fixtures/application"


def fixture(name):
    return json.loads((FIXTURES / f"{name}.json").read_text())


class VerificationTests(unittest.TestCase):
    def test_payload_sample_count_and_latency_corruption_fail(self):
        valid = fixture("load")
        verify.load(valid)
        mutations = [
            lambda r: r["clients"][0].update(response_sha256="0" * 64),
            lambda r: r["clients"][0]["latencies_ns"].pop(),
            lambda r: r["clients"].clear(),
            lambda r: r.update(completed=0),
            lambda r: r["latency_ns"].update(p99=1),
            lambda r: r.update(requests_per_second=1),
            lambda r: r["drained"].update(deadlines=1),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index), self.assertRaises(AssertionError):
                damaged = copy.deepcopy(valid)
                mutate(damaged)
                verify.load(damaged)

    def test_nearest_rank_and_invalid_samples(self):
        self.assertEqual(verify.percentiles(list(range(1, 101))), dict(p50=50, p95=95, p99=99))
        for samples in ([], [0], [-1], [1.5]):
            with self.assertRaises(AssertionError):
                verify.percentiles(samples)

    def test_missing_failure_proof_and_shutdown_leaks_fail(self):
        valid = fixture("faults")
        verify.faults(valid)
        mutations = [
            lambda r: r["overload"]["saturated"].update(rejected=0),
            lambda r: r["clients"]["expired"].update(deadlines=0),
            lambda r: r["slow_reader"]["writing"].update(io_writers=0),
            lambda r: r["clients"]["recovered"].update(active=1),
            lambda r: r["shutdown"].update(seconds=3),
            lambda r: r["shutdown"]["before"].update(pending=0),
            lambda r: r["shutdown"]["after"].update(registered=1),
            lambda r: r["shutdown"]["after"].update(completed=0),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index), self.assertRaises(AssertionError):
                damaged = copy.deepcopy(valid)
                mutate(damaged)
                verify.faults(damaged)

    def test_full_matrix_cannot_be_empty_small_or_unbounded(self):
        valid = dict(carriers=[1, 4], concurrency=[1, 16, 64, 256], rounds=128, fault_rounds=3)
        self.assertEqual(verify.matrix(valid), "full")
        for change in (dict(carriers=[1]), dict(rounds=8), dict(fault_rounds=1)):
            self.assertEqual(verify.matrix(dict(valid, **change)), "smoke")
        for change in (dict(carriers=[]), dict(carriers=[1, 1]), dict(concurrency=[257]),
                       dict(concurrency=[]), dict(rounds=513), dict(fault_rounds=0)):
            with self.assertRaises(AssertionError):
                verify.matrix(dict(valid, **change))

    def test_server_identity_resource_loss_and_capacity_breaches_fail(self):
        valid = fixture("server")
        valid["baseline"]["fds"] = 8
        valid["resources"][0]["fds"] = 9
        valid["recovered_resources"][0]["fds"] = 8
        config = copy.deepcopy(valid["config"])
        verify.server_record(valid, config, linux=True, recovery=True)
        mutations = [
            lambda r: r["config"].update(carriers=4),
            lambda r: r["recovered_resources"][0].update(fds=9),
            lambda r: r["recovered_resources"].clear(),
            lambda r: r["resources"].clear(),
            lambda r: r["stopped"].update(peak_active=257),
            lambda r: r["stopped"].update(peak_pending=513),
            lambda r: r.update(exit=1),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index), self.assertRaises(AssertionError):
                damaged = copy.deepcopy(valid)
                mutate(damaged)
                verify.server_record(damaged, config, linux=True, recovery=True)

    def test_file_corruption_empty_inventory_and_escape_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            file = root / "evidence.json"
            file.write_text("{}")
            files = {file.name: evidence.digest(file)}
            verify.inventory(root, files)
            file.write_text("[]")
            for invalid in (files, {}, {"../outside": "0" * 64}):
                with self.assertRaises(AssertionError):
                    verify.inventory(root, invalid)


if __name__ == "__main__":
    unittest.main()
