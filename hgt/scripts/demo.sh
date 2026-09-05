#!/usr/bin/env bash
# The experiment, end to end. Runs the A/B in one process, then the same thing across
# real TCP connections between separate node processes.
#
#   SEEDS=0..8 TICKS=3000 ./hgt/scripts/demo.sh          # override anything below
#   ONLY=ab ./hgt/scripts/demo.sh                        # just one section
set -euo pipefail
cd "$(dirname "$0")/../.."

SEEDS=${SEEDS:-0..8}
TICKS=${TICKS:-3000}
OUT=${OUT:-hgt/results/demo}
ONLY=${ONLY:-}
PROCESSES=${PROCESSES:-4}
BASE_PORT=${BASE_PORT:-9400}

cargo build --release --quiet -p hgt
BIN=target/release/hgt

run() {
  local name=$1
  shift
  if [ -n "$ONLY" ] && [ "$ONLY" != "$name" ]; then return 0; fi
  echo "=== $name: $* ==="
  "$@"
}

# 1. Does a population survive a stressor it was not born ready for, and does that
#    depend on being able to trade genes?
run ab bash -c "
  for m in none conj transf transd all; do
    $BIN --ticks $TICKS --hgt \$m sweep --seeds $SEEDS --out $OUT/\$m 2>&1 | sed \"s/^/  \$m: /\"
  done
"

# 2. Where did the genes that saved them come from — inheritance or the network?
run summary python3 hgt/scripts/plot.py "$OUT"

# 3. The same thing over sockets: only process 0 is founded with the genes for the later
#    stressors, so anything the others have was received from another process.
run arena "$BIN" --ticks 600 --nodes 12 --max-nodes 60 --epoch-ticks 120 \
  arena --processes "$PROCESSES" --base-port "$BASE_PORT" --tick-ms 2 --out "$OUT/arena"

# 4. The control: cut the transfer mechanisms and the same processes die.
run arena-none "$BIN" --ticks 600 --nodes 12 --max-nodes 60 --epoch-ticks 120 --hgt none \
  arena --processes "$PROCESSES" --base-port "$((BASE_PORT + 10))" --tick-ms 2 --out "$OUT/arena_none"
