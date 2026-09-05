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
        "organisms created (total) | organisms lived >= 1 window | frac frames maxP>3 | mean persistent SCCs/frame | "
        "mean persistent cells/frame | long-lived organisms with maxP>3 | mean parasites/frame | mean bg stability |",
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for name, (frames, lives, summaries) in data.items():
        if not frames:
            continue
        n_seeds = len({r["seed"] for r in frames})
        max_scc = max((max(r["sizes"]) if r["sizes"] else 0) for r in frames)
        frac_p = np.mean([r["max_persistence"] > PERSISTENT for r in frames])
        n_p = sum(1 for l in lives if l["max_persistence"] > PERSISTENT)
        created = sum(s_["organisms_seen"] for s_ in summaries)
        lines.append(
            f"| {name} | {n_seeds} | {len(frames)} | {np.mean([r['n_organisms'] for r in frames]):.1f} | "
            f"{np.mean([r['core_cells'] for r in frames]):.0f} | {max_scc} | {created} | {len(lives)} | "
            f"{frac_p:.3f} | {np.mean([r.get('n_persistent', 0) for r in frames]):.2f} | "
            f"{np.mean([r.get('persistent_cells', 0) for r in frames]):.1f} | {n_p} | "
            f"{np.mean([r['parasite_cells'] for r in frames]):.1f} | "
            f"{np.mean([r['background_stability'] for r in frames]):.2f} |"
        )
    lines.append("")

    # --- Persistence distribution over organism-frames ----------------------------
    fig, ax = plt.subplots(figsize=(8, 4.5))
    for name, (frames, _, _) in data.items():
        hists = [r["persistence_hist"] for r in frames if r.get("persistence_hist")]
        if hists:
            h = np.sum(np.array(hists, dtype=float), axis=0)
            edges = np.arange(len(h) + 1)
            ax.stairs(h / max(h.sum(), 1), edges, label=f"{name} (n={int(h.sum())} organism-frames)")
    ax.axvline(PERSISTENT, color="k", ls=":", lw=0.8)
    ax.set_yscale("log")
    ax.set_xlabel("persistence = (MI_region + floor) / (MI_background + floor); last bin open")
    ax.set_ylabel("fraction of organism-frames (all organisms)")
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
    ax.set_title("reported rows only: per frame, top-10 by size and top-10 by persistence", fontsize=9)
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

    # --- Ramp experiments: substrate-level view, core cells vs noise rate -----------
    if ramp:
        fig, axes = plt.subplots(1, 2, figsize=(11, 4.5))
        lines += ["## Extinction under the noise ramp", "",
                  "Per seed: the noise rate at the last frame that still had an SCC of the given size. Median (min–max) over seeds.",
                  "", "| experiment | extinction noise, any SCC (>= min_size) | extinction noise, SCC >= 10 cells | "
                  "extinction noise, SCC >= 100 cells | noise where mean core cells first < 1% of grid |",
                  "|---|---|---|---|---|"]
        for name, (frames, lives, _) in ramp.items():
            by_noise = defaultdict(list)
            for r in frames:
                by_noise[round(r["noise_rate"], 5)].append(r)
            xs = sorted(by_noise)
            core = np.array([np.mean([r["core_cells"] for r in by_noise[x]]) for x in xs])
            norg = np.array([np.mean([r["n_organisms"] for r in by_noise[x]]) for x in xs])
            axes[0].plot(xs, core, label=name)
            axes[1].plot(xs, norg, label=name)
            w = grid_width(exps[name], width_default)
            n_cells = w * w
            # First noise level after the peak at which the SCC cores hold < 1% of the grid.
            peak = int(np.argmax(core))
            first_low = next((x for x, c in zip(xs[peak:], core[peak:]) if c < 0.01 * n_cells), None)

            def extinction(min_size):
                vals = []
                for seed in {r["seed"] for r in frames}:
                    fs = [r for r in frames if r["seed"] == seed and r["sizes"] and max(r["sizes"]) >= min_size]
                    vals.append(max(fs, key=lambda r: r["tick"])["noise_rate"] if fs else 0.0)
                vals = np.array(vals)
                return f"{np.median(vals):.4f} ({vals.min():.4f}–{vals.max():.4f})"

            lines.append(
                f"| {name} | {extinction(1)} | {extinction(10)} | {extinction(100)} | "
                f"{('%.4f' % first_low) if first_low is not None else 'never'} |"
            )
        axes[0].set_xlabel("noise rate")
        axes[0].set_ylabel("mean cells in SCC cores")
        axes[1].set_xlabel("noise rate")
        axes[1].set_ylabel("mean candidate organisms")
        axes[0].legend(fontsize=8)
        axes[1].legend(fontsize=8)
        fig.tight_layout()
        fig.savefig(os.path.join(out, "ramp_core_vs_noise.png"), dpi=120)
        plt.close(fig)
        lines.append("")

    # --- Lifetimes ----------------------------------------------------------------
    fig, ax = plt.subplots(figsize=(8, 4.5))
    lines += ["## Lifetimes (organisms that lived >= report_min_life = one window)", "",
              "| experiment | organisms | median lifetime (ticks) | p90 | max | still alive at end | max size ever |",
              "|---|---|---|---|---|---|---|"]
    for name, (frames, lives, summaries) in data.items():
        if not lives:
            continue
        end = max(r["tick"] for r in frames) if frames else 0
        lt = np.array([l.get("lifetime", (l["died"] if l["died"] is not None else end) - l["born"]) for l in lives], dtype=float)
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

    # --- Localisation: where along x (the sun gradient) do organisms sit? ----------
    fig, axes = plt.subplots(1, 2, figsize=(11, 4.5))
    lines += ["## Localisation along the sun gradient (x)", "",
              "Core-cell density: fraction of all SCC-core cells (over all frames) per x band. "
              "Centroid columns use per-organism mean x, weighted by core size; older outputs fall back to the anchor.", "",
              "| experiment | core cells in darkest quarter | second | third | brightest quarter | "
              "size-weighted mean x of organisms | mean x of small organisms (<20 cells) |", "|---|---|---|---|---|---|---|"]
    for name, (frames, _, _) in data.items():
        w = grid_width(exps[name], width_default)
        hists = [r["core_x_hist"] for r in frames if r.get("core_x_hist")]
        if hists:
            h = np.sum(np.array(hists, dtype=float), axis=0)
            nb = len(h)
            quarters = [h[i * nb // 4:(i + 1) * nb // 4].sum() / max(h.sum(), 1) for i in range(4)]
            centers = (np.arange(nb) + 0.5) / nb
            axes[0].plot(centers, h / max(h.sum(), 1) * nb, label=name)
        else:
            quarters = [float("nan")] * 4
        xs = np.array([o.get("cx", o["anchor"] % w) / w for r in frames for o in r["organisms"]])
        ws = np.array([o["core_size"] for r in frames for o in r["organisms"]], dtype=float)
        small = np.array([o.get("cx", o["anchor"] % w) / w for r in frames for o in r["organisms"] if o["core_size"] < 20])
        if len(xs):
            axes[1].hist(xs, bins=32, range=(0, 1), weights=ws, histtype="step", density=True, label=name)
            wmean = np.average(xs, weights=ws)
            lines.append(
                f"| {name} | {quarters[0]:.3f} | {quarters[1]:.3f} | {quarters[2]:.3f} | {quarters[3]:.3f} | "
                f"{wmean:.3f} | {small.mean() if len(small) else float('nan'):.3f} |"
            )
    axes[0].set_xlabel("x / width  (linear sun: 0 = dark, 1 = bright)")
    axes[0].set_ylabel("SCC-core cell density (1 = uniform)")
    axes[1].set_xlabel("organism centroid x / width")
    axes[1].set_ylabel("density, weighted by core size")
    axes[0].legend(fontsize=8)
    axes[1].legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(os.path.join(out, "localisation.png"), dpi=120)
    plt.close(fig)
    lines.append("")

    # --- Null-twin stability ratio ----------------------------------------------------
    # If a `null` experiment (repair_source = none) exists in this root, every organism
    # row's byte stability is divided by the twin's background stability at the same tick
    # and x band: the proper "maintained vs untouched" baseline.
    if "null" in data and data["null"][0]:
        twin = defaultdict(list)
        for r in data["null"][0]:
            if r.get("stability_x_hist"):
                twin[r["tick"]].append(r["stability_x_hist"])
        twin = {t: np.mean(np.array(v), axis=0) for t, v in twin.items()}
        nb = len(next(iter(twin.values())))
        lines += ["## Null-twin stability ratio (organism stability / repair-disabled world at same tick and x band)", "",
                  "| experiment | organism-frames | median ratio | frac ratio > 3 | rows with ratio > 3 and size >= 10 | rows with ratio > 3 and size >= 100 |",
                  "|---|---|---|---|---|---|"]
        fig, ax = plt.subplots(figsize=(8, 4.5))
        for name, (frames, _, _) in data.items():
            if name == "null":
                continue
            w = grid_width(exps[name], width_default)
            ratios, big10, big100 = [], 0, 0
            for r in frames:
                base = twin.get(r["tick"])
                if base is None:
                    continue
                for o in r["organisms"]:
                    if o["mi_samples"] == 0:
                        continue
                    xb = min(int(o["cx"] / w * nb), nb - 1)
                    ratio = (o["stability"] + 0.02) / (base[xb] + 0.02)
                    ratios.append(ratio)
                    if ratio > 3 and o["core_size"] >= 10:
                        big10 += 1
                    if ratio > 3 and o["core_size"] >= 100:
                        big100 += 1
            if ratios:
                ratios = np.array(ratios)
                ax.hist(ratios, bins=60, range=(0, 6), histtype="step", density=True, label=f"{name} (n={len(ratios)})")
                lines.append(f"| {name} | {len(ratios)} | {np.median(ratios):.2f} | {np.mean(ratios > 3):.3f} | {big10} | {big100} |")
        ax.axvline(3, color="k", ls=":", lw=0.8)
        ax.set_xlabel("stability ratio vs null twin")
        ax.set_ylabel("density over reported organism rows")
        ax.legend(fontsize=8)
        fig.tight_layout()
        fig.savefig(os.path.join(out, "null_ratio_hist.png"), dpi=120)
        plt.close(fig)
        lines.append("")

    # --- Perturbation probes -----------------------------------------------------------
    probes = {}
    for name, d in exps.items():
        recs = []
        for path in sorted(glob.glob(os.path.join(d, "seed_*.jsonl"))):
            with open(path) as f:
                for line in f:
                    if line.startswith('{"kind":"probe"'):
                        recs.append(json.loads(line))
        if recs:
            probes[name] = recs
    if probes:
        lines += ["## Perturbation probes (fraction of overwritten bytes restored; organism vs matched background)", "",
                  "| experiment | probes | size class | n | restored 1w | 2w | 5w | background 1w | 2w | 5w |", "|---|---|---|---|---|---|---|---|---|---|"]
        fig, ax = plt.subplots(figsize=(9, 4.5))
        xs_lbl = []
        for k, (name, recs) in enumerate(probes.items()):
            for cls, lo, hi in [("<10", 0, 10), ("10-99", 10, 100), (">=100", 100, 10**9)]:
                sel = [r for r in recs if lo <= r["core_size"] < hi]
                if not sel:
                    continue
                res = np.array([r["restored"] for r in sel])
                bg = np.array([r["bg_restored"] for r in sel if r["bg_k"] > 0])
                bgm = np.nanmean(bg, axis=0) if len(bg) else [float("nan")] * 3
                lines.append(f"| {name} | {len(recs)} | {cls} | {len(sel)} | {res[:,0].mean():.3f} | {res[:,1].mean():.3f} | {res[:,2].mean():.3f} | "
                             f"{bgm[0]:.3f} | {bgm[1]:.3f} | {bgm[2]:.3f} |")
                if cls == ">=100" or (cls == "10-99" and not any(r["core_size"] >= 100 for r in recs)):
                    xs_lbl.append(f"{name}\n{cls}")
                    i = len(xs_lbl) - 1
                    ax.bar(i - 0.2, res[:, 0].mean(), 0.4, color="C0", label="organism, 1 window" if i == 0 else None)
                    ax.bar(i + 0.2, bgm[0], 0.4, color="C7", label="matched background" if i == 0 else None)
        ax.set_xticks(range(len(xs_lbl)))
        ax.set_xticklabels(xs_lbl, fontsize=8)
        ax.set_ylabel("fraction restored after one window")
        ax.set_ylim(0, 1)
        ax.legend(fontsize=8)
        fig.tight_layout()
        fig.savefig(os.path.join(out, "probe_restoration.png"), dpi=120)
        plt.close(fig)
        lines.append("")

    # --- Half-life of seeded structures at fixed noise (experiments named hl_<tag>_<noise>)
    # Uses the frame field seed_intact: the fraction of the injected structure's bytes
    # still equal to what was seeded (mean over seeds).
    hl = defaultdict(list)
    for name, (frames, _, _) in data.items():
        if not name.startswith("hl_") or not frames:
            continue
        tag = name.split("_")[1]
        try:
            with open(os.path.join(exps[name], "config.json")) as f:
                noise = json.load(f)["noise_rate"]
        except (OSError, KeyError, ValueError):
            continue
        by_tick = defaultdict(list)
        for r in frames:
            v = r.get("seed_intact")
            if v is not None and v == v:  # not NaN
                by_tick[r["tick"]].append(v)
        if not by_tick:
            continue
        ticks = sorted(by_tick)
        intact = np.array([np.mean(by_tick[t]) for t in ticks])
        t_half = next((t for t, v in zip(ticks, intact) if v <= 0.5), None)
        hl[tag].append((noise, t_half if t_half is not None else float("inf"), intact[-1], ticks[-1]))
    if hl:
        lines += ["## Half-life of the seeded structure vs noise (ticks until half of its bytes differ from the seed)", "",
                  "| structure | noise | half-life (ticks) | intact at end of run |", "|---|---|---|---|"]
        fig, ax = plt.subplots(figsize=(7, 4.5))
        for tag, rows in hl.items():
            rows.sort()
            for noise, th, end_v, t_end in rows:
                lines.append(f"| {tag} | {noise:g} | {th if th != float('inf') else f'> {t_end}'} | {end_v:.3f} |")
            xs = [n for n, th, _, _ in rows if np.isfinite(th) and th > 0]
            ys = [th for n, th, _, _ in rows if np.isfinite(th) and th > 0]
            if xs:
                ax.plot(xs, ys, "o-", label=tag)
        ax.set_xscale("log")
        ax.set_yscale("log")
        ax.set_xlabel("noise rate")
        ax.set_ylabel("half-life of the seeded bytes (ticks)")
        ax.legend(fontsize=8)
        fig.tight_layout()
        fig.savefig(os.path.join(out, "halflife_vs_noise.png"), dpi=120)
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
