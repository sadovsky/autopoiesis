# Results of the experiment protocol (plan §6)

All runs: 128×128 torus, 100 000 ticks, `SimConfig` defaults except where stated
(sun 4 at the bright edge of a linear gradient, noise 0.001, Repair cost 4,
`window` 100, `min_size` 3, metrics every 20 ticks). Produced by `scripts/sweep.sh`;
figures and tables by `scripts/plot.py` (in `plots/`). Per-seed raw JSONL (~13 MB per
run, 1.2 GB total) is not committed; `config.json` and `summary.jsonl` per experiment
are. Everything is reproducible from `(config, seed)`.

| experiment | seeds | flags |
|---|---|---|
| `baseline` | 50 | — |
| `seeded` | 20 | `--seed-ring --seed-ring-width 3` (3 columns of `Repair(S)` at the brightest column) |
| `ramp4` | 20 | `--noise-ramp 0.0:0.05:100000` |
| `ramp2` | 20 | `--noise-ramp 0.0:0.05:100000 --repair-cost 2` |
| `uniform` | 20 | `--sun-profile uniform --sun 2` (same total energy as the linear ramp) |

## Headline

Nothing that the plan's definition would call alive emerges from random
initialisation in this substrate. What emerges instead, within a few hundred ticks in
every seed, is a **grid-spanning "repair soup"**: a single strongly connected
component of 9 000–13 000 cells (of 16 384) whose bytes are almost all `Repair(d)` for
mixed directions. It is a genuine repair-graph organism by the §5a definition (every
member is repaired by another member) but it preserves no information about its own
past: its persistence sits at 0.4 (median; p90 0.7), *below* the background. Around
its dark-side frontier, hundreds of small SCCs (3–20 cells) are born and die each
window; a few per cent of these do score persistence > 3 for a few windows, and they
are the only things here that pass the §6 bar, but none lasts. The hand-written ring
is absorbed into the soup within the first frames. Under a noise ramp to 5 % per cell
per tick the soup never dissolves, so "vitality" of the substrate exceeds the ramp's
range, while its measured persistence *rises* with noise because the background
decorrelates faster than the soup does.

The reasons are structural, and they were visible before the sweep (see "Mechanisms").
They point straight at the plan's own open question §8: **`Repair` copying
`self.instr` makes `Repair` a self-replicator**, which is a stronger force here than
anything selection could build on top of it.

## Per-experiment answers

### 1. Baseline — does anything with persistence > 3 emerge from random init?

Briefly and small, yes; durably, no.

* Mean 125 candidate SCCs per frame; 8 800 of 16 384 cells sit in SCC cores at any
  time (`plots/scc_vs_tick.png`). The largest SCC in a frame is typically 5 000–12 000
  cells (max 13 063).
* Over 50 seeds, 13.0 M SCC identities were created; 2.04 M lived at least one window
  (100 ticks). Median lifetime of those is 100 ticks, p90 180, max 17 380 (a soup).
* 20 % of frames contain an organism with persistence > 3, but on average only 0.5
  such organisms holding 3.4 cells. Of the long-lived organisms, 25 629 (1.3 %) ever
  exceeded persistence 3 and 3 488 exceeded 5. These are small (median max size 6, p90
  17–19 cells) and short-lived (median 160–180 ticks, max 1 860); 20 of 25 629 were
  alive at the end of the run. Persistence is anti-correlated with size
  (`plots/persistence_vs_size.png`): the soups (≥ 1 000 cells) have median persistence
  0.40.
* Parasites (cells repaired by a core but never repairing) are ~1.6 % of core cells,
  ~140 per frame — the plan's "expected first result" is present but minor; the soup
  has few dependents because almost everything is already in it.
* Background stability over a 100-tick lag is 0.24: three quarters of bytes outside
  the SCCs change within a window, mostly by being overwritten, not by mutation.

### 2. Seeded — does the ring survive, grow, get parasitized?

It grows and dissolves into the soup; it does not survive as itself.

* In every seed the band's tracked identity reaches a max size of ~11 700 cells: the
  band is the nucleus of the soup, which expands from it by copy-self repair into the
  dying frontier. The identity is lost (Jaccard < 0.5 to any successor) after a median
  3 140 ticks (min 120, max 11 260); none is alive at the end.
