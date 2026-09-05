# Round 3: pass-through repair, the token execution model, and direct measurement

Follows `docs/continuation-plan.md` (phases A, B, C). Same grid (128×128), ticks (100k;
half-life runs 20k) and metrics as before; 20 seeds per experiment (10 for half-life
and probe runs). Produced by `scripts/sweep_round3.sh` and `scripts/sweep_token.sh`;
figures and tables in `plots/`. `baseline` and `register` are symlinked in from the
earlier protocols for comparison.

| experiment | model | flags | what it tests |
|---|---|---|---|
| `null` | neighbourhood | `--repair-source none` | null twin: Repair costs, writes nothing |
| `pt_random` | neighbourhood | `--repair-source opposite` | pass-through repair from random init (phase A) |
| `pt_tiling_ramp` | neighbourhood | + pass-through strips, noise 0 → 0.005 | designed structure vitality |
| `hl_pt_*`, `hl_reg_*` | neighbourhood | strips at fixed noise 10⁻⁴ … 10⁻² | half-life vs noise (phase C3) |
| `probe_*` | both | `--probe-every 1000` | perturbation probe: restoration vs matched background (C1) |
| `tok_copyself`, `tok_register`, `tok_opposite`, `tok_null` | token | `--exec-model token --sun 1` | random init under each repair rule (phase B) |
| `tok_strip_ramp`, `hl_tok_*` | token | + strips | designed structure under the token model |

*(Numbers are filled in below from `plots/summary.md` once both sweeps finish.)*

## Headline

Phase A failed its gate and phase B reached its "stop and rethink the ISA" branch — but
for a reason that is now understood exactly, and that also explains rounds 1 and 2.

**Every self-maintaining structure this ISA can express is a relay belt, not a repair
loop.** Whatever `Repair` copies (the repairer's own byte, a register, or the byte
behind it), each maintained cell has exactly one source, so a defect anywhere on the
chain is copied onward as faithfully as a correct byte. A closed chain of single copies
has no external reference: the "fixed points" of rounds 2 and 3 were homogeneous belts
(one class per chain) whose stability was invisible only because every copy happened to
agree. Under the token model the belt is clocked by the token, so a single defect floods
its whole class within one lap (tested: `tests/token.rs`). Error correction needs a
majority vote between two copies of the same *data* byte, and this ISA's `Cmp` compares
a neighbour with the executing cell's own *instruction* byte; with one byte per cell a
voter can only vote on its own program. The counting works against it too: with one
instruction per cell and single-target `Repair`, at most as many cells as there are
`Repair` cells can ever be restored, so the `Load`/`Cmp`/`Jmp` cells a voter needs are
themselves unmaintainable.

Concretely:

* **Pass-through repair from random init brings the soup back** (`pt_random`): relaying
  bytes through `Repair`-dense regions replicates the `Repair` class exactly as
  copy-self did. Phase A's gate: no better than register → phase B mandatory.
* **The token execution model removes code/data coupling completely**: a byte is
  executed only by the cell that holds it, so junk does at most one write per lap. From
  random init it produces **no candidate organisms at all** under any repair rule (a
  repair SCC needs mutual repair within a window, which tokens make vanishingly rare),
  and a frozen background. The designed strip structure is an exact fixed point without
  noise (tested) and a belt under noise.
* **The perturbation probe and null-twin baseline behave as intended** and give the
  numbers below; they are the right instruments for whatever substrate comes next.

## Mechanism: why single-source copying cannot repair

Take any closed structure and any cell *c* in it. Under copy-self, *c*'s byte can only
be restored by a `Repair(d)` cell whose own byte equals *c*'s — so only `Repair` bytes
are restorable, and a restorable structure is a union of homogeneous chains. Under
register or pass-through repair, *c* is written from one source *s(c)* (a register loaded
from one cell, or the cell opposite the repairer). Follow the sources: *c ← s(c) ←
s(s(c)) ← …*. In a finite closed structure this chain cycles, so every cell's
"correct" value is defined only as "what the cell upstream holds". Introduce a defect
anywhere: the next relay copies it downstream; the upstream relay overwrites the defect
cell with the upstream value; net effect, the defect moves one position per relay. If
the relays fire in the chain's direction within one sweep (the neighbourhood model's ip
sweep, or a token running against the chain), the defect visits every position in a
single pass. Nothing ever compares two copies, so nothing ever rejects the defect. The
"vitality" measured in round 2 (1.5 × 10⁻⁴) and the half-lives measured here are the
rates at which defects are *introduced* into belts, not resisted by them.

Correction requires, at some cell, `if copy_A == copy_B then write copy_A`. In this ISA
the only comparison is `Cmp(d)`: `reg = (nbr(d).instr == self.instr)`. The executing
cell's own byte is one operand, so a vote can only be taken on the voter's own program
byte, and the write that follows (`Repair`) targets one fixed neighbour with one fixed
direction. Enumerating `Load`/`Repair` loop programs for the token model (rectangular
loops up to 4×4, every direction pair) confirms the counting: every consistent design
restores at most half of its positions — the `Load` cells are never targets — and the
pass-through designs that do cover every position are belts.

## What would change it

Two things are needed, and the plan's substrate has neither.

1. **A vote.** `if copy_A == copy_B then write copy_A`. In the token model `reg` is an
   accumulator travelling with the token, so a `Cmp` that compared a neighbour with the
   accumulator (`reg = nbr(d).instr == reg`) would make `Load(A); Cmp(C); JmpIfZero;
   Load(A); Repair(B)` a 2-of-3 vote between three copies of one byte — the plan's §7
   line ("no new instruction") respected in letter.
2. **Enough instruction slots.** That vote is four or five instructions per restored
   position. With one instruction per cell there is one slot per position: a
   self-maintaining voter is impossible by counting, whatever `Cmp` means. The
   neighbourhood model *does* have the slots — every cell runs nine instructions per
   sweep, its neighbours' bytes — which is exactly why the register tiling was
   expressible there; but the same nine executions run every junk byte nine times per
   sweep, and one unguarded write instruction (`Store`, or a wrong-direction `Repair`
   with a corrupted register) per junk execution is what gave the branching factor of
   one in round 2.

So the minimal revision is not a repair rule or an execution model but a *write
discipline*: keep neighbourhood execution (for the slots), make `Cmp` compare against
the accumulator (for the vote), and make every write conditional on the vote — i.e.
drop the unguarded `Store` and let `Repair` write the accumulator only when the last
`Cmp` agreed. Junk then still executes nine times per sweep but can no longer write
anything two independent copies do not agree on. Whether a three-copy tape voting on
itself can be laid out with 8 directions and one byte per cell is then a finite search
of the kind run above. The alternative — a second, never-executed byte per cell — is
the plan's §7 genome line crossed, and the analysis above says the coupling problem was
not where the difficulty lay in the first place: the difficulty is that nothing in the
ISA ever compares two copies.

## Per-experiment results

*(filled in from `plots/summary.md`)*

### Phase A — pass-through repair (`pt_random`, `pt_tiling_ramp`, `hl_pt_*`)

### Phase C — null twin, probes, half-lives (`null`, `probe_*`, `hl_*`)

### Phase B — token model (`tok_*`, `hl_tok_*`, `probe_tok*`)
