# Sweep summary

| experiment | seeds | frames | mean SCCs/frame | mean core cells | max SCC | organisms created (total) | organisms lived >= 1 window | frac frames maxP>3 | mean persistent SCCs/frame | mean persistent cells/frame | long-lived organisms with maxP>3 | mean parasites/frame | mean bg stability |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| baseline | 50 | 50050 | 124.8 | 8823 | 13063 | 12981759 | 2044350 | 0.204 | 0.49 | 3.4 | 25629 | 138.6 | 0.24 |
| hl_pt_0.0001 | 10 | 2010 | 75.1 | 10997 | 14329 | 337354 | 41715 | 0.057 | 0.12 | 4.8 | 257 | 121.3 | 0.33 |
| hl_pt_0.0003 | 10 | 2010 | 65.0 | 11620 | 14427 | 301840 | 35933 | 0.056 | 0.12 | 8.4 | 302 | 120.2 | 0.33 |
| hl_pt_0.001 | 10 | 2010 | 54.7 | 12296 | 14095 | 274188 | 26512 | 0.071 | 0.15 | 6.9 | 264 | 143.7 | 0.24 |
| hl_pt_0.003 | 10 | 2010 | 45.6 | 12686 | 13956 | 239896 | 19132 | 0.059 | 0.10 | 1.0 | 186 | 163.6 | 0.15 |
| hl_pt_0.01 | 10 | 2010 | 48.7 | 12644 | 13535 | 268039 | 17959 | 0.177 | 0.30 | 1.4 | 330 | 189.8 | 0.05 |
| hl_reg_0.0001 | 10 | 2010 | 22.3 | 286 | 4416 | 46604 | 17432 | 0.000 | 0.00 | 0.0 | 0 | 8.9 | 0.80 |
| hl_reg_0.0003 | 10 | 2010 | 37.1 | 397 | 4600 | 76592 | 30635 | 0.000 | 0.00 | 0.0 | 0 | 14.4 | 0.69 |
| hl_reg_0.001 | 10 | 2010 | 44.5 | 437 | 8807 | 105907 | 39813 | 0.000 | 0.00 | 0.0 | 0 | 16.4 | 0.58 |
| hl_reg_0.003 | 10 | 2010 | 39.0 | 255 | 1673 | 108988 | 35459 | 0.004 | 0.01 | 0.0 | 15 | 18.1 | 0.49 |
| hl_reg_0.01 | 10 | 2010 | 66.4 | 330 | 1286 | 238945 | 44805 | 0.032 | 0.10 | 1.0 | 162 | 42.3 | 0.22 |
| null | 20 | 20020 | 0.0 | 0 | 0 | 0 | 0 | 0.000 | 0.00 | 0.0 | 0 | 0.0 | 0.81 |
| probe_baseline | 10 | 10010 | 124.0 | 8878 | 13063 | 2587252 | 406773 | 0.202 | 0.49 | 3.3 | 5113 | 139.6 | 0.24 |
| probe_pt | 10 | 10010 | 53.8 | 12398 | 14449 | 1335918 | 127952 | 0.051 | 0.08 | 3.4 | 1046 | 144.3 | 0.24 |
| probe_pt_tiling | 10 | 10010 | 64.3 | 11867 | 14443 | 1493187 | 171620 | 0.060 | 0.15 | 12.8 | 1412 | 131.7 | 0.30 |
| probe_register | 10 | 10010 | 44.0 | 345 | 8670 | 520360 | 199063 | 0.000 | 0.00 | 0.0 | 3 | 16.0 | 0.57 |
| pt_random | 20 | 20020 | 54.2 | 12374 | 14289 | 2696192 | 261341 | 0.060 | 0.10 | 5.0 | 2112 | 143.2 | 0.24 |
| pt_tiling_ramp | 20 | 20020 | 50.9 | 12541 | 14298 | 2569697 | 228866 | 0.063 | 0.10 | 2.4 | 1881 | 157.1 | 0.19 |
| register | 20 | 20020 | 16.8 | 94 | 951 | 353594 | 146007 | 0.000 | 0.00 | 0.0 | 0 | 9.9 | 0.72 |