* Its persistence as an organism is 0.88 (median): no better than the random-init
  soup. Statistically the `seeded` experiment is indistinguishable from `baseline`
  (125.3 vs 124.8 SCCs per frame, 8 861 vs 8 823 core cells, identical localisation
  curves). The initial condition is forgotten within a few hundred ticks.
* It is not parasitized in any specific sense; parasite counts match baseline.
  Mechanistically, in the single-column trace (`examples/ring_trace.rs`) the column's
  `ip`s get trapped by a neighbour's `MoveIp` pointing back at itself, so cells stop
  repairing for many ticks, and a mutated member propagates its byte down the ring as
  fast as the correct byte propagates behind it.

### 3. Noise sweep — vitality distribution; does lowering Repair cost to 2 shift it?

The soup outlives the ramp; the small organisms' vitality is just the noise level at
which they happened to die.

* Per-seed extinction noise (last frame with an SCC ≥ 100 cells) is 0.05 in every seed
  for both costs: the ramp's ceiling. Mean SCC-core cells never fall below 11 000
  (`plots/ramp_core_vs_noise.png`). At 5 % per-cell mutation per tick — half the bytes
  of a cell's 9-cell program change every ~2 ticks — the repair soup is still one
  connected component because `Repair` bytes overwrite their neighbours faster than
  noise removes them.
* Per-organism vitality (noise at dissolution) has median 0.0185 (Repair 4) and 0.0233
  (Repair 2), p90 ~0.044; organisms of ≥ 10 cells: 0.0159 vs 0.0218
  (`plots/vitality_hist.png`). The shift toward higher noise at Repair cost 2 is real
  but is a shift in *when things die during the ramp*, driven by there being fewer,
  larger, more stable SCCs at the cheaper cost (73 vs 96 per frame; 12 056 vs 10 996
  core cells), not a threshold. Cheaper repair means more of the grid is soup, so fewer
  frontier SCCs exist to die early.
* Measured persistence climbs with noise in both ramp runs, from ~2.2 at noise 0.001
  to ~4.5 at 0.05 (`plots/scc_vs_tick.png`, bottom): 58–61 % of frames have an
  organism above 3, 2.4–2.7 such organisms per frame. This is the background
  decorrelating (stability 0.04–0.05) while the soup's *opcode class* persists — the
  region stays "Repair" even as directions churn. It is the closest this substrate
  comes to the plan's picture (region holds MI while the surroundings decorrelate),
  and it is achieved by a structure with essentially no internal information.

### 4. Gradient off — do organisms still localize?

No; without a gradient the soup takes the whole grid and there is nothing left to
localise.

* Uniform sun 2: the whole torus becomes one SCC (16 384 cells, i.e. every cell)
  within ~200 ticks and stays there; 5 SCCs per frame on average, 15 446 core cells,
  persistence 0.7 (below background). Core-cell density along x is flat by
  construction; SCC identities are few (232 k created vs 13 M in baseline) and
  long-lived (28 alive at the end, max lifetime the full run).
* With the linear gradient, SCC cores localise strongly: essentially none in the
  darkest quarter (3.7 % of core cells), 40 % in the third quarter and 33 % in the
  brightest; density peaks at x ≈ 0.65, where sun ≈ 2.6 per tick
  (`plots/localisation.png`). The dark quarter (sun < 1) cannot sustain even `Nop`
  every tick, so cells starve, die and decohere there; the boundary between soup and
  dead zone at x ≈ 0.3–0.5 is where the small frontier SCCs live (mean x of organisms
  under 20 cells is 0.43, vs 0.65 size-weighted). Under the noise ramp the soup
  extends further into the dark side (Repair cost 2 more so) because dead cells are
  re-colonised faster than the frontier erodes.

So localisation is entirely imposed by the energy gradient, not by anything the
organisms do to hold a boundary.

## Mechanisms (why the substrate behaves this way)

These were established with small traces before the sweep and explain every curve
above.

1. **Copy-self `Repair` is a replicator, and repair is not error correction.**
   `Repair(d)` writes the executing cell's *own* byte into neighbour `d`. If that byte
   is itself `Repair(d′)`, the target now does the same thing. A single `Repair` byte
   therefore spreads along its direction; two directions spread into a plane. Nothing
   in the ISA can prefer the "right" byte: a mutated member of a ring propagates its
   mutation downstream exactly as the intact members propagate theirs, and the outcome
   is decided by energy ties and index order. At a repair-graph fixed point every edge
   `a → b` implies `instr[a] == instr[b]`, so a stable SCC is homogeneous and carries
   zero internal entropy, which is why in-core MI is degenerate and the metric has to
   look at the boundary.
