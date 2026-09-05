//! A node: one program with an energy budget, a genome, and a list of peers it can talk
//! to. In `--transport sim` a node is this struct; over TCP it is an operating-system
//! process holding one of these.
//!
//! Two design decisions live here. First, facing a stressor means *running* genes and
//! paying for every instruction, so a large genome is a real cost and hoarding every
//! gene that comes past is not a winning strategy. Second, the restriction barrier is
//! graded by strain distance rather than binary, so transfer is a rate set by
//! relatedness — which is what makes the barrier worth measuring.

use crate::config::HgtConfig;
use crate::gene::{Acquisition, Carried, Gene, GeneId, Genome, NodeId, mutate};
use crate::hazard::Challenge;
use crate::vm;
use rand::RngExt;
use serde::{Deserialize, Serialize};

/// What a node does when it is offered a gene, and whether it offers its own. The
/// sandbox is meant to host experiments rather than one model of behaviour, so the
/// decision is a policy and not an `if` buried in the tick loop.
#[derive(Serialize, Deserialize, clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum Policy {
    /// Take everything offered, offer everything held. The baseline.
    #[default]
    AlwaysAccept,
    /// Take everything, offer nothing. Dying is not a choice, though: a selfish node's
    /// genes still leak into the population when it starves, and a phage passing through
    /// does not ask permission to move on.
    Selfish,
    /// Only trade with energy to spare — donating when comfortable, accepting only when
    /// the integration cost is not the difference between living and dying.
    Thrifty,
}

/// A gene released by a dead node, waiting to be taken up.
#[derive(Clone, Debug)]
pub struct Fragment {
    pub gene: Gene,
    pub from: NodeId,
    /// The strain of the node that released it — the barrier applies to free DNA too.
    pub strain: u8,
    /// Tick after which the fragment has decayed.
    pub expires: u32,
}

/// The outcome of one node meeting one stressor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trial {
    pub survived: bool,
    /// The gene that answered, if one did.
    pub by: Option<GeneId>,
    /// Genes executed — what the trial was charged on.
    pub tried: u32,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: NodeId,
    pub strain: u8,
    pub energy: u32,
    pub genome: Genome,
    /// Who this node can send to. Sparse, so a gene has to make its way across.
    pub peers: Vec<NodeId>,
    pub parent: Option<NodeId>,
    pub born: u32,
    /// Free DNA this node is holding, from peers that died.
    pub fragments: Vec<Fragment>,
}

impl Node {
    pub fn new(id: NodeId, strain: u8, energy: u32, born: u32, parent: Option<NodeId>) -> Node {
        Node {
            id,
            strain,
            energy,
            genome: Genome::new(),
            peers: Vec::new(),
            parent,
            born,
            fragments: Vec::new(),
        }
    }

    pub fn alive(&self) -> bool {
        self.energy > 0
    }

    pub fn spend(&mut self, amount: u32) {
        self.energy = self.energy.saturating_sub(amount);
    }

    pub fn gain(&mut self, amount: u32, cap: u32) {
        self.energy = (self.energy + amount).min(cap);
    }

    /// Staying alive, plus one charge per gene carried.
    pub fn pay_upkeep(&mut self, cfg: &HgtConfig) {
        let genome_cost = cfg.costs.gene.saturating_mul(self.genome.len() as u32);
        self.spend(cfg.costs.upkeep.saturating_add(genome_cost));
    }

    /// Meet the stressor: run genes in genome order until one answers, paying for each.
    /// A node with no answer takes damage; a node that runs out of energy part-way
    /// through simply stops trying, which is its own kind of death spiral.
    pub fn face(&mut self, ch: &Challenge, cfg: &HgtConfig, tick: u32) -> Trial {
        let mut tried = 0;
        let mut by = None;
        let codes: Vec<(GeneId, Vec<u8>)> =
            self.genome.by_recent_use().iter().map(|c| (c.gene.id, c.gene.code.clone())).collect();
        for (id, code) in codes {
            if !self.alive() {
                break;
            }
            self.spend(cfg.costs.trial);
            tried += 1;
            if vm::run(&code, ch.payload, ch.kind, cfg.vm_budget).answer == Some(ch.answer) {
                by = Some(id);
                break;
            }
        }
        match by {
            Some(id) => {
                if let Some(c) = self.genome.get_mut(id) {
                    c.last_used = Some(tick);
                    // It just answered a stressor: whatever it was, it works.
                    c.proven = true;
                }
                self.gain(cfg.reward, cfg.energy_cap)
            }
            None => self.spend(cfg.damage),
        }
        Trial { survived: by.is_some(), by, tried }
    }

    pub fn ready_to_divide(&self, cfg: &HgtConfig) -> bool {
        self.energy >= cfg.fission_threshold.saturating_add(cfg.costs.fission)
    }

