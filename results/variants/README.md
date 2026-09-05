# §8 follow-up: template repair, no-self-jump, energy scarcity

Same grid, ticks and metrics as the first protocol (128×128, 100 000 ticks, noise 0.001,
linear sun 0..4 unless stated); 20 seeds per experiment, the original 50-seed `baseline`
as reference. Produced by `scripts/sweep_variants.sh`; figures and tables in `plots/`.

| experiment | flags | what it tests |
|---|---|---|
| `register` | `--repair-source register` | `Repair(d)` writes `reg`, so maintenance must be a `Load`/`Repair` loop |
| `previous` | `--repair-source previous` | `Repair(d)` writes the byte of the neighbourhood slot executed just before it (the plan's §8 wording) |
| `notrap` | `--no-self-jump` | a jump back onto the slot being fetched from advances instead (kills the `MoveIp` self-loop trap) |
| `scarce` | `--sun 3` | brightest sun below the Repair cost (4) |
| `combined` | register + no-self-jump + sun 3 | all three |
| `combined_prev` | previous + no-self-jump + sun 3 | all three, other template rule |
| `tiling_ramp` | register + no-self-jump + `--seed-tiling`, noise 0 → 0.002 | a hand-designed template-repairing structure under a noise ramp |
| `tiling_copyself` | `--seed-tiling`, same ramp, plan's repair | control |

## Headline

**Template repair removes the repair soup completely, and nothing replaces it.** With
`Repair` writing a loaded template instead of the repairer's own byte, `Repair` stops
being a replicator: SCC cores fall from 54 % of the grid (baseline) to 0.6 %
(`register`) or 0.05 % (`previous`), the largest SCC ever seen falls from 13 000 cells
to 951 / 78, and the number of organisms with persistence > 3 over a whole window
falls from 25 629 (50 seeds) to **zero** (20 seeds each). The background freezes
instead: 72–77 % of bytes are unchanged over a 100-tick lag versus 24 % in baseline,
because almost nothing writes any more (repairs per tick drop 10–65×).

The trap fix on its own changes nothing that matters (`notrap` ≈ baseline), and
scarcity only thins the soup (43 % of the grid instead of 54 %) while *raising* the
number of small frontier SCCs, because a poorer bright side has more turnover. The
combination behaves like `register` with a slightly larger, still empty, repair graph.

A hand-designed structure that *is* self-maintaining under register repair (columns
alternating `Repair(S)`/`Load(N)`, each cell rewriting its south neighbour from its
north neighbour's byte) is an exact fixed point without noise and cannot be expressed
under copy-self at all — but its measured **vitality is 1.5 × 10⁻⁴** (median noise rate at which the
last column dissolves under a ramp), about seven times below the baseline noise of
10⁻³; at 10⁻³ it is gone within ~1 000 ticks. The reason is the execution model, not
the repair rule: a mutated byte is *executed as an instruction* by its eight neighbours
before the next repair lands, and about half of all random bytes corrupt the executor's
register or write somewhere wrong, so each error seeds roughly one more.

## Per-experiment results

Numbers from `plots/summary.md`; "persistent" means persistence > 3.

| experiment | SCCs / frame | core cells / frame (of 16 384) | max SCC | long-lived organisms (≥ 1 window) | with maxP > 3 | frac frames with a persistent SCC | bg stability | repairs / tick | deaths / tick |
|---|---|---|---|---|---|---|---|---|---|
| baseline | 124.8 | 8 823 | 13 063 | 2 044 350 (50 seeds) | 25 629 | 0.204 | 0.24 | 5 670 | 958 |
| register | 16.8 | 94 | 951 | 146 007 | 0 | 0.000 | 0.72 | 517 | 130 |
| previous | 2.5 | 9 | 78 | 15 729 | 0 | 0.000 | 0.77 | 86 | 121 |
| notrap | 104.1 | 9 548 | 13 103 | 673 486 | 4 381 | 0.101 | 0.22 | 5 870 | 981 |
| scarce | 145.2 | 7 032 | 11 429 | 941 147 | 15 384 | 0.269 | 0.21 | 3 820 | 1 100 |
| combined | 35.4 | 263 | 5 455 | 315 398 | 4 | 0.000 | 0.59 | 800 | 195 |
| combined_prev | 3.2 | 12 | 66 | 16 832 | 4 | 0.000 | 0.67 | 69 | 172 |

### Template repair (`register`, `previous`)

* No soup. Cores hold 94 cells (`register`) or 9 (`previous`) on average; the largest
  SCC ever is 951 / 78 cells versus 13 063. Repairs per tick drop from 5 670 to 517 / 86:
  `Repair` bytes are now just 1/16 of random bytes instead of the dominant species.
* No persistent organisms at all. Long-lived SCCs have median size 4 / 3 cells, median
  lifetime one window, maximum 4 200 / 1 160 ticks; the best single-frame persistence
  is 3.8 (a 3-cell SCC, stability 0.20 against a background of 0.33). Persistence
  distributions sit at 0.5–1.0 (`plots/scc_vs_tick.png`, bottom).
* The background freezes: stability 0.72–0.77 over the lag. Deaths fall 7× because the
  soup's Repair spending is gone. Where SCCs do appear they sit on the *dark* side
  (`previous`: 68 % of core cells in the darkest quarter; `plots/localisation.png`):
  that is where cells still die and get random bytes, so random `Repair` bytes are
  briefly executed there. Under the plan's rule the bright side was where life-like
  structure was; under template repair the dark side is the only place anything
  happens, and what happens is noise.
* `register` writes `reg = 0` (Nop) until a cell has executed a `Load`; the
  `Repair(d)`-heavy neighbourhoods that used to grow now erase themselves.
  `previous` writes the byte executed one slot earlier, which is a `Repair` only when
  two `Repair` bytes sit in consecutive slots, so it replicates weakly along runs and
  produces the fewest SCCs of all.

### No-self-jump (`notrap`)

Statistically indistinguishable from baseline: 104 vs 125 SCCs per frame (within the
seed spread), 9 548 vs 8 823 core cells, same localisation, same persistence
distribution, same lifetime distribution, deaths and repairs within 3 %. The trap was
real (it froze whole columns of the seeded ring) but it is not what drives the soup;
the soup is driven by copy-self `Repair`.

### Scarcity (`scarce`, sun 3 < Repair cost 4)

The soup survives but thins: 7 032 core cells (43 % of the grid) versus 8 823, largest
SCC 11 429. It moves *toward* the bright edge (40 % of core cells in the brightest
quarter versus 33 %; size-weighted mean x 0.70 versus 0.65): the band that can afford a
`Repair`-heavy program narrows. Small frontier SCCs become more numerous (145 per
frame versus 125) and more of them pass persistence 3 (15 384 long-lived organisms vs
25 629 in 2.5× the seeds — a similar rate), because a poorer bright side has more
turnover; deaths rise 15 %. Scarcity alone does not force cooperation (`Absorb`/`Share`
to pay for repair); it just pushes the soup up the gradient.

### Combined

`combined` = `register` with a bit more churn (35 SCCs per frame, 263 core cells,
largest 5 455 — a transient at low sun where dying cells are re-randomised often),
4 long-lived organisms with maxP > 3 out of 315 398, best row persistence 4.8 on a
5-cell SCC. `combined_prev` = `previous`. Neither changes the conclusion.

### Hand-designed template structure (`tiling_ramp`, `tiling_copyself`)

Eight columns at the bright edge (x = 120–127), rows alternating `Repair(S)` /
`Load(N)`, registers pre-loaded; sun 4 so the `Load(N)` cells (3 energy per tick) are
affordable; noise ramped 0 → 0.002 over the run so that its dissolution point is a
vitality in the plan's sense. Each column is a closed torus loop in which every cell
rewrites its south neighbour from its north neighbour's byte, so the repair graph shows
eight 128-cell SCCs.

| | `tiling_ramp` (register + no-self-jump) | `tiling_copyself` (plan's repair) |
|---|---|---|
| column loops alive at t = 100 / 500 / 1000 / 2000 / 3000 / 5000 / 8000 (of 8, mean over 20 seeds) | 6.3 / 4.2 / 3.4 / 2.0 / 1.5 / 0.5 / 0 | 0.1 / 0 / 0 / 0 / 0.1 / 0 / 0 |
| tick of the last surviving column loop, median (min–max) | 7 500 (800–90 600) | none survive the first sweep |
| noise rate at that tick = **vitality**, median (min–max) | **1.5 × 10⁻⁴** (2 × 10⁻⁵ – 1.8 × 10⁻³) | — |
| byte stability of a living loop over a 100-tick lag | 0.84–0.88 | — |
| background stability at the same time | 0.89–0.92 | 0.23–0.32 |
| persistence of a living loop | 0.5 (1.2 in the first window) | 0.3–0.5 (soup) |

* Under the plan's repair the seed is destroyed within the first neighbourhood sweep:
  copy-self `Repair(S)` cells overwrite the `Load(N)` rows with `Repair(S)`, the band
  becomes ordinary soup (max size 11 444, one tracked piece), and the run is
  indistinguishable from `seeded` in the first protocol.
* Under register repair the structure does what it was designed to do — while a column
  is alive, 84–88 % of its bytes survive a 100-tick lag against a design that rewrites
  every one of them 3–6 times per sweep — but it erodes at roughly one column per
  1 000 ticks even while the ramp is still below 10⁻⁴, and the last column goes at a
  median noise of 1.5 × 10⁻⁴: **its vitality is about seven times below the baseline
  noise rate of 10⁻³.** Two things erode it. The band's outer columns have three
  background slots in their programs, so foreign bytes enter through the edges even at
  zero noise (the zero-noise control keeps ~96 % of the interior but loses the edges).
  And inside, a single mutated byte is executed as an instruction by its eight
  neighbours before the next repair lands; the ~half of random bytes that are `Load`,
  `Cmp`, `Store`, a wrong-direction `Repair` or a jump corrupt the executor's register
  or ip, and the next `Repair(S)` writes that corruption into the column below.
* The persistence metric, as specified, does not reward it: its MI is real (stability
  0.85 while the background changes… 10 %), but the register-mode background is *also*
  frozen — 90 % of untouched bytes survive the lag because nothing writes any more —
  so the ratio sits at 0.5. Only in the first window, before the background settles,
  does it read 1.2. The metric can only see maintenance against a decorrelating
  background, and at the noise where this background decorrelates the structure is
  long gone.

## What this says about the substrate

1. **The soup was the only thing the plan's substrate produced, and it was an artefact
   of copy-self `Repair`.** Remove the replicator and the repair graph is empty: no
   structure of any size mutually maintains itself for a window, in 20 × 100 000 ticks
   at 128×128, under either template rule.
2. **Template repair is expressive enough**: the hand-designed tiling is an exact
   fixed point under register repair (tested), so heterogeneous self-maintenance is
   possible in principle where copy-self made it impossible.
3. **But the neighbourhood-as-program execution model makes maintained structure
   fragile in a specific, quantifiable way**: every byte is *code* for its eight
   neighbours, executed once per 9-tick sweep by each, so a corrupted byte does
   damage ~1–3 times before repair (3–6 repairs per sweep) can land, and about half of
   the 256 bytes corrupt a register or write somewhere wrong. That is a branching
   factor near one; the tiling's vitality of 1.5 × 10⁻⁴ is where it crosses one.
4. Frozen background is the other half of the problem: with almost nothing writing,
   72–77 % of the background is unchanged over a window, so nothing short of a noise
   ramp can separate "maintained" from "untouched". At the noise levels where the
   background does decorrelate, the maintained structure decorrelates too.

## Suggested next change

Decouple code from data. Either (a) execute only the cell's *own* byte and let `MoveIp`
pass a control token between cells (the plan's original picture), so a corrupted byte
harms only the cell that holds it and whoever it targets; or (b) keep the neighbourhood
program but make `Load`/`Repair` operate on a second per-cell byte (a *genome* field
separate from `instr`) that is never executed. (a) needs no new state; (b) is the plan's
§7 "don't add a genome" line crossed deliberately, with the reason above.
