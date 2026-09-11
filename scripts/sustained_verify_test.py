"""Negative controls prevent short, leaking or inconsistent runs from qualifying."""

import unittest

import sustained_verify as verify


def fixture():
    tasks, iterations = 4096, 300
    lifetimes = (tasks + 5) * iterations
    return dict(contract=dict(duration=3600, carriers=4, tasks=tasks, smoke=False, sample_seconds=10),
                host=dict(system="Linux", machine="x86_64"), source_unchanged=True,
                process=dict(attempts=1, timed_out=False, wall_seconds=3600.5, returncode=0,
                             samples=[dict(seconds=t + 0.1, rss_kib=100_000, fds=12)
                                      for t in range(0, 3600, 10)]),
                report=dict(schema=1, workload="mixed-soak", connection_strategy="persistent-pair",
                            carriers=4, tasks=tasks, iterations=iterations, mutex_updates=iterations*tasks*8,
                            elapsed_ns=3_600_100_000_000, spawned=lifetimes, completed=lifetimes,
                            parks=10*lifetimes, wakes=10*lifetimes,
                            stack_allocated=4100, stack_reused=lifetimes-4100))


class SustainedEvidenceTests(unittest.TestCase):
    def test_both_supported_hosts_qualify(self):
        for system, machine in verify.HOSTS:
            receipt = fixture()
            receipt["host"] = dict(system=system, machine=machine)
            self.assertEqual(verify.validate(receipt)[0], [])

    def test_each_accounting_failure_is_rejected(self):
        for field in ("spawned", "completed", "wakes", "stack_reused", "mutex_updates"):
            receipt = fixture()
            receipt["report"][field] -= 1
            with self.subTest(field=field):
                self.assertTrue(verify.validate(receipt)[0])

    def test_invalid_reports_fail_closed(self):
        for report in (None, [], {}, {"iterations": True}):
            receipt = fixture()
            receipt["report"] = report
            # Malformed reports must produce errors without hiding a saved receipt.
            self.assertTrue(verify.validate(receipt)[0])

    def test_short_run_is_rejected(self):
        for field in ("wall_seconds",):
            receipt = fixture()
            receipt["process"][field] = 60
            self.assertTrue(verify.validate(receipt)[0])
        receipt = fixture()
        receipt["report"]["elapsed_ns"] = 60_000_000_000
        self.assertTrue(verify.validate(receipt)[0])

    def test_crash_restart_timeout_or_source_change_is_rejected(self):
        for field, value in (("returncode", -9), ("attempts", 2), ("timed_out", True)):
            receipt = fixture()
            receipt["process"][field] = value
            self.assertTrue(verify.validate(receipt)[0])
        receipt = fixture()
        receipt["source_unchanged"] = False
        self.assertTrue(verify.validate(receipt)[0])

    def test_short_profile_cannot_claim_production_qualification(self):
        for field, value in (("duration", 30), ("tasks", 64), ("carriers", 2)):
            receipt = fixture()
            receipt["contract"][field] = value
            self.assertIn("incomplete production qualification profile", verify.validate(receipt)[0])

    def test_missing_measurements_and_gaps_are_rejected(self):
        for name in ("rss_kib", "fds", "seconds"):
            receipt = fixture()
            receipt["process"]["samples"][100][name] = None
            self.assertTrue(verify.validate(receipt)[0])
        for replacement in ([], fixture()["process"]["samples"][:200],
                            fixture()["process"]["samples"][::2]):
            receipt = fixture()
            receipt["process"]["samples"] = replacement
            self.assertTrue(verify.validate(receipt)[0])

    def test_memory_and_descriptor_growth_are_rejected(self):
        for name, delta in (("rss_kib", 40_000), ("fds", 9)):
            receipt = fixture()
            for sample in receipt["process"]["samples"][300:]:
                sample[name] += delta
            self.assertIn(f"{name} exceeded warmed growth envelope", verify.validate(receipt)[0])

    def test_warmup_is_excluded_but_late_spikes_are_not(self):
        receipt = fixture()
        receipt["process"]["samples"][0]["rss_kib"] = 500_000
        self.assertEqual(verify.validate(receipt)[0], [])
        receipt["process"]["samples"][200]["rss_kib"] = 500_000
        self.assertTrue(verify.validate(receipt)[0])


if __name__ == "__main__":
    unittest.main()
