# hgt

A sandbox for **horizontal gene transfer between running programs**. A population of
software nodes faces a stressor that shifts under them; a node survives a tick only if
one of the genes it holds *computes* the right answer. Genes are executable bytecode and
they move between nodes over a network — by conjugation, by uptake of the DNA of the
dead, and by phages — so a trait can appear in a lineage that never inherited it.

The point is not that a population evolves. It is that the thing which saves it arrives
sideways, and can be measured doing so.

```
cargo run --release -p hgt -- --seed 1 --ticks 3000 --hgt none     # watch it die
cargo run --release -p hgt -- --seed 1 --ticks 3000                # watch it not
cargo run --release -p hgt -- --render                             # watch it happen
cargo run --release -p hgt -- --config hgt/configs/search.json     # watch it find a gene
cargo run --release -p hgt -- arena --processes 4 --base-port 9000  # over real sockets
./hgt/scripts/demo.sh                                              # every experiment
cargo test -p hgt                                                  # 56 tests
```

This crate is independent of the `autopoiesis` simulation in the repository root; it
shares its conventions — a run described by `(config, seed)`, JSONL metrics, `run` /
`analyze` / `sweep` — and none of its code.

## Layout

```
src/
  config.rs     HgtConfig — every tunable, documented defaults, JSON-loadable
  isa.rs        the gene instruction set: 15 ops, one byte each
  vm.rs         the interpreter genes run in: accumulator machine, hard step budget
  gene.rs       genes, provenance, genomes, mutation
  hazard.rs     the stressor schedule, and the compiler that writes a resistance gene
  node.rs       a node: metabolism, fission, the restriction barrier, policies
  protocol.rs   the wire format: Hello, Offer, Request, Transfer, Eulogy, Phage, Reject
  transport.rs  trait Transport, and the deterministic in-process implementation
  tcp.rs        the same envelopes over sockets, between processes
  world.rs      the tick loop and the three transfer mechanisms
  event.rs      the event stream — the ground truth everything is derived from
  metrics.rs    allele frequency, incongruence, fixation, the barrier as a rate
  render.rs     terminal view, coloured by how each node got what it holds
  main.rs       CLI: run, node, arena, analyze, sweep
tests/          isa_roundtrip, vm_sandbox, hgt_mechanisms, evolution, determinism, transport
scripts/        demo.sh (every experiment), plot.py (tables and figures)
configs/        search.json — the regime a gene can be *found* in
examples/       ab.rs (the A/B on stdout), gene.rs (disassemble a resistance gene)
```

## Genes are programs

A gene is up to 64 bytes of code. **Low nibble = opcode, high nibble = a 0..15 operand**,
one byte per instruction, so every byte string is a valid program: a mutation is a small
edit rather than a frame shift, and a truncated transfer is a shorter gene rather than
garbage.

| op | effect | | op | effect |
|---|---|---|---|---|
| `Nop` | nothing | | `XorAux` | `acc ^= aux` |
| `Imm(n)` | `acc = n` | | `AddAux` | `acc += aux` |
| `Shl(n)` | `acc <<= n` | | `Swap` | `swap(acc, aux)` |
| `Or(n)` | `acc \|= n` | | `Payload` | `acc = the stressor's payload` |
| `Xor(n)` | `acc ^= n` | | `Kind` | `acc = which stressor it is` |
| `Rotl(n)` | `acc = acc.rotate_left(n)` | | `JmpIfZero(n)` | if `acc == 0`, `pc = n` |
| `Add(n)` | `acc += n` | | `Emit` | answer with `acc`, stop |
| | | | `Halt` | stop |

Genes arrive from other machines and most are mutated copies of something that once
worked, so the interpreter is written to make *any* byte string safe to be handed: every
byte decodes, arithmetic wraps, jumps off the end halt, and a step budget bounds the run.
There is no way for a received gene to hang, panic, or reach anything outside four
`u32`s. A hundred thousand random byte strings are run in `tests/vm_sandbox.rs` to say so.

A gene's identity is the FNV-1a hash of its code. "The same gene" is therefore a fact
about the bytes, allele frequency needs no registry, and a mutated copy is honestly a
*different* allele rather than a damaged one.

## The stressor, and what answers it

Each tick the environment poses `(kind, payload)`. The correct response is
`rotate_left(payload ^ key_k, rot_k)`, with `(key_k, rot_k)` derived from the run seed.
The schedule is a pure function of `(seed, tick)`, so every node derives it independently
and there is no environment server to be the secret centre of a decentralised network.

