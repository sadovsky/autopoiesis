//! What the sandbox measures, computed from the event stream and nothing else.
//!
//! The analyzer reconstructs every node's genome from `Birth`, `Acquire` and `Lose`
//! rather than reading the live world, so `hgt run --metrics` and `hgt analyze` are the
//! same computation over the same input rather than two implementations that happen to
//! agree. It knows which genes are resistance genes because it can derive them: the
//! stressor schedule is a function of `(config, seed)`, and the analyzer has both.
//!
//! The measurement that matters is **incongruence**: the share of a gene's carriers that
//! received it sideways rather than inheriting it. It is identically zero when no
//! mechanism is on, and it is the thing that makes a gene tree disagree with a family
//! tree — the signature that told biologists horizontal transfer was happening at all.

use crate::config::HgtConfig;
use crate::event::{Cause, Event, Refusal};
use crate::gene::{Acquisition, GeneId, NodeId, short_id};
use crate::node::Policy;
use crate::hazard::Environment;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Cumulative acquisitions by how the gene arrived.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Acquisitions {
    pub birth: u64,
    pub conjugation: u64,
    pub transformation: u64,
    pub transduction: u64,
}

impl Acquisitions {
    fn count(&mut self, via: Acquisition) {
        match via {
            Acquisition::Founder | Acquisition::Birth => self.birth += 1,
            Acquisition::Conjugation => self.conjugation += 1,
            Acquisition::Transformation => self.transformation += 1,
            Acquisition::Transduction => self.transduction += 1,
        }
    }

    pub fn lateral(&self) -> u64 {
        self.conjugation + self.transformation + self.transduction
    }
}

/// Cumulative outcomes of transfer attempts.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Refusals {
    pub accepted: u64,
    pub redundant: u64,
    pub restricted: u64,
    pub broke: u64,
    pub declined: u64,
    pub full: u64,
    pub immune: u64,
}

/// Transfer attempts and successes at one strain distance: the barrier, measured.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BarrierRow {
    pub distance: u32,
    pub attempts: u64,
    pub accepted: u64,
    /// Attempts that could never have succeeded because the recipient already held the
    /// gene. Phages retry indiscriminately, so without this the barrier's effect is
    /// buried under redundant traffic.
    pub redundant: u64,
}

/// How the live population is divided between ways of behaving. A heritable trait, so
/// this is the answer to "does a free rider take over?".
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Policies {
    pub always_accept: u32,
    pub selfish: u32,
    pub thrifty: u32,
}

/// The two halves of a split network, counted separately. A partition is only
/// interesting if the sides can be seen to fare differently.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sides {
    pub here: u32,
    pub there: u32,
    /// Nodes on each side holding a gene that answers the current stressor.
    pub here_solvers: u32,
    pub there_solvers: u32,
}

/// One gene's standing in the live population.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct GeneRow {
    pub gene: String,
    /// The stressor this gene answers, if it is one of the compiled resistance genes.
    pub resists: Option<u8>,
    pub carriers: u32,
    pub freq: f64,
    /// Share of those carriers that acquired it laterally rather than inheriting it.
    pub incongruence: f64,
}

/// One analysis frame.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FrameRecord {
    pub kind: &'static str,
    pub seed: u64,
    pub tick: u32,
    pub epoch: u32,
    pub hazard: u8,
    pub population: u32,
    pub energy_mean: f64,
    /// Share of the live population that answered this tick's stressor.
    pub survival: f64,
    pub births: u64,
    pub deaths: u64,
    pub lysed: u64,
    pub distinct_genes: usize,
    pub genome_mean: f64,
    /// Share of all gene copies now held that were acquired laterally.
    pub lateral_share: f64,
    /// Is the network split in two right now?
    pub partitioned: bool,
    pub sides: Sides,
    /// How far the two sides' gene pools have drifted apart: one minus the Jaccard
    /// similarity of the sets of genes present on each side. Reported whether or not a
    /// partition is in force, so the rise while cut and the fall after healing are both
    /// visible.
    pub divergence: f64,
    /// The seeded gene answering each stressor kind, and how far it has spread.
    pub resistance: Vec<GeneRow>,
    /// Carriers of *any* gene known to answer each stressor — the seeded one plus every
    /// variant discovered since. This is the number that says whether the population can
    /// meet a stressor at all.
    pub solvers: Vec<u32>,
    /// Mutated genes found to answer a stressor, cumulative, and how many of those were
    /// a different program rather than a rediscovery of the seeded bytes.
    pub discoveries: u64,
    pub novel_discoveries: u64,
    /// The commonest genes, whatever they are.
    pub top: Vec<GeneRow>,
    pub acquisitions: Acquisitions,
    pub refusals: Refusals,
    pub barrier: Vec<BarrierRow>,
    pub policies: Policies,
}

