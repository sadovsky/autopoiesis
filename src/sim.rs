//! The run loop: double-buffered grid, VM step, noise, energy, repair log.

use crate::config::SimConfig;
use crate::energy::{self, SunField};
use crate::grid::{Grid, Topology};
use crate::isa::Instruction;
use crate::noise;
use crate::vm::{StepStats, Vm};
use anyhow::Result;
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use std::collections::VecDeque;

pub type Rng = Xoshiro256PlusPlus;

/// Rolling log of `(source, target)` repair events for the last `window` ticks.
#[derive(Clone, Debug, Default)]
pub struct RepairLog {
    window: u32,
    per_tick: VecDeque<Vec<(u32, u32)>>,
    spare: Vec<Vec<(u32, u32)>>,
}

impl RepairLog {
    pub fn new(window: u32) -> RepairLog {
        RepairLog {
            window: window.max(1),
            per_tick: VecDeque::new(),
            spare: Vec::new(),
        }
    }

    /// Take an empty buffer to fill for the current tick.
    pub fn begin_tick(&mut self) -> Vec<(u32, u32)> {
        let mut v = self.spare.pop().unwrap_or_default();
        v.clear();
        v
    }

    /// Store the current tick's events, evicting ticks older than the window.
    pub fn end_tick(&mut self, events: Vec<(u32, u32)>) {
        self.per_tick.push_back(events);
        while self.per_tick.len() as u32 > self.window {
            if let Some(old) = self.per_tick.pop_front() {
                self.spare.push(old);
            }
        }
    }

    /// Deduplicated, sorted edge set over the window.
    pub fn edges(&self) -> Vec<(u32, u32)> {
        let mut e: Vec<(u32, u32)> = self.per_tick.iter().flatten().copied().collect();
        e.sort_unstable();
        e.dedup();
        e
    }

    pub fn ticks_held(&self) -> usize {
        self.per_tick.len()
    }
}

/// Cumulative counters over a run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunStats {
    pub executed: u64,
    pub deaths: u64,
    pub starved: u64,
    pub repairs: u64,
    pub write_conflicts: u64,
    pub mutations: u64,
    pub injected: u64,
}

impl RunStats {
    fn add(&mut self, s: StepStats) {
        self.executed += s.executed;
        self.deaths += s.deaths;
        self.starved += s.starved;
        self.repairs += s.repairs;
        self.write_conflicts += s.write_conflicts;
    }
}

pub struct Sim {
    pub cfg: SimConfig,
    pub seed: u64,
    pub tick: u32,
    pub cur: Grid,
    next: Grid,
    pub topo: Topology,
    pub rng: Rng,
    vm: Vm,
    sun: SunField,
    diffusion_scratch: Vec<u32>,
    pub repair_log: RepairLog,
    pub stats: RunStats,
    pub last_step: StepStats,
}

impl Sim {
    /// Random initial grid from `seed` (plus the optional seeded ring).
    pub fn new(cfg: SimConfig, seed: u64) -> Result<Sim> {
        cfg.validate()?;
        let mut rng = Rng::seed_from_u64(seed);
        let mut grid = Grid::random(cfg.width, cfg.height, cfg.init_energy_max, &mut rng);
        if cfg.seed_ring {
            inject_ring(&cfg, &mut grid);
        }
        Sim::from_grid(cfg, seed, grid, rng)
    }

    /// Start from an explicit grid (used by tests and experiments).
    pub fn with_grid(cfg: SimConfig, seed: u64, grid: Grid) -> Result<Sim> {
        cfg.validate()?;
        anyhow::ensure!(
            grid.width == cfg.width && grid.height == cfg.height,
            "grid {}x{} does not match config {}x{}",
            grid.width,
            grid.height,
            cfg.width,
            cfg.height
        );
        let rng = Rng::seed_from_u64(seed);
        Sim::from_grid(cfg, seed, grid, rng)
    }

    fn from_grid(cfg: SimConfig, seed: u64, grid: Grid, rng: Rng) -> Result<Sim> {
        let n = cfg.n_cells();
        Ok(Sim {
            topo: Topology::new(cfg.width, cfg.height),
            next: grid.clone(),
            cur: grid,
            vm: Vm::new(n),
            sun: SunField::new(&cfg),
            diffusion_scratch: Vec::new(),
            repair_log: RepairLog::new(cfg.window),
            stats: RunStats::default(),
            last_step: StepStats::default(),
            rng,
            seed,
            tick: 0,
            cfg,
        })
    }

    pub fn noise_rate(&self) -> f64 {
        self.cfg.noise_rate_at(self.tick)
    }

    /// Advance one tick.
    pub fn step(&mut self) {
        self.next.cells.copy_from_slice(&self.cur.cells);
        let mut events = self.repair_log.begin_tick();
        let s = self.vm.step(
            &self.cfg,
            &self.topo,
            &self.cur,
            &mut self.next,
            &mut self.rng,
            &mut events,
        );
        self.repair_log.end_tick(events);
        self.last_step = s;
        self.stats.add(s);

        let rate = self.cfg.noise_rate_at(self.tick);
        self.stats.mutations += noise::apply(&mut self.next, rate, &mut self.rng);
        self.stats.injected += self.sun.apply(&mut self.next);
        energy::diffuse(&self.cfg, &self.topo, &mut self.next, &mut self.diffusion_scratch);

        std::mem::swap(&mut self.cur, &mut self.next);
        self.tick += 1;
    }

    pub fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.step();
        }
    }
}

/// Hand-written repairing ring: a torus-spanning column (band) of `Repair(S)` cells
/// with `tag = 1`, placed at the brightest column unless `seed_ring_x` is set. Every
/// cell repairs the one below it and the column wraps, so the repair graph is one
/// cycle: the smallest structure that is a closed repair loop under copy-self repair.
pub fn inject_ring(cfg: &SimConfig, grid: &mut Grid) {
    let x0 = cfg.seed_ring_x.unwrap_or_else(|| energy::brightest_column(cfg));
    let byte = Instruction::Repair(crate::grid::S).encode();
    for dx in 0..cfg.seed_ring_width {
        let x = (x0 + dx) % cfg.width;
        for y in 0..cfg.height {
            let c = grid.get_mut(x as i64, y as i64);
            c.instr = byte;
            c.tag = 1;
            c.ip = 0;
            c.reg = 0;
            c.energy = cfg.energy_cap / 2;
        }
    }
}
