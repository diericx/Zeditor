#!/usr/bin/env python3
"""Analyze a Zeditor render profile JSON and produce visual charts.

Usage:
    python scripts/analyze_profile.py <profile.json> [--output-dir DIR]

Produces:
    - render_timeline.png   Per-frame timing breakdown (line graph)
    - render_stages.png     Time by pipeline stage (pie chart)
    - render_heatmap.png    Per-frame sub-stage heatmap
    - Summary stats printed to stdout
"""

import argparse
import json
import sys
from pathlib import Path

import matplotlib
matplotlib.use("Agg")  # Non-interactive backend — always works headlessly
import matplotlib.pyplot as plt
import matplotlib.ticker as ticker


def load_profile(path: str) -> dict:
    with open(path) as f:
        return json.load(f)


def print_summary(profile: dict) -> None:
    print("=" * 60)
    print("RENDER PROFILE SUMMARY")
    print("=" * 60)
    cfg = profile["config"]
    print(f"  Output:      {cfg['output_path']}")
    print(f"  Resolution:  {cfg['width']}x{cfg['height']} @ {cfg['fps']:.2f} fps")
    print(f"  Codec:       preset={cfg['preset']}  crf={cfg['crf']}")
    print()
    print(f"  Total frames:  {profile['total_frames']}")
    print(f"  Total time:    {profile['total_duration_secs']:.2f}s")
    if profile["total_frames"] > 0 and profile["total_duration_secs"] > 0:
        realtime = profile["total_frames"] / cfg["fps"]
        speed = realtime / profile["total_duration_secs"]
        print(f"  Speed:         {speed:.2f}x realtime")
    print()
    print("  Frame timing (ms):")
    print(f"    avg:    {profile['avg_frame_ms']:.2f}")
    print(f"    median: {profile['median_frame_ms']:.2f}")
    print(f"    p95:    {profile['p95_frame_ms']:.2f}")
    print(f"    max:    {profile['max_frame_ms']:.2f}  (frame #{profile['slowest_frame_index']})")
    print()
    stages = profile["stages"]
    total_stage = sum(stages.values())
    print("  Stage breakdown:")
    for name, ms in stages.items():
        pct = (ms / total_stage * 100) if total_stage > 0 else 0
        print(f"    {name:20s}  {ms:10.1f} ms  ({pct:5.1f}%)")
    print("=" * 60)


def plot_timeline(profile: dict, output_dir: Path) -> None:
    frames = profile["frames"]
    if not frames:
        print("No per-frame data to plot.")
        return

    indices = [f["frame_index"] for f in frames]
    totals = [f["total_ms"] for f in frames]
    decodes = [f["decode_ms"] for f in frames]
    encodes = [f["encode_ms"] for f in frames]
    effects = [f["effects_ms"] for f in frames]
    composites = [f["composite_ms"] for f in frames]
    color_converts = [f["color_convert_ms"] for f in frames]

    fig, ax = plt.subplots(figsize=(14, 6))
    ax.plot(indices, totals, label="Total", alpha=0.9, linewidth=0.8, color="#2196F3")
    ax.plot(indices, decodes, label="Decode", alpha=0.7, linewidth=0.8, color="#4CAF50")
    ax.plot(indices, encodes, label="Encode", alpha=0.7, linewidth=0.8, color="#FF9800")
    ax.plot(indices, effects, label="Effects", alpha=0.7, linewidth=0.8, color="#9C27B0")
    ax.plot(indices, composites, label="Composite", alpha=0.5, linewidth=0.8, color="#F44336")
    ax.plot(indices, color_converts, label="Color Convert", alpha=0.5, linewidth=0.8, color="#00BCD4")

    avg = profile["avg_frame_ms"]
    p95 = profile["p95_frame_ms"]
    ax.axhline(y=avg, color="gray", linestyle="--", alpha=0.6,
               label=f"Avg: {avg:.1f}ms")
    ax.axhline(y=p95, color="red", linestyle="--", alpha=0.4,
               label=f"P95: {p95:.1f}ms")

    ax.set_xlabel("Frame Index")
    ax.set_ylabel("Time (ms)")
    ax.set_title(
        f"Render Profile \u2014 {profile['total_frames']} frames "
        f"in {profile['total_duration_secs']:.1f}s"
    )
    ax.legend(loc="upper right", fontsize=8)
    ax.grid(True, alpha=0.2)
    fig.tight_layout()

    path = output_dir / "render_timeline.png"
    fig.savefig(path, dpi=150)
    plt.close(fig)
    print(f"  Saved: {path}")


