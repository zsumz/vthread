"""Contract tests for the external sustained-run supervisor."""

import json
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
from unittest.mock import patch

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


def run_supervisor(stdout: str) -> tuple[int, dict]:
    class Process:
        pid = 42
        returncode = 0

        def poll(self):
            return self.returncode

        def communicate(self):
            return stdout, ""

    def fake_git(_candidate: Path, *arguments: str) -> str:
        values = {
            ("rev-parse", "HEAD"): "candidate",
            ("rev-parse", "HEAD^{tree}"): "tree",
            ("status", "--porcelain"): "",
        }
        return values[arguments]

    with TemporaryDirectory() as temporary:
        root = Path(temporary)
        arguments = SimpleNamespace(
            candidate=root,
            binary=root / "vthread-lab",
            output=root / "evidence",
            head="candidate",
            duration=3_600,
            carriers=4,
            tasks=4_096,
        )
        with (
            patch.object(supervisor, "parse_args", return_value=arguments),
            patch.object(supervisor, "git", side_effect=fake_git),
            patch.object(supervisor, "sha256", return_value="digest"),
            patch.object(supervisor.subprocess, "Popen", return_value=Process()),
        ):
            result = supervisor.main()
        receipt = json.loads((arguments.output / "receipt.json").read_text())
    return result, receipt


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

    def test_empty_report_fails_closed(self):
        result, receipt = run_supervisor("{}")
        self.assertEqual(result, 1)
        self.assertEqual(receipt["status"], "failed")
        self.assertTrue(receipt["errors"])

    def test_array_report_fails_closed(self):
        result, receipt = run_supervisor("[]")
        self.assertEqual(result, 1)
        self.assertEqual(receipt["status"], "failed")
        self.assertEqual(receipt["errors"], ["report must be a JSON object"])

    def test_null_report_fails_closed(self):
        result, receipt = run_supervisor("null")
        self.assertEqual(result, 1)
        self.assertEqual(receipt["status"], "failed")
        self.assertEqual(receipt["errors"], ["report must be a JSON object"])


if __name__ == "__main__":
    unittest.main()
