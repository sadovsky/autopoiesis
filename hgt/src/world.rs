//! The run loop: a population of nodes meeting a shifting stressor.
//!
//! Determinism, in `--transport sim`, comes from the same discipline as the grid crate:
//! one RNG stream, drawn from in a fixed order. Nodes live in a `BTreeMap` so every
//! phase iterates in node-id order regardless of birth and death history, and every
//! choice that could depend on iteration order (which peer to donate to, which node
//! divides first when the population is at its ceiling) is made in that order.
//!
//! The tick is: upkeep → face the stressor → network → death → fission. Death before
//! fission matters — a node that cannot pay for its genome does not get to divide first.

use crate::config::HgtConfig;
use crate::event::{Cause, Event};
use crate::gene::{Acquisition, Carried, Gene, GeneId, NodeId};
use crate::hazard::Environment;
use crate::node::Node;
use anyhow::Result;
use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use std::collections::BTreeMap;

pub type Rng = Xoshiro256PlusPlus;

/// Counters for the end-of-run summary. Everything interesting is computed from the
/// event stream instead; these are the cheap totals a human wants on one line.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub births: u64,
    pub deaths: u64,
    pub attempts: u64,
    pub transfers: u64,
    pub peak_population: usize,
    pub survived: u64,
    pub failed: u64,
}

pub struct World {
    pub cfg: HgtConfig,
    pub seed: u64,
    pub tick: u32,
    pub env: Environment,
    pub nodes: BTreeMap<NodeId, Node>,
    pub stats: Stats,
    next_id: NodeId,
    rng: Rng,
}

impl World {
    /// Build the founding population. Every founder can answer the stressor of epoch 0;
    /// for each *later* stressor, `founder_carriers` nodes carry the gene that answers it
    /// and everyone else does not. Each epoch shift therefore starts as a rare gene in a
    /// doomed population, which is the situation the sandbox exists to watch.
    pub fn new(cfg: HgtConfig, seed: u64) -> Result<World> {
        cfg.validate()?;
        let env = Environment::new(&cfg, seed);
        let mut rng = Rng::seed_from_u64(seed);
        let mut nodes: BTreeMap<NodeId, Node> = BTreeMap::new();

        for id in 0..cfg.nodes as NodeId {
            let strain = rng.random_range(0..cfg.strains);
            let mut node = Node::new(id, strain, cfg.energy_init, 0, None);
            node.genome.insert(founder_gene(&env, 0));
            nodes.insert(id, node);
        }

        let ids: Vec<NodeId> = nodes.keys().copied().collect();
        for kind in 1..env.kinds() {
            for _ in 0..cfg.founder_carriers.min(ids.len()) {
                let pick = ids[rng.random_range(0..ids.len())];
                let node = nodes.get_mut(&pick).expect("founder exists");
                node.genome.insert(founder_gene(&env, kind));
            }
        }

        for i in 0..ids.len() {
            let me = ids[i];
            let mut peers = Vec::with_capacity(cfg.degree);
            while peers.len() < cfg.degree.min(ids.len().saturating_sub(1)) {
                let p = ids[rng.random_range(0..ids.len())];
                if p != me && !peers.contains(&p) {
                    peers.push(p);
                }
            }
            nodes.get_mut(&me).expect("founder exists").peers = peers;
        }

        let next_id = cfg.nodes as NodeId;
        let peak_population = nodes.len();
        Ok(World {
            cfg,
            seed,
            tick: 0,
            env,
            nodes,
            stats: Stats { peak_population, ..Stats::default() },
            next_id,
            rng,
        })
    }

    pub fn population(&self) -> usize {
        self.nodes.len()
    }

