//! Energy injection along a spatial gradient, plus optional diffusion.
//!
//! Injection is fractional (`sun * f(x)` per tick) but energy is integral, so each
//! cell carries a 16.16 fixed-point accumulator: the integer part is credited each
//! tick and the remainder carried forward. This is exact on average and fully
//! deterministic without consuming RNG.

use crate::config::{SimConfig, SunProfile};
use crate::grid::{Grid, Topology};

const FRAC_BITS: u32 = 16;
const FRAC_MASK: u32 = (1 << FRAC_BITS) - 1;

/// Gradient factor `f(x)` in `[0, 1]` for column `x`.
pub fn profile(cfg: &SimConfig, x: usize) -> f64 {
    let w = cfg.width.max(2) as f64;
    match cfg.sun_profile {
        SunProfile::Linear => x as f64 / (w - 1.0),
        SunProfile::Gaussian => {
            let mu = (w - 1.0) / 2.0;
            let sigma = (cfg.sun_sigma_frac * w).max(1e-9);
            let z = (x as f64 - mu) / sigma;
            (-0.5 * z * z).exp()
        }
        SunProfile::Uniform => 1.0,
    }
}

/// Column with the largest injection (ties → lowest x).
pub fn brightest_column(cfg: &SimConfig) -> usize {
    let mut best = 0;
    let mut best_v = f64::NEG_INFINITY;
    for x in 0..cfg.width {
        let v = profile(cfg, x);
        if v > best_v {
            best_v = v;
            best = x;
        }
    }
    best
}

pub struct SunField {
    /// Per-column injection in 16.16 fixed point.
    per_column: Vec<u32>,
    /// Per-cell fractional accumulator.
    acc: Vec<u32>,
    width: usize,
    cap: u16,
}

impl SunField {
    pub fn new(cfg: &SimConfig) -> SunField {
        let per_column = (0..cfg.width)
            .map(|x| {
                let v = cfg.sun * profile(cfg, x);
                ((v * (1u64 << FRAC_BITS) as f64).round().max(0.0)) as u32
            })
            .collect();
        SunField {
            per_column,
            acc: vec![0; cfg.n_cells()],
            width: cfg.width,
            cap: cfg.energy_cap,
        }
    }

    /// Nominal injection for column `x` in energy units per tick (fractional).
    pub fn rate(&self, x: usize) -> f64 {
        self.per_column[x] as f64 / (1u64 << FRAC_BITS) as f64
    }

    pub fn is_zero(&self) -> bool {
        self.per_column.iter().all(|&v| v == 0)
    }

    /// Credit every cell with this tick's sunlight. Returns the total injected.
    pub fn apply(&mut self, grid: &mut Grid) -> u64 {
        if self.is_zero() {
            return 0;
        }
        let mut total = 0u64;
        let w = self.width;
        for (i, cell) in grid.cells.iter_mut().enumerate() {
            let inc = self.per_column[i % w];
            if inc == 0 {
                continue;
            }
            let acc = self.acc[i] + inc;
            let whole = (acc >> FRAC_BITS) as u16;
            self.acc[i] = acc & FRAC_MASK;
            if whole > 0 {
                let before = cell.energy;
                cell.energy = self.cap.min(cell.energy.saturating_add(whole));
                total += (cell.energy - before) as u64;
            }
        }
        total
    }
}

/// Spread a fraction `cfg.diffusion` of each cell's energy evenly over its eight
/// neighbours. Integer arithmetic: each neighbour receives `floor(out / 8)`, the
/// remainder stays with the source, and receivers are clamped to the cap. Never
/// creates energy. `scratch` must have `grid.len()` entries and is overwritten.
pub fn diffuse(cfg: &SimConfig, topo: &Topology, grid: &mut Grid, scratch: &mut Vec<u32>) {
    if cfg.diffusion <= 0.0 {
        return;
    }
    let n = grid.len();
    scratch.clear();
    scratch.resize(n, 0);
    let frac = ((cfg.diffusion * (1u64 << FRAC_BITS) as f64).round().max(0.0)) as u64;
    for i in 0..n {
        let e = grid.cells[i].energy as u64;
        let out = ((e * frac) >> FRAC_BITS) as u32;
        let share = out / 8;
        if share == 0 {
            continue;
        }
        grid.cells[i].energy -= (share * 8) as u16;
        for d in 0..8u8 {
            let j = topo.neighbor(i, d);
            scratch[j] += share;
        }
    }
    let cap = cfg.energy_cap as u32;
    for i in 0..n {
        if scratch[i] > 0 {
            let e = grid.cells[i].energy as u32 + scratch[i];
            grid.cells[i].energy = e.min(cap) as u16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_profile_spans_zero_to_one() {
        let cfg = SimConfig {
            width: 11,
            ..SimConfig::default()
        };
        assert_eq!(profile(&cfg, 0), 0.0);
        assert!((profile(&cfg, 5) - 0.5).abs() < 1e-12);
        assert_eq!(profile(&cfg, 10), 1.0);
        assert_eq!(brightest_column(&cfg), 10);
        let g = SimConfig {
            sun_profile: SunProfile::Gaussian,
            width: 11,
            ..SimConfig::default()
        };
        assert_eq!(brightest_column(&g), 5);
    }

    #[test]
    fn fractional_injection_is_exact_on_average() {
        let cfg = SimConfig {
            width: 4,
            height: 1,
            sun: 0.25,
            sun_profile: SunProfile::Uniform,
            energy_cap: 10_000,
            ..SimConfig::default()
        };
        let mut sun = SunField::new(&cfg);
        let mut g = Grid::new(4, 1);
        for _ in 0..400 {
            sun.apply(&mut g);
        }
        for c in &g.cells {
            assert_eq!(c.energy, 100);
        }
    }

    #[test]
    fn diffusion_conserves_below_cap() {
        let cfg = SimConfig {
            width: 8,
            height: 8,
            diffusion: 0.5,
            energy_cap: 60_000,
            ..SimConfig::default()
        };
        let topo = Topology::new(8, 8);
        let mut g = Grid::new(8, 8);
        g.cells[10].energy = 800;
        g.cells[33].energy = 123;
        let before = g.total_energy();
        let mut scratch = Vec::new();
        for _ in 0..20 {
            diffuse(&cfg, &topo, &mut g, &mut scratch);
        }
        assert_eq!(g.total_energy(), before);
        assert!(g.cells[10].energy < 800);
    }
}
