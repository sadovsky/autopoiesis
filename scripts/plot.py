#!/usr/bin/env python3
"""Plot sweep results: SCC count vs tick, persistence, vitality histograms, and
organism localisation. Usage: scripts/plot.py results/ [--width 128]

Reads every results/<experiment>/seed_*.jsonl written by `autopoiesis sweep` and
writes PNGs plus a summary.md into results/plots/.
"""
import glob
import json
import os
import sys
from collections import defaultdict

import numpy as np

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

PERSISTENT = 3.0  # plan §6 question 1: "anything with persistence > 3"


def load(exp_dir):
    frames, lives, summaries = [], [], []
    for path in sorted(glob.glob(os.path.join(exp_dir, "seed_*.jsonl"))):
        with open(path) as f:
            for line in f:
                r = json.loads(line)
                k = r.get("kind")
                if k == "frame":
                    frames.append(r)
                elif k == "life":
                    lives.append(r)
                elif k == "summary":
                    summaries.append(r)
    return frames, lives, summaries


def per_tick(frames, key, fn=np.mean):
    by = defaultdict(list)
    for r in frames:
        by[r["tick"]].append(key(r))
    ticks = sorted(by)
    vals = np.array([fn(by[t]) for t in ticks])
    lo = np.array([np.percentile(by[t], 25) for t in ticks])
    hi = np.array([np.percentile(by[t], 75) for t in ticks])
    return np.array(ticks), vals, lo, hi


