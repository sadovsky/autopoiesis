#!/usr/bin/env bash
# Round 3 (docs/continuation-plan.md, phases A and C): pass-through repair, the
# alternating Repair(S)/Repair(N) tiling, half-life curves at fixed noise, the null
# twin, and perturbation probes. Same grid and metrics as the earlier protocols.
#
# Environment overrides: SEEDS (0..20), HL_SEEDS (0..10), TICKS (100000), HL_TICKS
# (20000), OUT (results/round3), JOBS (nproc), ONLY (subset of experiment names).
set -euo pipefail
cd "$(dirname "$0")/.."

SEEDS=${SEEDS:-0..20}
HL_SEEDS=${HL_SEEDS:-0..10}
TICKS=${TICKS:-100000}
HL_TICKS=${HL_TICKS:-20000}
OUT=${OUT:-results/round3}
JOBS=${JOBS:-$(nproc)}
ONLY=${ONLY:-all}

cargo build --release --quiet
BIN=target/release/autopoiesis
mkdir -p "$OUT"
[[ -d results/baseline && ! -e "$OUT/baseline" ]] && ln -s ../baseline "$OUT/baseline"
[[ -d results/variants/register && ! -e "$OUT/register" ]] && ln -s ../variants/register "$OUT/register"

run() { # name seeds ticks extra args...
  local name=$1 seeds=$2 ticks=$3; shift 3
  if [[ "$ONLY" != all && " $ONLY " != *" $name "* ]]; then return; fi
  echo "=== $name: $* (seeds $seeds, $ticks ticks, $JOBS jobs)"
  "$BIN" --ticks "$ticks" "$@" sweep --seeds "$seeds" --out "$OUT/$name" --jobs "$JOBS"
}

PT="--repair-source opposite --no-self-jump"
PTT="$PT --seed-tiling --seed-tiling-pattern pass-through"
REGT="--repair-source register --no-self-jump --seed-tiling --seed-tiling-pattern register"

# Null twin: the same world with Repair disabled (costs, writes nothing). Baseline for
# stability at every energy niche; shared by every experiment at noise 0.001 / sun 4.
run null "$SEEDS" "$TICKS" --repair-source none

# A. Pass-through repair from random init: does any soup come back?
run pt_random "$SEEDS" "$TICKS" $PT

# A. The alternating tiling under a noise ramp (vitality).
run pt_tiling_ramp "$SEEDS" "$TICKS" $PTT --noise-ramp "0.0:0.005:$TICKS"

# A/C3. Half-life curves: fixed noise levels, both template tilings.
for nz in 0.0001 0.0003 0.001 0.003 0.01; do
  run "hl_pt_$nz" "$HL_SEEDS" "$HL_TICKS" $PTT --noise "$nz"
  run "hl_reg_$nz" "$HL_SEEDS" "$HL_TICKS" $REGT --noise "$nz"
done

# C1. Perturbation probes (perturb the run, so separate from the clean runs).
run probe_baseline "$HL_SEEDS" "$TICKS" --probe-every 1000
run probe_pt "$HL_SEEDS" "$TICKS" $PT --probe-every 1000
run probe_register "$HL_SEEDS" "$TICKS" --repair-source register --no-self-jump --probe-every 1000
run probe_pt_tiling "$HL_SEEDS" "$TICKS" $PTT --noise 0.0003 --probe-every 1000 --probe-min-size 100

python3 scripts/plot.py "$OUT"
