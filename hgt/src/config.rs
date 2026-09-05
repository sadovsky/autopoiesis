//! Every tunable in one place: a run is fully described by `(HgtConfig, seed)` in the
//! simulated transport, and by `(HgtConfig, seed)` plus the operating system's scheduling
//! decisions over real sockets.
//!
//! Same recipe as the grid crate: one flat struct, `#[serde(default)]` so any subset of
//! the JSON loads, a hand-written `Default` so the defaults are literals you can read,
//! and a `validate` that refuses nonsense before a run starts rather than during it.

use crate::node::Policy;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// What each action costs the node that takes it. Everything is paid out of the same
/// energy pool a node needs to survive a stressor, so every capability is a trade-off.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct Costs {
    /// Staying alive for one tick. Default 4.
    pub upkeep: u32,
    /// Carrying one gene for one tick, on top of the trial cost of running it. This is
    /// why hoarding is not free, and therefore why a gene that answers nothing decays out
    /// of a population. Default 1.
    pub gene: u32,
    /// Running one gene against a stressor. Default 1.
    pub trial: u32,
    /// Donating: opening a connection and pushing a plasmid. Default 6.
    pub conjugate: u32,
    /// Receiving: integrating a gene into the genome. Default 3.
    pub integrate: u32,
    /// Dividing, on top of handing half the energy to the child. Default 10.
    pub fission: u32,
}

impl Default for Costs {
    fn default() -> Costs {
        Costs { upkeep: 4, gene: 1, trial: 1, conjugate: 6, integrate: 3, fission: 10 }
    }
}

/// Which transfer mechanisms are switched on. The A/B experiment is this struct.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct Mechanisms {
    pub conjugation: bool,
    pub transformation: bool,
    pub transduction: bool,
}

impl Default for Mechanisms {
    fn default() -> Mechanisms {
        Mechanisms { conjugation: true, transformation: true, transduction: true }
    }
}

impl Mechanisms {
    pub fn none() -> Mechanisms {
        Mechanisms { conjugation: false, transformation: false, transduction: false }
    }

    pub fn any(&self) -> bool {
        self.conjugation || self.transformation || self.transduction
    }

    /// `none`, `all`, or a comma list of `conj`, `transf`, `transd`.
    pub fn parse(s: &str) -> Result<Mechanisms> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("none") || s.is_empty() {
            return Ok(Mechanisms::none());
        }
        if s.eq_ignore_ascii_case("all") {
            return Ok(Mechanisms::default());
        }
        let mut m = Mechanisms::none();
        for part in s.split(',') {
            match part.trim() {
                "conj" | "conjugation" => m.conjugation = true,
                "transf" | "transformation" => m.transformation = true,
                "transd" | "transduction" => m.transduction = true,
                other => bail!("unknown mechanism `{other}`; expected none, all, conj, transf, transd"),
            }
        }
        Ok(m)
    }

    /// Canonical label for records and filenames.
    pub fn label(&self) -> String {
        if !self.any() {
            return "none".to_string();
        }
        let mut parts = Vec::new();
        if self.conjugation {
            parts.push("conj");
        }
        if self.transformation {
            parts.push("transf");
        }
        if self.transduction {
            parts.push("transd");
        }
        parts.join(",")
    }
}