2. **A cell's program is its neighbourhood**, so a cell surrounded by `Repair` bytes
   executes `Repair` in every direction with its own byte, densely wiring the region
   into one SCC. This makes the soup connected and makes the SCC boundary the boundary
   of the Repair-rich region, which is set by energy, not by the organism.
3. **`MoveIp` traps.** A neighbour holding `MoveIp(d)` where `d` points back at that
   neighbour freezes any cell whose `ip` lands on it (it executes `MoveIp` forever at
   cost 1). Whole columns of the seeded band were frozen this way for hundreds of
   ticks. `JmpIfZero` with `reg == 0` does the same.
4. **Where nothing writes, nothing changes.** At noise 0.001 an unrepaired byte lasts
   ~1 000 ticks; background turnover comes from `Store`/`Repair` and from death, not
   from mutation. A frozen region is as "persistent" as a maintained one; only the
   noise ramp separates them, and then only the soup remains.
5. **Energy decides everything spatial.** Cells that cannot afford their fetched
   instruction die and decohere before sunlight arrives (fixed during Phase 6: before
   that fix any sun ≥ 1 kept starved cells alive forever and nothing ever died on the
   bright side). Below sun ≈ 1 per tick a cell cannot even run `Nop` every tick, so the
   dark quarter is a death zone, the bright half is soup, and the frontier between
   them is the only place with turnover of *structures* rather than of bytes.

## Metric notes that mattered

* Plug-in MI over 256 byte values on a few dozen cells is pure finite-sample bias
  (≈ log₂ m regardless of dependence). MI is estimated on the 11-opcode alphabet,
  pooled over the five frames in a window, shuffle-corrected (subtracting the mean MI
  over permutations of the time pairing, which shares the exact marginals) and floored
  at 0.05 bit on both sides of the ratio. The random baseline is drawn from cells that
  belong to no organism's region; without that exclusion a "random" region in a world
  with a ring in it samples the ring.
* Because stable cores are homogeneous (mechanism 1), the region is the core dilated
  by one cell: persistence measures whether the organism-plus-boundary holds
  information while the background does not. A raw stability (fraction of bytes
  unchanged over the lag) is reported next to it and tells the same story.
* Vitality per organism is well-defined but, with median lifetimes of one window, it
  mostly records *when* an SCC happened to die during the ramp. The substrate-level
  curves (core cells vs noise, per-seed extinction noise) are the informative version.

## What to change next (plan §8, informed by the above)

*Update: items 1–3 were run; see `variants/README.md`. Template repair removes the soup
and nothing replaces it; the trap fix alone changes nothing; scarcity thins the soup. A
hand-designed template structure is an exact fixed point without noise but has a
vitality of 1.5 × 10⁻⁴, because every byte is executed as code by its eight neighbours.*

1. **`Repair` should not copy `self.instr`.** The §8 alternative — copy from the cell
   behind in the `ip` chain — makes repair an act of *reading a template* rather than
   *cloning yourself*, so a heterogeneous structure can be maintained and a mutation
   is not automatically hereditary. Equivalent cheaper variant: `Repair(d)` writes
   `reg`, so maintenance must be encoded as a `Load`/`Repair` loop. Either removes the
   replicator and lets the repair graph mean "maintains" rather than "overwrites".
2. **Break the `MoveIp` self-trap**, e.g. `MoveIp` costs its target's cost, or a
   trapped cell (same `ip` for `k` ticks) advances. Otherwise `MoveIp` is a cheap
   weapon that freezes neighbours.
3. **Energy scarcity on the bright side.** Sun 4 at the edge is more than a `Repair`
   every tick costs; the soup pays nothing for being a soup. Sun below the Repair cost
   everywhere (or Repair cost above the brightest sun) forces repair to be paid for by
   `Absorb`/`Share`, which is where cooperation would have to show up.
4. **Keep `tag` out of the metrics** (it already is): the boundary is the repair graph
   plus energy, and `tag` only follows `Repair` around.
5. Radius 2 is not worth trying before 1–3: it only adds directions to the replicator.

The measurement machinery — SCC tracking, background-relative MI, vitality under a
ramp — behaved as intended and separated the synthetic ring (persistence > 5) from a
fabricated cycle on random bytes (≈ 1) in tests. It reports, correctly, that this
substrate's dominant structure is an unliving replicator.
