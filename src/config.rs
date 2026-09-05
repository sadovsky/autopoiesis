//! All simulation tunables. Nothing in the simulation is hardcoded: every knob is a
//! field here with a documented default, so a run is fully described by
//! `(SimConfig, seed)`.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Shape of the energy-injection gradient across the x axis.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SunProfile {
    /// `f(x) = x / (width - 1)`: dark on the left, bright on the right.
    Linear,
    /// Bell curve centred on the middle column, width `sun_sigma_frac * width`.
    Gaussian,
    /// `f(x) = 1` everywhere (experiment 4: "energy gradient off").
    Uniform,
}

/// What `Repair(d)` writes into its target (plan §8: is copying `self.instr` too easy?).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepairSource {
    /// `nbr(d).instr = self.instr` — the plan's original. Makes `Repair` a replicator.
    CopySelf,
    /// `nbr(d).instr = reg` — repair writes a template the cell previously `Load`ed,
    /// so maintenance has to be encoded as a Load/Repair loop.
    Register,
    /// `nbr(d).instr = <byte of the cell behind in the ip chain>` — the neighbourhood
    /// cell executed one step before the `Repair` byte (ip − 1). Structures are
    /// (template, Repair) pairs in neighbourhood order.
    Previous,
}

/// Energy debited per executed instruction.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct Costs {
    pub nop: u16,
    pub move_ip: u16,
    pub load: u16,
    pub store: u16,
    pub repair: u16,
    pub cmp: u16,
    pub jmp_if_zero: u16,
    pub absorb: u16,
    pub share: u16,
    pub set_tag: u16,
    pub halt: u16,
}

impl Default for Costs {
    fn default() -> Self {
        Costs {
            nop: 1,
            move_ip: 1,
            load: 1,
            store: 3,
            repair: 4,
            cmp: 1,
            jmp_if_zero: 1,
            absorb: 1,
            share: 1,
            set_tag: 1,
            halt: 0,
        }
    }
}

/// Linear ramp of the per-cell mutation probability over time.
/// Parsed from the CLI as `from:to:ticks`, e.g. `0.0:0.05:1000`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct NoiseRamp {
    pub from: f64,
    pub to: f64,
    pub over_ticks: u32,
}

