# hgt results

`demo/` is what `hgt/scripts/demo.sh` writes: one directory per section — `ab/` (does
transfer keep a population alive), `search/` (can a gene be found), `policy/` (do free
riders take over), `immunity/`, `partition/`, `unproven/` — and inside each, one directory
per run holding the effective `config.json` and a `summary.jsonl` with one line per seed.
The per-seed metric streams (`seed_*.jsonl`) and the arena's per-process streams
(`node_*.jsonl`) are large and untracked — re-run the script to regenerate them.

`demo/trees/` is one run's family tree (`ancestry.tsv`, `ancestry.newick`) and transfer
graph (`transfers.tsv`): every row of the latter is a place where a gene's history departs
from the tree.

`demo/summary.md` is the tables, written by `hgt/scripts/plot.py`; it also draws
frequency-curve figures if matplotlib is installed. The headline numbers are quoted in
`hgt/README.md` under "Results".

`demo/arena/` and `demo/arena_none/` hold the per-process configs of the TCP runs: only
process 0 is founded with the genes for the later stressors, so anything the other
processes end up holding arrived over a socket.