/// One epoch: the unit the A/B is read in. Did the population survive the shift, and how
/// long did the gene that answers the new stressor take to sweep?
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct EpochRecord {
    pub kind: &'static str,
    pub seed: u64,
    pub epoch: u32,
    pub hazard: u8,
    pub start_tick: u32,
    pub end_tick: u32,
    pub start_population: u32,
    pub min_population: u32,
    pub end_population: u32,
    pub deaths: u64,
    /// Frequency of the answering gene when the stressor arrived.
    pub start_freq: f64,
    pub end_freq: f64,
    /// Ticks from the shift until that gene reached `fixation_freq`. `None` means it
    /// never did — either it swept too slowly to matter or the population died first.
    pub rescue_ticks: Option<u32>,
    /// Lateral acquisitions of the answering gene during this epoch.
    pub rescued_laterally: u64,
    pub survived: bool,
}

/// One gene's whole history, emitted at the end of the run.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct GeneRecord {
    pub kind: &'static str,
    pub seed: u64,
    pub gene: String,
    pub resists: Option<u8>,
    pub first_seen: u32,
    pub origin: NodeId,
    pub max_carriers: u32,
    pub vertical: u64,
    pub lateral: u64,
    pub fixed_at: Option<u32>,
    pub extinct_at: Option<u32>,
    pub alive_at_end: bool,
}

/// Anything the analyzer emits. Serialized untagged — each variant carries its own
/// `kind`, exactly like the grid crate's JSONL.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Record {
    Frame(Box<FrameRecord>),
    Epoch(Box<EpochRecord>),
    Gene(Box<GeneRecord>),
}

fn resistance_map(resistance: &[GeneId]) -> BTreeMap<GeneId, u8> {
    resistance.iter().enumerate().map(|(k, g)| (*g, k as u8)).collect()
}

#[derive(Clone, Debug, Default)]
struct NodeState {
    /// Gene id → how *this* node got it.
    genes: BTreeMap<GeneId, Acquisition>,
    policy: Policy,
}

#[derive(Clone, Debug)]
struct GeneStat {
    first_seen: u32,
    origin: NodeId,
    carriers: u32,
    lateral_carriers: u32,
    max_carriers: u32,
    vertical: u64,
    lateral: u64,
    fixed_at: Option<u32>,
    extinct_at: Option<u32>,
}

/// The two graphs this sandbox exists to compare: who descended from whom, and who
/// *gave* what to whom. In a world without transfer the second is empty and a gene's
/// history is a subtree of the first; every edge in it is a place where the two disagree.
#[derive(Clone, Debug, Default)]
pub struct Trees {
    /// (node, parent, born, died) for every node that ever lived.
    pub ancestry: Vec<(NodeId, Option<NodeId>, u32, Option<u32>)>,
    /// (tick, gene, donor, recipient, mechanism) for every lateral acquisition.
    pub transfers: Vec<(u32, GeneId, NodeId, NodeId, Acquisition)>,
}

