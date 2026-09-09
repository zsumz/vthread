"""Contract tests for the external sustained-run supervisor."""

import unittest

import supervise_sustained as supervisor


def report(iterations: int = 2_439) -> dict:
    tasks = 4_096
    lifetimes = iterations * (tasks + 5)
    return {
        "schema": 1,
        "workload": "mixed-soak",
        "connection_strategy": "persistent-pair",
        "carriers": 4,
        "tasks": tasks,
        "iterations": iterations,
        "mutex_updates": iterations * tasks * 8,
        "elapsed_ns": 3_600_000_000_000,
        "spawned": lifetimes,
        "completed": lifetimes,
        "parks": 27_000_000,
        "wakes": 27_000_000,
        "stack_allocated": 4_096,
        "stack_reused": lifetimes - 4_096,
    }


def validate(candidate: dict) -> list[str]:
    return supervisor.validate_report(
        candidate,
        duration=3_600,
        carriers=4,
        tasks=4_096,
        wall_seconds=3_600.5,
        returncode=0,
        system="Linux",
        machine="x86_64",
    )


class SupervisorContractTest(unittest.TestCase):
    def test_exact_threshold_candidate_passes(self):
        self.assertEqual(validate(report()), [])

    def test_below_threshold_candidate_fails(self):
        self.assertIn("lifetime threshold not met", validate(report(2_438)))

    def test_accounting_mismatch_fails(self):
        candidate = report()
        candidate["wakes"] -= 1
        candidate["stack_reused"] -= 1
        self.assertEqual(
            validate(candidate),
            ["park/wake accounting mismatch", "stack acquisition accounting mismatch"],
        )

    def test_missing_field_fails_closed(self):
        candidate = report()
        del candidate["completed"]
        self.assertEqual(validate(candidate), ["missing report fields: completed"])

    def test_invalid_field_type_fails_closed(self):
        candidate = report()
        candidate["iterations"] = "2439"
        self.assertEqual(validate(candidate), ["invalid integer fields: iterations"])


if __name__ == "__main__":
    unittest.main()
