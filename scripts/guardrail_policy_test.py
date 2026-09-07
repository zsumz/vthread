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


class ReleaseQualificationTests(unittest.TestCase):
    def setUp(self):
        self.workflow = (ROOT / ".github/workflows/ci.yml").read_text()

    def errors(self, workflow):
        errors = []
        POLICY.check_release_qualification(errors, workflow)
        return errors

    def test_current_release_qualification_is_required(self):
        self.assertEqual(self.errors(self.workflow), [])

    def test_missing_or_reduced_offered_load_is_rejected(self):
        for old, new in (("--offered-rates 2000", ""), ("--offered-count 256", ""),
                         ("--offered-count 256", "--offered-count 64")):
            with self.subTest(argument=old, replacement=new):
                self.assertTrue(self.errors(self.workflow.replace(old, new)))

    def test_missing_or_unverified_package_is_rejected(self):
        for old, new in (("Verify workspace packages", "Skip workspace packages"),
                         ("--locked --offline --workspace", "--locked --workspace"),
                         ("--exclude vthread-lab", "--exclude vthread-lab --no-verify")):
            with self.subTest(argument=old, replacement=new):
                self.assertTrue(self.errors(self.workflow.replace(old, new)))

    def test_packages_must_follow_application(self):
        application = self.workflow.index("      - name: Qualify application\n")
        package = self.workflow.index("      - name: Verify workspace packages\n")
        upload = self.workflow.index("      - name: Upload qualification evidence\n")
        reordered = (self.workflow[:application] + self.workflow[package:upload]
                     + self.workflow[application:package] + self.workflow[upload:])
        self.assertTrue(self.errors(reordered))

    def test_package_archives_must_be_uploaded(self):
        old = "            ${{ env.CARGO_TARGET_DIR }}/package/*.crate\n"
        self.assertTrue(self.errors(self.workflow.replace(old, "")))

    def test_missing_application_or_job_is_rejected(self):
        for name in ("Qualify application", "qualification:"):
            with self.subTest(name=name):
                self.assertTrue(self.errors(self.workflow.replace(name, "omitted")))


if __name__ == "__main__":
    unittest.main()