impl Trees {
    /// The ancestry as tab-separated rows.
    pub fn ancestry_tsv(&self) -> String {
        let mut out = String::from("node\tparent\tborn\tdied\n");
        for (node, parent, born, died) in &self.ancestry {
            let parent = parent.map_or(String::from("-"), |p| p.to_string());
            let died = died.map_or(String::from("-"), |d| d.to_string());
            out.push_str(&format!("{node}\t{parent}\t{born}\t{died}\n"));
        }
        out
    }

    /// Lateral transfers as tab-separated rows: the edges that are not in the tree.
    pub fn transfers_tsv(&self) -> String {
        let mut out = String::from("tick\tgene\tfrom\tto\tvia\n");
        for (tick, gene, from, to, via) in &self.transfers {
            out.push_str(&format!("{tick}\t{}\t{from}\t{to}\t{}\n", short_id(*gene), via.name()));
        }
        out
    }

    /// The ancestry as Newick, one line per founder — the format every tree viewer reads.
    /// Branch lengths are lifespans in ticks. Built with an explicit stack, because a
    /// long run's lineages are deeper than a comfortable recursion.
    pub fn newick(&self) -> String {
        let mut children: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        let mut info: BTreeMap<NodeId, (u32, Option<u32>)> = BTreeMap::new();
        let mut roots = Vec::new();
        for (node, parent, born, died) in &self.ancestry {
            info.insert(*node, (*born, *died));
            match parent {
                Some(p) => children.entry(*p).or_default().push(*node),
                None => roots.push(*node),
            }
        }
        let length = |node: NodeId, info: &BTreeMap<NodeId, (u32, Option<u32>)>| -> u32 {
            match info.get(&node) {
                Some((born, Some(died))) => died.saturating_sub(*born),
                Some(_) => 0,
                None => 0,
            }
        };

        let mut out = String::new();
        for root in roots {
            // Post-order without recursion: push a node, then its children; on the way
            // back up, fold each finished child list into its parent's string.
            let mut stack = vec![(root, false)];
            let mut done: BTreeMap<NodeId, String> = BTreeMap::new();
            while let Some((node, expanded)) = stack.pop() {
                let kids = children.get(&node).cloned().unwrap_or_default();
                if !expanded && !kids.is_empty() {
                    stack.push((node, true));
                    for kid in kids {
                        stack.push((kid, false));
                    }
                    continue;
                }
                let kids = children.get(&node).cloned().unwrap_or_default();
                let label = format!("n{node}:{}", length(node, &info));
                let text = if kids.is_empty() {
                    label
                } else {
                    let inner: Vec<String> = kids
                        .iter()
                        .map(|k| done.remove(k).unwrap_or_else(|| format!("n{k}:0")))
                        .collect();
                    format!("({}){label}", inner.join(","))
                };
                done.insert(node, text);
            }
            if let Some(text) = done.remove(&root) {
                out.push_str(&text);
                out.push_str(";\n");
            }
        }
        out
    }
}

#[derive(Clone, Debug)]
struct EpochAccum {
    epoch: u32,
    hazard: u8,
    start_tick: u32,
    start_population: u32,
    min_population: u32,
    deaths: u64,
    start_freq: f64,
    rescue_tick: Option<u32>,
    rescued_laterally: u64,
}

pub struct Analyzer {
    seed: u64,
    analysis_every: u32,
    report_top: usize,
    report_min_carriers: u32,
    fixation_freq: f64,
    epoch_ticks: u32,
    /// The compiled resistance gene for each stressor kind, derived from (config, seed).
    resistance: Vec<GeneId>,
    /// Every gene id known to answer each stressor: the seeded one, plus discoveries.
    solving: BTreeMap<GeneId, u8>,
    discoveries: u64,
    novel_discoveries: u64,
    partitioned: bool,
    partition_frac: f64,
    trees: Trees,
    /// Where each node's row lives in `trees.ancestry`, so recording a death is a lookup
    /// rather than a scan of every node that ever lived.
    ancestry_index: BTreeMap<NodeId, usize>,
    nodes: BTreeMap<NodeId, NodeState>,
    genes: BTreeMap<GeneId, GeneStat>,
    acquisitions: Acquisitions,
    refusals: Refusals,
    barrier: BTreeMap<u32, BarrierRow>,
    births: u64,
    deaths: u64,
    lysed: u64,
    epoch: Option<EpochAccum>,
    last_tick: u32,
}