## Vitality (noise ramp experiments)

| experiment | organisms died | survived to end | vitality median | vitality p90 | max | median of organisms with max_size>=10 |
|---|---|---|---|---|---|---|
| pt_tiling_ramp | 228817 | 49 | 0.0020 | 0.0044 | 0.0050 | 0.0016 |

## Extinction under the noise ramp

Per seed: the noise rate at the last frame that still had an SCC of the given size. Median (min–max) over seeds.

| experiment | extinction noise, any SCC (>= min_size) | extinction noise, SCC >= 10 cells | extinction noise, SCC >= 100 cells | noise where mean core cells first < 1% of grid |
|---|---|---|---|---|
| pt_tiling_ramp | 0.0050 (0.0050–0.0050) | 0.0050 (0.0050–0.0050) | 0.0050 (0.0050–0.0050) | never |

## Lifetimes (organisms that lived >= report_min_life = one window)

| experiment | organisms | median lifetime (ticks) | p90 | max | still alive at end | max size ever |
|---|---|---|---|---|---|---|
| baseline | 2044350 | 100 | 180 | 17380 | 423 | 13063 |
| hl_pt_0.0001 | 41715 | 100 | 200 | 11380 | 64 | 14371 |
| hl_pt_0.0003 | 35933 | 100 | 200 | 18780 | 49 | 14436 |
| hl_pt_0.001 | 26512 | 100 | 180 | 15380 | 34 | 14274 |
| hl_pt_0.003 | 19132 | 100 | 160 | 19840 | 22 | 14027 |
| hl_pt_0.01 | 17959 | 100 | 160 | 19940 | 20 | 13620 |
| hl_reg_0.0001 | 17432 | 120 | 340 | 9100 | 121 | 4416 |
| hl_reg_0.0003 | 30635 | 120 | 340 | 4820 | 167 | 4600 |
| hl_reg_0.001 | 39813 | 120 | 280 | 3460 | 160 | 8807 |
| hl_reg_0.003 | 35459 | 100 | 220 | 1480 | 45 | 1210 |
| hl_reg_0.01 | 44805 | 100 | 160 | 620 | 27 | 1286 |
| probe_baseline | 406773 | 100 | 180 | 22640 | 59 | 13063 |
| probe_pt | 127952 | 100 | 180 | 31580 | 24 | 14457 |
| probe_pt_tiling | 171620 | 100 | 200 | 27340 | 26 | 14548 |
| probe_register | 199063 | 120 | 280 | 3060 | 141 | 8799 |
| pt_random | 261341 | 100 | 180 | 30140 | 89 | 14478 |
| pt_tiling_ramp | 228866 | 100 | 180 | 75660 | 49 | 14387 |
| register | 146007 | 100 | 300 | 4200 | 120 | 1311 |

## Localisation along the sun gradient (x)

Core-cell density: fraction of all SCC-core cells (over all frames) per x band. Centroid columns use per-organism mean x, weighted by core size; older outputs fall back to the anchor.

