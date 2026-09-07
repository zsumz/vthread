"""Negative controls for mandatory default-engine qualification."""

import copy
import importlib.util
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("guardrail_policy", ROOT / "scripts/guardrail-policy.py")
POLICY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(POLICY)


class NativeQualificationTests(unittest.TestCase):
    def setUp(self):
        self.tasks = tomllib.loads((ROOT / "zcheck.toml").read_text())["tasks"]

    def errors(self, tasks):
        errors = []
        POLICY.check_native_qualification(errors, tasks)
        return errors

    def test_current_matrix_is_required(self):
        self.assertEqual(self.errors(self.tasks), [])

    def test_missing_native_gate_is_rejected(self):
        for name in ("test-native", "test-native-release"):
            with self.subTest(name=name):
                tasks = copy.deepcopy(self.tasks)
                del tasks[name]
                self.assertTrue(self.errors(tasks))

    def test_diagnostic_features_cannot_replace_default_tests(self):
        for name in ("test-native", "test-native-release"):
            with self.subTest(name=name):
                tasks = copy.deepcopy(self.tasks)
                tasks[name]["run"].append("--all-features")
                self.assertTrue(self.errors(tasks))

    def test_release_profile_cannot_be_omitted(self):
        self.tasks["test-native-release"]["run"].remove("--release")
        self.assertTrue(self.errors(self.tasks))

    def test_optional_or_unordered_tests_do_not_qualify(self):
        for task, required in (
            ("check", "test-native-release"),
            ("test-native-release", "test-native"),
            ("test-native", "test"),
        ):
            with self.subTest(task=task):
                tasks = copy.deepcopy(self.tasks)
                tasks[task]["needs"].remove(required)
                self.assertTrue(self.errors(tasks))


class HistoryPerformanceQualificationTests(unittest.TestCase):
    def setUp(self):
        self.tasks = tomllib.loads((ROOT / "zcheck.toml").read_text())["tasks"]

    def errors(self, tasks):
        errors = []
        POLICY.check_history_performance(errors, tasks)
        return errors

    def test_current_timing_guard_is_preserved(self):
        self.assertEqual(self.errors(self.tasks), [])

    def test_missing_performance_guard_is_rejected(self):
        del self.tasks["perf-cancellation-history"]
        self.assertTrue(self.errors(self.tasks))

    def test_unexecuted_or_unoptimized_timing_guard_is_rejected(self):
        for argument in ("--release", "--ignored", "--exact"):
            tasks = copy.deepcopy(self.tasks)
            tasks["perf-cancellation-history"]["run"].remove(argument)
            self.assertTrue(self.errors(tasks))


class BenchmarkQualificationTests(unittest.TestCase):
    def setUp(self):
        self.tasks = tomllib.loads((ROOT / "zcheck.toml").read_text())["tasks"]

    def errors(self, tasks):
        errors = []
        POLICY.check_benchmark_qualification(errors, tasks)
        return errors

    def test_current_harness_is_required(self):
        self.assertEqual(self.errors(self.tasks), [])

    def test_omitted_stage_is_rejected(self):
        for name in ("benchmark-format", "benchmark-clippy", "benchmark-test", "benchmark-test-features"):
            tasks = copy.deepcopy(self.tasks)
            del tasks[name]
            self.assertTrue(self.errors(tasks))

    def test_unchecked_workspace_cannot_substitute_for_the_harness(self):
        self.tasks["benchmark-test"]["run"] = ["cargo", "test", "--locked", "--workspace"]
        self.assertTrue(self.errors(self.tasks))

    def test_instrumentation_cannot_replace_default_harness_tests(self):
        self.tasks["benchmark-test"]["run"].append("--all-features")
        self.assertTrue(self.errors(self.tasks))

    def test_optional_or_unordered_harness_is_rejected(self):
        for name, prerequisite in (
            ("check", "benchmark-test-features"),
            ("benchmark-format", "application-smoke"),
            ("benchmark-clippy", "benchmark-format"),
            ("benchmark-test", "benchmark-clippy"),
            ("benchmark-test-features", "benchmark-test"),
        ):
            tasks = copy.deepcopy(self.tasks)
            tasks[name]["needs"].remove(prerequisite)
            self.assertTrue(self.errors(tasks))


if __name__ == "__main__":
    unittest.main()