impl Analyzer {
    pub fn new(cfg: &HgtConfig, seed: u64) -> Analyzer {
        let env = Environment::new(cfg, seed);
        let resistance: Vec<GeneId> = (0..env.kinds())
            .map(|k| crate::gene::fnv1a(&env.resistance_gene(k)))
            .collect();
        let solving = resistance_map(&resistance);
        Analyzer {
            seed,
            analysis_every: cfg.analysis_every,
            report_top: cfg.report_top,
            report_min_carriers: cfg.report_min_carriers,
            fixation_freq: cfg.fixation_freq,
            epoch_ticks: cfg.epoch_ticks,
            resistance,
            solving,
            discoveries: 0,
            novel_discoveries: 0,
            partitioned: false,
            partition_frac: cfg.partition_frac,
            trees: Trees::default(),
            ancestry_index: BTreeMap::new(),
            nodes: BTreeMap::new(),
            genes: BTreeMap::new(),
            acquisitions: Acquisitions::default(),
            refusals: Refusals::default(),
            barrier: BTreeMap::new(),
            births: 0,
            deaths: 0,
            lysed: 0,
            epoch: None,
            last_tick: 0,
        }
    }

    pub fn population(&self) -> usize {
        self.nodes.len()
    }

    /// The family tree and the transfer graph, for export.
    pub fn trees(&self) -> &Trees {
        &self.trees
    }

    /// Which stressor a gene answers, if any — including one discovered mid-run.
    fn resists(&self, gene: GeneId) -> Option<u8> {
        self.solving.get(&gene).copied()
    }

    /// Feed one event. Returns whatever records that event completed — a frame on
    /// analysis ticks, an epoch record when a stressor shifts.
    pub fn observe(&mut self, e: &Event) -> Vec<Record> {
        self.last_tick = e.tick();
        let mut out = Vec::new();
        match e {
            Event::Birth { tick, node, parent, policy, genes, .. } => {
                self.births += 1;
                let via = if parent.is_some() { Acquisition::Birth } else { Acquisition::Founder };
                let mut state = NodeState { policy: *policy, ..NodeState::default() };
                for gene in genes {
                    state.genes.insert(*gene, via);
                    self.gain(*gene, *node, *tick, via);
                }
                self.nodes.insert(*node, state);
                self.ancestry_index.insert(*node, self.trees.ancestry.len());
                self.trees.ancestry.push((*node, *parent, *tick, None));
            }
            Event::Acquire { tick, node, gene, via, from } => {
                if let Some(state) = self.nodes.get_mut(node)
                    && state.genes.insert(*gene, *via).is_none()
                {
                    self.gain(*gene, *node, *tick, *via);
                    if via.is_lateral()
                        && let Some(donor) = from
                    {
                        self.trees.transfers.push((*tick, *gene, *donor, *node, *via));
                    }
                    if let Some(acc) = &mut self.epoch
                        && self.resistance.get(acc.hazard as usize) == Some(gene)
                        && via.is_lateral()
                    {
                        acc.rescued_laterally += 1;
                    }
                }
            }
            Event::Lose { tick, node, gene } => {
                if let Some(state) = self.nodes.get_mut(node)
                    && let Some(via) = state.genes.remove(gene)
                {
                    self.drop_copy(*gene, via, *tick);
                }
            }
            Event::Death { tick, node, cause } => {
                self.deaths += 1;
                if *cause == Cause::Lysed {
                    self.lysed += 1;
                }
                if let Some(state) = self.nodes.remove(node) {
                    for (gene, via) in state.genes {
                        self.drop_copy(gene, via, *tick);
                    }
                }
                if let Some(i) = self.ancestry_index.get(node) {
                    self.trees.ancestry[*i].3 = Some(*tick);
                }
            }
            Event::Network { partitioned, .. } => self.partitioned = *partitioned,
            Event::Discovery { gene, kind, novel, .. } => {
                self.solving.insert(*gene, *kind);
                self.discoveries += 1;
                if *novel {
                    self.novel_discoveries += 1;
                }
            }
            Event::Transfer { distance, refusal, .. } => {
                let row = self
                    .barrier
                    .entry(*distance)
                    .or_insert(BarrierRow { distance: *distance, ..BarrierRow::default() });
                row.attempts += 1;
                match refusal {
                    None => {
                        row.accepted += 1;
                        self.refusals.accepted += 1;
                    }
                    Some(Refusal::Redundant) => {
                        row.redundant += 1;
                        self.refusals.redundant += 1;
                    }
                    Some(Refusal::Restricted) => self.refusals.restricted += 1,
                    Some(Refusal::Broke) => self.refusals.broke += 1,
                    Some(Refusal::Declined) => self.refusals.declined += 1,
                    Some(Refusal::Full) => self.refusals.full += 1,
                    Some(Refusal::Immune) => self.refusals.immune += 1,
                }
            }
            Event::Epoch { tick, hazard } => {
                if let Some(rec) = self.close_epoch(*tick) {
                    out.push(Record::Epoch(Box::new(rec)));
                }
                self.open_epoch(*tick, *hazard);
            }
            Event::Tick { tick, hazard, alive, survived, energy_total, .. } => {
                if self.epoch.is_none() {
                    self.open_epoch(*tick, *hazard);
                }
                self.track_epoch(*alive);
                if tick.is_multiple_of(self.analysis_every) {
                    out.push(Record::Frame(Box::new(self.frame(
                        *tick,
                        *hazard,
                        *alive,
                        *survived,
                        *energy_total,
                    ))));
                }
            }
        }
        out
    }