Resistance is a program. `cargo run -p hgt --example gene` disassembles one:

```
stressor 0: answer = rotl(payload ^ 0xff3c3a95, 6)
  20 bytes  i<|<|<|<|<|<|<|sPX@E
  Imm(15) Shl(4) Or(15) Shl(4) Or(3) Shl(4) Or(12) Shl(4) Or(3) Shl(4) Or(10)
  Shl(4) Or(9) Shl(4) Or(5) Swap Payload XorAux Rotl(6) Emit
```

Fourteen of those twenty bytes are the key, built a nibble at a time — which is why one
mutated byte in the wrong place destroys the gene, and why most mutants are junk.

## The tick

**upkeep → face the stressor → network → death → fission.**

A node pays to stay alive and pays again per gene it carries, then runs its genes against
the stressor — most recently useful first — paying for each trial and stopping early on an
exact answer. Above a threshold a node divides, and the child inherits the genome with
point mutations and occasional plasmid loss. At zero energy it dies, and at the population
ceiling a birth displaces a node chosen at random.

**Credit** is what the best gene scored. An exact answer is 1; a near miss earns
`hazard_gradient` times its score above chance (`hazard.rs`); silence earns nothing.
Energy gained and damage taken are split by it. At `hazard_gradient = 0` the stressor is
answered exactly or not at all and a working gene can only ever be inherited or received;
above 0 the key becomes a gradient a lineage can climb, and genes can be **found**.

Two rules keep this from degenerating, and both were forced by the experiment rather than
designed in:

* **Genomes are bounded.** Every mutated copy in flight is a new gene; with no ceiling a
  population accumulates every copy that ever passed through it and starves under the
  upkeep. Taking on a new gene evicts the most recently acquired one that has never
  answered anything.
* **Only genes known to work are offered** (`offer_unproven` turns this off, so the
  regime it prevents can be measured rather than asserted). A gene carries whether it has
  been seen to work — seeded, or having answered a stressor, or inherited or received
  unchanged from a copy for which one of those was true. Junk therefore dies with its
  host, while a spare gene that is useless *this* epoch still circulates freely. Deleting
  long-disused genes was tried first and is exactly wrong: it removes the reservoir the
  population needs at the next shift.
* **A birth at the ceiling displaces someone.** A population pinned at `max_nodes` with no
  turnover cannot be selected on, however good a gene is. The displaced node is chosen at
  random — evicting the *weakest* was tried and is worse, because the weakest node is
  usually one that has just divided and handed half its energy away, so it selects against
  reproducing at all.
* **Mutation flips one bit**, rather than replacing a byte. Replacing changes the opcode
  fifteen times in sixteen, so nearly every mutation is fatal and no lineage can walk
  anywhere; a bit flip lands in the operand half the time and leaves the instruction doing
  the same thing to a slightly different value.

## How genes move

Each mechanism is independently switchable (`--hgt none|all|conj,transf,transd`).

* **Conjugation** — a three-message handshake: `Offer` (everything mobile I hold),
  `Request` (the one I lack), `Transfer` (the code). A donor cannot push code into a node
  that did not ask for it.
* **Transformation** — a dying node broadcasts its mobile genes to its peers, which hold
  them as fragments for `fragment_ttl` ticks and take them up at a rate. Free DNA comes
  from the dead, which is why it is the weakest mechanism here: the dead are largely
  those who lacked the gene worth having.
* **Transduction** — a phage packet needs no handshake and no consent. It infects,
  sometimes lyses, repackages whatever the new host holds, and hops on — which is how a
  gene reaches a lineage that would never have asked for it.

A **restriction barrier** grades every arrival by strain distance (differing bits in the
strain label), so transfer is a rate set by relatedness rather than a certainty. On top of
it, `crispr_rate` gives nodes an **acquired immune memory**: surviving a phage can teach a
node to cut that gene on sight, and the memory is inherited. It cannot tell a parasite
from a gene the node will need, which is the point — protection has a price, and the price
is measured below.

A **policy** (`always_accept`, `selfish`, `thrifty`) decides what a node offers and
accepts. It is a heritable trait of a node, not a setting of the run: children inherit it,
`policy_drift` mutates it, and `selfish_founders` drops free riders into a population of
donors to see what happens. Dying is not a choice, so a selfish node's genes still leak
when it starves.

`recombination_rate` pairs transfer with **recombination**: an accepted gene is spliced
into a resident copy at a crossover point, so what is integrated is a program neither node
had. Without it a gene is taken whole or not at all, and every improvement has to be walked
to by one lineage alone.