/// The whole sandbox, as data.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct HgtConfig {
    /// Nodes in the initial population. Default 48.
    pub nodes: usize,
    /// Hard ceiling on live nodes; fission is refused above it. Default 512.
    pub max_nodes: usize,
    /// Peers each node knows initially — the network is sparse, so a gene has to travel.
    /// Default 4.
    pub degree: usize,

    /// Energy a founder starts with. Default 120.
    pub energy_init: u32,
    /// Energy a node may hold. Default 400.
    pub energy_cap: u32,
    /// Energy at which a node divides. Default 240.
    pub fission_threshold: u32,
    pub costs: Costs,
    /// Energy gained by answering the tick's stressor. Default 14.
    pub reward: u32,
    /// Energy lost by failing to answer it. Default 22.
    pub damage: u32,

    /// How many distinct stressors exist. Default 4.
    pub hazard_kinds: u8,
    /// Ticks between stressor shifts — the length of an epoch. Default 300.
    pub epoch_ticks: u32,
    /// Instruction budget for one gene against one stressor. Default 96.
    pub vm_budget: u32,

    /// Per-byte substitution probability when a genome is copied at fission. Default 0.004.
    pub mutation_rate: f64,
    /// Probability a mobile gene is dropped at fission — plasmids segregate badly, which
    /// is how a gene nobody needs right now leaves a population. Default 0.005.
    pub plasmid_loss: f64,

    /// Genes a node can hold. Bounded because every mutated copy in flight is a new gene,
    /// and an unbounded genome fills with junk until its upkeep starves the node. Taking
    /// on a new gene evicts the most recently acquired one that has never answered
    /// anything. Default 5.
    pub max_genes: usize,

    /// Number of strains — the restriction-modification barrier is between them. Default 4.
    pub strains: u8,
    /// Probability a child's strain drifts from its parent's. Default 0.01.
    pub strain_drift: f64,
    /// Probability a node refuses a gene arriving from a different strain. Transfer is a
    /// rate, not a certainty, and relatedness is what sets it. Default 0.75.
    pub restriction: f64,

    /// Probability a node attempts a conjugation each tick. Default 0.08.
    pub conjugation_rate: f64,
    /// Probability a node takes up one held fragment each tick. Default 0.10.
    pub transformation_rate: f64,
    /// Probability a node spawns a phage carrying one of its genes each tick. Default 0.02.
    pub transduction_rate: f64,
    /// Hops a phage makes before it decays. Default 6.
    pub phage_hops: u32,
    /// Probability an infected node is damaged by the phage. Default 0.10.
    pub lysis_prob: f64,
    /// Energy lost when that happens. Default 30.
    pub lysis_damage: u32,
    /// Ticks a dead node's released fragments remain takeable. Default 30.
    pub fragment_ttl: u32,
    /// A node gossips its peer list once every this many ticks, so peer lists survive the
    /// deaths of the nodes in them. Default 16.
    pub gossip_every: u32,

    /// Delivery delay in ticks for the simulated transport. Default 1.
    pub latency: u32,
    /// Probability the simulated transport drops a message. Default 0.0.
    pub loss: f64,

    /// Founders seeded with the resistance gene for each *future* stressor. Two, not one,
    /// so that a run does not turn on whether a single node happens to die early.
    /// Default 2.
    pub founder_carriers: usize,

    /// Ticks between metric frames. Default 10.
    pub analysis_every: u32,
    /// Ticks between emitted frame records; must be a multiple of `analysis_every`.
    /// Default 50.
    pub report_every: u32,
    /// Genes reported per frame, by carrier count. Default 10.
    pub report_top: usize,

    pub mechanisms: Mechanisms,
    /// How nodes decide what to give and what to take. Default `always_accept`.
    pub policy: Policy,
}

impl Default for HgtConfig {
    fn default() -> HgtConfig {
        HgtConfig {
            nodes: 48,
            max_nodes: 512,
            degree: 4,

            energy_init: 120,
            energy_cap: 400,
            fission_threshold: 240,
            costs: Costs::default(),
            reward: 14,
            damage: 22,

            hazard_kinds: 4,
            epoch_ticks: 300,
            vm_budget: 96,

            mutation_rate: 0.004,
            plasmid_loss: 0.005,

            max_genes: 5,

            strains: 4,
            strain_drift: 0.01,
            restriction: 0.75,

            conjugation_rate: 0.08,
            transformation_rate: 0.10,
            transduction_rate: 0.02,
            phage_hops: 6,
            lysis_prob: 0.10,
            lysis_damage: 30,
            fragment_ttl: 30,
            gossip_every: 16,

            latency: 1,
            loss: 0.0,

            founder_carriers: 2,

            analysis_every: 10,
            report_every: 50,
            report_top: 10,

            mechanisms: Mechanisms::default(),
            policy: Policy::AlwaysAccept,
        }
    }
}