    /// Close the run: the last epoch, and a record for every gene that ever mattered.
    pub fn finish(&mut self) -> Vec<Record> {
        let mut out = Vec::new();
        if let Some(rec) = self.close_epoch(self.last_tick) {
            out.push(Record::Epoch(Box::new(rec)));
        }
        for (id, stat) in &self.genes {
            if stat.max_carriers < self.report_min_carriers && self.resists(*id).is_none() {
                continue;
            }
            out.push(Record::Gene(Box::new(GeneRecord {
                kind: "gene",
                seed: self.seed,
                gene: short_id(*id),
                resists: self.resists(*id),
                first_seen: stat.first_seen,
                origin: stat.origin,
                max_carriers: stat.max_carriers,
                vertical: stat.vertical,
                lateral: stat.lateral,
                fixed_at: stat.fixed_at,
                extinct_at: stat.extinct_at,
                alive_at_end: stat.carriers > 0,
            })));
        }
        out
    }

    fn gain(&mut self, gene: GeneId, node: NodeId, tick: u32, via: Acquisition) {
        self.acquisitions.count(via);
        let stat = self.genes.entry(gene).or_insert(GeneStat {
            first_seen: tick,
            origin: node,
            carriers: 0,
            lateral_carriers: 0,
            max_carriers: 0,
            vertical: 0,
            lateral: 0,
            fixed_at: None,
            extinct_at: None,
        });
        stat.carriers += 1;
        stat.max_carriers = stat.max_carriers.max(stat.carriers);
        stat.extinct_at = None;
        if via.is_lateral() {
            stat.lateral += 1;
            stat.lateral_carriers += 1;
        } else {
            stat.vertical += 1;
        }
    }

    fn drop_copy(&mut self, gene: GeneId, via: Acquisition, tick: u32) {
        if let Some(stat) = self.genes.get_mut(&gene) {
            stat.carriers = stat.carriers.saturating_sub(1);
            if via.is_lateral() {
                stat.lateral_carriers = stat.lateral_carriers.saturating_sub(1);
            }
            if stat.carriers == 0 {
                stat.extinct_at = Some(tick);
            }
        }
    }