impl NoiseRamp {
    pub fn parse(s: &str) -> Result<NoiseRamp> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 {
            bail!("noise ramp must be `from:to:ticks`, got `{s}`");
        }
        let from: f64 = parts[0].parse().context("ramp `from`")?;
        let to: f64 = parts[1].parse().context("ramp `to`")?;
        let over_ticks: u32 = parts[2].parse().context("ramp `ticks`")?;
        for v in [from, to] {
            if !(0.0..=1.0).contains(&v) {
                bail!("noise rates must lie in [0, 1], got {v}");
            }
        }
        Ok(NoiseRamp { from, to, over_ticks })
    }

    /// Noise rate at `tick`; holds at `to` once the ramp is complete.
    pub fn rate_at(&self, tick: u32) -> f64 {
        if self.over_ticks == 0 || tick >= self.over_ticks {
            return self.to;
        }
        let t = tick as f64 / self.over_ticks as f64;
        self.from + (self.to - self.from) * t
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SimConfig {
    /// Grid width in cells (toroidal). Default 128.
    pub width: usize,
    /// Grid height in cells (toroidal). Default 128.
    pub height: usize,
    /// Moore neighbourhood radius. Only 1 is implemented; the direction operand of
    /// the ISA is a compass direction, so larger radii need a different encoding.
    pub radius: usize,
    /// Maximum energy a cell can hold. Excess is destroyed. Default 1000.
    pub energy_cap: u16,
    /// Random initial energy is drawn uniformly from `1..=init_energy_max`. Default 100.
    pub init_energy_max: u16,
    /// Energy injected per tick into a cell at the brightest point of the gradient.
    /// Fractional values are dithered exactly with a per-cell accumulator. Default 4.0.
    pub sun: f64,
    /// Shape of the gradient. Default linear (dark left, bright right).
    pub sun_profile: SunProfile,
    /// Gaussian profile width as a fraction of grid width. Default 0.15.
    pub sun_sigma_frac: f64,
    /// Fraction of each cell's energy that spreads evenly to its 8 neighbours per
    /// tick. Default 0.0 (off).
    pub diffusion: f64,
    /// Per-cell, per-tick probability that `instr` is replaced by a random byte.
    /// Default 0.001. Ignored while `noise_ramp` is set.
    pub noise_rate: f64,
    /// Optional linear ramp of the noise rate; overrides `noise_rate`. Default none.
    pub noise_ramp: Option<NoiseRamp>,
    /// Max energy `Absorb` pulls from a neighbour per execution. Default 4.
    pub absorb_rate: u16,
    /// Max energy `Share` pushes to a neighbour per execution. Default 4.
    pub share_rate: u16,
    /// A cell executing `Halt` stays dormant (ip frozen, no cost) while its energy is
    /// at or below this threshold. Default 20.
    pub halt_threshold: u16,
    /// Per-instruction energy costs.
    pub costs: Costs,
    /// What `Repair` writes. Default `copy_self`.
    pub repair_source: RepairSource,
    /// If true, a `MoveIp`/taken `JmpIfZero` whose destination is the neighbourhood
    /// slot the cell is *currently* fetching from advances instead of jumping. This
    /// breaks the self-loop trap (a neighbour holding `MoveIp` pointing back at itself
    /// freezes every cell whose ip lands on it). Default false.
    pub no_self_jump: bool,
    /// Repair-graph window in ticks: edge `a -> b` exists if `a` repaired `b` within
    /// the last `window` ticks. Also the grace period before an unmatched organism is
    /// declared dead. Default 100.
    pub window: u32,
    /// Minimum strongly-connected-component size to count as a candidate organism.
    /// Default 3.
    pub min_size: usize,
    /// Lag Δ (ticks) for self-mutual-information `MI(R_t ; R_{t-Δ})`. Rounded up to a
    /// multiple of `snapshot_every` when analysed. Default 100.
    pub mi_lag: u32,
    /// The MI region is the organism's core dilated by this many cells so that the
    /// boundary it maintains is measured too. Default 1.
    pub mi_dilate: usize,
    /// Number of random equal-sized regions averaged for the MI baseline. Default 8.
    pub mi_samples: usize,
    /// Cap on the number of region cells fed into one MI estimate (larger regions are
    /// evenly subsampled; the random baseline uses the same count). 0 = no cap.
    /// Default 512.
    pub mi_max_cells: usize,
    /// Number of random permutations of the time pairing used to estimate and subtract
    /// the finite-sample bias of the MI estimate (shuffle correction). Default 4.
    pub mi_shuffles: usize,
    /// Resolution floor (bits) added to both sides of the persistence ratio; roughly
    /// the sampling noise of the corrected MI estimate at a few hundred samples.
    /// Default 0.05.
    pub mi_floor: f64,
    /// Interval in ticks between snapshot files. Default 100.
    pub snapshot_every: u32,
    /// Interval in ticks between online metric frames (repair-graph SCCs, MI, organism
    /// tracking). Several frames per `window` let MI samples pool. Default 20.
    pub analysis_every: u32,
    /// Emit a frame record every this many ticks (a multiple of `analysis_every`;
    /// analysis itself still runs every `analysis_every`). Default 100.
    pub report_every: u32,
    /// Per-frame organism rows are limited to the union of the top N by core size and
    /// the top N by persistence; frame aggregates always cover every organism.
    /// Default 10.
    pub report_top: usize,
    /// Life records are emitted only for organisms that lived at least this many
    /// ticks; frame-level counts include everything. Default 100.
    pub report_min_life: u32,
    /// Persistence above which an organism counts as persistent in frame aggregates.
    /// Default 3.0.
    pub persistent_threshold: f64,
    /// Inject a hand-written repairing ring (a torus-spanning column of `Repair(S)`)
    /// at t = 0. Default false.
    pub seed_ring: bool,
    /// Column for the seeded ring; defaults to the brightest column of the gradient.
    pub seed_ring_x: Option<usize>,
    /// Width in cells of the seeded ring band. Default 1.
    pub seed_ring_width: usize,
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            width: 128,
            height: 128,
            radius: 1,
            energy_cap: 1000,
            init_energy_max: 100,
            sun: 4.0,
            sun_profile: SunProfile::Linear,
            sun_sigma_frac: 0.15,
            diffusion: 0.0,
            noise_rate: 0.001,
            noise_ramp: None,
            absorb_rate: 4,
            share_rate: 4,
            halt_threshold: 20,
            costs: Costs::default(),
            repair_source: RepairSource::CopySelf,
            no_self_jump: false,
            window: 100,
            min_size: 3,
            mi_lag: 100,
            mi_dilate: 1,
            mi_samples: 8,
            mi_max_cells: 512,
            mi_shuffles: 4,
            mi_floor: 0.05,
            snapshot_every: 100,
            analysis_every: 20,
            report_every: 100,
            report_top: 10,
            report_min_life: 100,
            persistent_threshold: 3.0,
            seed_ring: false,
            seed_ring_x: None,
            seed_ring_width: 1,
        }
    }
}

