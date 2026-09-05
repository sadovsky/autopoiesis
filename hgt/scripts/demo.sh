#!/usr/bin/env bash
# The experiments, end to end. Each section is a question; the tables that answer them
# are written by plot.py into $OUT/summary.md.
#
#   ./hgt/scripts/demo.sh                    # everything
#   ONLY=discovery ./hgt/scripts/demo.sh     # one section
#   SEEDS=0..16 TICKS=6000 ./hgt/scripts/demo.sh
set -euo pipefail
cd "$(dirname "$0")/../.."

SEEDS=${SEEDS:-0..8}
TICKS=${TICKS:-3000}
OUT=${OUT:-hgt/results/demo}
ONLY=${ONLY:-}
PROCESSES=${PROCESSES:-4}
BASE_PORT=${BASE_PORT:-9400}
SEARCH_TICKS=${SEARCH_TICKS:-10000}

cargo build --release --quiet -p hgt
BIN=target/release/hgt

run() {
  local name=$1
  shift
  if [ -n "$ONLY" ] && [ "$ONLY" != "$name" ]; then return 0; fi
  echo "=== $name ==="
  "$@"
}

# 1. Does a population survive stressors it was not born ready for, and does that depend
#    on being able to trade genes?
run ab bash -c "
  set -euo pipefail
  for m in none conj transf transd all; do
    $BIN --ticks $TICKS --hgt \$m sweep --seeds $SEEDS --out $OUT/ab/\$m 2>&1 | sed \"s/^/  \$m: /\"
  done
"

# 2. Can a gene be *found* rather than received? Founders start a known number of bit
#    flips from a gene that works, with one stressor that never shifts.
run discovery bash -c "
  set -euo pipefail
  for bits in 4 8 12 16; do
    for m in none all; do
      $BIN --config hgt/configs/search.json --ticks $SEARCH_TICKS --founder-miss-bits \$bits \
           --hgt \$m sweep --seeds $SEEDS --out $OUT/search/bits\${bits}_\$m 2>&1 \
        | sed \"s/^/  \$bits bits, \$m: /\"
    done
  done
"

# 3. Does a free rider — take genes, offer none — take over a population of donors?
run policy bash -c "
  set -euo pipefail
  $BIN --ticks $TICKS --selfish-founders 8 sweep --seeds $SEEDS --out $OUT/policy/invasion 2>&1 | sed 's/^/  invasion: /'
  $BIN --ticks $TICKS --selfish-founders 8 --policy-drift 0.02 sweep --seeds $SEEDS --out $OUT/policy/drift 2>&1 | sed 's/^/  drift: /'
"

# 4. What does an immune system buy, and what does it cost?
run immunity bash -c "
  set -euo pipefail
  for r in 0.0 0.5 1.0; do
    $BIN --ticks $TICKS --crispr-rate \$r sweep --seeds $SEEDS --out $OUT/immunity/crispr\$r 2>&1 | sed \"s/^/  crispr \$r: /\"
  done
"

# 5. What does cutting the network in two cost the population?
run partition bash -c "
  set -euo pipefail
  $BIN --ticks $TICKS --founder-carriers 1 sweep --seeds $SEEDS --out $OUT/partition/whole 2>&1 | sed 's/^/  whole: /'
  $BIN --ticks $TICKS --founder-carriers 1 --partition-at 0 sweep --seeds $SEEDS --out $OUT/partition/cut 2>&1 | sed 's/^/  cut: /'
  $BIN --ticks $TICKS --founder-carriers 1 --partition-at 0 --partition-heal-at 400 sweep --seeds $SEEDS --out $OUT/partition/healed 2>&1 | sed 's/^/  healed: /'
"

# 6. What happens if nodes offer genes nobody has seen work — junk included?
run unproven bash -c "
  set -euo pipefail
  $BIN --ticks $TICKS sweep --seeds $SEEDS --out $OUT/unproven/proven_only 2>&1 | sed 's/^/  proven only: /'
  $BIN --ticks $TICKS --offer-unproven sweep --seeds $SEEDS --out $OUT/unproven/everything 2>&1 | sed 's/^/  everything: /'
"

# 7. The two graphs, for one run: who descended from whom, and who gave what to whom.
run trees "$BIN" --ticks "$TICKS" --seed 1 --trees "$OUT/trees" --metrics /dev/null

# 8. The tables.
run summary python3 hgt/scripts/plot.py "$OUT"

# 9. The same thing over sockets: only process 0 is founded with the genes for the later
#    stressors, so anything the others have was received from another process.
run arena "$BIN" --ticks 600 --nodes 12 --max-nodes 60 --epoch-ticks 120 \
  arena --processes "$PROCESSES" --base-port "$BASE_PORT" --tick-ms 2 --out "$OUT/arena"

# 10. The control: cut the transfer mechanisms and the same processes die.
run arena-none "$BIN" --ticks 600 --nodes 12 --max-nodes 60 --epoch-ticks 120 --hgt none \
  arena --processes "$PROCESSES" --base-port "$((BASE_PORT + 10))" --tick-ms 2 --out "$OUT/arena_none"
