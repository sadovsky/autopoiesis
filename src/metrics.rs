//! Where "is it alive?" becomes a number.
//!
//! * **Repair graph + SCCs** — edge `a → b` if `a` executed `Repair` on `b` within the
//!   window. A strongly connected component of size ≥ `min_size` is a *candidate
//!   organism*: a set of cells that mutually maintain each other. Cells repaired by
//!   the core but not in it are *dependents*; dependents that executed no `Repair`
//!   at all are *parasites*.
//! * **Self-mutual-information** — `MI(R_t ; R_{t-Δ})` over the region's instruction
//!   bytes, compared with equal-sized random regions drawn from the *background* (cells
//!   in no candidate organism's region): `persistence = MI_R / MI_rand`.
//!
//!   Estimator notes. With `m` cells and a 256-symbol alphabet the plug-in estimate is
//!   pure finite-sample bias (≈ log2 m) for any region, persistent or not, so MI is
//!   estimated on the 11-symbol *opcode* alphabet with the Miller–Madow correction,
//!   and samples are pooled over every frame pair inside the window. The baseline uses
//!   the identical protocol on random cell sets of the same size, so what remains of
//!   the bias largely cancels in the ratio. Because `Repair` copies the repairer's
//!   own byte, a stable core is homogeneous and has zero entropy on its own; the
//!   region is therefore the core dilated by `mi_dilate` cells, which measures the
//!   boundary the organism maintains against a churning background. A plain
//!   *stability* (fraction of cells whose byte is unchanged over Δ) is reported
//!   alongside as a robust companion.
//! * **Vitality** — under a noise ramp, the noise rate at which an organism's SCC
//!   dissolves (unmatched for `window` ticks). Organisms are tracked across frames by
//!   Jaccard overlap ≥ 0.5 of their core cells.

use crate::config::SimConfig;
use crate::grid::{Grid, Topology};
use crate::isa::Instruction;
use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};

// ---------------------------------------------------------------------------
// Repair graph and strongly connected components
// ---------------------------------------------------------------------------

/// Compressed adjacency built from an edge list.
struct Adjacency {
    offsets: Vec<u32>,
    targets: Vec<u32>,
    outdeg: Vec<u32>,
    indeg: Vec<u32>,
}

impl Adjacency {
    fn new(n: usize, edges: &[(u32, u32)]) -> Adjacency {
        let mut outdeg = vec![0u32; n];
        let mut indeg = vec![0u32; n];
        for &(a, b) in edges {
            outdeg[a as usize] += 1;
            indeg[b as usize] += 1;
        }
        let mut offsets = vec![0u32; n + 1];
        for i in 0..n {
            offsets[i + 1] = offsets[i] + outdeg[i];
        }
        let mut fill = offsets.clone();
        let mut targets = vec![0u32; edges.len()];
        for &(a, b) in edges {
            let p = &mut fill[a as usize];
            targets[*p as usize] = b;
            *p += 1;
        }
        Adjacency {
            offsets,
            targets,
            outdeg,
            indeg,
        }
    }

    #[inline]
    fn out(&self, v: usize) -> &[u32] {
        &self.targets[self.offsets[v] as usize..self.offsets[v + 1] as usize]
    }
}

/// Tarjan's algorithm, iterative. Returns every SCC (including singletons) as a
/// sorted list of cell indices; components are sorted by their smallest member.
pub fn tarjan_scc(n: usize, edges: &[(u32, u32)]) -> Vec<Vec<u32>> {
    let adj = Adjacency::new(n, edges);
    tarjan_on(&adj, n)
}

