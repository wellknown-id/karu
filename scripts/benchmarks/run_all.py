#!/usr/bin/env python3

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


CORE_SUITE = "core"
ADVANCED_SUITE = "advanced"
WASM_NODE_SUITE = "wasm-node"
ALL_SUITES = [CORE_SUITE, ADVANCED_SUITE, WASM_NODE_SUITE]

CI_CRITERION_ARGS = [
    "--noplot",
    "--sample-size",
    "20",
    "--warm-up-time",
    "1",
    "--measurement-time",
    "1",
]
LOCAL_CRITERION_ARGS = ["--noplot"]


def run(command, cwd, env=None, capture_output=False):
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)

    print(f"+ ({cwd}) {' '.join(command)}", flush=True)
    return subprocess.run(
        command,
        cwd=cwd,
        env=merged_env,
        check=True,
        text=True,
        capture_output=capture_output,
    )


def ensure_clean_dir(path: Path):
    if path.exists():
        shutil.rmtree(path)


def ensure_parent(path: Path):
    path.parent.mkdir(parents=True, exist_ok=True)


def git_sha(repo: Path) -> str:
    result = run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo,
        capture_output=True,
    )
    return result.stdout.strip()


def criterion_metrics(criterion_dir: Path, suite: str):
    metrics = []
    for estimates_path in sorted(criterion_dir.glob("**/new/estimates.json")):
        relative = estimates_path.relative_to(criterion_dir)
        benchmark_name = "/".join(relative.parts[:-2])
        if not benchmark_name:
            continue

        with estimates_path.open() as handle:
            estimates = json.load(handle)

        metrics.append(
            {
                "name": f"{suite}/{benchmark_name}",
                "suite": suite,
                "kind": "criterion",
                "unit": "ns",
                "lower_is_better": True,
                "value": estimates["mean"]["point_estimate"],
                "source": str(estimates_path),
            }
        )

    if not metrics:
        raise RuntimeError(f"No Criterion results were found in {criterion_dir}")

    return metrics


def patch_advanced_harness_for_ffi(repo: Path):
    cargo_toml = (
        repo
        / "crates"
        / "karu"
        / "benches"
        / "advanced-benchmark"
        / "karu-wasm-harness"
        / "Cargo.toml"
    )
    if not cargo_toml.exists():
        return False

    contents = cargo_toml.read_text()
    updated = contents

    exact = 'karu = { path = "../../..", default-features = false, features = ["cedar"] }'
    if exact in updated:
        updated = updated.replace(
            exact,
            'karu = { path = "../../..", default-features = false, features = ["ffi", "cedar"] }',
        )
    elif "features = [" in updated and '"ffi"' not in updated:
        updated = re.sub(
            r'features = \[(.*?)\]',
            lambda match: (
                f'features = [{match.group(1)}, "ffi"]'
                if match.group(1).strip()
                else 'features = ["ffi"]'
            ),
            updated,
            count=1,
        )

    if updated != contents:
        cargo_toml.write_text(updated)
        return True

    return False


def build_advanced_wasm_artifacts(repo: Path):
    advanced_dir = repo / "crates" / "karu" / "benches" / "advanced-benchmark"
    wasm_dir = advanced_dir / "wasm"
    karu_harness = advanced_dir / "karu-wasm-harness"
    cedar_harness = advanced_dir / "cedar-wasm-harness"

    patch_advanced_harness_for_ffi(repo)
    wasm_dir.mkdir(parents=True, exist_ok=True)

    run(["rustup", "target", "add", "wasm32-wasip1"], cwd=repo)

    run(
        ["cargo", "build", "--release", "--target", "wasm32-wasip1"],
        cwd=karu_harness,
    )
    shutil.copy2(
        karu_harness
        / "target"
        / "wasm32-wasip1"
        / "release"
        / "karu_wasm_harness.wasm",
        wasm_dir / "karu.wasm",
    )

    run(
        ["cargo", "build", "--release", "--target", "wasm32-wasip1"],
        cwd=cedar_harness,
    )
    shutil.copy2(
        cedar_harness
        / "target"
        / "wasm32-wasip1"
        / "release"
        / "cedar_wasm_harness.wasm",
        wasm_dir / "cedar.wasm",
    )


def run_core(repo: Path, criterion_args):
    ensure_clean_dir(repo / "target" / "criterion")
    run(
        [
            "cargo",
            "bench",
            "-p",
            "karu",
            "--features",
            "dev",
            "--bench",
            "evaluation",
            "--bench",
            "parser_compare",
            "--",
            *criterion_args,
        ],
        cwd=repo,
    )
    return criterion_metrics(repo / "target" / "criterion", CORE_SUITE)