impl HgtConfig {
    pub fn load(path: &Path) -> Result<HgtConfig> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let cfg: HgtConfig = serde_json::from_str(&text)
            .with_context(|| format!("parsing config {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        if self.nodes == 0 {
            bail!("nodes must be > 0");
        }
        if self.max_nodes < self.nodes {
            bail!("max_nodes ({}) must be >= nodes ({})", self.max_nodes, self.nodes);
        }
        if self.degree == 0 {
            bail!("degree must be > 0, or nothing can ever be transferred");
        }
        if self.hazard_kinds == 0 {
            bail!("hazard_kinds must be > 0");
        }
        if self.epoch_ticks == 0 {
            bail!("epoch_ticks must be > 0");
        }
        if self.vm_budget == 0 {
            bail!("vm_budget must be > 0");
        }
        if self.energy_cap == 0 || self.energy_init == 0 {
            bail!("energy_init and energy_cap must be > 0");
        }
        if self.fission_threshold > self.energy_cap {
            bail!(
                "fission_threshold ({}) exceeds energy_cap ({}), so nothing can ever divide",
                self.fission_threshold, self.energy_cap
            );
        }
        if self.gossip_every == 0 {
            bail!("gossip_every must be > 0");
        }
        if self.analysis_every == 0 || self.report_every == 0 {
            bail!("analysis_every and report_every must be > 0");
        }
        if !self.report_every.is_multiple_of(self.analysis_every) {
            bail!(
                "report_every ({}) must be a multiple of analysis_every ({})",
                self.report_every, self.analysis_every
            );
        }
        for (name, v) in [
            ("mutation_rate", self.mutation_rate),
            ("plasmid_loss", self.plasmid_loss),
            ("strain_drift", self.strain_drift),
            ("restriction", self.restriction),
            ("conjugation_rate", self.conjugation_rate),
            ("transformation_rate", self.transformation_rate),
            ("transduction_rate", self.transduction_rate),
            ("lysis_prob", self.lysis_prob),
            ("loss", self.loss),
        ] {
            if !(0.0..=1.0).contains(&v) {
                bail!("{name} must be in [0, 1], got {v}");
            }
        }
        if self.max_genes == 0 {
            bail!("max_genes must be > 0");
        }
        if self.strains == 0 {
            bail!("strains must be > 0");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_config_loads_and_keeps_the_other_defaults() {
        let cfg: HgtConfig = serde_json::from_str(r#"{"nodes": 8, "costs": {"upkeep": 1}}"#).unwrap();
        assert_eq!(cfg.nodes, 8);
        assert_eq!(cfg.costs.upkeep, 1);
        assert_eq!(cfg.costs.gene, Costs::default().gene);
        assert_eq!(cfg.epoch_ticks, HgtConfig::default().epoch_ticks);
        cfg.validate().unwrap();
    }

    #[test]
    fn validation_rejects_impossible_worlds() {
        let bad = HgtConfig { report_every: 55, ..HgtConfig::default() };
        assert!(bad.validate().is_err(), "report_every must be a multiple of analysis_every");
        let bad = HgtConfig { restriction: 1.5, ..HgtConfig::default() };
        assert!(bad.validate().is_err());
        let bad = HgtConfig { fission_threshold: 10_000, ..HgtConfig::default() };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn mechanism_lists_parse_and_round_trip() {
        assert_eq!(Mechanisms::parse("none").unwrap(), Mechanisms::none());
        assert_eq!(Mechanisms::parse("all").unwrap(), Mechanisms::default());
        let m = Mechanisms::parse("conj,transd").unwrap();
        assert!(m.conjugation && m.transduction && !m.transformation);
        assert_eq!(Mechanisms::parse(&m.label()).unwrap(), m);
        assert!(Mechanisms::parse("plasmids").is_err());
    }
}
