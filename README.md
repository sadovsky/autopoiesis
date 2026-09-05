# autopoiesis

A Rust simulation testing a minimal definition of computational life: a region of a
noisy, energy-limited substrate that **actively maintains its own encoding and
boundary** against decay. Life is measured, not declared. A region is "alive" to the
degree it preserves mutual information with its own past while the surrounding noise
decorrelates.

There is no organism type. Everything is cells; an organism is whatever the repair
graph says is mutually maintaining itself.

```
cargo run --release -- --seed 1 --ticks 5000 --render          # watch it
cargo run --release -- --seed 1 --ticks 100000 --metrics m.jsonl   # measure it
./scripts/sweep.sh                                                # the experiment protocol
cargo test                                                        # 29 tests
```

Results of the protocol in `results/` (see `results/README.md`).

## Layout

```
src/
  config.rs    SimConfig — every tunable, with documented defaults; JSON-loadable
  grid.rs      Cell (6 bytes), toroidal Grid, Moore topology
  isa.rs       11-op instruction set, canonical byte encoding
  vm.rs        per-cell interpreter; write-conflict and death rules
  energy.rs    sun gradient (exact fixed-point dithering), diffusion
  noise.rs     mutation with geometric skipping; noise ramp lives in config
  sim.rs       run loop, double buffering, windowed repair log
  metrics.rs   repair-graph SCCs, self-mutual-information, organism tracking, vitality
  snapshot.rs  lossless binary snapshots (grid + repair edges + noise rate)
  render.rs    crossterm renderer (tag / ip / instr modes)
  main.rs      CLI: run, `analyze DIR`, `sweep`
tests/         isa_roundtrip, energy_conservation, vm_loop, snapshot_files, metrics
scripts/       sweep.sh (protocol), plot.py (figures + summary.md)
examples/      ring_trace.rs — print a column of the seeded ring over time
```

## The substrate

**Cell**: `instr: u8, energy: u16, ip: u8, reg: u8, tag: u8`. Grid is toroidal and
double-buffered; the whole run is deterministic given `(config, seed)` (tested).

