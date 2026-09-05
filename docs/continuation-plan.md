# Continuation plan

> **Status (after round 3):** phases A, B and C were run; see `results/round3/README.md`.
> A failed its gate (pass-through repair is a bigger soup). B was implemented and reached
> its "stop and rethink the ISA" branch: the token model removes code/data coupling and
> leaves an empty repair graph, and every structure this ISA can maintain turns out to be
> a single-source relay belt, not a repair loop. C's instruments (probe, null twin,
> half-life) work and all read "no active maintenance anywhere". D was not started. The
> next design question is a write discipline (accumulator `Cmp`, vote-gated `Repair`, no
> unguarded `Store`), argued at the end of the round-3 write-up.


Where the two protocols leave us, and what to do next. Each phase ends with a runnable
experiment, a number, and a decision gate. Work the phases in order unless a gate says
to skip.

## What we know (from `results/` and `results/variants/`)

1. **Copy-self `Repair` is a replicator.** With the plan's ISA the only thing that
   emerges is a grid-spanning "repair soup" that preserves no information about its
   past (persistence 0.4, below background). Every other observation in the first
   protocol — the seeded ring being absorbed, vitality beyond the ramp, localisation
   set purely by sun — follows from that.
2. **Template repair removes the soup and nothing replaces it.** With `Repair` writing
   a loaded template, SCC cores fall from 54 % of the grid to < 1 %, zero organisms
   hold persistence > 3 for a window, and the background freezes (72–77 % of bytes
   unchanged over a 100-tick lag).
3. **Template repair is expressive enough** — a hand-designed column tiling is an
   exact fixed point without noise and impossible under copy-self — **but its
   vitality is 1.5 × 10⁻⁴**, seven times below the baseline noise rate.
4. **The bottleneck is code/data coupling in the execution model, not the repair
   rule.** In the neighbourhood-as-program VM every byte is executed as an instruction
   by its eight neighbours once per 9-tick sweep. A mutated byte is executed 1–3 times
   before a repair lands, and about half of all bytes corrupt the executor's register
   or write somewhere wrong. Error branching factor ≈ 1.
5. **The persistence metric cannot separate "maintained" from "untouched".** Its
   baseline is the same world's background, which is either soup (first protocol) or
   frozen (second). Only a noise ramp separates them, and at the noise where the
   background decorrelates, the structures are gone.
6. Neither the `MoveIp` trap fix nor energy scarcity changes anything on its own.

So the next round has two jobs: **make maintained structure robust** (fix 4) and
**measure maintenance directly** (fix 5). Emergence is only worth searching for once a
hand-built structure survives the baseline noise.

## Phase A — cheapest repair rule that removes the register channel

**Hypothesis.** Half of the error branching comes through `reg`: a junk `Load`/`Cmp`
executed by a neighbour poisons its register and the next `Repair` writes the poison.
A repair rule with no register dependence should cut the branching factor roughly in
half without touching the execution model.

**Change.** `repair_source = opposite` ("pass-through"): `Repair(d)` writes
`nbr(opposite(d)).instr` into `nbr(d)`. The cell relays the byte behind it to the cell
in front of it. This is the plan's §8 wording ("copies from the cell behind it") read
spatially rather than along the ip chain.

**Designed structure.** Rows alternating `Repair(S)` / `Repair(N)`. A `Repair(S)` cell
copies its north neighbour (a `Repair(N)`) into its south neighbour (also `Repair(N)`);
a `Repair(N)` cell does the mirror. Every byte is restored from an independent copy two
cells away, no register is involved, and — check this by hand before running — every
byte a cell executes from its eight neighbours is consistent with the pattern (same
row ⇒ same class ⇒ same copy direction; other row ⇒ the mirror copy, which is also
consistent). Add it as `seed_tiling` pattern `pass_through`; keep the register tiling
as `register`.

**Experiments.** 20 seeds × 100k, 128×128: (a) random init under `opposite` (does a
soup come back? `Repair` runs replicate `Repair` bytes only along the run); (b) the
pass-through tiling under the noise ramp 0 → 0.002 (vitality); (c) the same at fixed
noise 10⁻⁴, 3 × 10⁻⁴, 10⁻³, 3 × 10⁻³ for 20k ticks each, fitting column half-life
against noise (cleaner than the ramp; add `half_life` to the plot script).

**Gate.** If the pass-through tiling's vitality ≥ 10⁻³ (survives baseline noise),
skip Phase B and go to C/D with this substrate. If it is 2–5× better than 1.5 × 10⁻⁴
but still < 10⁻³, the register channel was real but not sufficient: do Phase B. If it
is no better, the register channel was not the main path and Phase B is mandatory.

Effort: a day. All within existing config/tests/scripts machinery.

## Phase B — decouple code from data (execution-model change)

**Hypothesis.** If a corrupted byte can only harm the cell that holds it (and
whatever that cell targets), a structure's error branching factor drops well below 1
at baseline noise, and maintained structures become robust.

**Design: control tokens.** Add `exec_model = token` alongside the current
`neighbourhood` model.

