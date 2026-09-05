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
cargo run --release -p hgt -- arena --processes 4 --base-port 9000  # over real sockets
./hgt/scripts/demo.sh                                              # the whole experiment
cargo test -p hgt                                                  # 47 tests
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
tests/          isa_roundtrip, vm_sandbox, hgt_mechanisms, determinism, transport
scripts/        demo.sh (the experiment), plot.py (tables and figures)
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
the stressor — most recently useful first — until one answers, paying for each trial. An
answer earns energy; no answer costs `damage`. Above a threshold a node divides, and the
child inherits the genome with per-byte mutation and occasional plasmid loss. At zero
energy it dies.

Two rules keep this from degenerating, and both were forced by the experiment rather than
designed in:

* **Genomes are bounded.** Every mutated copy in flight is a new gene; with no ceiling a
  population accumulates every copy that ever passed through it and starves under the
  upkeep. Taking on a new gene evicts the most recently acquired one that has never
  answered anything.
* **Only genes known to work are offered.** A gene carries whether it has been seen to
  work — seeded, or having answered a stressor, or inherited or received unchanged from a
  copy for which one of those was true. Junk therefore dies with its host, while a spare
  gene that is useless *this* epoch still circulates freely. Deleting long-disused genes
  was tried first and is exactly wrong: it removes the reservoir the population needs at
  the next shift.

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
strain label), so transfer is a rate set by relatedness rather than a certainty. A
**policy** (`always_accept`, `selfish`, `thrifty`) decides what a node offers and accepts;
dying is not a choice, so a selfish node's genes still leak when it starves.

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

## Results

`./hgt/scripts/demo.sh` — 8 seeds, 3000 ticks (10 epochs), defaults otherwise. Full
tables in `results/demo/summary.md`.

| transfer | survived | epochs survived | freq at shift | lateral share |
|---|---|---|---|---|
| none | 0/8 | 2 | 0.01 | 0.000 |
| conjugation | 8/8 | 10 | 0.94 | 0.093 |
| transformation | 4/8 | 6 | 0.69 | 0.010 |
| transduction | 8/8 | 10 | 0.96 | 0.107 |
| all three | 8/8 | 10 | 0.97 | 0.108 |

Without transfer every seed is extinct by tick 614 — the second shift, once the founders'
descendants run out of stressors they were born ready for. The interesting column is
**freq at shift**: with transfer the answering gene is already in 94-97% of the population
*when the stressor arrives*. Transfer does not rescue the population after the crisis; it
distributes the gene before there is one. Transformation is the weak mechanism for the
reason given above, and shows it: half the seeds die.

The restriction barrier, over attempts that were not already redundant:

| strain distance | conjugation | transformation | transduction |
|---|---|---|---|
| 0 | 0.097 | 0.692 | 0.516 |
| 1 | 0.040 | 0.482 | 0.391 |
| 2 | 0.018 | 0.179 | 0.176 |

Over real sockets, four processes, 600 ticks, only process 0 founded with the genes for
the later stressors:

```
process 0: 60 nodes, 47 acquisitions (14 from another process), 10195 envelopes sent
process 1: 60 nodes, 64 acquisitions (30 from another process),  9626 envelopes sent
process 2: 60 nodes, 63 acquisitions (21 from another process),  9539 envelopes sent
process 3: 60 nodes, 54 acquisitions (18 from another process),  9014 envelopes sent
240 nodes alive; 83 genes crossed a socket
```

The same command with `--hgt none`: every process extinct, nothing crossed. Processes 1-3
began with no spare genes at all, so everything that kept them alive arrived as bytes on
a TCP connection.

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
to another. No recombination inside a gene — transfer moves whole genes, mutation is
substitution only. No genuine sandboxing claim beyond the interpreter's own limits: the
VM is safe because it is tiny, not because it is hardened. No attempt to make the TCP path
deterministic, and no global observer over it — each process measures its own deme,
because in a real distributed system that is all anyone has.
