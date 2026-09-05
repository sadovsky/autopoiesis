# hgt sweep: hgt/results/demo

## Does the population survive stressors it was not born ready for?

| transfer | seeds | survived | epochs survived | freq at shift | rescue ticks | lateral share |
|---|---|---|---|---|---|---|
| all | 8 | 8 | 10 | 0.97 | 0 | 0.108 |
| conj | 8 | 8 | 10 | 0.94 | 0 | 0.093 |
| none | 8 | 0 | 2 | 0.01 | 20 | 0.000 |
| transd | 8 | 8 | 10 | 0.96 | 0 | 0.107 |
| transf | 8 | 4 | 6 | 0.69 | 20 | 0.010 |

## Where did the genes come from?

`incongruence` is the share of a resistance gene's carriers that received it
sideways rather than inheriting it.

| transfer | birth | conjugation | transformation | transduction | incongruence |
|---|---|---|---|---|---|
| all | 30816 | 1871 | 79 | 2597 | 0.076 |
| conj | 33732 | 1912 | 0 | 0 | 0.104 |
| none | 13369 | 0 | 0 | 0 | 0.000 |
| transd | 33025 | 0 | 0 | 3922 | 0.055 |
| transf | 35048 | 0 | 148 | 0 | 0.001 |

## The restriction barrier, as a rate

| transfer | strain distance | attempts | redundant | accepted | rate |
|---|---|---|---|---|---|
| all | 0 | 904183 | 887312 | 2622 | 0.155 |
| all | 1 | 368164 | 361608 | 1461 | 0.223 |
| all | 2 | 159501 | 155407 | 464 | 0.113 |
| conj | 0 | 9137 | 32 | 885 | 0.097 |
| conj | 1 | 20508 | 54 | 820 | 0.040 |
| conj | 2 | 11327 | 29 | 207 | 0.018 |
| transd | 0 | 883079 | 878726 | 2244 | 0.516 |
| transd | 1 | 371753 | 368586 | 1239 | 0.391 |
| transd | 2 | 162283 | 159795 | 439 | 0.176 |
| transf | 0 | 128 | 21 | 74 | 0.692 |
| transf | 1 | 128 | 14 | 55 | 0.482 |
| transf | 2 | 123 | 17 | 19 | 0.179 |

_(matplotlib not installed: tables only)_