The network itself is a scenario knob: `partition_at` splits it in two and
`partition_heal_at` puts it back. Which side a node is on is a pure function of its id, so
nodes born during a partition need nobody to assign them.

## Two ways to run it

`hgt run` keeps the whole population in one process on `SimTransport`, which models
latency, loss and cuttable links and is deterministic given the seed. This is where every
number comes from; `tests/determinism.rs` holds it to that.

`hgt arena` spawns one process per **deme** and routes envelopes between them over real
TCP. Ownership is arithmetic — node ids are allocated in stripes, so `to / 2^24` names the
owning process — which means no registry, no coordinator, and no handshake to move a gene
across a machine boundary. A frame is one line of JSON, so a run can be watched with `nc`.
This path is not deterministic and is not meant to be: two processes interleave however
the kernel decides. It exists to show that the transfer is network traffic and not a
metaphor.

## Metrics

Everything is derived from the event stream — `Birth`, `Acquire`, `Lose`, `Death`,
`Transfer`, `Tick` — and nothing else. The analyzer reconstructs each genome from those
records rather than reading the world, which is why `hgt run --metrics` and
`hgt analyze events.jsonl` are the same computation and produce byte-identical output
(`tests/determinism.rs`). It derives which genes are resistance genes itself, because the
stressor schedule is a function of `(config, seed)` and it has both.

* **Incongruence** — the share of a gene's carriers that received it sideways rather than
  inheriting it. Identically zero with no mechanism on, and the reason a gene tree and a
  family tree disagree: the signature that told biologists lateral transfer was happening.
* **Allele frequency** per stressor over time — the sweep curves.
* **Acquisitions** split by mechanism, cumulative.
* **The barrier as a rate** — attempts and acceptances by strain distance, with redundant
  attempts (the recipient already had it) counted separately, since phages retry
  indiscriminately and would otherwise bury the signal.
* **Epoch records** — population through each shift, how common the answering gene was
  when the stressor arrived, and the ticks from the shift to it sweeping.
* **Answerers** per stressor — carriers of *any* gene that answers it, by function rather
  than by gene id, so a variant discovered mid-run counts.
* **Discoveries** — a mutated copy that answers a stressor its parent could not. A variant
  of a gene the parent already had is not a discovery, however different its bytes.
* **Policy composition**, and **the two sides** of a partition with the frequency distance
  between their gene pools.
* **The two graphs.** `--trees DIR` writes the family tree (`ancestry.tsv`,
  `ancestry.newick`) and the transfer graph (`transfers.tsv`). With no mechanism on, the
  second file is empty: a gene's history is a subtree of the first. Every row in it is a
  place where the two disagree.

## Results

`./hgt/scripts/demo.sh` — 8 seeds, 3000 ticks (10 epochs) unless noted, defaults
otherwise. Full tables in `results/demo/summary.md`.

**Does a population survive stressors it was not born ready for?**

| transfer | survived | epochs survived | freq at shift | lateral share |
|---|---|---|---|---|
| none | 0/8 | 2 | 0.00 | 0.000 |
| conjugation | 4/8 | 6 | 0.21 | 0.047 |
| transformation | 0/8 | 2 | 0.00 | 0.018 |
| transduction | 8/8 | 10 | 0.46 | 0.053 |
| all three | 8/8 | 10 | 0.55 | 0.057 |

Without transfer every seed is extinct in the second epoch, once the founders' descendants
run out of stressors they were born ready for. The column that explains the rest is **freq
at shift**: how common the answering gene already was *when the stressor arrived*.
Transfer does not rescue a population after the crisis; it distributes the gene before
there is one. Transformation alone never manages it — free DNA comes from the dead, and
the dead are mostly those who lacked the gene worth having.

**The restriction barrier**, as an acceptance rate over attempts that were not already
redundant:

| strain distance | conjugation | transformation | transduction |
|---|---|---|---|
| 0 | 0.231 | 0.943 | 0.284 |
| 1 | 0.213 | 0.630 | 0.197 |
| 2 | 0.120 | 0.259 | 0.113 |

**Can a gene be found rather than received?** `configs/search.json`: one stressor that
never shifts, an income a population can search on, and founders a known number of bit
flips from a gene that works. 16 seeds, 10000 ticks.