def grid_width(exp_dir, default):
    try:
        with open(os.path.join(exp_dir, "config.json")) as f:
            return json.load(f)["width"]
    except (OSError, KeyError, ValueError):
        return default


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "results"
    width_default = 128
    if "--width" in sys.argv:
        width_default = int(sys.argv[sys.argv.index("--width") + 1])
    out = os.path.join(root, "plots")
    os.makedirs(out, exist_ok=True)

    exps = {}
    for d in sorted(glob.glob(os.path.join(root, "*"))):
        if os.path.isdir(d) and glob.glob(os.path.join(d, "seed_*.jsonl")):
            exps[os.path.basename(d)] = d
    if not exps:
        sys.exit(f"no experiments with seed_*.jsonl under {root}")

    data = {name: load(d) for name, d in exps.items()}
    lines = ["# Sweep summary", ""]

    # --- SCC count, core cells and max persistence vs tick -------------------------
    fig, axes = plt.subplots(3, 1, figsize=(9, 10), sharex=True)
    for name, (frames, lives, summaries) in data.items():
        if not frames:
            continue
        t, v, lo, hi = per_tick(frames, lambda r: r["n_organisms"])
        axes[0].plot(t, v, label=name)
        axes[0].fill_between(t, lo, hi, alpha=0.15)
        t, v, lo, hi = per_tick(frames, lambda r: r["core_cells"])
        axes[1].plot(t, v, label=name)
        axes[1].fill_between(t, lo, hi, alpha=0.15)
        t, v, lo, hi = per_tick(frames, lambda r: r["max_persistence"])
        axes[2].plot(t, v, label=name)
        axes[2].fill_between(t, lo, hi, alpha=0.15)
    axes[0].set_ylabel("candidate organisms (SCCs)")
    axes[1].set_ylabel("cells in SCC cores")
    axes[2].set_ylabel("max persistence in frame")
    axes[2].axhline(PERSISTENT, color="k", ls=":", lw=0.8)
    axes[2].set_xlabel("tick")
    axes[0].legend(loc="upper right", fontsize=8)
    axes[0].set_title("mean over seeds, band = interquartile range")
    fig.tight_layout()
    fig.savefig(os.path.join(out, "scc_vs_tick.png"), dpi=120)
    plt.close(fig)

    # --- Per-experiment summary table ---------------------------------------------
    lines += [
        "| experiment | seeds | frames | mean SCCs/frame | mean core cells | max SCC | "
        "organisms tracked | frac frames maxP>3 | organisms with maxP>3 | "
        "mean parasites/frame | mean bg stability |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for name, (frames, lives, summaries) in data.items():
        if not frames:
            continue
        n_seeds = len({r["seed"] for r in frames})
        max_scc = max((max(r["sizes"]) if r["sizes"] else 0) for r in frames)
        frac_p = np.mean([r["max_persistence"] > PERSISTENT for r in frames])
        n_p = sum(1 for l in lives if l["max_persistence"] > PERSISTENT)
        lines.append(
            f"| {name} | {n_seeds} | {len(frames)} | {np.mean([r['n_organisms'] for r in frames]):.1f} | "
            f"{np.mean([r['core_cells'] for r in frames]):.0f} | {max_scc} | {len(lives)} | "
            f"{frac_p:.3f} | {n_p} | {np.mean([r['parasite_cells'] for r in frames]):.1f} | "
            f"{np.mean([r['background_stability'] for r in frames]):.2f} |"
        )
    lines.append("")

    # --- Persistence distribution over organism-frames ----------------------------
    fig, ax = plt.subplots(figsize=(8, 4.5))
    for name, (frames, _, _) in data.items():
        ps = [o["persistence"] for r in frames for o in r["organisms"] if o["mi_samples"] > 0]
        if ps:
            ax.hist(ps, bins=60, range=(0, 12), histtype="step", density=True, label=f"{name} (n={len(ps)})")
    ax.axvline(PERSISTENT, color="k", ls=":", lw=0.8)
    ax.set_xlabel("persistence = (MI_region + floor) / (MI_background + floor)")
    ax.set_ylabel("density over organism-frames")
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(os.path.join(out, "persistence_hist.png"), dpi=120)
    plt.close(fig)

    # --- Persistence vs core size -------------------------------------------------
    fig, ax = plt.subplots(figsize=(8, 4.5))
    for name, (frames, _, _) in data.items():
        xs = [o["core_size"] for r in frames for o in r["organisms"] if o["mi_samples"] > 0]
        ys = [o["persistence"] for r in frames for o in r["organisms"] if o["mi_samples"] > 0]
        if xs:
            ax.scatter(xs, ys, s=3, alpha=0.25, label=name)
    ax.set_xscale("log")
    ax.axhline(PERSISTENT, color="k", ls=":", lw=0.8)
    ax.set_xlabel("SCC core size (cells)")
    ax.set_ylabel("persistence")
    ax.legend(fontsize=8, markerscale=4)
    fig.tight_layout()
    fig.savefig(os.path.join(out, "persistence_vs_size.png"), dpi=120)
    plt.close(fig)

    # --- Vitality histograms (ramp experiments) -----------------------------------
    # Vitality only means something when the noise rate actually varied over the run.
    ramp = {
        n: d
        for n, d in data.items()
        if d[0] and max(r["noise_rate"] for r in d[0]) > min(r["noise_rate"] for r in d[0]) + 1e-12
    }
    if ramp:
        fig, axes = plt.subplots(1, 2, figsize=(11, 4.5))
        lines += ["## Vitality (noise ramp experiments)", "",
                  "| experiment | organisms died | survived to end | vitality median | vitality p90 | max | "
                  "median of organisms with max_size>=10 |", "|---|---|---|---|---|---|---|"]
        for name, (frames, lives, _) in ramp.items():
            vit = np.array([l["vitality"] for l in lives if l["vitality"] is not None])
            big = np.array([l["vitality"] for l in lives if l["vitality"] is not None and l["max_size"] >= 10])
            surv = sum(1 for l in lives if l["vitality"] is None)
            if len(vit):
                axes[0].hist(vit, bins=50, range=(0, 0.05), histtype="step", density=True, label=f"{name} (n={len(vit)})")
                # weight by max_size so big organisms are visible
                w = np.array([l["max_size"] for l in lives if l["vitality"] is not None], dtype=float)
                axes[1].hist(vit, bins=50, range=(0, 0.05), weights=w, histtype="step", density=True, label=f"{name}")
                lines.append(
                    f"| {name} | {len(vit)} | {surv} | {np.median(vit):.4f} | {np.percentile(vit, 90):.4f} | "
                    f"{vit.max():.4f} | {np.median(big) if len(big) else float('nan'):.4f} |"
                )
        axes[0].set_xlabel("vitality = noise rate at which the SCC dissolved")
        axes[0].set_ylabel("density (per organism)")
        axes[1].set_xlabel("vitality")
        axes[1].set_ylabel("density (weighted by max size)")
        axes[0].legend(fontsize=8)
        axes[1].legend(fontsize=8)
        fig.tight_layout()
        fig.savefig(os.path.join(out, "vitality_hist.png"), dpi=120)
        plt.close(fig)
        lines.append("")

    # --- Lifetimes ----------------------------------------------------------------
    fig, ax = plt.subplots(figsize=(8, 4.5))
    lines += ["## Lifetimes", "", "| experiment | organisms | median lifetime (ticks) | p90 | max | still alive at end | max size ever |",
              "|---|---|---|---|---|---|---|"]
    for name, (frames, lives, summaries) in data.items():
        if not lives:
            continue
        end = max(r["tick"] for r in frames) if frames else 0
        lt = np.array([(l["died"] if l["died"] is not None else end) - l["born"] for l in lives], dtype=float)
        ax.hist(lt, bins=np.logspace(1, np.log10(max(lt.max(), 20)), 40), histtype="step", label=name)
        alive = sum(1 for l in lives if l["died"] is None)
        lines.append(
            f"| {name} | {len(lives)} | {np.median(lt):.0f} | {np.percentile(lt, 90):.0f} | {lt.max():.0f} | {alive} | "
            f"{max(l['max_size'] for l in lives)} |"
        )
    ax.set_xscale("log")
    ax.set_xlabel("organism lifetime (ticks)")
    ax.set_ylabel("count")
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(os.path.join(out, "lifetimes_hist.png"), dpi=120)
    plt.close(fig)
    lines.append("")

    # --- Localisation: where along x do organisms sit? ----------------------------
    fig, ax = plt.subplots(figsize=(8, 4.5))
    lines += ["## Localisation along the sun gradient (x)", "",
              "| experiment | mean x of organism anchors (fraction of width) | std |", "|---|---|---|"]
    for name, (frames, _, _) in data.items():
        w = grid_width(exps[name], width_default)
        xs = np.array([(o["anchor"] % w) / w for r in frames for o in r["organisms"]])
        if len(xs):
            ax.hist(xs, bins=32, range=(0, 1), histtype="step", density=True, label=name)
            lines.append(f"| {name} | {xs.mean():.3f} | {xs.std():.3f} |")
    ax.set_xlabel("x / width  (linear sun: 0 = dark, 1 = bright)")
    ax.set_ylabel("density of organism anchors")
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(os.path.join(out, "localisation.png"), dpi=120)
    plt.close(fig)
    lines.append("")

    # --- Run summaries ------------------------------------------------------------
    lines += ["## Run totals (mean per seed)", "", "| experiment | executed | repairs | deaths | starved | mutations | elapsed s |",
              "|---|---|---|---|---|---|---|"]
    for name, (_, _, summaries) in data.items():
        if summaries:
            m = lambda k: np.mean([s[k] for s in summaries])  # noqa: E731
            lines.append(f"| {name} | {m('executed'):.3g} | {m('repairs'):.3g} | {m('deaths'):.3g} | "
                         f"{m('starved'):.3g} | {m('mutations'):.3g} | {m('elapsed_s'):.1f} |")
    with open(os.path.join(out, "summary.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    print("\n".join(lines))
    print(f"\nplots written to {out}/")


if __name__ == "__main__":
    main()