    fn row(&self, gene: GeneId, population: u32) -> GeneRow {
        let stat = self.genes.get(&gene);
        let carriers = stat.map_or(0, |s| s.carriers);
        let lateral = stat.map_or(0, |s| s.lateral_carriers);
        GeneRow {
            gene: short_id(gene),
            resists: self.resists(gene),
            carriers,
            freq: if population == 0 { 0.0 } else { carriers as f64 / population as f64 },
            incongruence: if carriers == 0 { 0.0 } else { lateral as f64 / carriers as f64 },
        }
    }

    fn frame(
        &mut self,
        tick: u32,
        hazard: u8,
        alive: u32,
        survived: u32,
        energy_total: u64,
    ) -> FrameRecord {
        let population = self.nodes.len() as u32;
        let copies: usize = self.nodes.values().map(|n| n.genes.len()).sum();
        let lateral_copies: usize = self
            .nodes
            .values()
            .map(|n| n.genes.values().filter(|v| v.is_lateral()).count())
            .sum();
        let distinct: BTreeSet<GeneId> =
            self.nodes.values().flat_map(|n| n.genes.keys().copied()).collect();

        let resistance: Vec<GeneRow> =
            self.resistance.clone().iter().map(|g| self.row(*g, population)).collect();
        let solvers: Vec<u32> = (0..self.resistance.len() as u8)
            .map(|kind| {
                self.nodes
                    .values()
                    .filter(|n| {
                        n.genes.keys().any(|g| self.solving.get(g) == Some(&kind))
                    })
                    .count() as u32
            })
            .collect();

        let mut top: Vec<GeneRow> = distinct.iter().map(|g| self.row(*g, population)).collect();
        top.sort_by(|a, b| b.carriers.cmp(&a.carriers).then(a.gene.cmp(&b.gene)));
        top.truncate(self.report_top);

        // Fixation is a fact about a gene, recorded the first time it is true.
        for (id, stat) in self.genes.iter_mut() {
            if stat.fixed_at.is_none()
                && population > 0
                && stat.carriers as f64 >= self.fixation_freq * population as f64
            {
                stat.fixed_at = Some(tick);
            }
            let _ = id;
        }
        if let Some(acc) = &mut self.epoch
            && acc.rescue_tick.is_none()
            && let Some(gene) = self.resistance.get(acc.hazard as usize)
            && let Some(stat) = self.genes.get(gene)
            && population > 0
            && stat.carriers as f64 >= self.fixation_freq * population as f64
        {
            acc.rescue_tick = Some(tick);
        }

        FrameRecord {
            kind: "frame",
            seed: self.seed,
            tick,
            epoch: tick / self.epoch_ticks,
            hazard,
            population,
            energy_mean: if alive == 0 { 0.0 } else { energy_total as f64 / alive as f64 },
            survival: if alive == 0 { 0.0 } else { survived as f64 / alive as f64 },
            births: self.births,
            deaths: self.deaths,
            lysed: self.lysed,
            distinct_genes: distinct.len(),
            genome_mean: if population == 0 { 0.0 } else { copies as f64 / population as f64 },
            lateral_share: if copies == 0 { 0.0 } else { lateral_copies as f64 / copies as f64 },
            partitioned: self.partitioned,
            sides: self.sides(hazard),
            divergence: self.divergence(),
            resistance,
            solvers,
            discoveries: self.discoveries,
            novel_discoveries: self.novel_discoveries,
            top,
            acquisitions: self.acquisitions,
            refusals: self.refusals,
            barrier: self.barrier.values().copied().collect(),
            policies: self.policies(),
        }
    }