fn tarjan_on(adj: &Adjacency, n: usize) -> Vec<Vec<u32>> {
    const UNVISITED: u32 = u32::MAX;
    let mut index = vec![UNVISITED; n];
    let mut lowlink = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<u32> = Vec::new();
    let mut call: Vec<(u32, u32)> = Vec::new(); // (node, next edge position)
    let mut next_index = 0u32;
    let mut comps: Vec<Vec<u32>> = Vec::new();

    for root in 0..n {
        if index[root] != UNVISITED {
            continue;
        }
        if adj.outdeg[root] == 0 {
            // Trivial singleton; skip the machinery.
            index[root] = next_index;
            next_index += 1;
            comps.push(vec![root as u32]);
            continue;
        }
        index[root] = next_index;
        lowlink[root] = next_index;
        next_index += 1;
        stack.push(root as u32);
        on_stack[root] = true;
        call.push((root as u32, adj.offsets[root]));

        while let Some(&mut (v, ref mut pos)) = call.last_mut() {
            let v = v as usize;
            if *pos < adj.offsets[v + 1] {
                let w = adj.targets[*pos as usize] as usize;
                *pos += 1;
                if index[w] == UNVISITED {
                    index[w] = next_index;
                    lowlink[w] = next_index;
                    next_index += 1;
                    stack.push(w as u32);
                    on_stack[w] = true;
                    call.push((w as u32, adj.offsets[w]));
                } else if on_stack[w] {
                    lowlink[v] = lowlink[v].min(index[w]);
                }
            } else {
                call.pop();
                if let Some(&(parent, _)) = call.last() {
                    let p = parent as usize;
                    lowlink[p] = lowlink[p].min(lowlink[v]);
                }
                if lowlink[v] == index[v] {
                    let mut comp = Vec::new();
                    loop {
                        let w = stack.pop().expect("tarjan stack underflow");
                        on_stack[w as usize] = false;
                        comp.push(w);
                        if w as usize == v {
                            break;
                        }
                    }
                    comp.sort_unstable();
                    comps.push(comp);
                }
            }
        }
    }
    comps.sort_by_key(|c| c[0]);
    comps
}

/// A candidate organism as seen in one frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Organism {
    /// SCC members (sorted).
    pub core: Vec<u32>,
    /// Cells repaired by core members that are not themselves in the core (sorted).
    pub dependents: Vec<u32>,
    /// Dependents that executed no `Repair` within the window (sorted).
    pub parasites: Vec<u32>,
}