* A cell executes only its **own** byte, and only in ticks when it holds a **token**.
  After executing, the token moves to the neighbour in the cell's `ip` direction
  (`ip` becomes "outgoing direction", 0–7; `MoveIp(d)` sets it). A cell with no token
  does nothing and pays nothing; a token on a dead cell is destroyed; a token arriving
  at a cell that already holds one is destroyed (tokens don't stack). This is the
  plan's "execution flows spatially" taken literally.
* Token supply: every cell has probability `token_rate` per tick of spontaneously
  gaining a token when it has none (sunlight-like), so random regions still execute
  and dead space gets probed; a loop of cells keeps circulating its own token. So a
  *program* is a closed path of cells, executed once per lap; cost is per lap, which
  changes the economics (repair becomes much cheaper per tick).
* `reg` travels with the token (accumulator model), so `Load(d)` in one cell and
  `Repair(d′)` in the next cell on the path compose into a copy. Otherwise a one-byte
  cell can never both load and write.
* `Repair` copies `instr` and `ip` (the path is part of the genome, otherwise noise
  can't break loops and the structure is trivially safe). Noise should then also hit
  `ip` with the same rate, or the result isn't comparable.
* Cell needs a token flag: fold it into a bit of `ip` (bit 7) rather than a new
  field, keep `Cell` at 6 bytes, bump the snapshot version.

**Designed structure.** A double loop: two parallel closed paths of length L whose
cells alternate `Load(E)` and `Repair(S)`-style copies so each strand restores the
other from its own copy, one cell per lap. Work out the exact bytes on paper first;
prove fixed-point-ness with a test as for the tiling.

**Experiments.** Same trio as Phase A (random init, ramp, half-life vs noise), plus
the copy-self and register variants under the token model — copy-self may stop being
a soup here, because a `Repair` byte no longer gets executed by eight neighbours.

**Gate.** Designed structure vitality ≥ 10⁻³ ⇒ proceed to D. Otherwise stop and
rethink the ISA (this would mean single-byte cells can't carry enough redundancy).

Effort: two to three days including tests (determinism, energy conservation, snapshot
round-trip, token conservation).

## Phase C — measure maintenance directly (do in parallel with A/B)

Three additions to `metrics.rs`/`plot.py`, all cheap, all needed before D:

1. **Perturbation probe (`--probe`).** Every `probe_every` ticks, for each tracked
   organism above `probe_min_size`, overwrite `probe_k` random core bytes and record
   the fraction restored (byte equal to pre-perturbation) after 1, 2, 5 windows; do
   the same for equal-sized background regions matched in x. *Restoration* is the
   direct operationalisation of "actively maintains its encoding"; MI persistence is a
   proxy for it. Deterministic given the seed. Report `restoration` alongside
   `persistence` in frame rows and life records.
2. **Null-twin baseline.** For each configuration, also run the same seed with
   `repair_source = none` (Repair costs but writes nothing). The background stability
   distribution of the null twin, binned by x (energy niche) and region size, is the
   proper baseline for MI persistence: `persistence_null = MI_R / MI_twin(x, size)`.
   The current same-world baseline is confounded by the soup and by freezing.
3. **Half-life curves.** Fixed-noise runs at 5 levels for designed structures and for
   the whole grid's SCC-core census; fit exponential decay; plot half-life vs noise on
   log axes. Vitality becomes a slope, not a single ramp crossing.

Also: raise baseline noise for emergence runs to 3 × 10⁻³ and add a `--noise` axis to
the plots. At 10⁻³ an unmaintained byte lives ~1 000 ticks, longer than most SCCs, so
nothing selects for maintenance; at 3 × 10⁻³ the frozen background problem largely
disappears.

## Phase D — emergence search (only after A or B passes its gate)

A substrate that can host robust structures does not necessarily produce them. Two
ingredients are needed and both are measurable:

1. **Turnover.** Death and noise must clear space faster than unmaintained structure
   accumulates. Sweep noise ∈ {10⁻³, 3 × 10⁻³, 10⁻²} × sun ∈ {2, 3, 4} × the passing
   substrate(s), 20 seeds × 200k ticks. Report restoration-positive regions per frame
   (probe from C1), not SCC counts.
2. **Reproduction.** Does the designed structure *spread*? Seed one copy at the bright
   edge under baseline noise and measure occupied area vs tick, and whether copies
   detach (separate SCCs with Jaccard 0 to the parent and > 0.8 byte similarity).
   Under template repair, a structure that repairs into dead space with its own
   template *is* reproducing; the tiling-with-edges "conveyor" was exactly this,
   pointed the wrong way. If it spreads, run the competition experiment: seed two
   variants differing in one byte and record which occupies more area after 100k
   ticks over 20 seeds. That is the first selection measurement.

**Gate for calling something "alive" here:** a region with restoration > 0.8 (vs
background < 0.2), persistence_null > 3, that spreads, and that was not seeded — i.e.
emerged from random init — in at least 1 of 20 seeds. Anything less is reported as
what it is.

## Phase E — engineering as needed

* `rayon` over cells in the decide phase if the half-life and emergence sweeps exceed
  ~2 hours (write conflicts and energy transfers are already resolved in a separate
  apply pass, so the decide pass parallelises).
* Snapshot v2 when `Cell` changes; keep `analyze` byte-identical to online metrics.
* Keep the determinism test and the energy-conservation test green for every
  execution model; add token conservation for Phase B.

## Deprioritised (from the plan's §8 and from our results)

* Radius 2: only adds directions to whichever replicator exists. Revisit after D.
* `tag`: still unused by metrics; leave it.
* `MoveIp` trap fix: keep on, no effect on its own.
* Energy scarcity alone: pushes the soup up the gradient, does not force cooperation.
  Revisit once there is something to cooperate.

## Order and rough cost

| step | what | compute | gate |
|---|---|---|---|
| A | pass-through repair + alternating tiling + half-life curves | ~1 h of sweeps | vitality ≥ 10⁻³ ⇒ skip B |
| C1–C3 | probe, null-twin, half-life plots | ~1 h | — |
| B | token execution model + double-loop structure | ~2 h of sweeps | vitality ≥ 10⁻³ ⇒ D, else stop and rethink ISA |
| D | turnover × substrate sweep; reproduction; competition | ~6 h | emergence criterion above |
| E | as needed | — | — |