    /// How far apart the two sides' gene pools are: the total-variation distance between
    /// the gene *frequencies* on each side. Zero when both sides carry the same genes in
    /// the same proportions, one when they share nothing.
    ///
    /// Frequencies rather than sets, because most gene ids in a population are one-off
    /// mutants that exist in a single node: a set comparison is almost entirely a measure
    /// of junk, and barely moves when the network is actually cut.
    fn divergence(&self) -> f64 {
        let mut here: BTreeMap<GeneId, f64> = BTreeMap::new();
        let mut there: BTreeMap<GeneId, f64> = BTreeMap::new();
        let (mut n_here, mut n_there) = (0.0, 0.0);
        for (id, state) in &self.nodes {
            let (counts, total) = if crate::transport::side(*id, self.partition_frac) {
                (&mut here, &mut n_here)
            } else {
                (&mut there, &mut n_there)
            };
            for gene in state.genes.keys() {
                *counts.entry(*gene).or_default() += 1.0;
                *total += 1.0;
            }
        }
        if n_here == 0.0 || n_there == 0.0 {
            return 0.0;
        }
        let genes: BTreeSet<GeneId> = here.keys().chain(there.keys()).copied().collect();
        let sum: f64 = genes
            .iter()
            .map(|g| {
                let p = here.get(g).copied().unwrap_or(0.0) / n_here;
                let q = there.get(g).copied().unwrap_or(0.0) / n_there;
                (p - q).abs()
            })
            .sum();
        sum / 2.0
    }

    fn sides(&self, hazard: u8) -> Sides {
        let mut s = Sides::default();
        for (id, state) in &self.nodes {
            let solves = state.genes.keys().any(|g| self.solving.get(g) == Some(&hazard));
            if crate::transport::side(*id, self.partition_frac) {
                s.here += 1;
                s.here_solvers += u32::from(solves);
            } else {
                s.there += 1;
                s.there_solvers += u32::from(solves);
            }
        }
        s
    }

    fn policies(&self) -> Policies {
        let mut out = Policies::default();
        for node in self.nodes.values() {
            match node.policy {
                Policy::AlwaysAccept => out.always_accept += 1,
                Policy::Selfish => out.selfish += 1,
                Policy::Thrifty => out.thrifty += 1,
            }
        }
        out
    }

    fn open_epoch(&mut self, tick: u32, hazard: u8) {
        let population = self.nodes.len() as u32;
        let start_freq = self
            .resistance
            .get(hazard as usize)
            .and_then(|g| self.genes.get(g))
            .map_or(0.0, |s| if population == 0 { 0.0 } else { s.carriers as f64 / population as f64 });
        self.epoch = Some(EpochAccum {
            epoch: tick / self.epoch_ticks,
            hazard,
            start_tick: tick,
            start_population: population,
            min_population: population,
            deaths: self.deaths,
            start_freq,
            rescue_tick: None,
            rescued_laterally: 0,
        });
    }

    fn track_epoch(&mut self, alive: u32) {
        if let Some(acc) = &mut self.epoch {
            acc.min_population = acc.min_population.min(alive);
        }
    }