| distance | no transfer | transfer, proven genes only | transfer, unproven shared |
|---|---|---|---|
| 4 bits | 7/16 found, tick 3300 | 12/16, 1320 | 16/16, 1430 |
| 8 bits | 9/16, 3600 | 10/16, 2700 | 13/16, 3280 |
| 12 bits | 3/16, 6620 | 6/16, 3540 | 8/16, 3970 |
| 16 bits | 0/16 | 0/16 | 0/16 |

Yes, up to a horizon — and transfer helps, but only once it is allowed to move things that
do not work yet. A node offers a gene when it has seen it work, so a half-finished answer
is nobody's to give; `--offer-unproven` lifts that, and it finds the gene in every seed at
4 bits and in half again as many at 12. What it does *not* do is keep it: the last column
of the full table is nodes still able to answer at the end of the run, and sharing junk
along with the half-answers leaves 0-2 of them against 6-8 when only proven genes move.
Sharing everything explores; sharing what works consolidates.

**What is the horizon made of?** Two different things, and only one of them is a slope.

A gene whose key is `d` bits wrong emits an answer `d` bits wrong and scores `(16-d)/16`
— that is arithmetic, and `tests/hazard.rs` asserts it. At `d = 16` the answer is at
chance and the slope is gone, which is exactly where the search stops working. The other
half is the rotation, and it is flat: every wrong rotation scrambles the whole answer, so
no key is worth more than any other until the rotation is right.

| founders start | found it | median tick |
|---|---|---|
| rotation 1 bit wrong, key correct | 12/16 | 200 |
| rotation 1 bit wrong, key 4 bits wrong | 12/16 | 1390 |
| rotation 2 bits wrong, key correct | 0/16 | never |

One bit of rotation error is a single lucky flip away and takes 200 ticks. Two bits is a
valley — the intermediate state pays nothing, so nothing selects for crossing it — and in
sixteen seeds and ten thousand ticks nobody ever crosses it. The population is not
searching a hard problem badly; it is searching a smooth problem that has a cliff nested
inside it, which is a fair description of most real fitness landscapes.

**Does splicing genes on transfer help?** `recombination_rate` splices an arriving gene
into a resident copy at a crossover point, so what gets integrated is a program neither
node had. It is the only way two lineages' partial answers can combine.

| distance | shared, no splicing | shared, splicing at 0.3 |
|---|---|---|
| 4 bits | 16/16 found, 2 answerers at the end | 15/16, 5 |
| 8 bits | 13/16, 0 | 11/16, 12 |
| 12 bits | 8/16, 2 | 6/16, 30 |
| 16 bits | 0/16 | 0/16 |

Not as a search. Splicing finds the gene in slightly fewer seeds — it breaks as many
half-answers as it completes — and it does nothing at all beyond the horizon, where there
is no gradient for a hybrid to be better *at*. What it changes is what happens afterwards:
where an answer is found, five to fifteen times as many nodes still hold one at the end of
the run. Recombination is not a better way of finding things. It is a ratchet: partial
copies keep reconstituting the answer, so the population stops losing it.

**Do free riders take over?** Eight of forty-eight founders start `selfish` — they accept
genes and offer none — and policy is inherited.

| run | survived | free riders at start | free riders at end | transfers |
|---|---|---|---|---|
| inherited only | 8/8 | 0.167 | 0.626 | 8610 |
| with 2% drift | 8/8 | 0.167 | 0.823 | 8615 |

They do. A free rider pays none of conjugation's cost and takes everything a phage or a
corpse offers, so its share rises from a sixth to two thirds — and the population survives
anyway, because transduction and transformation need no donor's consent. A world with only
conjugation would not be so lucky.

**What does an immune system buy, and cost?**

| crispr_rate | phage kills | immune refusals | transduced acquisitions |
|---|---|---|---|
| 0.0 | 739 | 0 | 32991 |
| 0.5 | 159 | 316115 | 22320 |
| 1.0 | 112 | 375959 | 18815 |

Phage kills fall by a factor of six. The price is a third of the population's transduced
gene flow: an immune memory cannot tell a parasite from a gene you are going to need.

**What does cutting the network cost?** One founder per future stressor, split from tick 0.

| network | survived | min population | worst side's answerers | peak divergence |
|---|---|---|---|---|
| whole | 8/8 | 48 | 22 | 0.141 |
| cut at 0 | 2/8 | 46 | 0 | 0.165 |
| cut, healed at 400 | 2/8 | 48 | 0 | 0.140 |

Six of eight populations die when the network is split, because the gene that answers the
next stressor is on one side of the cut and the stressor is on both. Healing at tick 400
does not help: the first shift is at 300, and by then it has happened.