| experiment | core cells in darkest quarter | second | third | brightest quarter | size-weighted mean x of organisms | mean x of small organisms (<20 cells) |
|---|---|---|---|---|---|---|
| baseline | 0.037 | 0.224 | 0.405 | 0.334 | 0.654 | 0.431 |
| hl_pt_0.0001 | 0.107 | 0.320 | 0.314 | 0.260 | 0.561 | 0.366 |
| hl_pt_0.0003 | 0.104 | 0.317 | 0.317 | 0.262 | 0.562 | 0.361 |
| hl_pt_0.001 | 0.094 | 0.299 | 0.314 | 0.293 | 0.582 | 0.286 |
| hl_pt_0.003 | 0.086 | 0.295 | 0.315 | 0.304 | 0.590 | 0.238 |
| hl_pt_0.01 | 0.081 | 0.296 | 0.315 | 0.309 | 0.594 | 0.236 |
| hl_reg_0.0001 | 0.056 | 0.190 | 0.265 | 0.489 | 0.723 | 0.472 |
| hl_reg_0.0003 | 0.052 | 0.205 | 0.343 | 0.400 | 0.674 | 0.517 |
| hl_reg_0.001 | 0.056 | 0.240 | 0.377 | 0.327 | 0.640 | 0.560 |
| hl_reg_0.003 | 0.114 | 0.307 | 0.295 | 0.283 | 0.591 | 0.550 |
| hl_reg_0.01 | 0.194 | 0.261 | 0.266 | 0.279 | 0.587 | 0.562 |
| probe_baseline | 0.037 | 0.225 | 0.404 | 0.334 | 0.653 | 0.430 |
| probe_pt | 0.095 | 0.300 | 0.316 | 0.289 | 0.581 | 0.294 |
| probe_pt_tiling | 0.103 | 0.307 | 0.312 | 0.278 | 0.570 | 0.323 |
| probe_register | 0.070 | 0.304 | 0.372 | 0.254 | 0.584 | 0.540 |
| pt_random | 0.095 | 0.301 | 0.315 | 0.289 | 0.580 | 0.295 |
| pt_tiling_ramp | 0.089 | 0.297 | 0.315 | 0.299 | 0.586 | 0.265 |
| register | 0.128 | 0.233 | 0.365 | 0.273 | 0.576 | 0.541 |

## Null-twin stability ratio (organism stability / repair-disabled world at same tick and x band)

| experiment | organism-frames | median ratio | frac ratio > 3 | rows with ratio > 3 and size >= 10 | rows with ratio > 3 and size >= 100 |
|---|---|---|---|---|---|
| baseline | 972399 | 0.18 | 0.000 | 0 | 0 |
| hl_pt_0.0001 | 36391 | 0.38 | 0.000 | 0 | 0 |
| hl_pt_0.0003 | 35979 | 0.35 | 0.000 | 0 | 0 |
| hl_pt_0.001 | 35932 | 0.26 | 0.000 | 0 | 0 |
| hl_pt_0.003 | 36203 | 0.18 | 0.000 | 0 | 0 |
| hl_pt_0.01 | 36816 | 0.09 | 0.000 | 0 | 0 |
| hl_reg_0.0001 | 18677 | 0.60 | 0.000 | 0 | 0 |
| hl_reg_0.0003 | 27319 | 0.55 | 0.000 | 0 | 0 |
| hl_reg_0.001 | 32306 | 0.48 | 0.000 | 0 | 0 |
| hl_reg_0.003 | 34515 | 0.45 | 0.000 | 0 | 0 |
| hl_reg_0.01 | 37415 | 0.25 | 0.000 | 0 | 0 |
| probe_baseline | 194570 | 0.18 | 0.000 | 0 | 0 |
| probe_pt | 179819 | 0.26 | 0.000 | 0 | 0 |
| probe_pt_tiling | 179640 | 0.34 | 0.000 | 0 | 0 |
| probe_register | 162591 | 0.46 | 0.000 | 0 | 0 |
| pt_random | 359676 | 0.27 | 0.000 | 0 | 0 |
| pt_tiling_ramp | 361858 | 0.20 | 0.000 | 0 | 0 |
| register | 238766 | 0.61 | 0.000 | 0 | 0 |

## Perturbation probes (fraction of overwritten bytes restored; organism vs matched background)

| experiment | probes | size class | n | restored 1w | 2w | 5w | background 1w | 2w | 5w |
|---|---|---|---|---|---|---|---|---|---|
| probe_baseline | 4924 | 10-99 | 3006 | 0.080 | 0.056 | 0.043 | 0.075 | 0.054 | 0.038 |
| probe_baseline | 4924 | >=100 | 1918 | 0.175 | 0.141 | 0.103 | 0.156 | 0.112 | 0.080 |
| probe_pt | 4640 | 10-99 | 2858 | 0.089 | 0.056 | 0.036 | 0.096 | 0.064 | 0.038 |
| probe_pt | 4640 | >=100 | 1782 | 0.219 | 0.168 | 0.106 | 0.174 | 0.114 | 0.068 |
| probe_pt_tiling | 2158 | >=100 | 2158 | 0.316 | 0.258 | 0.172 | 0.259 | 0.199 | 0.143 |
| probe_register | 2663 | 10-99 | 2409 | 0.269 | 0.201 | 0.116 | 0.279 | 0.216 | 0.119 |
| probe_register | 2663 | >=100 | 254 | 0.288 | 0.203 | 0.101 | 0.306 | 0.224 | 0.123 |

