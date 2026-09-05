# hgt write-ups

Two pages, both generated from the runs in `../results/demo/`:

* `report.html` — **Genes on the Wire**. The technical report: model, method, every experiment
  with its tables, the fitness landscape drawn from the numbers a test asserts, and a section on
  four things that were wrong in the model before a measurement removed them.
* `explainer.html` — **Programs That Swap Genes**. The same work in plain language, for someone
  who has never heard of horizontal gene transfer.

Both are self-contained: open either file in a browser. The figures in `report.html` are drawn
from data extracted out of `../results/demo/ab/*/seed_0.jsonl`, so re-running the demo changes
the numbers in the tables but not the series inlined in the page — regenerate that inline block
if the two need to agree.