**What if nodes offer genes nobody has seen work?**

| offering | survived | genes per node | distinct genes |
|---|---|---|---|
| proven only | 8/8 | 4.94 | 197 |
| everything | 7/8 | 4.96 | 239 |

Milder than it used to be, and for a reason worth stating: the genome cap now absorbs most
of the damage that this rule was introduced to prevent. What is left is a wider junk pool
(239 distinct genes against 197) and one seed lost.

**Over real sockets**, four processes, 600 ticks, only process 0 founded with the genes
for the later stressors:

```
process 0: 60 nodes, 50 acquisitions (13 from another process), 8061 envelopes sent
process 1: 60 nodes, 46 acquisitions (18 from another process), 8551 envelopes sent
process 2: 60 nodes, 46 acquisitions (15 from another process), 8400 envelopes sent
process 3:  0 nodes, 46 acquisitions (17 from another process), 4222 envelopes sent
180 nodes alive; 63 genes crossed a socket
```

The same command with `--hgt none`: every process extinct, nothing crossed. Processes 1-3
began with no spare genes at all, so everything that kept them alive arrived as bytes on a
TCP connection — and process 3 went under anyway, which is what a partition of one looks
like when the genes arrive too late.

## Acceptance tests

* every byte decodes and re-encodes canonically; each op has a distinct glyph
  (`tests/isa_roundtrip.rs`)
* 100k random genes terminate inside their budget without panicking, and a gene's answer
  never depends on what ran before it (`tests/vm_sandbox.rs`)
* nothing is acquired laterally with no mechanism on; conjugation puts a gene in nodes
  that never inherited it; each mechanism works alone and only its own acquisitions
  appear; a phage delivers genes it was never sent with; a selfish node never conjugates
  but its death still leaks its genes; the barrier is graded by strain distance
  (`tests/hgt_mechanisms.rs`)
* one seed is one run — world hash, event log and metrics all identical; measuring live
  and re-deriving from the log agree exactly; a lossy network is still repeatable
  (`tests/determinism.rs`)
* a population climbs to a gene nobody gave it, and cannot on a flat landscape; policy is
  inherited and drifts; an immune memory cuts phages and costs the genes they carried;
  splicing produces programs neither side had; the transfer graph is empty without a
  mechanism and every edge in it joins the family tree sideways (`tests/evolution.rs`)
* the landscape is the shape the results claim it is: credit falls exactly a sixteenth per
  wrong key bit and reaches chance at sixteen, and no wrong rotation earns credit at any
  key distance (`src/hazard.rs`)
* a split network bottlenecks harder than a whole one and reports itself split
  (`tests/transport.rs`)
* a gene crosses a real socket unchanged; a deme founded with no spare genes survives on
  what arrives over the wire, and dies without it (`tests/transport.rs`, `src/tcp.rs`)

## CLI

```
hgt [--seed N] [--ticks N] [--config cfg.json]
    [--hgt none|all|conj,transf,transd] [--policy always_accept|selfish|thrifty]
    [--nodes N --max-nodes N --degree D] [--epoch-ticks N --hazard-kinds K]
    [--restriction R] [--latency T --loss P] [--mutation-rate M --max-genes G]
    [--render [--show resistance|energy|strain] [--render-every N] [--fps F]]
    [--metrics FILE] [--events FILE] [--analysis-every N --report-every N]
    [--progress N] [--dump-config]
hgt [...] analyze EVENTS.jsonl [--out FILE]    # same --seed and --config as the run
hgt [...] sweep --seeds 0..50 --out DIR [--jobs J]
hgt [...] node --index I --listen ADDR --peers A,B,C [--tick-ms MS]
hgt [...] arena [--processes P] [--base-port PORT] [--tick-ms MS] [--out DIR]
```

`--dump-config` prints the effective `HgtConfig`; any subset of its fields can be given
with `--config`.

## Things deliberately not done

No fitness function beyond surviving the stressor, and no reward for anything a node does
to another. Recombination is one crossover point and nothing else: no double crossover, no
homology search for where the two copies line up, and nothing is ever inserted or deleted,
so every gene is exactly as long as the one it came from. No genuine sandboxing claim beyond the interpreter's own limits: the
VM is safe because it is tiny, not because it is hardened. No attempt to make the TCP path
deterministic, and no global observer over it — each process measures its own deme,
because in a real distributed system that is all anyone has. The renderer has never been
run in this repository's CI, which has no terminal; its painting functions are tested, the
live view is not.