    fn close_epoch(&mut self, tick: u32) -> Option<EpochRecord> {
        let acc = self.epoch.take()?;
        let population = self.nodes.len() as u32;
        let end_freq = self
            .resistance
            .get(acc.hazard as usize)
            .and_then(|g| self.genes.get(g))
            .map_or(0.0, |s| if population == 0 { 0.0 } else { s.carriers as f64 / population as f64 });
        Some(EpochRecord {
            kind: "epoch",
            seed: self.seed,
            epoch: acc.epoch,
            hazard: acc.hazard,
            start_tick: acc.start_tick,
            end_tick: tick,
            start_population: acc.start_population,
            min_population: acc.min_population,
            end_population: population,
            deaths: self.deaths - acc.deaths,
            start_freq: acc.start_freq,
            end_freq,
            rescue_ticks: acc.rescue_tick.map(|t| t.saturating_sub(acc.start_tick)),
            rescued_laterally: acc.rescued_laterally,
            survived: population > 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Mechanisms;
    use crate::world::World;

    fn drive(cfg: HgtConfig, seed: u64, ticks: u32) -> (Analyzer, Vec<Record>, World) {
        let mut a = Analyzer::new(&cfg, seed);
        let mut w = World::new(cfg, seed).unwrap();
        let mut records = Vec::new();
        for e in w.founding_events() {
            records.extend(a.observe(&e));
        }
        for _ in 0..ticks {
            for e in w.step() {
                records.extend(a.observe(&e));
            }
            if w.extinct() {
                break;
            }
        }
        records.extend(a.finish());
        (a, records, w)
    }

    fn cfg(mechanisms: Mechanisms) -> HgtConfig {
        HgtConfig { nodes: 32, max_nodes: 128, epoch_ticks: 150, hazard_kinds: 3, mechanisms, ..HgtConfig::default() }
    }

    #[test]
    fn the_analyzer_reconstructs_the_population_from_events_alone() {
        let (a, _, w) = drive(cfg(Mechanisms::default()), 4, 400);
        assert_eq!(a.population(), w.population(), "reconstructed population must match the world");
        for (id, node) in &w.nodes {
            let reconstructed: BTreeSet<GeneId> =
                a.nodes[id].genes.keys().copied().collect();
            let actual: BTreeSet<GeneId> = node.genome.ids().collect();
            assert_eq!(reconstructed, actual, "node {id}'s genome was reconstructed wrongly");
        }
    }

    #[test]
    fn incongruence_is_zero_without_transfer_and_positive_with_it() {
        let last_frame = |records: &[Record]| -> FrameRecord {
            records
                .iter()
                .rev()
                .find_map(|r| match r {
                    Record::Frame(f) => Some((**f).clone()),
                    _ => None,
                })
                .expect("at least one frame")
        };

        let (_, bare, _) = drive(cfg(Mechanisms::none()), 4, 300);
        let f = last_frame(&bare);
        assert_eq!(f.lateral_share, 0.0, "nothing can be lateral with no mechanism on");
        assert!(f.resistance.iter().all(|r| r.incongruence == 0.0));
        assert_eq!(f.acquisitions.lateral(), 0);

        let (_, full, _) = drive(cfg(Mechanisms::default()), 4, 300);
        let f = last_frame(&full);
        assert!(f.lateral_share > 0.0, "transfer is on and nothing was acquired sideways");
        assert!(
            f.resistance.iter().any(|r| r.incongruence > 0.0),
            "a resistance gene should be in nodes that never inherited it: {:?}",
            f.resistance
        );
        assert!(f.acquisitions.lateral() > 0);
    }

    #[test]
    fn epoch_records_show_transfer_spreading_the_gene_before_the_crisis() {
        let epochs = |records: &[Record]| -> Vec<EpochRecord> {
            records
                .iter()
                .filter_map(|r| match r {
                    Record::Epoch(e) => Some((**e).clone()),
                    _ => None,
                })
                .collect()
        };

        let (_, bare, _) = drive(cfg(Mechanisms::none()), 4, 600);
        let bare = epochs(&bare);
        let (_, full, _) = drive(cfg(Mechanisms::default()), 4, 600);
        let full = epochs(&full);

        assert!(bare.len() >= 2 && full.len() >= 3, "600 ticks at 150 apiece should close epochs");
        assert_eq!(bare[0].epoch, 0);
        assert!(bare[0].rescue_ticks.is_some(), "everyone starts able to answer epoch 0");

        // The gene that answers epoch 1 is rare when the shift lands if nothing can move
        // it, and already widespread if something can: transfer does its work *before*
        // the crisis, which is why the population is still there afterwards.
        assert!(bare[1].start_freq < 0.35, "without transfer the gene should still be rare: {:?}", bare[1]);
        assert!(
            full[1].start_freq > bare[1].start_freq,
            "transfer should have spread it ahead of the shift: {} vs {}",
            full[1].start_freq, bare[1].start_freq
        );
        assert!(full[1].survived && full[1].end_freq > 0.5, "{:?}", full[1]);
        assert!(!bare.last().unwrap().survived, "no transfer, no population: {:?}", bare.last());
    }
}