## Half-life of the seeded structure vs noise (ticks until half the column loops are gone)

| structure | noise | loops at t=0 | half-life (ticks) |
|---|---|---|---|
| pt | 0.0001 | 0.0 | 100 |
| pt | 0.0003 | 0.0 | 100 |
| pt | 0.001 | 0.0 | 200 |
| pt | 0.003 | 0.0 | 500 |
| pt | 0.01 | 0.0 | > run |
| reg | 0.0001 | 0.0 | 500 |
| reg | 0.0003 | 0.0 | 500 |
| reg | 0.001 | 0.0 | 500 |
| reg | 0.003 | 0.0 | 300 |
| reg | 0.01 | 0.0 | 300 |

## Run totals (mean per seed)

| experiment | executed | repairs | deaths | starved | mutations | elapsed s |
|---|---|---|---|---|---|---|
| baseline | 1.55e+09 | 5.67e+08 | 9.58e+07 | 3.35e+07 | 1.64e+06 | 82.3 |
| hl_pt_0.0001 | 3.16e+08 | 1.05e+08 | 1.11e+07 | 3.78e+06 | 3.27e+04 | 17.8 |
| hl_pt_0.0003 | 3.16e+08 | 1.04e+08 | 1.15e+07 | 4.01e+06 | 9.83e+04 | 17.5 |
| hl_pt_0.001 | 3.16e+08 | 1.04e+08 | 9.42e+06 | 3.07e+06 | 3.28e+05 | 16.9 |
| hl_pt_0.003 | 3.16e+08 | 9.26e+07 | 7.71e+06 | 2.21e+06 | 9.83e+05 | 16.2 |
| hl_pt_0.01 | 3.14e+08 | 5.71e+07 | 6.13e+06 | 1.1e+06 | 3.28e+06 | 16.9 |
| hl_reg_0.0001 | 3.21e+08 | 2.32e+07 | 2.34e+06 | 4.68e+05 | 3.28e+04 | 11.3 |
| hl_reg_0.0003 | 3.21e+08 | 2.77e+07 | 2.91e+06 | 6.19e+05 | 9.83e+04 | 18.8 |
| hl_reg_0.001 | 3.2e+08 | 2.35e+07 | 3.15e+06 | 5.96e+05 | 3.28e+05 | 12.7 |
| hl_reg_0.003 | 3.19e+08 | 1.29e+07 | 3.22e+06 | 4.63e+05 | 9.83e+05 | 12.5 |
| hl_reg_0.01 | 3.15e+08 | 6.31e+06 | 4.08e+06 | 5.31e+05 | 3.28e+06 | 12.9 |
| null | 1.6e+09 | 0 | 1.24e+07 | 1.64e+06 | 1.64e+06 | 49.4 |
| probe_baseline | 1.55e+09 | 5.68e+08 | 9.58e+07 | 3.35e+07 | 1.64e+06 | 99.8 |
| probe_pt | 1.58e+09 | 5.16e+08 | 4.79e+07 | 1.59e+07 | 1.64e+06 | 83.1 |
| probe_pt_tiling | 1.58e+09 | 5.35e+08 | 5.26e+07 | 1.78e+07 | 4.92e+05 | 86.3 |
| probe_register | 1.6e+09 | 1.03e+08 | 1.62e+07 | 3.12e+06 | 1.64e+06 | 63.0 |
| pt_random | 1.58e+09 | 5.16e+08 | 4.79e+07 | 1.59e+07 | 1.64e+06 | 84.6 |
| pt_tiling_ramp | 1.58e+09 | 4.78e+08 | 4.23e+07 | 1.28e+07 | 4.1e+06 | 85.3 |
| register | 1.6e+09 | 5.17e+07 | 1.3e+07 | 1.83e+06 | 1.64e+06 | 56.5 |