impl SimConfig {
    pub fn load(path: &Path) -> Result<SimConfig> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let cfg: SimConfig = serde_json::from_str(&text)
            .with_context(|| format!("parsing config {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        if self.width < 3 || self.height < 3 {
            bail!("grid must be at least 3x3 (got {}x{})", self.width, self.height);
        }
        if self.width * self.height > u32::MAX as usize {
            bail!("grid too large");
        }
        if self.radius != 1 {
            bail!("only radius = 1 is implemented (got {})", self.radius);
        }
        if self.energy_cap == 0 {
            bail!("energy_cap must be > 0");
        }
        if self.init_energy_max == 0 || self.init_energy_max > self.energy_cap {
            bail!("init_energy_max must lie in 1..=energy_cap");
        }
        if !(0.0..=1.0).contains(&self.noise_rate) {
            bail!("noise_rate must lie in [0, 1]");
        }
        if !(0.0..=1.0).contains(&self.diffusion) {
            bail!("diffusion must lie in [0, 1]");
        }
        if self.sun < 0.0 || !self.sun.is_finite() {
            bail!("sun must be finite and >= 0");
        }
        if self.snapshot_every == 0 || self.analysis_every == 0 || self.report_every == 0 {
            bail!("snapshot_every, analysis_every and report_every must be > 0");
        }
        if !self.report_every.is_multiple_of(self.analysis_every) {
            bail!("report_every must be a multiple of analysis_every");
        }
        if self.window == 0 {
            bail!("window must be > 0");
        }
        if self.min_size == 0 {
            bail!("min_size must be > 0");
        }
        if self.seed_ring_width == 0 {
            bail!("seed_ring_width must be > 0");
        }
        Ok(())
    }

    /// Noise rate in force at `tick`.
    pub fn noise_rate_at(&self, tick: u32) -> f64 {
        match &self.noise_ramp {
            Some(r) => r.rate_at(tick),
            None => self.noise_rate,
        }
    }

    pub fn n_cells(&self) -> usize {
        self.width * self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_parses_and_interpolates() {
        let r = NoiseRamp::parse("0.0:0.05:1000").unwrap();
        assert_eq!(r.rate_at(0), 0.0);
        assert!((r.rate_at(500) - 0.025).abs() < 1e-12);
        assert_eq!(r.rate_at(1000), 0.05);
        assert_eq!(r.rate_at(5000), 0.05);
        assert!(NoiseRamp::parse("1:2").is_err());
        assert!(NoiseRamp::parse("0:2:10").is_err());
    }

    #[test]
    fn default_config_roundtrips_json() {
        let cfg = SimConfig::default();
        cfg.validate().unwrap();
        let s = serde_json::to_string(&cfg).unwrap();
        let back: SimConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(cfg, back);
        // Partial configs fill in defaults.
        let partial: SimConfig = serde_json::from_str(r#"{"width": 16}"#).unwrap();
        assert_eq!(partial.width, 16);
        assert_eq!(partial.height, 128);
    }
}
