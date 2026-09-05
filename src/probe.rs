//! Perturbation probe: the direct test of "actively maintains its encoding".
//!
//! Every `probe_every` ticks, for the largest tracked organisms, `probe_k` random core
//! bytes are overwritten with random bytes and, for each, one background byte in the
//! same column (same sunlight) is overwritten too. After 1, 2 and 5 windows the
//! fraction of probed bytes that again equal their pre-perturbation value is recorded
//! for the organism and for the matched background. A maintained region restores;
//! a frozen one does not; a churning one is already gone. The probe perturbs the run,
//! deterministically (its own seeded RNG), so probe runs are kept separate from clean
//! ones.

use crate::config::SimConfig;
use crate::grid::Grid;
use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use serde::Serialize;

/// Windows after the perturbation at which restoration is measured.
pub const CHECKS: [u32; 3] = [1, 2, 5];

#[derive(Clone, Debug)]
struct Pending {
    tick: u32,
    organism_id: u64,
    core_size: usize,
    /// (cell, original byte) for the organism's probed cells.
    cells: Vec<(u32, u8)>,
    /// (cell, original byte) for the matched background cells.
    background: Vec<(u32, u8)>,
    restored: [Option<f64>; 3],
    bg_restored: [Option<f64>; 3],
}

#[derive(Clone, Debug, Serialize)]
pub struct ProbeRecord {
    pub kind: &'static str,
    pub seed: u64,
    pub tick: u32,
    pub organism_id: u64,
    pub core_size: usize,
    pub k: usize,
    pub bg_k: usize,
    /// Fraction of probed organism bytes restored after 1, 2, 5 windows.
    pub restored: [f64; 3],
    /// Same for the matched background bytes (NaN if none could be matched).
    pub bg_restored: [f64; 3],
}

pub struct Prober {
    seed: u64,
    window: u32,
    k: usize,
    min_size: usize,
    max_organisms: usize,
    rng: Xoshiro256PlusPlus,
    pending: Vec<Pending>,
}

impl Prober {
    pub fn new(cfg: &SimConfig, seed: u64) -> Prober {
        Prober {
            seed,
            window: cfg.window,
            k: cfg.probe_k.max(1),
            min_size: cfg.probe_min_size,
            max_organisms: cfg.probe_max_organisms.max(1),
            rng: Xoshiro256PlusPlus::seed_from_u64(seed ^ 0x9e37_79b9_7f4a_7c15),
            pending: Vec::new(),
        }
    }

    /// Overwrite bytes in the largest organisms (and matched background cells) now.
    pub fn perturb(&mut self, tick: u32, grid: &mut Grid, organisms: &[(u64, Vec<u32>)]) {
        let n = grid.len();
        let w = grid.width.max(1);
        let mut in_core = vec![false; n];
        for (_, core) in organisms {
            for &c in core {
                in_core[c as usize] = true;
            }
        }
        let mut order: Vec<usize> = (0..organisms.len()).filter(|&i| organisms[i].1.len() >= self.min_size).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(organisms[i].1.len()));
        for &oi in order.iter().take(self.max_organisms) {
            let (id, core) = &organisms[oi];
            let mut cells = Vec::with_capacity(self.k);
            let mut background = Vec::with_capacity(self.k);
            let mut taken = vec![false; core.len()];
            for _ in 0..self.k.min(core.len()) {
                let mut j = self.rng.random_range(0..core.len());
                while taken[j] {
                    j = (j + 1) % core.len();
                }
                taken[j] = true;
                let c = core[j] as usize;
                cells.push((c as u32, grid.cells[c].instr));
                grid.cells[c].instr = self.rng.random::<u8>();
                // Matched background cell: same column (same sunlight) if possible,
                // else the nearest column with free cells (a torus-spanning loop owns
                // its whole column). Not in any core; a few random tries per column.
                let x = c % w;
                let h = n / w;
                'cols: for dx in [0i64, -1, 1, -2, 2, -3, 3] {
                    let bx = (x as i64 + dx).rem_euclid(w as i64) as usize;
                    for _ in 0..8 {
                        let y = self.rng.random_range(0..h);
                        let b = y * w + bx;
                        if !in_core[b] && !background.iter().any(|&(bb, _)| bb as usize == b) {
                            background.push((b as u32, grid.cells[b].instr));
                            grid.cells[b].instr = self.rng.random::<u8>();
                            break 'cols;
                        }
                    }
                }
            }
            if cells.is_empty() {
                continue;
            }
            self.pending.push(Pending {
                tick,
                organism_id: *id,
                core_size: core.len(),
                cells,
                background,
                restored: [None; 3],
                bg_restored: [None; 3],
            });
        }
    }

    /// Evaluate due checks; return completed records.
    pub fn check(&mut self, tick: u32, grid: &Grid) -> Vec<ProbeRecord> {
        let mut done = Vec::new();
        let seed = self.seed;
        let window = self.window;
        let frac = |cells: &[(u32, u8)]| -> f64 {
            if cells.is_empty() {
                return f64::NAN;
            }
            cells.iter().filter(|&&(c, b)| grid.cells[c as usize].instr == b).count() as f64 / cells.len() as f64
        };
        self.pending.retain_mut(|p| {
            for (j, &mult) in CHECKS.iter().enumerate() {
                if p.restored[j].is_none() && tick >= p.tick + mult * window {
                    p.restored[j] = Some(frac(&p.cells));
                    p.bg_restored[j] = Some(frac(&p.background));
                }
            }
            if p.restored.iter().all(|r| r.is_some()) {
                done.push(ProbeRecord {
                    kind: "probe",
                    seed,
                    tick: p.tick,
                    organism_id: p.organism_id,
                    core_size: p.core_size,
                    k: p.cells.len(),
                    bg_k: p.background.len(),
                    restored: [p.restored[0].unwrap_or(0.0), p.restored[1].unwrap_or(0.0), p.restored[2].unwrap_or(0.0)],
                    bg_restored: [
                        p.bg_restored[0].unwrap_or(f64::NAN),
                        p.bg_restored[1].unwrap_or(f64::NAN),
                        p.bg_restored[2].unwrap_or(f64::NAN),
                    ],
                });
                false
            } else {
                true
            }
        });
        done
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}