/// SCCs of size ≥ `min_size` with their dependents and parasites.
pub fn find_organisms(n: usize, edges: &[(u32, u32)], min_size: usize) -> Vec<Organism> {
    let adj = Adjacency::new(n, edges);
    let comps = tarjan_on(&adj, n);
    let mut in_core = vec![false; n];
    let mut out = Vec::new();
    for core in comps.into_iter().filter(|c| c.len() >= min_size.max(1)) {
        // A singleton only counts if it repairs itself (self-loop); min_size >= 2 in
        // practice, but keep the definition honest.
        if core.len() == 1 && !adj.out(core[0] as usize).contains(&core[0]) {
            continue;
        }
        for &c in &core {
            in_core[c as usize] = true;
        }
        let mut dependents: Vec<u32> = core
            .iter()
            .flat_map(|&c| adj.out(c as usize).iter().copied())
            .filter(|&t| !in_core[t as usize])
            .collect();
        dependents.sort_unstable();
        dependents.dedup();
        let parasites = dependents
            .iter()
            .copied()
            .filter(|&d| adj.outdeg[d as usize] == 0 && adj.indeg[d as usize] > 0)
            .collect();
        for &c in &core {
            in_core[c as usize] = false;
        }
        out.push(Organism {
            core,
            dependents,
            parasites,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Regions and mutual information
// ---------------------------------------------------------------------------

/// Cells within `radius` (Chebyshev) of any cell in `cells`, sorted, deduplicated.
pub fn dilate(cells: &[u32], topo: &Topology, radius: usize) -> Vec<u32> {
    let n = topo.nbrs.len();
    let mut mark = vec![false; n];
    let mut frontier: Vec<u32> = cells.to_vec();
    for &c in cells {
        mark[c as usize] = true;
    }
    for _ in 0..radius {
        let mut next = Vec::new();
        for &c in &frontier {
            for &nb in &topo.nbrs[c as usize] {
                if !mark[nb as usize] {
                    mark[nb as usize] = true;
                    next.push(nb);
                }
            }
        }
        frontier = next;
    }
    let mut out: Vec<u32> = mark
        .iter()
        .enumerate()
        .filter(|&(_, &m)| m)
        .map(|(i, _)| i as u32)
        .collect();
    out.sort_unstable();
    out
}

/// `size` distinct random cells drawn from `pool` (sorted output). If `pool` has at
/// most `size` cells the whole pool is returned.
pub fn random_region<R: RngExt>(pool: &[u32], size: usize, rng: &mut R) -> Vec<u32> {
    let mut scratch = Vec::new();
    random_region_with(pool, size, rng, &mut scratch)
}

/// As `random_region`, reusing `scratch` (a marker buffer) across calls.
pub fn random_region_with<R: RngExt>(pool: &[u32], size: usize, rng: &mut R, scratch: &mut Vec<bool>) -> Vec<u32> {
    if pool.len() <= size {
        return pool.to_vec();
    }
    if scratch.len() < pool.len() {
        scratch.resize(pool.len(), false);
    }
    let mut picked: Vec<usize> = Vec::with_capacity(size);
    while picked.len() < size {
        let i = rng.random_range(0..pool.len());
        if !scratch[i] {
            scratch[i] = true;
            picked.push(i);
        }
    }
    let mut out: Vec<u32> = picked.iter().map(|&i| pool[i]).collect();
    for i in picked {
        scratch[i] = false;
    }
    out.sort_unstable();
    out
}

/// At most `max_cells` of `cells`, taken with an even stride so the sample covers the
/// region uniformly. Deterministic.
pub fn subsample(cells: &[u32], max_cells: usize) -> Vec<u32> {
    if max_cells == 0 || cells.len() <= max_cells {
        return cells.to_vec();
    }
    (0..max_cells)
        .map(|i| cells[(i as u64 * cells.len() as u64 / max_cells as u64) as usize])
        .collect()
}

/// Representative size for a region of `n` cells: geometric buckets (ratio 1.25), so
/// baselines can be shared between similarly sized regions.
pub fn size_bucket(n: usize) -> usize {
    if n <= 8 {
        return n.max(1);
    }
    let k = (n as f64).ln() / 1.25f64.ln();
    let rep = 1.25f64.powf(k.round()).round() as usize;
    rep.max(1)
}

/// Cells of `0..n` that are in none of the given (sorted) regions: the background.
pub fn background_pool(n: usize, regions: &[Vec<u32>]) -> Vec<u32> {
    let mut taken = vec![false; n];
    for r in regions {
        for &c in r {
            taken[c as usize] = true;
        }
    }
    (0..n as u32).filter(|&i| !taken[i as usize]).collect()
}

/// Number of symbols in the MI alphabet (opcodes).
pub const MI_SYMBOLS: usize = crate::isa::N_OPS as usize;

#[inline]
fn symbol(byte: u8) -> usize {
    Instruction::decode(byte).opcode() as usize
}

/// Result of an MI estimate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct MiEstimate {
    /// Shuffle-corrected mutual information in bits: the plug-in estimate minus the
    /// mean plug-in estimate over `shuffles` random permutations of the time pairing.
    /// The permuted data has identical marginals and therefore identical finite-sample
    /// bias, so this is ≈ 0 (± sampling noise) under independence. May be slightly
    /// negative; callers clamp when forming ratios.
    pub mi: f64,
    /// Uncorrected plug-in estimate in bits.
    pub mi_plugin: f64,
    /// Fraction of samples whose full byte was unchanged across the lag.
    pub stability: f64,
    /// Number of (past, present) samples pooled.
    pub samples: usize,
}

/// Plug-in MI (bits) from paired symbol sequences over a `k`-symbol alphabet.
fn plugin_mi(xs: &[u8], ys: &[u8], k: usize, joint: &mut [u32]) -> f64 {
    debug_assert_eq!(xs.len(), ys.len());
    joint.iter_mut().for_each(|c| *c = 0);
    let mut px = vec![0u32; k];
    let mut py = vec![0u32; k];
    for (&a, &b) in xs.iter().zip(ys) {
        joint[a as usize * k + b as usize] += 1;
        px[a as usize] += 1;
        py[b as usize] += 1;
    }
    let n = xs.len();
    if n == 0 {
        return 0.0;
    }
    let nf = n as f64;
    let mut mi = 0.0;
    for a in 0..k {
        if px[a] == 0 {
            continue;
        }
        for b in 0..k {
            let c = joint[a * k + b];
            if c == 0 {
                continue;
            }
            let pxy = c as f64 / nf;
            let pa = px[a] as f64 / nf;
            let pb = py[b] as f64 / nf;
            mi += pxy * (pxy / (pa * pb)).log2();
        }
    }
    mi
}

/// `MI(R_t ; R_{t-Δ})` over `cells`, pooling the given `(past, present)` grid pairs,
/// shuffle-corrected with `shuffles` permutations drawn from `rng`.
pub fn mutual_information<R: RngExt>(cells: &[u32], pairs: &[(&Grid, &Grid)], shuffles: usize, rng: &mut R) -> MiEstimate {
    let k = MI_SYMBOLS;
    let n = cells.len() * pairs.len();
    let mut xs: Vec<u8> = Vec::with_capacity(n);
    let mut ys: Vec<u8> = Vec::with_capacity(n);
    let mut same = 0usize;
    for &(past, now) in pairs {
        for &c in cells {
            let a = past.cells[c as usize].instr;
            let b = now.cells[c as usize].instr;
            xs.push(symbol(a) as u8);
            ys.push(symbol(b) as u8);
            same += (a == b) as usize;
        }
    }
    if n == 0 {
        return MiEstimate::default();
    }
    let mut joint = vec![0u32; k * k];
    let full = plugin_mi(&xs, &ys, k, &mut joint);
    let mut null = 0.0;
    if shuffles > 0 && n > 1 {
        for _ in 0..shuffles {
            // Fisher–Yates on the present-side symbols.
            for i in (1..n).rev() {
                let j = rng.random_range(0..=i);
                ys.swap(i, j);
            }
            null += plugin_mi(&xs, &ys, k, &mut joint);
        }
        null /= shuffles as f64;
    }
    MiEstimate {
        mi: full - null,
        mi_plugin: full,
        stability: same as f64 / n as f64,
        samples: n,
    }
}

/// `(MI_region + floor) / (MI_random + floor)`. The floor is the estimator's
/// resolution: below it, differences are noise, and it keeps the ratio finite when
/// the baseline is zero.
pub fn persistence_ratio(region: f64, random: f64, floor: f64) -> f64 {
    let f = floor.max(1e-9);
    (region.max(0.0) + f) / (random.max(0.0) + f)
}

// ---------------------------------------------------------------------------
// Tracking across frames, vitality
// ---------------------------------------------------------------------------

/// Jaccard similarity of two sorted, deduplicated index lists.
pub fn jaccard(a: &[u32], b: &[u32]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let (mut i, mut j, mut inter) = (0, 0, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                inter += 1;
                i += 1;
                j += 1;
            }
        }
    }
    let union = a.len() + b.len() - inter;
    inter as f64 / union as f64
}

#[derive(Clone, Debug)]
struct Tracked {
    id: u64,
    born: u32,
    core: Vec<u32>,
    last_seen: u32,
    max_size: usize,
    max_persistence: f64,
    /// (tick, noise rate) of the first frame in which the organism went unmatched.
    missing_since: Option<(u32, f64)>,
}

/// Per-organism row inside a frame record.
#[derive(Clone, Debug, Serialize)]
pub struct OrganismRow {
    pub id: u64,
    pub core_size: usize,
    pub dependents: usize,
    pub parasites: usize,
    /// Smallest core index, for locating the organism in a snapshot.
    pub anchor: u32,
    /// Mean x and y of the core cells (wrap-naive; x is the sun-gradient axis).
    pub cx: f64,
    pub cy: f64,
    pub mi_region: f64,
    pub mi_random: f64,
    pub persistence: f64,
    pub stability: f64,
    pub stability_random: f64,
    pub mi_samples: usize,
}

/// One analysis frame.
#[derive(Clone, Debug, Serialize)]
pub struct FrameRecord {
    pub kind: &'static str,
    pub seed: u64,
    pub tick: u32,
    pub noise_rate: f64,
    pub n_organisms: usize,
    pub core_cells: usize,
    pub parasite_cells: usize,
    pub sizes: Vec<usize>,
    pub max_persistence: f64,
    /// Organisms with persistence above `persistent_threshold`, and their core cells.
    pub n_persistent: usize,
    pub persistent_cells: usize,
    /// Histogram of organism persistence: `PERSISTENCE_BINS` unit-width bins from 0,
    /// the last bin open-ended.
    pub persistence_hist: Vec<u32>,
    /// Fraction of all cells whose byte is unchanged over the lag (0 if no lag frame yet).
    pub background_stability: f64,
    /// Number of SCC-core cells per column band: `CORE_X_BINS` equal bins across x.
    pub core_x_hist: Vec<u32>,
    pub repair_edges: usize,
    pub organisms: Vec<OrganismRow>,
}

/// Number of x bins in `FrameRecord::core_x_hist`.
pub const CORE_X_BINS: usize = 16;
/// Number of bins in `FrameRecord::persistence_hist`.
pub const PERSISTENCE_BINS: usize = 12;

impl FrameRecord {
    /// Copy with organism rows limited to the union of the top `top` by core size and
    /// the top `top` by persistence (aggregates untouched).
    pub fn trimmed(&self, top: usize) -> FrameRecord {
        let mut keep = vec![false; self.organisms.len()];
        let mut by_size: Vec<usize> = (0..self.organisms.len()).collect();
        by_size.sort_by(|&a, &b| self.organisms[b].core_size.cmp(&self.organisms[a].core_size));
        let mut by_p: Vec<usize> = (0..self.organisms.len()).collect();
        by_p.sort_by(|&a, &b| {
            self.organisms[b]
                .persistence
                .partial_cmp(&self.organisms[a].persistence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for &i in by_size.iter().take(top).chain(by_p.iter().take(top)) {
            keep[i] = true;
        }
        FrameRecord {
            organisms: self
                .organisms
                .iter()
                .zip(&keep)
                .filter(|&(_, &k)| k)
                .map(|(o, _)| o.clone())
                .collect(),
            ..self.clone()
        }
    }
}

/// Lifetime record, emitted when an organism dies or when the run ends.
#[derive(Clone, Debug, Serialize)]
pub struct LifeRecord {
    pub kind: &'static str,
    pub seed: u64,
    pub organism_id: u64,
    pub born: u32,
    /// `None` if still alive when the run ended.
    pub died: Option<u32>,
    /// Last frame in which it was matched.
    pub last_seen: u32,
    /// `died.unwrap_or(last_seen) - born`.
    pub lifetime: u32,
    pub max_size: usize,
    /// Noise rate in force when the SCC dissolved; `None` if it survived.
    pub vitality: Option<f64>,
    pub max_persistence: f64,
}

/// `(past, present)` grid pairs at lag `lag` whose present tick lies within the
/// window ending at `tick`. Requires `tick`'s grid to be in `history` already.
fn lag_pairs(history: &VecDeque<(u32, Grid)>, tick: u32, lag: u32, window: u32) -> Vec<(&Grid, &Grid)> {
    let lo = tick.saturating_sub(window.saturating_sub(1));
    let mut pairs = Vec::new();
    for (t_now, g_now) in history {
        if *t_now < lo || *t_now > tick || *t_now < lag {
            continue;
        }
        let t_past = *t_now - lag;
        if let Some((_, g_past)) = history.iter().find(|(t, _)| *t == t_past) {
            pairs.push((g_past, g_now));
        }
    }
    pairs
}

pub struct FrameReport {
    pub frame: FrameRecord,
    pub deaths: Vec<LifeRecord>,
}

/// Feeds on `(tick, noise_rate, grid, edges)` frames in tick order and produces
/// frame and lifetime records. Works identically online and over snapshot files.
pub struct Analyzer {
    cfg: SimConfig,
    seed: u64,
    topo: Topology,
    history: VecDeque<(u32, Grid)>,
    tracked: Vec<Tracked>,
    next_id: u64,
    rng: Xoshiro256PlusPlus,
    last_tick: Option<u32>,
}

impl Analyzer {
    pub fn new(cfg: &SimConfig, seed: u64) -> Analyzer {
        Analyzer {
            topo: Topology::new(cfg.width, cfg.height),
            cfg: cfg.clone(),
            seed,
            history: VecDeque::new(),
            tracked: Vec::new(),
            next_id: 0,
            // Decoupled from the sim's RNG so metrics never perturb the run.
            rng: Xoshiro256PlusPlus::seed_from_u64(seed ^ 0x5eed_0fba_5e1e_5000),
            last_tick: None,
        }
    }

    pub fn tracked_count(&self) -> usize {
        self.tracked.len()
    }

    /// Total number of organisms ever assigned an id.
    pub fn organisms_created(&self) -> u64 {
        self.next_id
    }

    pub fn observe(&mut self, tick: u32, noise_rate: f64, grid: &Grid, edges: &[(u32, u32)]) -> FrameReport {
        if let Some(prev) = self.last_tick {
            assert!(tick > prev, "frames must be observed in increasing tick order");
        }
        self.last_tick = Some(tick);
        let n = grid.len();

        // Keep enough history for lag pairs across the whole window.
        self.history.push_back((tick, grid.clone()));
        let keep_from = tick.saturating_sub(self.cfg.mi_lag + self.cfg.window);
        while let Some((t, _)) = self.history.front()
            && *t < keep_from
        {
            self.history.pop_front();
        }

        let organisms = find_organisms(n, edges, self.cfg.min_size);
        let pairs = lag_pairs(&self.history, tick, self.cfg.mi_lag, self.cfg.window);

        let background_stability = if pairs.is_empty() {
            0.0
        } else {
            let all: Vec<u32> = (0..n as u32).collect();
            let all = subsample(&all, self.cfg.mi_max_cells.max(1) * 4);
            mutual_information(&all, &pairs, 0, &mut self.rng).stability
        };

        // Match to tracked organisms by Jaccard >= 0.5 (greedy, best first).
        let mut candidates: Vec<(f64, usize, usize)> = Vec::new(); // (sim, org idx, tracked idx)
        for (oi, org) in organisms.iter().enumerate() {
            for (ti, tr) in self.tracked.iter().enumerate() {
                let s = jaccard(&org.core, &tr.core);
                if s >= 0.5 {
                    candidates.push((s, oi, ti));
                }
            }
        }
        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
        let mut org_to_tracked: Vec<Option<usize>> = vec![None; organisms.len()];
        let mut tracked_used = vec![false; self.tracked.len()];
        for (_, oi, ti) in candidates {
            if org_to_tracked[oi].is_none() && !tracked_used[ti] {
                org_to_tracked[oi] = Some(ti);
                tracked_used[ti] = true;
            }
        }

        // Regions (dilated cores) for every organism, and the background they leave.
        let regions: Vec<Vec<u32>> = organisms
            .iter()
            .map(|o| dilate(&o.core, &self.topo, self.cfg.mi_dilate))
            .collect();
        let pool = background_pool(n, &regions);

        let mut rows = Vec::with_capacity(organisms.len());
        let mut sizes = Vec::with_capacity(organisms.len());
        let mut core_x_hist = vec![0u32; CORE_X_BINS];
        let mut persistence_hist = vec![0u32; PERSISTENCE_BINS];
        let (mut n_persistent, mut persistent_cells) = (0usize, 0usize);
        let width = grid.width.max(1);
        let mut core_cells = 0;
        let mut parasite_cells = 0;
        let mut max_persistence = 0.0f64;
        // The random baseline depends on region size only through sampling noise (the
        // shuffle correction removes the size-dependent bias), so organisms whose
        // regions fall in the same geometric size bucket share one baseline per frame.
        let mut scratch: Vec<bool> = Vec::new();
        let mut baselines: HashMap<usize, (f64, f64)> = HashMap::new();
        for (oi, org) in organisms.iter().enumerate() {
            // Large regions are subsampled: a few thousand pooled samples estimate MI
            // well, and the baseline uses the same number of cells for equal bias.
            let region = subsample(&regions[oi], self.cfg.mi_max_cells);
            let est = mutual_information(&region, &pairs, self.cfg.mi_shuffles, &mut self.rng);
            let (mi_rand, stab_rand) = if pool.is_empty() {
                (0.0, 0.0)
            } else {
                let bucket = size_bucket(region.len());
                if let Some(&b) = baselines.get(&bucket) {
                    b
                } else {
                    let samples = self.cfg.mi_samples.max(1);
                    let (mut mi_rand, mut stab_rand) = (0.0, 0.0);
                    for _ in 0..samples {
                        let r = random_region_with(&pool, bucket, &mut self.rng, &mut scratch);
                        let e = mutual_information(&r, &pairs, self.cfg.mi_shuffles, &mut self.rng);
                        mi_rand += e.mi;
                        stab_rand += e.stability;
                    }
                    let b = (mi_rand / samples as f64, stab_rand / samples as f64);
                    baselines.insert(bucket, b);
                    b
                }
            };
            let persistence = if pairs.is_empty() { 0.0 } else { persistence_ratio(est.mi, mi_rand, self.cfg.mi_floor) };
            max_persistence = max_persistence.max(persistence);
            persistence_hist[(persistence.max(0.0).floor() as usize).min(PERSISTENCE_BINS - 1)] += 1;
            if persistence > self.cfg.persistent_threshold {
                n_persistent += 1;
                persistent_cells += org.core.len();
            }

            let ti = match org_to_tracked[oi] {
                Some(ti) => {
                    let tr = &mut self.tracked[ti];
                    tr.core = org.core.clone();
                    tr.last_seen = tick;
                    tr.max_size = tr.max_size.max(org.core.len());
                    tr.max_persistence = tr.max_persistence.max(persistence);
                    tr.missing_since = None;
                    ti
                }
                None => {
                    self.tracked.push(Tracked {
                        id: self.next_id,
                        born: tick,
                        core: org.core.clone(),
                        last_seen: tick,
                        max_size: org.core.len(),
                        max_persistence: persistence,
                        missing_since: None,
                    });
                    self.next_id += 1;
                    self.tracked.len() - 1
                }
            };
            core_cells += org.core.len();
            parasite_cells += org.parasites.len();
            sizes.push(org.core.len());
            let (mut sx, mut sy) = (0.0f64, 0.0f64);
            for &c in &org.core {
                let x = c as usize % width;
                sx += x as f64;
                sy += (c as usize / width) as f64;
                core_x_hist[x * CORE_X_BINS / width] += 1;
            }
            rows.push(OrganismRow {
                id: self.tracked[ti].id,
                core_size: org.core.len(),
                dependents: org.dependents.len(),
                parasites: org.parasites.len(),
                anchor: org.core[0],
                cx: sx / org.core.len() as f64,
                cy: sy / org.core.len() as f64,
                mi_region: est.mi,
                mi_random: mi_rand,
                persistence,
                stability: est.stability,
                stability_random: stab_rand,
                mi_samples: est.samples,
            });
        }

        // Retire organisms that have been missing for a full window.
        let mut deaths = Vec::new();
        let window = self.cfg.window;
        let seed = self.seed;
        for (ti, tr) in self.tracked.iter_mut().enumerate() {
            if tracked_used.get(ti).copied().unwrap_or(false) || tr.last_seen == tick {
                continue;
            }
            if tr.missing_since.is_none() {
                tr.missing_since = Some((tick, noise_rate));
            }
        }
        let mut i = 0;
        while i < self.tracked.len() {
            let retire = match self.tracked[i].missing_since {
                Some((t_miss, _)) => tick.saturating_sub(t_miss) >= window,
                None => false,
            };
            if retire {
                let tr = self.tracked.swap_remove(i);
                let (t_miss, noise_at) = tr.missing_since.unwrap_or((tick, noise_rate));
                deaths.push(LifeRecord {
                    kind: "life",
                    seed,
                    organism_id: tr.id,
                    born: tr.born,
                    died: Some(t_miss),
                    last_seen: tr.last_seen,
                    lifetime: t_miss - tr.born,
                    max_size: tr.max_size,
                    vitality: Some(noise_at),
                    max_persistence: tr.max_persistence,
                });
            } else {
                i += 1;
            }
        }
        deaths.sort_by_key(|d| d.organism_id);
        sizes.sort_unstable_by(|a, b| b.cmp(a));

        FrameReport {
            frame: FrameRecord {
                kind: "frame",
                seed,
                tick,
                noise_rate,
                n_organisms: organisms.len(),
                core_cells,
                parasite_cells,
                sizes,
                max_persistence,
                n_persistent,
                persistent_cells,
                persistence_hist,
                background_stability,
                core_x_hist,
                repair_edges: edges.len(),
                organisms: rows,
            },
            deaths,
        }
    }

    /// Emit lifetime records for everything still tracked. Organisms already
    /// missing are reported as dead at the tick they vanished; the rest survived.
    pub fn finish(&mut self) -> Vec<LifeRecord> {
        let seed = self.seed;
        let mut out: Vec<LifeRecord> = self
            .tracked
            .drain(..)
            .map(|tr| match tr.missing_since {
                Some((t_miss, noise_at)) => LifeRecord {
                    kind: "life",
                    seed,
                    organism_id: tr.id,
                    born: tr.born,
                    died: Some(t_miss),
                    last_seen: tr.last_seen,
                    lifetime: t_miss - tr.born,
                    max_size: tr.max_size,
                    vitality: Some(noise_at),
                    max_persistence: tr.max_persistence,
                },
                None => LifeRecord {
                    kind: "life",
                    seed,
                    organism_id: tr.id,
                    born: tr.born,
                    died: None,
                    last_seen: tr.last_seen,
                    lifetime: tr.last_seen - tr.born,
                    max_size: tr.max_size,
                    vitality: None,
                    max_persistence: tr.max_persistence,
                },
            })
            .collect();
        out.sort_by_key(|d| d.organism_id);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tarjan_finds_cycles_and_singletons() {
        // 0->1->2->0 is a cycle; 3->4 is a chain; 5 isolated; 6 self-loop.
        let edges = [(0, 1), (1, 2), (2, 0), (3, 4), (2, 3), (6, 6)];
        let comps = tarjan_scc(7, &edges);
        assert!(comps.contains(&vec![0, 1, 2]));
        assert!(comps.contains(&vec![3]));
        assert!(comps.contains(&vec![4]));
        assert!(comps.contains(&vec![5]));
        assert!(comps.contains(&vec![6]));
        assert_eq!(comps.len(), 5);

        let orgs = find_organisms(7, &edges, 2);
        assert_eq!(orgs.len(), 1);
        assert_eq!(orgs[0].core, vec![0, 1, 2]);
        assert_eq!(orgs[0].dependents, vec![3]);
        // 3 repairs 4, so it is a dependent but not a parasite.
        assert!(orgs[0].parasites.is_empty());
        let edges2 = [(0, 1), (1, 0), (1, 9)];
        let orgs = find_organisms(10, &edges2, 2);
        assert_eq!(orgs[0].parasites, vec![9]);
    }

    #[test]
    fn tarjan_handles_long_chains_without_recursion() {
        let n = 200_000;
        let mut edges: Vec<(u32, u32)> = (0..n as u32 - 1).map(|i| (i, i + 1)).collect();
        edges.push((n as u32 - 1, 0));
        let comps = tarjan_scc(n, &edges);
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].len(), n);
    }

    #[test]
    fn jaccard_basics() {
        assert_eq!(jaccard(&[1, 2, 3], &[1, 2, 3]), 1.0);
        assert_eq!(jaccard(&[1, 2, 3], &[4, 5]), 0.0);
        assert!((jaccard(&[1, 2, 3, 4], &[3, 4, 5, 6]) - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn dilate_grows_by_one_ring() {
        let topo = Topology::new(10, 10);
        let d = dilate(&[55], &topo, 1);
        assert_eq!(d.len(), 9);
        let d2 = dilate(&[55], &topo, 2);
        assert_eq!(d2.len(), 25);
        let d0 = dilate(&[55, 56], &topo, 0);
        assert_eq!(d0, vec![55, 56]);
    }

    #[test]
    fn mi_is_zero_for_independent_and_high_for_identical() {
        use rand::SeedableRng;
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        let a = Grid::random(64, 64, 10, &mut rng);
        let b = Grid::random(64, 64, 10, &mut rng);
        let all: Vec<u32> = (0..4096).collect();
        let indep = mutual_information(&all, &[(&a, &b)], 4, &mut rng);
        let same = mutual_information(&all, &[(&a, &a)], 4, &mut rng);
        assert!(indep.mi.abs() < 0.02, "independent MI {}", indep.mi);
        assert!(indep.mi_plugin > indep.mi, "plug-in is biased upward");
        assert!(same.mi > 2.5, "identical MI {}", same.mi);
        assert!((same.stability - 1.0).abs() < 1e-12);
        assert!(indep.stability < 0.02);
    }
}