def run_advanced(repo: Path, criterion_args):
    advanced_dir = repo / "crates" / "karu" / "benches" / "advanced-benchmark"
    ensure_clean_dir(advanced_dir / "target" / "criterion")
    ensure_clean_dir(advanced_dir / "data")
    ensure_clean_dir(advanced_dir / "wasm")

    build_advanced_wasm_artifacts(repo)
    run(["cargo", "run", "--release", "--bin", "seed"], cwd=advanced_dir)
    run(
        [
            "cargo",
            "bench",
            "--bench",
            "compare",
            "--bench",
            "scale",
            "--",
            *criterion_args,
        ],
        cwd=advanced_dir,
    )
    return criterion_metrics(advanced_dir / "target" / "criterion", ADVANCED_SUITE)


def parse_wasm_output(stdout: str, suite: str, command_name: str):
    metrics = []
    pattern = re.compile(r"^(?P<label>.+?):\s+(?P<value>[0-9]+(?:\.[0-9]+)?)\s+μs/op$", re.MULTILINE)
    for match in pattern.finditer(stdout):
        label = match.group("label").strip()
        value_us = float(match.group("value"))
        metric_name = re.sub(r"[^a-z0-9]+", "-", label.lower()).strip("-")
        metrics.append(
            {
                "name": f"{suite}/{command_name}/{metric_name}",
                "suite": suite,
                "kind": "node",
                "unit": "ns",
                "display_unit": "μs/op",
                "lower_is_better": True,
                "value": value_us * 1000.0,
                "source": label,
            }
        )
    if not metrics:
        raise RuntimeError(f"No Node benchmark metrics were parsed from {command_name}")
    return metrics


def run_wasm_node(repo: Path):
    bench_dir = repo / "crates" / "karu" / "benches" / "wasm_bench"
    ensure_clean_dir(bench_dir / "pkg")

    run(["npm", "ci"], cwd=bench_dir)
    run(
        [
            "wasm-pack",
            "build",
            ".",
            "--target",
            "nodejs",
            "--no-default-features",
            "--features",
            "wasm,cedar",
            "--out-dir",
            "benches/wasm_bench/pkg",
        ],
        cwd=repo / "crates" / "karu",
    )

    bench_node = run(["node", "bench_node.mjs"], cwd=bench_dir, capture_output=True)
    bench_cedar = run(["node", "bench_cedar.mjs"], cwd=bench_dir, capture_output=True)

    metrics = []
    metrics.extend(parse_wasm_output(bench_node.stdout, WASM_NODE_SUITE, "bench-node"))
    metrics.extend(parse_wasm_output(bench_cedar.stdout, WASM_NODE_SUITE, "bench-cedar"))
    return metrics


def main():
    parser = argparse.ArgumentParser(description="Run Karu benchmark suites and emit JSON results.")
    parser.add_argument("--repo", required=True, help="Absolute path to the repository checkout.")
    parser.add_argument("--output", required=True, help="Where to write the JSON report.")
    parser.add_argument(
        "--suite",
        action="append",
        choices=ALL_SUITES,
        help="Benchmark suite(s) to run. Defaults to all suites.",
    )
    parser.add_argument(
        "--ci",
        action="store_true",
        help="Use shorter Criterion settings intended for CI comparisons.",
    )
    args = parser.parse_args()

    repo = Path(args.repo).resolve()
    output = Path(args.output).resolve()
    ensure_parent(output)

    if not repo.is_dir():
        raise SystemExit(f"Repository path does not exist: {repo}")

    suites = args.suite or ALL_SUITES
    criterion_args = CI_CRITERION_ARGS if args.ci else LOCAL_CRITERION_ARGS

    benchmarks = []
    for suite in suites:
        if suite == CORE_SUITE:
            benchmarks.extend(run_core(repo, criterion_args))
        elif suite == ADVANCED_SUITE:
            benchmarks.extend(run_advanced(repo, criterion_args))
        elif suite == WASM_NODE_SUITE:
            benchmarks.extend(run_wasm_node(repo))
        else:
            raise RuntimeError(f"Unsupported suite: {suite}")

    payload = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "repo": str(repo),
        "git_sha": git_sha(repo),
        "suites": suites,
        "benchmarks": sorted(benchmarks, key=lambda item: item["name"]),
    }

    output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        if error.stdout:
            sys.stdout.write(error.stdout)
        if error.stderr:
            sys.stderr.write(error.stderr)
        raise
