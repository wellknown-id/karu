#!/usr/bin/env python3

import argparse
import json
from pathlib import Path


def format_duration_ns(value_ns: float) -> str:
    if value_ns >= 1_000_000_000:
        return f"{value_ns / 1_000_000_000:.2f} s"
    if value_ns >= 1_000_000:
        return f"{value_ns / 1_000_000:.2f} ms"
    if value_ns >= 1_000:
        return f"{value_ns / 1_000:.2f} μs"
    return f"{value_ns:.2f} ns"


def format_percent(value: float) -> str:
    sign = "+" if value > 0 else ""
    return f"{sign}{value:.2f}%"


def to_dict(payload):
    return {item["name"]: item for item in payload["benchmarks"]}


def markdown_report(base_payload, head_payload, threshold):
    base = to_dict(base_payload)
    head = to_dict(head_payload)

    common_names = sorted(set(base) & set(head))
    added = sorted(set(head) - set(base))
    removed = sorted(set(base) - set(head))

    regressions = []
    improvements = []
    stable = []

    for name in common_names:
        before = base[name]
        after = head[name]
        before_value = before["value"]
        after_value = after["value"]
        delta_percent = ((after_value - before_value) / before_value) * 100 if before_value else 0.0

        row = {
            "name": name,
            "suite": after["suite"],
            "base": before_value,
            "head": after_value,
            "delta_percent": delta_percent,
        }

        # Ignore regressions that are less than 500ns in absolute time to avoid micro-benchmark noise
        if delta_percent > threshold and (after_value - before_value) > 500.0:
            regressions.append(row)
        elif delta_percent < -threshold:
            improvements.append(row)
        else:
            stable.append(row)

    regressions.sort(key=lambda item: item["delta_percent"], reverse=True)
    improvements.sort(key=lambda item: item["delta_percent"])

    lines = []
    lines.append("# Benchmark regression check")
    lines.append("")
    lines.append(f"- Compared metrics: **{len(common_names)}**")
    lines.append(f"- Regression threshold: **{threshold:.2f}%**")
    lines.append(f"- Regressions: **{len(regressions)}**")
    lines.append(f"- Improvements: **{len(improvements)}**")
    lines.append(f"- Stable: **{len(stable)}**")
    lines.append(f"- Added metrics: **{len(added)}**")
    lines.append(f"- Removed metrics: **{len(removed)}**")
    lines.append("")

    if regressions:
        lines.append("## Regressions")
        lines.append("")
        lines.append("| Benchmark | Base | Head | Delta |")
        lines.append("| --- | ---: | ---: | ---: |")
        for row in regressions[:50]:
            lines.append(
                f"| `{row['name']}` | {format_duration_ns(row['base'])} | {format_duration_ns(row['head'])} | {format_percent(row['delta_percent'])} |"
            )
        lines.append("")
    else:
        lines.append("## Regressions")
        lines.append("")
        lines.append("None.")
        lines.append("")

    if improvements:
        lines.append("## Improvements")
        lines.append("")
        lines.append("| Benchmark | Base | Head | Delta |")
        lines.append("| --- | ---: | ---: | ---: |")
        for row in improvements[:50]:
            lines.append(
                f"| `{row['name']}` | {format_duration_ns(row['base'])} | {format_duration_ns(row['head'])} | {format_percent(row['delta_percent'])} |"
            )
        lines.append("")

    if added:
        lines.append("## Added metrics")
        lines.append("")
        for name in added:
            lines.append(f"- `{name}`")
        lines.append("")

    if removed:
        lines.append("## Removed metrics")
        lines.append("")
        for name in removed:
            lines.append(f"- `{name}`")
        lines.append("")

    return "\n".join(lines), regressions


def main():
    parser = argparse.ArgumentParser(description="Compare two benchmark result sets.")
    parser.add_argument("--base", required=True, help="Base benchmark JSON file.")
    parser.add_argument("--head", required=True, help="Head benchmark JSON file.")
    parser.add_argument("--output", required=True, help="Markdown report output path.")
    parser.add_argument(
        "--threshold",
        type=float,
        default=10.0,
        help="Percent slowdown at which a metric is treated as a regression.",
    )
    args = parser.parse_args()

    base_path = Path(args.base).resolve()
    head_path = Path(args.head).resolve()
    output_path = Path(args.output).resolve()

    base_payload = json.loads(base_path.read_text())
    head_payload = json.loads(head_path.read_text())

    markdown, regressions = markdown_report(base_payload, head_payload, args.threshold)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(markdown + "\n")

    if regressions:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