def plot_stages(profile: dict, output_dir: Path) -> None:
    stages = profile["stages"]
    labels = []
    values = []
    colors = ["#2196F3", "#4CAF50", "#FF9800", "#9C27B0", "#F44336"]
    for (name, ms), color in zip(stages.items(), colors):
        if ms > 0:
            # Pretty-print the key name
            pretty = name.replace("_ms", "").replace("_", " ").title()
            labels.append(f"{pretty}\n{ms:.0f}ms")
            values.append(ms)

    if not values:
        return

    fig, ax = plt.subplots(figsize=(8, 6))
    wedges, texts, autotexts = ax.pie(
        values,
        labels=labels,
        autopct="%1.1f%%",
        colors=colors[: len(values)],
        startangle=90,
    )
    for t in autotexts:
        t.set_fontsize(9)
    ax.set_title("Time by Pipeline Stage")
    fig.tight_layout()

    path = output_dir / "render_stages.png"
    fig.savefig(path, dpi=150)
    plt.close(fig)
    print(f"  Saved: {path}")


def plot_heatmap(profile: dict, output_dir: Path) -> None:
    frames = profile["frames"]
    if not frames:
        return

    substages = ["find_clips_ms", "decode_ms", "effects_ms",
                 "composite_ms", "color_convert_ms", "encode_ms"]
    pretty_names = ["Find Clips", "Decode", "Effects",
                    "Composite", "Color Convert", "Encode"]

    data = []
    for stage in substages:
        data.append([f[stage] for f in frames])

    fig, ax = plt.subplots(figsize=(14, 4))
    im = ax.imshow(data, aspect="auto", cmap="YlOrRd", interpolation="nearest")
    ax.set_yticks(range(len(pretty_names)))
    ax.set_yticklabels(pretty_names, fontsize=9)
    ax.set_xlabel("Frame Index")
    ax.set_title("Per-Frame Sub-Stage Heatmap (ms)")

    # Thin out x-axis labels for readability
    n = len(frames)
    if n > 50:
        ax.xaxis.set_major_locator(ticker.MaxNLocator(nbins=20, integer=True))

    cbar = fig.colorbar(im, ax=ax, pad=0.02)
    cbar.set_label("ms")
    fig.tight_layout()

    path = output_dir / "render_heatmap.png"
    fig.savefig(path, dpi=150)
    plt.close(fig)
    print(f"  Saved: {path}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Analyze a Zeditor render profile JSON."
    )
    parser.add_argument("profile", help="Path to .profile.json file")
    parser.add_argument(
        "--output-dir", "-o",
        default=None,
        help="Directory for output PNGs (default: same dir as profile)",
    )
    args = parser.parse_args()

    profile_path = Path(args.profile)
    if not profile_path.exists():
        print(f"Error: {profile_path} not found", file=sys.stderr)
        sys.exit(1)

    output_dir = Path(args.output_dir) if args.output_dir else profile_path.parent
    output_dir.mkdir(parents=True, exist_ok=True)

    profile = load_profile(str(profile_path))

    print_summary(profile)
    print()
    print("Generating charts...")
    plot_timeline(profile, output_dir)
    plot_stages(profile, output_dir)
    plot_heatmap(profile, output_dir)
    print("Done.")


if __name__ == "__main__":
    main()