    /// Divide. The child gets half the energy and a copy of the genome — mutated per
    /// byte, and missing any plasmid that failed to segregate. Genes whose code changed
    /// are new genes, with the child as their origin: that is how novelty enters.
    pub fn divide<R: RngExt>(
        &mut self,
        child_id: NodeId,
        tick: u32,
        cfg: &HgtConfig,
        rng: &mut R,
    ) -> Node {
        self.spend(cfg.costs.fission);
        let share = self.energy / 2;
        self.energy -= share;

        let mut strain = self.strain;
        if cfg.strain_drift > 0.0 && rng.random::<f64>() < cfg.strain_drift {
            strain = rng.random_range(0..cfg.strains);
        }
        let mut child = Node::new(child_id, strain, share, tick, Some(self.id));
        child.peers = self.peers.clone();
        if !child.peers.contains(&self.id) {
            child.peers.push(self.id);
        }

        for carried in self.genome.iter() {
            if carried.gene.mobile && rng.random::<f64>() < cfg.plasmid_loss {
                continue;
            }
            // A mutated copy is a new gene that nobody has ever seen work; an unchanged
            // one carries its parent's standing.
            let (gene, proven) = match mutate(&carried.gene.code, cfg.mutation_rate, rng) {
                Some(code) => (Gene::new(code, child_id, tick, carried.gene.mobile), false),
                None => (carried.gene.clone(), carried.proven),
            };
            child.genome.insert(Carried::new(gene, Acquisition::Birth, Some(self.id), tick, proven));
        }
        child
    }

    /// How far apart two strains are: differing bits in their labels. Zero means the same
    /// strain, which the restriction system never blocks.
    pub fn strain_distance(a: u8, b: u8) -> u32 {
        (a ^ b).count_ones()
    }

    /// Does this node's restriction system cut a gene arriving from `donor_strain`?
    /// Graded: the further apart the strains, the likelier the refusal.
    pub fn restricts<R: RngExt>(&self, donor_strain: u8, cfg: &HgtConfig, rng: &mut R) -> bool {
        let d = Node::strain_distance(self.strain, donor_strain);
        if d == 0 || cfg.restriction <= 0.0 {
            return false;
        }
        let bits = (u8::BITS - cfg.strains.saturating_sub(1).leading_zeros()).max(1);
        let p = cfg.restriction * (d as f64 / bits as f64).min(1.0);
        rng.random::<f64>() < p
    }

    /// Will this node offer its genes to anyone this tick?
    pub fn will_donate(&self, policy: Policy, cfg: &HgtConfig) -> bool {
        match policy {
            Policy::AlwaysAccept => true,
            Policy::Selfish => false,
            Policy::Thrifty => self.energy >= cfg.fission_threshold / 2,
        }
    }

    /// Will it take one that arrives?
    pub fn will_accept(&self, policy: Policy, cfg: &HgtConfig) -> bool {
        match policy {
            Policy::AlwaysAccept | Policy::Selfish => true,
            Policy::Thrifty => self.energy > cfg.damage + cfg.costs.integrate,
        }
    }

    /// Drop fragments that have decayed.
    pub fn expire_fragments(&mut self, tick: u32) {
        self.fragments.retain(|f| f.expires > tick);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hazard::Environment;
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;

    fn resistant_node(cfg: &HgtConfig, env: &Environment, kind: u8) -> Node {
        let mut n = Node::new(0, 0, cfg.energy_init, 0, None);
        n.genome.insert(Carried::new(
            Gene::new(env.resistance_gene(kind), 0, 0, true),
            Acquisition::Founder,
            None,
            0,
            true,
        ));
        n
    }

    #[test]
    fn carrying_the_right_gene_is_the_difference_between_gaining_and_losing_energy() {
        let cfg = HgtConfig::default();
        let env = Environment::new(&cfg, 5);
        let ch = env.challenge_at(0);

        let mut good = resistant_node(&cfg, &env, ch.kind);
        let t = good.face(&ch, &cfg, 0);
        assert!(t.survived && t.by.is_some(), "the compiled gene must answer: {t:?}");
        assert_eq!(good.energy, cfg.energy_init - cfg.costs.trial + cfg.reward);

        let mut wrong = resistant_node(&cfg, &env, (ch.kind + 1) % cfg.hazard_kinds);
        let t = wrong.face(&ch, &cfg, 0);
        assert!(!t.survived, "a gene for another stressor must not help");
        assert_eq!(wrong.energy, cfg.energy_init - cfg.costs.trial - cfg.damage);
    }

    #[test]
    fn a_child_inherits_the_genome_and_half_the_energy() {
        let cfg = HgtConfig { mutation_rate: 0.0, plasmid_loss: 0.0, strain_drift: 0.0, ..HgtConfig::default() };
        let env = Environment::new(&cfg, 5);
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(9);
        let mut parent = resistant_node(&cfg, &env, 0);
        parent.energy = 300;
        let child = parent.divide(1, 12, &cfg, &mut rng);
        assert_eq!(child.energy + parent.energy, 300 - cfg.costs.fission);
        assert_eq!(child.parent, Some(0));
        assert_eq!(child.genome.ids().collect::<Vec<_>>(), parent.genome.ids().collect::<Vec<_>>());
        assert!(child.genome.iter().all(|c| c.via == Acquisition::Birth));
        assert!(child.peers.contains(&0));
    }

    #[test]
    fn the_restriction_barrier_is_free_within_a_strain_and_graded_across_them() {
        let cfg = HgtConfig { restriction: 1.0, strains: 4, ..HgtConfig::default() };
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
        let n = Node::new(0, 0b00, cfg.energy_init, 0, None);
        assert!(!n.restricts(0b00, &cfg, &mut rng), "same strain is never cut");

        let near = (0..400).filter(|_| n.restricts(0b01, &cfg, &mut rng)).count();
        let far = (0..400).filter(|_| n.restricts(0b11, &cfg, &mut rng)).count();
        assert!(near > 100 && near < 300, "distance-1 refusals {near}/400");
        assert_eq!(far, 400, "distance-2 refusals at restriction 1.0 must be certain");
    }
}
