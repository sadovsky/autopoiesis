#!/usr/bin/env python3
"""Summarise an `hgt` results tree: hgt/scripts/plot.py hgt/results/demo

Walks every directory holding seed_*.jsonl, groups them by the section they sit in
(ab/, search/, policy/, ...), and writes summary.md with one table per question the
demo script asks. Draws frequency curves too if matplotlib is installed; the numbers do
not depend on it.
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
    if not summaries and os.path.exists(os.path.join(exp_dir, "summary.jsonl")):
        with open(os.path.join(exp_dir, "summary.jsonl")) as f:
            summaries = [json.loads(line) for line in f]
    return frames, epochs, genes, summaries


def discover(root):
    """Every experiment directory under root, keyed by its path relative to root."""
    found = {}
    for dirpath, _dirs, files in os.walk(root):
        if any(f.startswith("seed_") and f.endswith(".jsonl") for f in files):
            found[os.path.relpath(dirpath, root)] = load(dirpath)
    return dict(sorted(found.items()))


def section(experiments, name):
    """The experiments under one top-level section, with the section prefix stripped."""
    out = {}
    for key, value in experiments.items():
        parts = key.split(os.sep)
        if parts[0] == name:
            out[os.sep.join(parts[1:]) or name] = value
    return out


def median(xs, default=float("nan")):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else default


def last_frame_per_seed(frames):
    last = {}
    for f in frames:
        if f["seed"] not in last or f["tick"] > last[f["seed"]]["tick"]:
            last[f["seed"]] = f
    return last


def table(header, rows):
    if not rows:
        return "_(no runs)_"
    line = "| " + " | ".join(header) + " |"
    rule = "|" + "|".join("---" for _ in header) + "|"
    return "\n".join([line, rule] + ["| " + " | ".join(str(c) for c in r) + " |" for r in rows])


def survival_table(experiments):
    rows = []
    for name, (frames, epochs, _g, summaries) in experiments.items():
        alive = sum(1 for s in summaries if s["extinct_at"] is None)
        eps = median([s["epochs_survived"] for s in summaries])
        at_shift = median([e["start_freq"] for e in epochs if e["epoch"] > 0])
        lateral = median([f["lateral_share"] for f in frames]) if frames else float("nan")
        rows.append([name, len(summaries), alive, f"{eps:.0f}", f"{at_shift:.2f}", f"{lateral:.3f}"])
    return table(
        ["run", "seeds", "survived", "epochs survived", "freq at shift", "lateral share"], rows
    )


def acquisition_table(experiments):
    rows = []
    for name, (frames, _e, _g, _s) in experiments.items():
        last = last_frame_per_seed(frames)
        if not last:
            continue
        acq = defaultdict(int)
        for f in last.values():
            for k, v in f["acquisitions"].items():
                acq[k] += v
        inc = median(
            [r["incongruence"] for f in last.values() for r in f["resistance"] if r["carriers"] > 0]
        )
        rows.append(
            [name, acq["birth"], acq["conjugation"], acq["transformation"], acq["transduction"],
             f"{inc:.3f}"]
        )
    return table(
        ["run", "birth", "conjugation", "transformation", "transduction", "incongruence"], rows
    )


def barrier_table(experiments):
    rows = []
    for name, (frames, _e, _g, _s) in experiments.items():
        totals = defaultdict(lambda: [0, 0, 0])
        for f in last_frame_per_seed(frames).values():
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
            rows.append([name, d, attempts, redundant, accepted, f"{rate:.3f}"])
    return table(["run", "strain distance", "attempts", "redundant", "accepted", "rate"], rows)


def discovery_table(experiments):
    rows = []
    for name, (_f, _e, _g, summaries) in experiments.items():
        if not summaries:
            continue
        found = [s for s in summaries if s.get("first_discovery") is not None]
        firsts = [s["first_discovery"] for s in found]
        novel = sum(s.get("novel_discoveries", 0) for s in summaries)
        # Only the runs that found something have an answerer count worth reporting; the
        # rest are zero by construction and would drag the median to zero.
        answerers = median([s["solvers"][0] for s in found if s.get("solvers")])
        rows.append(
            [name, len(summaries), len(found), f"{median(firsts):.0f}" if firsts else "-",
             novel, f"{answerers:.0f}" if firsts else "-"]
        )
    return table(
        ["run", "seeds", "found it", "median tick found", "novel programs",
         "answerers at the end, where found"],
        rows,
    )


def policy_table(experiments):
    rows = []
    for name, (frames, _e, _g, summaries) in experiments.items():
        per_seed = defaultdict(list)
        for f in frames:
            per_seed[f["seed"]].append(f)
        starts, ends = [], []
        for seed_frames in per_seed.values():
            seed_frames.sort(key=lambda f: f["tick"])
            for f, target in ((seed_frames[0], starts), (seed_frames[-1], ends)):
                pol = f["policies"]
                total = pol["always_accept"] + pol["selfish"] + pol["thrifty"]
                target.append(pol["selfish"] / total if total else 0.0)
        alive = sum(1 for s in summaries if s["extinct_at"] is None)
        transfers = median([s["transfers"] for s in summaries])
        rows.append(
            [name, len(summaries), alive, f"{median(starts):.3f}", f"{median(ends):.3f}",
             f"{transfers:.0f}"]
        )
    return table(
        ["run", "seeds", "survived", "free riders at start", "free riders at end", "transfers"],
        rows,
    )


def immunity_table(experiments):
    rows = []
    for name, (frames, _e, _g, _s) in experiments.items():
        last = last_frame_per_seed(frames)
        if not last:
            continue
        lysed = sum(f["lysed"] for f in last.values())
        immune = sum(f["refusals"]["immune"] for f in last.values())
        transduced = sum(f["acquisitions"]["transduction"] for f in last.values())
        rows.append([name, lysed, immune, transduced])
    return table(["run", "phage kills", "immune refusals", "transduced acquisitions"], rows)


def partition_table(experiments):
    rows = []
    for name, (frames, _e, _g, summaries) in experiments.items():
        per_seed = defaultdict(list)
        for f in frames:
            per_seed[f["seed"]].append(f)
        mins, divs, worst = [], [], []
        for seed_frames in per_seed.values():
            mins.append(min(f["population"] for f in seed_frames))
            divs.append(max(f["divergence"] for f in seed_frames))
            worst.append(
                min(min(f["sides"]["here_solvers"], f["sides"]["there_solvers"]) for f in seed_frames)
            )
        alive = sum(1 for s in summaries if s["extinct_at"] is None)
        rows.append(
            [name, len(summaries), alive, f"{median(mins):.0f}", f"{median(worst):.0f}",
             f"{median(divs):.3f}"]
        )
    return table(
        ["run", "seeds", "survived", "min population", "worst side's answerers", "peak divergence"],
        rows,
    )


def genome_table(experiments):
    rows = []
    for name, (frames, _e, _g, summaries) in experiments.items():
        last = last_frame_per_seed(frames)
        if not last:
            continue
        alive = sum(1 for s in summaries if s["extinct_at"] is None)
        rows.append(
            [name, alive, f"{median([f['genome_mean'] for f in last.values()]):.2f}",
             f"{median([f['distinct_genes'] for f in last.values()]):.0f}",
             sum(f["refusals"]["redundant"] for f in last.values())]
        )
    return table(["run", "survived", "genes per node", "distinct genes", "redundant arrivals"], rows)


def figures(experiments, out_dir):
    if plt is None:
        return []
    written = []
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
        if not series:
            continue
        fig, ax = plt.subplots(figsize=(8, 3.5))
        for kind in sorted(series):
            xs, ys = series[kind]
            ax.plot(xs, ys, label=f"stressor {kind}")
        ax.set_xlabel("tick")
        ax.set_ylabel("carrier fraction")
        ax.set_title(f"resistance gene frequency — {name}, seed {seed}")
        ax.legend(fontsize="small")
        fig.tight_layout()
        path = os.path.join(out_dir, f"frequency_{name.replace(os.sep, '_').replace(',', '-')}.png")
        fig.savefig(path, dpi=130)
        plt.close(fig)
        written.append(path)
    return written


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "hgt/results/demo"
    experiments = discover(root)
    if not experiments:
        sys.exit(f"no sweep output under {root}")

    ab = section(experiments, "ab")
    parts = [f"# hgt: {root}", ""]

    if ab:
        parts += [
            "## Does the population survive stressors it was not born ready for?",
            "",
            survival_table(ab),
            "",
            "`freq at shift` is how common the answering gene already was when the stressor",
            "arrived. That, not a rescue afterwards, is where transfer does its work.",
            "",
            "## Where did the genes come from?",
            "",
            "`incongruence` is the share of a resistance gene's carriers that received it",
            "sideways rather than inheriting it.",
            "",
            acquisition_table(ab),
            "",
            "## The restriction barrier, as a rate",
            "",
            barrier_table(ab),
            "",
        ]

    for name, title, fn in [
        ("search", "## Can a gene be found rather than received?", discovery_table),
        ("policy", "## Do free riders take over?", policy_table),
        ("immunity", "## What does an immune system buy, and cost?", immunity_table),
        ("partition", "## What does cutting the network cost?", partition_table),
        ("unproven", "## What if nodes offer genes nobody has seen work?", genome_table),
    ]:
        sub = section(experiments, name)
        if sub:
            parts += [title, "", fn(sub), ""]

    for path in figures(ab or experiments, root):
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
