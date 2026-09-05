#!/usr/bin/env bash
# Follow-up protocol (results/README.md "What to change next"): template repair, the
# no-self-jump rule, energy scarcity, and their combination. Same grid, ticks and
# metrics as scripts/sweep.sh; the original baseline is symlinked in for comparison.
#
# Environment overrides: SEEDS (default 0..20), TICKS (100000), OUT (results/variants),
# JOBS (nproc), ONLY (subset of: register previous notrap scarce combined combined_prev).
set -euo pipefail
cd "$(dirname "$0")/.."

SEEDS=${SEEDS:-0..20}
TICKS=${TICKS:-100000}
OUT=${OUT:-results/variants}
JOBS=${JOBS:-$(nproc)}
ONLY=${ONLY:-"register previous notrap scarce combined combined_prev"}

cargo build --release --quiet
BIN=target/release/autopoiesis
mkdir -p "$OUT"
# Reference: the plan's original substrate, from the first protocol.
if [[ -d results/baseline && ! -e "$OUT/baseline" ]]; then ln -s ../baseline "$OUT/baseline"; fi

run() { # name, extra args...
  local name=$1; shift
  if [[ " $ONLY " != *" $name "* ]]; then return; fi
  echo "=== $name: $* (seeds $SEEDS, $TICKS ticks, $JOBS jobs)"
  "$BIN" --ticks "$TICKS" "$@" sweep --seeds "$SEEDS" --out "$OUT/$name" --jobs "$JOBS"
}

# 1. Template repair: Repair writes `reg` (maintenance = Load/Repair loop) …
run register --repair-source register
#    … or the byte of the neighbourhood slot executed just before the Repair (§8).
run previous --repair-source previous

# 2. Break the MoveIp self-loop trap.
run notrap --no-self-jump

# 3. Energy scarcity: brightest sun (3) below the Repair cost (4).
run scarce --sun 3

# 4. All three together, with each template variant.
run combined --repair-source register --no-self-jump --sun 3
run combined_prev --repair-source previous --no-self-jump --sun 3

python3 scripts/plot.py "$OUT"
