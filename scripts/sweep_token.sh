#!/usr/bin/env bash
# Round 3, phase B (docs/continuation-plan.md): the token execution model. Writes into
# the same root as scripts/sweep_round3.sh so the plots compare against `null`,
# `baseline` and the neighbourhood-model runs.
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

run() { # name seeds ticks extra args...
  local name=$1 seeds=$2 ticks=$3; shift 3
  if [[ "$ONLY" != all && " $ONLY " != *" $name "* ]]; then return; fi
  echo "=== $name: $* (seeds $seeds, $ticks ticks, $JOBS jobs)"
  "$BIN" --ticks "$ticks" "$@" sweep --seeds "$seeds" --out "$OUT/$name" --jobs "$JOBS"
}

# Cells only spend while they hold a token, so the sun is scaled down (1.0 at the bright
# edge, mean 0.5) to keep starvation and hence turnover in play.
TOK="--exec-model token --sun 1.0"
STRIP="$TOK --repair-source opposite --seed-tiling --seed-tiling-pattern pass-through"

# Random init under each repair rule, and the null twin of this world.
run tok_copyself "$SEEDS" "$TICKS" $TOK --repair-source copy-self
run tok_register "$SEEDS" "$TICKS" $TOK --repair-source register
run tok_opposite "$SEEDS" "$TICKS" $TOK --repair-source opposite
run tok_null     "$SEEDS" "$TICKS" $TOK --repair-source none

# The designed strip structure under a noise ramp, and its half-life at fixed noise.
run tok_strip_ramp "$SEEDS" "$TICKS" $STRIP --noise-ramp "0.0:0.005:$TICKS"
for nz in 0.0001 0.0003 0.001 0.003 0.01; do
  run "hl_tok_$nz" "$HL_SEEDS" "$HL_TICKS" $STRIP --noise "$nz"
done

# Perturbation probes.
run probe_tok       "$HL_SEEDS" "$TICKS" $TOK --repair-source opposite --probe-every 1000 --probe-min-size 3
run probe_tok_strip "$HL_SEEDS" "$TICKS" $STRIP --noise 0.0003 --probe-every 1000 --probe-min-size 100

python3 scripts/plot.py "$OUT"