    pub fn extinct(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The founding population as `Birth` events, so the event stream is self-contained.
    pub fn founding_events(&self) -> Vec<Event> {
        let mut out = vec![Event::Epoch { tick: 0, hazard: self.env.kind_at(0) }];
        for node in self.nodes.values() {
            out.push(Event::Birth {
                tick: 0,
                node: node.id,
                parent: None,
                strain: node.strain,
                genes: node.genome.ids().collect(),
            });
        }
        out
    }

    /// One tick. Returns the events it produced, in the order they happened.
    pub fn step(&mut self) -> Vec<Event> {
        let tick = self.tick;
        let mut events = Vec::new();
        if tick > 0 && tick.is_multiple_of(self.cfg.epoch_ticks) {
            events.push(Event::Epoch { tick, hazard: self.env.kind_at(tick) });
        }
        let ch = self.env.challenge_at(tick);

        let ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        let (mut survived, mut failed) = (0u32, 0u32);
        for id in &ids {
            let node = self.nodes.get_mut(id).expect("live node");
            node.pay_upkeep(&self.cfg);
            if !node.alive() {
                failed += 1;
                continue;
            }
            if node.face(&ch, &self.cfg).survived {
                survived += 1;
            } else {
                failed += 1;
            }
        }

        self.network_phase(tick, &mut events);

        for id in &ids {
            if !self.nodes.get(id).is_some_and(|n| n.alive()) {
                self.reap(*id, tick, Cause::Starved, &mut events);
            }
        }

        for id in &ids {
            if self.nodes.len() >= self.cfg.max_nodes {
                break;
            }
            let ready = self.nodes.get(id).is_some_and(|n| n.ready_to_divide(&self.cfg));
            if !ready {
                continue;
            }
            let child_id = self.next_id;
            self.next_id += 1;
            let child = {
                let parent = self.nodes.get_mut(id).expect("live node");
                parent.divide(child_id, tick, &self.cfg, &mut self.rng)
            };
            events.push(Event::Birth {
                tick,
                node: child.id,
                parent: child.parent,
                strain: child.strain,
                genes: child.genome.ids().collect(),
            });
            self.stats.births += 1;
            self.nodes.insert(child_id, child);
        }

        let alive = self.nodes.len() as u32;
        let energy_mean = if alive == 0 {
            0.0
        } else {
            self.nodes.values().map(|n| n.energy as f64).sum::<f64>() / alive as f64
        };
        events.push(Event::Tick { tick, hazard: ch.kind, alive, survived, failed, energy_mean });

        self.stats.survived += survived as u64;
        self.stats.failed += failed as u64;
        self.stats.peak_population = self.stats.peak_population.max(self.nodes.len());
        self.tick += 1;
        events
    }

    /// Transfer. Empty until the transport lands; the tick order is fixed around it now
    /// so that adding mechanisms does not move anything else.
    fn network_phase(&mut self, _tick: u32, _events: &mut Vec<Event>) {}

    fn reap(&mut self, id: NodeId, tick: u32, cause: Cause, events: &mut Vec<Event>) {
        if self.nodes.remove(&id).is_some() {
            events.push(Event::Death { tick, node: id, cause });
            self.stats.deaths += 1;
        }
    }

    /// FNV-1a over every node's state, in id order. The determinism test hashes this.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |b: u8| {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for node in self.nodes.values() {
            for b in node.id.to_le_bytes() {
                eat(b);
            }
            for b in node.energy.to_le_bytes() {
                eat(b);
            }
            eat(node.strain);
            for id in node.genome.ids() {
                for b in id.to_le_bytes() {
                    eat(b);
                }
            }
            for p in &node.peers {
                for b in p.to_le_bytes() {
                    eat(b);
                }
            }
        }
        h
    }

    /// How many live nodes hold `gene`.
    pub fn carriers(&self, gene: GeneId) -> usize {
        self.nodes.values().filter(|n| n.genome.contains(gene)).count()
    }
}

/// A founder's copy of a compiled resistance gene. Mobile, so it can be donated: a gene
/// nobody can pass on is not what this sandbox is about.
fn founder_gene(env: &Environment, kind: u8) -> Carried {
    let gene = Gene::new(env.resistance_gene(kind), 0, 0, true);
    Carried { gene, via: Acquisition::Founder, from: None, since: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> HgtConfig {
        HgtConfig { nodes: 12, max_nodes: 64, epoch_ticks: 40, hazard_kinds: 2, ..HgtConfig::default() }
    }

    #[test]
    fn a_population_grows_while_it_can_answer_the_stressor() {
        let mut w = World::new(small(), 1).unwrap();
        let start = w.population();
        for _ in 0..30 {
            w.step();
        }
        assert!(w.population() > start, "population {} did not grow from {start}", w.population());
        assert!(w.stats.births > 0);
    }

    #[test]
    fn the_population_collapses_when_the_stressor_shifts_and_nothing_can_transfer() {
        let cfg = HgtConfig { founder_carriers: 0, ..small() };
        let mut w = World::new(cfg, 2).unwrap();
        for _ in 0..40 {
            w.step();
        }
        let before = w.population();
        for _ in 0..40 {
            w.step();
        }
        assert!(
            w.population() * 2 < before,
            "population went {before} -> {} across a stressor shift with no resistance anywhere",
            w.population()
        );
    }

    #[test]
    fn the_same_seed_gives_the_same_world() {
        let run = |seed: u64| {
            let mut w = World::new(small(), seed).unwrap();
            for _ in 0..120 {
                w.step();
            }
            (w.hash(), w.population(), w.stats.births)
        };
        assert_eq!(run(5), run(5));
        assert_ne!(run(5).0, run(6).0);
    }
}
