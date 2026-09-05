#!/usr/bin/env bash
# Experiment protocol (plan §6). Writes one directory per experiment under $OUT with a
# metrics JSONL per seed, then plots everything with scripts/plot.py.
#
# Environment overrides:
#   SEEDS   seed range, half-open (default 0..50)
#   TICKS   ticks per run          (default 100000)
#   OUT     output directory       (default results)
#   JOBS    parallel runs          (default: nproc)
#   ONLY    space-separated subset of: baseline seeded ramp4 ramp2 uniform
set -euo pipefail
cd "$(dirname "$0")/.."

SEEDS=${SEEDS:-0..50}
TICKS=${TICKS:-100000}
OUT=${OUT:-results}
JOBS=${JOBS:-$(nproc)}
ONLY=${ONLY:-"baseline seeded ramp4 ramp2 uniform"}

cargo build --release --quiet
BIN=target/release/autopoiesis
mkdir -p "$OUT"

run() { # name, extra args...
  local name=$1; shift
  if [[ " $ONLY " != *" $name "* ]]; then return; fi
  echo "=== $name: $* (seeds $SEEDS, $TICKS ticks, $JOBS jobs)"
  "$BIN" --ticks "$TICKS" "$@" sweep --seeds "$SEEDS" --out "$OUT/$name" --jobs "$JOBS"
}

# 1. Baseline: 128x128, noise 0.001, linear sun gradient (0..4 across x).
#    Does anything with persistence > 3 ever emerge from random init?
run baseline

# 2. Seeded: a hand-written repairing band (3 columns of Repair(S)) at the brightest
#    column at t=0. Does it survive, grow, get parasitized?
run seeded --seed-ring --seed-ring-width 3

# 3. Noise sweep: ramp 0 -> 0.05 over the run; vitality distribution, and does it
#    shift when Repair costs 2 instead of 4?
run ramp4 --noise-ramp "0.0:0.05:$TICKS"
run ramp2 --noise-ramp "0.0:0.05:$TICKS" --repair-cost 2

# 4. Gradient off: uniform sun with the same total energy as the linear ramp (mean 2).
#    Do organisms still localize?
run uniform --sun-profile uniform --sun 2

python3 scripts/plot.py "$OUT"
