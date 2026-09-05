#!/usr/bin/env python3
"""Summarise an `hgt sweep` tree: hgt/scripts/plot.py hgt/results/demo

Reads every <dir>/<mechanisms>/seed_*.jsonl and summary.jsonl and writes summary.md
next to them. Draws figures too if matplotlib is installed; the numbers are the point,
so the tables are written either way.
"""
import glob
import json
import os
import statistics
import sys
from collections import defaultdict

try:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
except ImportError:  # figures are optional, tables are not
    plt = None


def load(exp_dir):
    frames, epochs, genes, summaries = [], [], [], []
    for path in sorted(glob.glob(os.path.join(exp_dir, "seed_*.jsonl"))):
        with open(path) as f:
            for line in f:
                r = json.loads(line)
                {"frame": frames, "epoch": epochs, "gene": genes, "summary": summaries}[
                    r["kind"]
                ].append(r)
    return frames, epochs, genes, summaries


def median(xs, default=float("nan")):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else default


def survival_table(experiments):
    rows = [
        "| transfer | seeds | survived | epochs survived | freq at shift | rescue ticks "
        "| lateral share |",
        "|---|---|---|---|---|---|---|",
    ]
    for name, (frames, epochs, _genes, summaries) in experiments.items():
        n = len(summaries)
        alive = sum(1 for s in summaries if s["extinct_at"] is None)
        eps = median([s["epochs_survived"] for s in summaries])
        # Rescue ticks for the shifts that were actually survived, epoch 1 onwards: how
        # long the population took to make the answering gene common after a shift.
        rescues = [e["rescue_ticks"] for e in epochs if e["epoch"] > 0 and e["survived"]]
        # How common the answering gene already was when the stressor arrived. This is
        # where transfer does its work: not in a rescue after the shift, but in having
        # spread the gene around before it.
        at_shift = median([e["start_freq"] for e in epochs if e["epoch"] > 0])
        lateral = median([f["lateral_share"] for f in frames]) if frames else float("nan")
        rows.append(
            f"| {name} | {n} | {alive} | {eps:.0f} | {at_shift:.2f} | {median(rescues):.0f} "
            f"| {lateral:.3f} |"
        )
    return "\n".join(rows)


def acquisition_table(experiments):
    rows = ["| transfer | birth | conjugation | transformation | transduction | incongruence |",
            "|---|---|---|---|---|---|"]
    for name, (frames, _epochs, _genes, _summaries) in experiments.items():
        if not frames:
            continue
        # The last frame of each seed carries the cumulative counts for that run.
        last = {}
        for f in frames:
            last[f["seed"]] = f
        acq = defaultdict(int)
        for f in last.values():
            for k, v in f["acquisitions"].items():
                acq[k] += v
        inc = median(
            [r["incongruence"] for f in last.values() for r in f["resistance"] if r["carriers"] > 0]
        )
        rows.append(
            f"| {name} | {acq['birth']} | {acq['conjugation']} | {acq['transformation']} "
            f"| {acq['transduction']} | {inc:.3f} |"
        )
    return "\n".join(rows)


def barrier_table(experiments):
    rows = [
        "| transfer | strain distance | attempts | redundant | accepted | rate |",
        "|---|---|---|---|---|---|",
    ]
    for name, (frames, _e, _g, _s) in experiments.items():
        last = {}
        for f in frames:
            last[f["seed"]] = f
        totals = defaultdict(lambda: [0, 0, 0])
        for f in last.values():
            for row in f["barrier"]:
                totals[row["distance"]][0] += row["attempts"]
                totals[row["distance"]][1] += row["accepted"]
                totals[row["distance"]][2] += row["redundant"]
        for d in sorted(totals):
            attempts, accepted, redundant = totals[d]
            # Redundant attempts — the recipient already had the gene — could never have
            # succeeded, so the barrier's rate is over the ones that could.
            live = attempts - redundant
            rate = accepted / live if live else float("nan")
            rows.append(
                f"| {name} | {d} | {attempts} | {redundant} | {accepted} | {rate:.3f} |"
            )
    return "\n".join(rows)


def figures(experiments, out_dir):
    if plt is None:
        return []
    written = []
    # Allele frequency of each resistance gene over time, for the lowest seed of each set.
    for name, (frames, _e, _g, _s) in experiments.items():
        if not frames:
            continue
        seed = min(f["seed"] for f in frames)
        series = defaultdict(lambda: ([], []))
        for f in sorted((f for f in frames if f["seed"] == seed), key=lambda f: f["tick"]):
            for row in f["resistance"]:
                if row["resists"] is None:
                    continue
                xs, ys = series[row["resists"]]
                xs.append(f["tick"])
                ys.append(row["freq"])
        fig, ax = plt.subplots(figsize=(8, 3.5))
        for kind in sorted(series):
            xs, ys = series[kind]
            ax.plot(xs, ys, label=f"stressor {kind}")
        ax.set_xlabel("tick")
        ax.set_ylabel("carrier fraction")
        ax.set_title(f"resistance gene frequency — hgt={name}, seed {seed}")
        ax.legend(fontsize="small")
        fig.tight_layout()
        path = os.path.join(out_dir, f"frequency_{name.replace(',', '_')}.png")
        fig.savefig(path, dpi=130)
        plt.close(fig)
        written.append(path)
    return written


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "hgt/results/demo"
    experiments = {}
    for entry in sorted(os.listdir(root)):
        exp_dir = os.path.join(root, entry)
        if os.path.isdir(exp_dir) and glob.glob(os.path.join(exp_dir, "seed_*.jsonl")):
            experiments[entry] = load(exp_dir)
    if not experiments:
        sys.exit(f"no sweep output under {root}")

    parts = [
        f"# hgt sweep: {root}",
        "",
        "## Does the population survive stressors it was not born ready for?",
        "",
        survival_table(experiments),
        "",
        "## Where did the genes come from?",
        "",
        "`incongruence` is the share of a resistance gene's carriers that received it",
        "sideways rather than inheriting it.",
        "",
        acquisition_table(experiments),
        "",
        "## The restriction barrier, as a rate",
        "",
        barrier_table(experiments),
        "",
    ]
    for path in figures(experiments, root):
        parts.append(f"![{os.path.basename(path)}]({os.path.basename(path)})")
    if plt is None:
        parts.append("_(matplotlib not installed: tables only)_")

    out = os.path.join(root, "summary.md")
    with open(out, "w") as f:
        f.write("\n".join(parts) + "\n")
    print("\n".join(parts))
    print(f"\nwrote {out}")


if __name__ == "__main__":
    main()
