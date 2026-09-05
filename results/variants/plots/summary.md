# Sweep summary

| experiment | seeds | frames | mean SCCs/frame | mean core cells | max SCC | organisms created (total) | organisms lived >= 1 window | frac frames maxP>3 | mean persistent SCCs/frame | mean persistent cells/frame | long-lived organisms with maxP>3 | mean parasites/frame | mean bg stability |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| baseline | 50 | 50050 | 124.8 | 8823 | 13063 | 12981759 | 2044350 | 0.204 | 0.49 | 3.4 | 25629 | 138.6 | 0.24 |
| combined | 20 | 20020 | 35.4 | 263 | 5455 | 870791 | 315398 | 0.000 | 0.00 | 0.0 | 4 | 14.6 | 0.59 |
| combined_prev | 20 | 20020 | 3.2 | 12 | 66 | 122896 | 16832 | 0.000 | 0.00 | 0.0 | 4 | 3.4 | 0.67 |
| notrap | 20 | 20020 | 104.1 | 9548 | 13103 | 4346061 | 673486 | 0.101 | 0.20 | 1.9 | 4381 | 120.9 | 0.22 |
| previous | 20 | 20020 | 2.5 | 9 | 78 | 90728 | 15729 | 0.000 | 0.00 | 0.0 | 0 | 2.6 | 0.77 |
| register | 20 | 20020 | 16.8 | 94 | 951 | 353594 | 146007 | 0.000 | 0.00 | 0.0 | 0 | 9.9 | 0.72 |
| scarce | 20 | 20020 | 145.2 | 7032 | 11429 | 6049159 | 941147 | 0.269 | 0.71 | 5.2 | 15384 | 140.5 | 0.21 |
| tiling_copyself | 20 | 20020 | 123.6 | 8606 | 12843 | 5091253 | 814408 | 0.178 | 0.41 | 2.9 | 8576 | 137.2 | 0.25 |
| tiling_ramp | 20 | 20020 | 40.7 | 355 | 7456 | 950580 | 359566 | 0.000 | 0.00 | 0.0 | 9 | 15.6 | 0.60 |

## Vitality (noise ramp experiments)

| experiment | organisms died | survived to end | vitality median | vitality p90 | max | median of organisms with max_size>=10 |
|---|---|---|---|---|---|---|
| tiling_copyself | 814279 | 129 | 0.0010 | 0.0018 | 0.0020 | 0.0009 |
| tiling_ramp | 359384 | 182 | 0.0012 | 0.0018 | 0.0020 | 0.0011 |

## Extinction under the noise ramp

Per seed: the noise rate at the last frame that still had an SCC of the given size. Median (min–max) over seeds.

| experiment | extinction noise, any SCC (>= min_size) | extinction noise, SCC >= 10 cells | extinction noise, SCC >= 100 cells | noise where mean core cells first < 1% of grid |
|---|---|---|---|---|
| tiling_copyself | 0.0020 (0.0020–0.0020) | 0.0020 (0.0020–0.0020) | 0.0020 (0.0020–0.0020) | never |
| tiling_ramp | 0.0020 (0.0020–0.0020) | 0.0020 (0.0020–0.0020) | 0.0020 (0.0019–0.0020) | never |

## Lifetimes (organisms that lived >= report_min_life = one window)

| experiment | organisms | median lifetime (ticks) | p90 | max | still alive at end | max size ever |
|---|---|---|---|---|---|---|
| baseline | 2044350 | 100 | 180 | 17380 | 423 | 13063 |
| combined | 315398 | 100 | 260 | 2560 | 234 | 5467 |
| combined_prev | 16832 | 100 | 120 | 1280 | 0 | 90 |
| notrap | 673486 | 100 | 160 | 20780 | 89 | 13103 |
| previous | 15729 | 100 | 120 | 1160 | 1 | 78 |
| register | 146007 | 100 | 300 | 4200 | 120 | 1311 |
| scarce | 941147 | 100 | 180 | 9480 | 222 | 11429 |
| tiling_copyself | 814408 | 100 | 180 | 21120 | 129 | 13265 |
| tiling_ramp | 359566 | 120 | 280 | 7520 | 182 | 7634 |

## Localisation along the sun gradient (x)

Core-cell density: fraction of all SCC-core cells (over all frames) per x band. Centroid columns use per-organism mean x, weighted by core size; older outputs fall back to the anchor.

| experiment | core cells in darkest quarter | second | third | brightest quarter | size-weighted mean x of organisms | mean x of small organisms (<20 cells) |
|---|---|---|---|---|---|---|
| baseline | 0.037 | 0.224 | 0.405 | 0.334 | 0.654 | 0.431 |
| combined | 0.047 | 0.225 | 0.433 | 0.295 | 0.625 | 0.564 |
| combined_prev | 0.573 | 0.281 | 0.067 | 0.079 | 0.293 | 0.284 |
| notrap | 0.035 | 0.251 | 0.378 | 0.336 | 0.645 | 0.309 |
| previous | 0.678 | 0.121 | 0.093 | 0.108 | 0.287 | 0.278 |
| register | 0.128 | 0.233 | 0.365 | 0.273 | 0.576 | 0.541 |
| scarce | 0.022 | 0.154 | 0.423 | 0.401 | 0.701 | 0.513 |
| tiling_copyself | 0.037 | 0.228 | 0.403 | 0.332 | 0.652 | 0.426 |
| tiling_ramp | 0.064 | 0.267 | 0.365 | 0.304 | 0.623 | 0.541 |

## Run totals (mean per seed)

| experiment | executed | repairs | deaths | starved | mutations | elapsed s |
|---|---|---|---|---|---|---|
| baseline | 1.55e+09 | 5.67e+08 | 9.58e+07 | 3.35e+07 | 1.64e+06 | 82.3 |
| combined | 1.59e+09 | 8e+07 | 1.95e+07 | 3.54e+06 | 1.64e+06 | 60.7 |
| combined_prev | 1.59e+09 | 6.85e+06 | 1.72e+07 | 2.52e+06 | 1.64e+06 | 50.4 |
| notrap | 1.55e+09 | 5.87e+08 | 9.81e+07 | 3.42e+07 | 1.64e+06 | 94.3 |
| previous | 1.6e+09 | 8.56e+06 | 1.21e+07 | 1.5e+06 | 1.64e+06 | 53.2 |
| register | 1.6e+09 | 5.17e+07 | 1.3e+07 | 1.83e+06 | 1.64e+06 | 56.5 |
| scarce | 1.54e+09 | 3.82e+08 | 1.1e+08 | 3.59e+07 | 1.64e+06 | 100.6 |
| tiling_copyself | 1.55e+09 | 5.65e+08 | 9.54e+07 | 3.32e+07 | 1.64e+06 | 95.7 |
| tiling_ramp | 1.6e+09 | 1.04e+08 | 1.54e+07 | 2.95e+06 | 1.64e+06 | 64.5 |
