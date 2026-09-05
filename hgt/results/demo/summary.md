# hgt: hgt/results/demo

## Does the population survive stressors it was not born ready for?

| run | seeds | survived | epochs survived | freq at shift | lateral share |
|---|---|---|---|---|---|
| all | 8 | 8 | 10 | 0.55 | 0.057 |
| conj | 8 | 4 | 6 | 0.21 | 0.047 |
| none | 8 | 0 | 2 | 0.00 | 0.000 |
| transd | 8 | 8 | 10 | 0.46 | 0.053 |
| transf | 8 | 0 | 2 | 0.00 | 0.018 |

`freq at shift` is how common the answering gene already was when the stressor
arrived. That, not a rescue afterwards, is where transfer does its work.

## Where did the genes come from?

`incongruence` is the share of a resistance gene's carriers that received it
sideways rather than inheriting it.

| run | birth | conjugation | transformation | transduction | incongruence |
|---|---|---|---|---|---|
| all | 1028283 | 31948 | 8508 | 32991 | 0.047 |
| conj | 633168 | 33034 | 0 | 0 | 0.034 |
| none | 72115 | 0 | 0 | 0 | 0.000 |
| transd | 1035246 | 0 | 0 | 62233 | 0.059 |
| transf | 114028 | 0 | 2763 | 0 | 0.027 |

## The restriction barrier, as a rate

| run | strain distance | attempts | redundant | accepted | rate |
|---|---|---|---|---|---|
| all | 0 | 934032 | 611652 | 38647 | 0.120 |
| all | 1 | 554544 | 269280 | 26334 | 0.092 |
| all | 2 | 271547 | 124981 | 8466 | 0.058 |
| conj | 0 | 85124 | 1663 | 19317 | 0.231 |
| conj | 1 | 52975 | 1043 | 11041 | 0.213 |
| conj | 2 | 22653 | 297 | 2676 | 0.120 |
| transd | 0 | 741929 | 619143 | 34904 | 0.284 |
| transd | 1 | 361211 | 251899 | 21583 | 0.197 |
| transd | 2 | 149686 | 98684 | 5746 | 0.113 |
| transf | 0 | 2190 | 216 | 1861 | 0.943 |
| transf | 1 | 1316 | 128 | 748 | 0.630 |
| transf | 2 | 629 | 34 | 154 | 0.259 |

## Can a gene be found rather than received?

| run | seeds | found it | median tick found | novel programs | answerers at the end, where found |
|---|---|---|---|---|---|
| bits12_all | 16 | 6 | 3540 | 518 | 6 |
| bits12_none | 16 | 3 | 6620 | 75 | 0 |
| bits16_all | 16 | 0 | - | 0 | - |
| bits16_none | 16 | 0 | - | 0 | - |
| bits4_all | 16 | 12 | 1320 | 1760 | 7 |
| bits4_none | 16 | 7 | 3300 | 359 | 0 |
| bits8_all | 16 | 10 | 2700 | 1253 | 8 |
| bits8_none | 16 | 9 | 3600 | 476 | 0 |

## Do free riders take over?

| run | seeds | survived | free riders at start | free riders at end | transfers |
|---|---|---|---|---|---|
| drift | 8 | 8 | 0.167 | 0.823 | 8615 |
| invasion | 8 | 8 | 0.167 | 0.626 | 8610 |

## What does an immune system buy, and cost?

| run | phage kills | immune refusals | transduced acquisitions |
|---|---|---|---|
| crispr0.0 | 739 | 0 | 32991 |
| crispr0.5 | 159 | 316115 | 22320 |
| crispr1.0 | 112 | 375959 | 18815 |

## What does cutting the network cost?

| run | seeds | survived | min population | worst side's answerers | peak divergence |
|---|---|---|---|---|---|
| cut | 8 | 2 | 46 | 0 | 0.165 |
| healed | 8 | 2 | 48 | 0 | 0.140 |
| whole | 8 | 8 | 48 | 22 | 0.141 |

## What if nodes offer genes nobody has seen work?

| run | survived | genes per node | distinct genes | redundant arrivals |
|---|---|---|---|---|
| everything | 7 | 4.96 | 239 | 758338 |
| proven_only | 8 | 4.94 | 197 | 1005913 |

_(matplotlib not installed: tables only)_
