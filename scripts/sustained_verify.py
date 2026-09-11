"""Fail closed on incomplete sustained runtime and resource evidence."""

import math
from statistics import median


SAMPLE_SECONDS = 10
MINIMUM_LIFETIMES = 1_000_000
HOSTS = {("Linux", "x86_64"), ("Darwin", "arm64")}


def validate_report(report, *, duration, carriers, tasks, wall_seconds, returncode):
    if not isinstance(report, dict):
        return ["report must be an object"]
    fields = {"schema", "carriers", "tasks", "iterations", "mutex_updates", "elapsed_ns",
              "spawned", "completed", "parks", "wakes", "stack_allocated", "stack_reused"}
    if any(type(report.get(name)) is not int or report[name] < 0 for name in fields):
        return ["missing or invalid nonnegative integer fields"]
    lifetimes = report["iterations"] * (tasks + 5)
    checks = [
        (returncode == 0, "unsuccessful process exit"),
        (report["schema"] == 1, "unexpected schema"),
        (report.get("workload") == "mixed-soak", "unexpected workload"),
        (report.get("connection_strategy") == "persistent-pair", "unexpected connections"),
        (report["carriers"] == carriers and report["tasks"] == tasks, "workload mismatch"),
        (report["iterations"] > 0, "no completed iterations"),
        (duration <= wall_seconds <= duration + 120, "supervised duration outside bounds"),
        (duration * 1e9 <= report["elapsed_ns"] <= wall_seconds * 1e9, "internal duration outside bounds"),
        (report["spawned"] == report["completed"] == lifetimes, "task accounting mismatch"),
        (report["parks"] == report["wakes"] > 0, "park/wake accounting mismatch"),
        (report["stack_allocated"] + report["stack_reused"] == lifetimes, "stack accounting mismatch"),
        (report["mutex_updates"] == report["iterations"] * tasks * 8, "mutex accounting mismatch"),
    ]
    return [message for passed, message in checks if not passed]


def validate_resources(samples, duration, interval=SAMPLE_SECONDS):
    errors, summary = [], {}
    if not isinstance(samples, list) or len(samples) < duration / interval * 0.9:
        return ["insufficient resource samples"], summary
    previous = -1
    for sample in samples:
        if not isinstance(sample, dict):
            return ["invalid resource sample"], summary
        seconds = sample.get("seconds")
        if (type(seconds) not in (int, float) or not math.isfinite(seconds)
                or not previous < seconds <= duration + 120
                or seconds - max(previous, 0) > interval * 3):
            return ["invalid sample timing or sampling gap"], summary
        for name in ("rss_kib", "fds"):
            if type(sample.get(name)) is not int or sample[name] <= 0:
                return [f"missing or invalid {name} measurement"], summary
        previous = seconds
    if samples[-1]["seconds"] < duration - interval * 3:
        errors.append("resource sampling ended early")
    baseline = [s for s in samples if duration / 6 <= s["seconds"] < duration / 3]
    tail = [s for s in samples if duration * 5 / 6 <= s["seconds"] <= duration]
    if len(baseline) < 2 or len(tail) < 2:
        return ["missing warm or final resource window"], summary
    warm = [s for s in samples if s["seconds"] >= duration / 6]
    for name, allowance in (("rss_kib", 32768), ("fds", 8)):
        start = median(s[name] for s in baseline)
        end = median(s[name] for s in tail)
        # A declared observational envelope, not a universal allocator or latency guarantee.
        limit = max(allowance, start * 0.2) if name == "rss_kib" else allowance
        peak = max(s[name] for s in warm)
        summary[name] = dict(baseline_median=start, final_median=end, warmed_peak=peak,
                             growth=end - start, allowed_growth=limit)
        if end - start > limit or peak - start > limit:
            errors.append(f"{name} exceeded warmed growth envelope")
    return errors, summary


def validate(receipt):
    contract, process = receipt["contract"], receipt["process"]
    errors = validate_report(receipt["report"], duration=contract["duration"],
                             carriers=contract["carriers"], tasks=contract["tasks"],
                             wall_seconds=process["wall_seconds"], returncode=process["returncode"])
    resources, summary = validate_resources(process["samples"], contract["duration"],
                                           contract["sample_seconds"])
    errors.extend(resources)
    if (receipt["host"]["system"], receipt["host"]["machine"]) not in HOSTS:
        errors.append("unsupported qualification host")
    if process["attempts"] != 1 or process["timed_out"]:
        errors.append("process restarted or timed out")
    if not receipt["source_unchanged"]:
        errors.append("source changed during qualification")
    if not contract["smoke"]:
        if contract["duration"] < 3600 or contract["carriers"] not in (1, 4) or contract["tasks"] != 4096:
            errors.append("incomplete production qualification profile")
        report = receipt["report"]
        completed = report.get("completed") if isinstance(report, dict) else None
        if type(completed) is not int or completed < MINIMUM_LIFETIMES:
            errors.append("minimum lifetime volume not reached")
    return errors, summary
