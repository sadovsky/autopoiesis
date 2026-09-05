# Sweep summary

| experiment | seeds | frames | mean SCCs/frame | mean core cells | max SCC | organisms created (total) | organisms lived >= 1 window | frac frames maxP>3 | mean persistent SCCs/frame | mean persistent cells/frame | long-lived organisms with maxP>3 | mean parasites/frame | mean bg stability |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| baseline | 50 | 50050 | 124.8 | 8823 | 13063 | 12981759 | 2044350 | 0.204 | 0.49 | 3.4 | 25629 | 138.6 | 0.24 |
| ramp2 | 20 | 20020 | 73.5 | 12056 | 13865 | 4112348 | 273799 | 0.576 | 2.38 | 9.8 | 27397 | 247.9 | 0.05 |
| ramp4 | 20 | 20020 | 96.4 | 10996 | 12951 | 5288533 | 377520 | 0.612 | 2.74 | 11.3 | 32614 | 247.6 | 0.04 |
| seeded | 20 | 20020 | 125.3 | 8861 | 12661 | 5204773 | 823223 | 0.214 | 0.51 | 3.8 | 10522 | 141.1 | 0.24 |
| uniform | 20 | 20020 | 5.0 | 15446 | 16384 | 231770 | 16818 | 0.011 | 0.02 | 0.1 | 194 | 115.9 | 0.11 |

## Vitality (noise ramp experiments)

| experiment | organisms died | survived to end | vitality median | vitality p90 | max | median of organisms with max_size>=10 |
|---|---|---|---|---|---|---|
| ramp2 | 273732 | 67 | 0.0233 | 0.0449 | 0.0500 | 0.0218 |
| ramp4 | 377449 | 71 | 0.0185 | 0.0438 | 0.0500 | 0.0159 |

## Extinction under the noise ramp

Per seed: the noise rate at the last frame that still had an SCC of the given size. Median (min–max) over seeds.

| experiment | extinction noise, any SCC (>= min_size) | extinction noise, SCC >= 10 cells | extinction noise, SCC >= 100 cells | noise where mean core cells first < 1% of grid |
|---|---|---|---|---|
| ramp2 | 0.0500 (0.0500–0.0500) | 0.0500 (0.0500–0.0500) | 0.0500 (0.0500–0.0500) | never |
| ramp4 | 0.0500 (0.0500–0.0500) | 0.0500 (0.0500–0.0500) | 0.0500 (0.0500–0.0500) | never |

## Lifetimes (organisms that lived >= report_min_life = one window)

| experiment | organisms | median lifetime (ticks) | p90 | max | still alive at end | max size ever |
|---|---|---|---|---|---|---|
| baseline | 2044350 | 100 | 180 | 17380 | 423 | 13063 |
| ramp2 | 273799 | 100 | 160 | 99940 | 67 | 13961 |
| ramp4 | 377520 | 100 | 160 | 99940 | 71 | 13153 |
| seeded | 823223 | 100 | 180 | 15020 | 193 | 12764 |
| uniform | 16818 | 120 | 180 | 99940 | 28 | 16384 |

## Localisation along the sun gradient (x)

Core-cell density: fraction of all SCC-core cells (over all frames) per x band. Centroid columns use per-organism mean x, weighted by core size; older outputs fall back to the anchor.

| experiment | core cells in darkest quarter | second | third | brightest quarter | size-weighted mean x of organisms | mean x of small organisms (<20 cells) |
|---|---|---|---|---|---|---|
| baseline | 0.037 | 0.224 | 0.405 | 0.334 | 0.654 | 0.431 |
| ramp2 | 0.101 | 0.312 | 0.311 | 0.276 | 0.571 | 0.369 |
| ramp4 | 0.064 | 0.296 | 0.338 | 0.303 | 0.605 | 0.382 |
| seeded | 0.037 | 0.222 | 0.404 | 0.337 | 0.655 | 0.435 |
| uniform | 0.250 | 0.250 | 0.250 | 0.250 | 0.496 | 0.497 |

## Run totals (mean per seed)

| experiment | executed | repairs | deaths | starved | mutations | elapsed s |
|---|---|---|---|---|---|---|
| baseline | 1.55e+09 | 5.67e+08 | 9.58e+07 | 3.35e+07 | 1.64e+06 | 82.3 |
| ramp2 | 1.54e+09 | 3.07e+08 | 5.05e+07 | 8.4e+06 | 4.1e+07 | 69.7 |
| ramp4 | 1.54e+09 | 2.46e+08 | 5.81e+07 | 1.36e+07 | 4.1e+07 | 77.7 |
| seeded | 1.55e+09 | 5.67e+08 | 9.59e+07 | 3.35e+07 | 1.64e+06 | 81.1 |
| uniform | 1.57e+09 | 5.45e+08 | 1.02e+08 | 6.56e+07 | 1.64e+06 | 48.8 |