**ISA** (low nibble = opcode, high nibble = operand; invalid opcodes decode to `Nop`;
operand's low 3 bits are a compass direction N, NE, E, SE, S, SW, W, NW):

| op | effect | cost |
|---|---|---|
| `Nop` | nothing | 1 |
| `MoveIp(d)` | `ip` jumps to neighbour `d` | 1 |
| `Load(d)` | `reg = nbr(d).instr` | 1 |
| `Store(d)` | `nbr(d).instr = reg` | 3 |
| `Repair(d)` | `nbr(d).instr = self.instr; nbr(d).tag = self.tag` | 4 |
| `Cmp(d)` | `reg = (nbr(d).instr == self.instr)` | 1 |
| `JmpIfZero(d)` | if `reg == 0`, `ip` jumps to neighbour `d` | 1 |
| `Absorb(d)` | pull ≤ `absorb_rate` energy from `nbr(d)` | 1 |
| `Share(d)` | push ≤ `share_rate` energy to `nbr(d)` | 1 |
| `SetTag` | `tag = reg` | 1 |
| `Halt` | dormant (ip frozen, free) while `energy <= halt_threshold` | 0 |

**Execution model** (the main design decision the plan left open; documented in
`vm.rs`): a cell's *program is its neighbourhood*. `ip ∈ 0..9` indexes the 9 cells of
the Moore neighbourhood (0 = self). Each tick the cell fetches the byte held by the
cell at `ip` and executes it *as itself* (its own energy, `reg`, `tag`, directions
relative to itself), then `ip` advances by one, wrapping. `MoveIp` / a taken
`JmpIfZero` set `ip` directly, so control can loop inside the neighbourhood and a
region of identical bytes behaves as one program. A cell whose `ip` sits on a
neighbour's `MoveIp` pointing back at that neighbour is trapped there indefinitely
(this happens and matters; see results).

**Write conflicts**: several cells writing one target in a tick → highest energy wins,
ties → lowest index. **Energy**: transfers are clamped and applied in index order;
energy is never created and clamping at the cap destroys it (tested: with `sun = 0`
the total is monotonically non-increasing). **Death**: a cell that reaches 0 energy
during a tick (starved or drained) decoheres at the end of that tick, before sunlight
is injected: random byte, tag and ip reset. Repairs into a dead cell are applied after
that, so repairing outward into dead space colonises it.

**Sun**: `inject(x) = sun · f(x)` with `f` linear (default), Gaussian or uniform;
fractional injection is dithered exactly with a per-cell fixed-point accumulator, no
RNG. **Noise**: each cell's byte is replaced with probability `noise_rate` per tick;
`--noise-ramp a:b:n` ramps linearly.

## Metrics (`metrics.rs`)

* **Repair graph + SCCs.** Edge `a → b` if `a` executed `Repair` on `b` within the
  last `window` ticks (kept as dense per-cell direction counts, so the edge set is a
  linear scan). Iterative Tarjan. An SCC of size ≥ `min_size` is a *candidate
  organism*. Cells repaired by the core but not in it are *dependents*; dependents that
  never repair anyone are *parasites*. Organisms are tracked across frames by Jaccard
  ≥ 0.5 of their cores; lifetimes and the noise rate at dissolution (*vitality*) are
  emitted as JSON lines.
* **Self-mutual-information.** `MI(R_t ; R_{t−Δ})` over the region's bytes versus the
  same for equal-sized random regions of the background (cells in no organism):
  `persistence = (MI_R + floor) / (MI_bg + floor)`. Estimator details that turned out
  to matter: with tens of cells and a 256-symbol alphabet the plug-in MI is pure
  finite-sample bias (≈ log₂ m for *any* region), so MI is computed on the 11-opcode
  alphabet, pooled over the frames inside the window, and **shuffle-corrected** (minus
  the mean MI with the time pairing permuted, which has identical marginals and thus
  identical bias). Because copy-self `Repair` makes a stable core homogeneous (zero
  entropy on its own), the region is the core dilated by one cell, which is the
  boundary the organism maintains. A plain *stability* (fraction of bytes unchanged
  over Δ) is reported alongside.
* **Vitality.** Under `--noise-ramp`, the noise rate at which an organism's SCC
  dissolves (unmatched for a full window). `scripts/plot.py` also reports the
  substrate-level view: SCC core cells versus noise rate, and each seed's extinction
  noise.

Online (`--metrics`) and offline (`analyze DIR` over snapshots) analysis are the same
code and produce byte-identical output for the same seed.

## Acceptance tests (plan §1–§5)

* every `u8` decodes; canonical re-encoding is a fixed point (`tests/isa_roundtrip.rs`)
* a hand-written 4-cell repair loop survives 10k ticks at `noise_rate = 0` and heals a
  zapped member within 9 ticks; two runs with one seed hash identically at tick 1000;
  write-conflict rules (`tests/vm_loop.rs`)
* `sun = 0` ⇒ total energy non-increasing; `sun > 0` with no execution ⇒ rises to cap
  and plateaus at the predicted tick (`tests/energy_conservation.rs`)
* snapshots round-trip losslessly through disk (`tests/snapshot_files.rs`)
* a synthetic self-repairing ring yields exactly one SCC with persistence > 5; a
  random grid yields none; a fabricated repair cycle on a random grid has persistence
  ≈ 1 (an SCC alone is not life); death/vitality bookkeeping; live-sim wiring
  (`tests/metrics.rs`)

## CLI

```
autopoiesis [--seed N] [--ticks N] [--config cfg.json] [--width W --height H]
            [--noise p | --noise-ramp a:b:n] [--sun S --sun-profile linear|gaussian|uniform]
            [--repair-cost C] [--seed-ring [--seed-ring-x X] [--seed-ring-width W]]
            [--render [--show tag|ip|instr] [--render-every N] [--fps F]]
            [--snapshots DIR [--snapshot-every N]] [--metrics FILE [--analysis-every N]]
            [--progress N] [--dump-config]
autopoiesis analyze DIR [--out FILE]          # metrics from snapshots
autopoiesis [...] sweep --seeds 0..50 --out DIR [--jobs J]   # parallel seeds
```

`--dump-config` prints the effective `SimConfig` as JSON; any subset of its fields can
be supplied with `--config`.

## Things deliberately not done (plan §7)

No genome or replication op (a region that repairs into dead space *is* reproducing,
and this is observed), no hardcoded organism boundary (only the repair graph), no
optimisation beyond what the sweep needed (dense repair-edge counts, MI subsampling,
`rayon` only across seeds), ISA held at 11 ops.

## A second crate: `hgt/`

Separate from this simulation, `hgt/` is a small sandbox for **horizontal gene transfer
between networked programs**: software nodes holding executable genes, facing a shifting
stressor, passing genes to each other by conjugation, uptake of the dead, and phages —
over a deterministic in-process transport, or over real TCP between separate processes.
It shares this crate's conventions and none of its code. See `hgt/README.md`.
