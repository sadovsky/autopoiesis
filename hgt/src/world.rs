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

use crate::config::{FounderGenes, HgtConfig};
use crate::event::{Cause, Event, Refusal};
use crate::gene::{Acquisition, Carried, Gene, GeneId, Insert, NodeId};
use crate::hazard::Environment;
use crate::node::{Fragment, Node};
use crate::protocol::{Envelope, Message};
use crate::transport::{SimTransport, Transport};
use anyhow::Result;
use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use std::collections::{BTreeMap, BTreeSet};

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
    /// Mutated genes that turned out to answer a stressor.
    pub discoveries: u64,
}

pub struct World {
    pub cfg: HgtConfig,
    pub seed: u64,
    pub tick: u32,
    pub env: Environment,
    pub nodes: BTreeMap<NodeId, Node>,
    pub stats: Stats,
    pub transport: Box<dyn Transport>,
    /// The slice of the id space this world holds. Everything else is another process's.
    owns: std::ops::Range<NodeId>,
    /// Nodes a phage damaged this tick, so a death can name its cause.
    lysed: BTreeSet<NodeId>,
    next_id: NodeId,
    rng: Rng,
}

impl World {
    /// Build the founding population. Every founder can answer the stressor of epoch 0;
    /// for each *later* stressor, `founder_carriers` nodes carry the gene that answers it
    /// and everyone else does not. Each epoch shift therefore starts as a rare gene in a
    /// doomed population, which is the situation the sandbox exists to watch.
    pub fn new(cfg: HgtConfig, seed: u64) -> Result<World> {
        let transport = Box::new(SimTransport::new(cfg.latency, cfg.loss, seed));
        World::with_transport(cfg, seed, transport)
    }

    /// The same world over a different network. `tcp.rs` hands in a socket-backed
    /// transport; the tick loop cannot tell the difference.
    pub fn with_transport(cfg: HgtConfig, seed: u64, transport: Box<dyn Transport>) -> Result<World> {
        World::with_deme(cfg, seed, transport, 0..NodeId::MAX)
    }

    /// A deme: the same world, holding only the nodes in `owns`. Over TCP each process
    /// owns one stripe of the id space (`tcp::ID_STRIDE`), which is how an envelope finds
    /// the process holding its recipient without a registry.
    ///
    /// The distinction matters for one thing beyond addressing: a node id outside `owns`
    /// belongs to another process, so this world cannot know whether it is still alive
    /// and must not treat "not in my node table" as "dead".
    pub fn with_deme(
        cfg: HgtConfig,
        seed: u64,
        transport: Box<dyn Transport>,
        owns: std::ops::Range<NodeId>,
    ) -> Result<World> {
        cfg.validate()?;
        let id_base = owns.start;
        let env = Environment::new(&cfg, seed);
        let mut rng = Rng::seed_from_u64(seed);
        let mut nodes: BTreeMap<NodeId, Node> = BTreeMap::new();

        for id in id_base..id_base + cfg.nodes as NodeId {
            let strain = rng.random_range(0..cfg.strains);
            let mut node = Node::new(id, strain, cfg.energy_init, 0, None);
            node.policy = cfg.policy;
            match cfg.founder_genes {
                FounderGenes::Seeded => {
                    node.genome.insert(founder_gene(&env, 0));
                }
                FounderGenes::Random => {
                    let len = env.resistance_gene(0).len();
                    let code: Vec<u8> = (0..len).map(|_| rng.random::<u8>()).collect();
                    let gene = Gene::new(code, id, 0, true);
                    // Unproven: nobody has seen it do anything, because it does nothing.
                    node.genome.insert(Carried::new(gene, Acquisition::Founder, None, 0, false));
                }
            }
            nodes.insert(id, node);
        }

        let ids: Vec<NodeId> = nodes.keys().copied().collect();
        for _ in 0..cfg.selfish_founders.min(ids.len()) {
            let pick = ids[rng.random_range(0..ids.len())];
            nodes.get_mut(&pick).expect("founder exists").policy = crate::node::Policy::Selfish;
        }
        if cfg.founder_genes == FounderGenes::Seeded {
            for kind in 1..env.kinds() {
                for _ in 0..cfg.founder_carriers.min(ids.len()) {
                    let pick = ids[rng.random_range(0..ids.len())];
                    let node = nodes.get_mut(&pick).expect("founder exists");
                    node.genome.insert(founder_gene(&env, kind));
                }
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

        let next_id = id_base + cfg.nodes as NodeId;
        let peak_population = nodes.len();
        Ok(World {
            cfg,
            seed,
            tick: 0,
            env,
            nodes,
            stats: Stats { peak_population, ..Stats::default() },
            transport,
            owns,
            lysed: BTreeSet::new(),
            next_id,
            rng,
        })
    }

    /// Add nodes from other processes to local peer lists, so the demes are one network
    /// rather than several. Each local node learns `per_node` of them.
    pub fn introduce(&mut self, remote: &[NodeId], per_node: usize) {
        if remote.is_empty() {
            return;
        }
        let ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        for id in ids {
            for _ in 0..per_node {
                let pick = remote[self.rng.random_range(0..remote.len())];
                if let Some(n) = self.nodes.get_mut(&id)
                    && !n.peers.contains(&pick)
                {
                    n.peers.push(pick);
                }
            }
        }
    }

    /// Is this node one of ours? Anything else lives in another process.
    fn is_local(&self, id: NodeId) -> bool {
        self.owns.contains(&id)
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
                policy: node.policy,
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
        if self.cfg.partition_at == Some(tick) {
            self.transport.partition(Some(self.cfg.partition_frac));
            events.push(Event::Network { tick, partitioned: true });
        }
        if self.cfg.partition_heal_at == Some(tick) {
            self.transport.partition(None);
            events.push(Event::Network { tick, partitioned: false });
        }
        let ch = self.env.challenge_at(tick);
        let probes: Vec<crate::hazard::Challenge> = (0..self.cfg.probes)
            .map(|i| {
                let payload = self.env.probe_at(tick, i);
                crate::hazard::Challenge {
                    kind: ch.kind,
                    payload,
                    answer: self.env.answer(ch.kind, payload),
                }
            })
            .collect();
        self.lysed.clear();

        let ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        let (mut survived, mut failed) = (0u32, 0u32);
        for id in &ids {
            let node = self.nodes.get_mut(id).expect("live node");
            node.gain(self.cfg.income, self.cfg.energy_cap);
            node.pay_upkeep(&self.cfg);
            if !node.alive() {
                failed += 1;
                continue;
            }
            if node.face(&probes, &self.cfg, tick).survived {
                survived += 1;
            } else {
                failed += 1;
            }
        }

        self.network_phase(tick, &mut events);

        for id in &ids {
            if !self.nodes.get(id).is_some_and(|n| n.alive()) {
                self.reap(*id, tick, &mut events);
            }
        }

        for id in &ids {
            let ready = self.nodes.get(id).is_some_and(|n| n.ready_to_divide(&self.cfg));
            if !ready {
                continue;
            }
            // At the ceiling a birth costs somebody their place, chosen at random —
            // a Moran step. Without turnover the population simply freezes at the cap and
            // nothing can be selected for, however good a gene is; and evicting the
            // *weakest* would be worse than random, because the weakest node is usually
            // one that just divided and handed half its energy away, which would select
            // against reproducing at all.
            if self.nodes.len() >= self.cfg.max_nodes {
                let others: Vec<NodeId> =
                    self.nodes.keys().copied().filter(|n| n != id).collect();
                if others.is_empty() {
                    continue;
                }
                let victim = others[self.rng.random_range(0..others.len())];
                self.reap_with(victim, tick, Cause::Crowded, &mut events);
            }
            let child_id = self.next_id;
            self.next_id += 1;
            let child = {
                let parent = self.nodes.get_mut(id).expect("live node");
                parent.divide(child_id, tick, &self.cfg, &mut self.rng)
            };
            // A mutated copy is a new program. Most are junk; occasionally one computes
            // the answer to a stressor nobody in the population could answer, which is
            // the only way a working gene enters this world after founding.
            for carried in child.genome.iter() {
                if carried.gene.origin != child.id {
                    continue;
                }
                if let Some(kind) = self.env.solved_kind(&carried.gene.code, self.cfg.vm_budget) {
                    let novel = carried.gene.id != crate::gene::fnv1a(&self.env.resistance_gene(kind));
                    events.push(Event::Discovery {
                        tick,
                        node: child.id,
                        gene: carried.gene.id,
                        kind,
                        novel,
                    });
                    self.stats.discoveries += 1;
                }
            }
            events.push(Event::Birth {
                tick,
                node: child.id,
                parent: child.parent,
                strain: child.strain,
                policy: child.policy,
                genes: child.genome.ids().collect(),
            });
            self.stats.births += 1;
            self.nodes.insert(child_id, child);
        }

        let alive = self.nodes.len() as u32;
        let energy_total: u64 = self.nodes.values().map(|n| n.energy as u64).sum();
        events.push(Event::Tick { tick, hazard: ch.kind, alive, survived, failed, energy_total });

        self.stats.survived += survived as u64;
        self.stats.failed += failed as u64;
        self.stats.peak_population = self.stats.peak_population.max(self.nodes.len());
        self.tick += 1;
        events
    }
    /// Transfer: deliver what has arrived, then let nodes initiate, in node-id order.
    fn network_phase(&mut self, tick: u32, events: &mut Vec<Event>) {
        self.transport.set_tick(tick);
        for env in self.transport.deliver(tick) {
            self.handle(env, tick, events);
        }

        let ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        for id in ids {
            if let Some(n) = self.nodes.get_mut(&id) {
                n.expire_fragments(tick);
            }
            self.gossip(id, tick);

            let donates = self.nodes[&id].will_donate(&self.cfg);
            if self.cfg.mechanisms.conjugation
                && donates
                && self.rng.random::<f64>() < self.cfg.conjugation_rate
            {
                self.conjugate(id);
            }
            if self.cfg.mechanisms.transduction
                && donates
                && self.rng.random::<f64>() < self.cfg.transduction_rate
            {
                self.release_phage(id);
            }
            if self.cfg.mechanisms.transformation
                && !self.nodes[&id].fragments.is_empty()
                && self.rng.random::<f64>() < self.cfg.transformation_rate
            {
                self.take_up_fragment(id, tick, events);
            }
        }
    }

    /// Tell a peer who you are and who you know, so peer lists survive the deaths of the
    /// nodes in them. Staggered by node id rather than done by everyone at once.
    fn gossip(&mut self, id: NodeId, tick: u32) {
        if !(tick + id).is_multiple_of(self.cfg.gossip_every) {
            return;
        }
        let Some(peer) = self.pick_peer(id) else { return };
        let (strain, peers) = {
            let n = &self.nodes[&id];
            (n.strain, n.peers.clone())
        };
        self.transport.send(Envelope::new(id, peer, Message::Hello { strain, peers }));
    }

    /// Offer a peer everything mobile this node holds. The recipient decides what it
    /// wants; a donor cannot push code into a node that did not ask for it.
    fn conjugate(&mut self, id: NodeId) {
        let Some(node) = self.nodes.get(&id) else { return };
        if node.energy <= self.cfg.costs.conjugate {
            return;
        }
        let genes: Vec<GeneId> =
            node.genome.mobile(self.cfg.offer_unproven).map(|c| c.gene.id).collect();
        if genes.is_empty() {
            return;
        }
        let strain = node.strain;
        let Some(target) = self.pick_peer(id) else { return };
        self.transport.send(Envelope::new(id, target, Message::Offer { strain, genes }));
    }

    /// Send one of this node's genes off on its own. A phage needs no handshake and no
    /// consent — which is exactly why it can move a gene into a lineage that would never
    /// have asked for it.
    fn release_phage(&mut self, id: NodeId) {
        let Some(gene) = self.random_mobile_gene(id) else { return };
        let strain = self.nodes[&id].strain;
        let Some(target) = self.pick_peer(id) else { return };
        let msg = Message::Phage { gene, origin: id, strain, hops: self.cfg.phage_hops };
        self.transport.send(Envelope::new(id, target, msg));
    }

    /// Take up one piece of the free DNA this node is holding. Fragments come from peers
    /// that died, so transformation is a population's memory of its own dead.
    fn take_up_fragment(&mut self, id: NodeId, tick: u32, events: &mut Vec<Event>) {
        let Some(node) = self.nodes.get_mut(&id) else { return };
        let Some(frag) = node.fragments.pop() else { return };
        self.receive_gene(
            Arrival {
                from: frag.from,
                to: id,
                donor_strain: frag.strain,
                gene: frag.gene,
                via: Acquisition::Transformation,
            },
            tick,
            events,
        );
    }

    fn random_mobile_gene(&mut self, id: NodeId) -> Option<Gene> {
        let genes: Vec<Gene> = self
            .nodes
            .get(&id)?
            .genome
            .mobile(self.cfg.offer_unproven)
            .map(|c| c.gene.clone())
            .collect();
        if genes.is_empty() {
            return None;
        }
        let i = self.rng.random_range(0..genes.len());
        Some(genes[i].clone())
    }

    /// A live peer, or — if every peer this node knows is dead — any live node, which is
    /// what a seed list or a rendezvous server does on a real network.
    fn pick_peer(&mut self, id: NodeId) -> Option<NodeId> {
        let live: Vec<NodeId> = self
            .nodes
            .get(&id)?
            .peers
            .iter()
            .copied()
            .filter(|p| *p != id && (!self.is_local(*p) || self.nodes.contains_key(p)))
            .collect();
        if !live.is_empty() {
            return Some(live[self.rng.random_range(0..live.len())]);
        }
        let others: Vec<NodeId> = self.nodes.keys().copied().filter(|p| *p != id).collect();
        if others.is_empty() {
            return None;
        }
        let pick = others[self.rng.random_range(0..others.len())];
        if let Some(n) = self.nodes.get_mut(&id) {
            n.peers.push(pick);
        }
        Some(pick)
    }

    fn handle(&mut self, env: Envelope, tick: u32, events: &mut Vec<Event>) {
        let Envelope { from, to, msg } = env;
        if !self.nodes.contains_key(&to) {
            return;
        }
        match msg {
            Message::Hello { strain: _, peers } => {
                let cap = self.cfg.degree * 2;
                let node = self.nodes.get_mut(&to).expect("checked above");
                for p in std::iter::once(from).chain(peers) {
                    if p != to && !node.peers.contains(&p) && node.peers.len() < cap {
                        node.peers.push(p);
                    }
                }
            }
            Message::Offer { strain: _, genes } => {
                if !self.cfg.mechanisms.conjugation {
                    return;
                }
                let node = self.nodes.get(&to).expect("checked above");
                if !node.will_accept(&self.cfg) {
                    return;
                }
                let Some(want) = genes.into_iter().find(|g| !node.genome.contains(*g)) else {
                    return;
                };
                self.transport.send(Envelope::new(to, from, Message::Request { gene: want }));
            }
            Message::Request { gene } => {
                let node = self.nodes.get(&to).expect("checked above");
                let Some(carried) = node.genome.get(gene) else { return };
                if !carried.gene.mobile || node.energy <= self.cfg.costs.conjugate {
                    return;
                }
                let payload = carried.gene.clone();
                let strain = node.strain;
                self.nodes.get_mut(&to).expect("checked above").spend(self.cfg.costs.conjugate);
                let msg = Message::Transfer { strain, gene: payload, via: Acquisition::Conjugation };
                self.transport.send(Envelope::new(to, from, msg));
            }
            Message::Transfer { strain, gene, via } => {
                let arrival = Arrival { from, to, donor_strain: strain, gene, via };
                self.receive_gene(arrival, tick, events);
            }
            Message::Eulogy { strain, genes } => {
                if !self.cfg.mechanisms.transformation {
                    return;
                }
                let ttl = self.cfg.fragment_ttl;
                let node = self.nodes.get_mut(&to).expect("checked above");
                let cap = self.cfg.degree * 2;
                for gene in genes {
                    if !node.genome.contains(gene.id) && node.fragments.len() < cap {
                        node.fragments.push(Fragment { gene, from, strain, expires: tick + ttl });
                    }
                }
            }
            Message::Phage { gene, origin, strain, hops } => {
                if !self.cfg.mechanisms.transduction {
                    return;
                }
                // An immune node cuts the incoming DNA before it can do anything —
                // that is what the memory is *for*, and why it is worth the cost of
                // sometimes refusing a gene it needed. The phage dies here.
                if self.nodes[&to].immune_to(gene.id) {
                    self.stats.attempts += 1;
                    let distance = Node::strain_distance(strain, self.nodes[&to].strain);
                    events.push(Event::Transfer {
                        tick,
                        from: origin,
                        to,
                        gene: gene.id,
                        via: Acquisition::Transduction,
                        distance,
                        refusal: Some(Refusal::Immune),
                    });
                    return;
                }
                if self.rng.random::<f64>() < self.cfg.lysis_prob {
                    let damage = self.cfg.lysis_damage;
                    let learn = self.rng.random::<f64>() < self.cfg.crispr_rate;
                    let capacity = self.cfg.immunity_capacity;
                    let node = self.nodes.get_mut(&to).expect("checked above");
                    node.spend(damage);
                    // Surviving an attack is what teaches: a node that dies learns nothing.
                    if learn && node.alive() {
                        node.immunise(gene.id, capacity);
                    }
                    self.lysed.insert(to);
                }
                if self.nodes.get(&to).is_some_and(|n| n.alive()) {
                    let arrival = Arrival {
                        from: origin,
                        to,
                        donor_strain: strain,
                        gene,
                        via: Acquisition::Transduction,
                    };
                    self.receive_gene(arrival, tick, events);
                }
                // The phage repackages whatever this host has and moves on, which is how
                // transduction carries genes it was never sent with.
                if hops > 1
                    && let Some(next_gene) = self.random_mobile_gene(to)
                    && let Some(next) = self.pick_peer(to)
                {
                    let strain = self.nodes[&to].strain;
                    let msg =
                        Message::Phage { gene: next_gene, origin: to, strain, hops: hops - 1 };
                    self.transport.send(Envelope::new(to, next, msg));
                }
            }
            Message::Reject { .. } => {}
        }
    }

    /// A gene has arrived. Whether it is kept is the recipient's decision, and every
    /// outcome — including the refusals — is recorded: refusals are the denominator of
    /// the barrier metric.
    fn receive_gene(&mut self, arrival: Arrival, tick: u32, events: &mut Vec<Event>) {
        let Arrival { from, to, donor_strain, gene, via } = arrival;
        if !self.nodes.contains_key(&to) {
            return;
        }
        let distance = Node::strain_distance(donor_strain, self.nodes[&to].strain);
        self.stats.attempts += 1;
        let refusal = if self.nodes[&to].genome.contains(gene.id) {
            Some(Refusal::Redundant)
        } else if self.nodes[&to].immune_to(gene.id) {
            Some(Refusal::Immune)
        } else if !self.nodes[&to].will_accept(&self.cfg) {
            Some(Refusal::Declined)
        } else if self.nodes[&to].restricts(donor_strain, &self.cfg, &mut self.rng) {
            Some(Refusal::Restricted)
        } else if self.nodes[&to].energy <= self.cfg.costs.integrate {
            Some(Refusal::Broke)
        } else {
            None
        };
        let gene_id = gene.id;
        let mut refusal = refusal;
        if refusal.is_none() {
            let max = self.cfg.max_genes;
            let node = self.nodes.get_mut(&to).expect("checked above");
            // Donors only offer genes known to work, so an arriving gene carries that
            // standing with it.
            let carried = Carried::new(gene, via, Some(from), tick, true);
            match node.genome.insert_bounded(carried, max) {
                Insert::Inserted(evicted) => {
                    node.spend(self.cfg.costs.integrate);
                    self.stats.transfers += 1;
                    if let Some(lost) = evicted {
                        events.push(Event::Lose { tick, node: to, gene: lost });
                    }
                    events.push(Event::Acquire { tick, node: to, gene: gene_id, via, from: Some(from) });
                }
                Insert::Duplicate => refusal = Some(Refusal::Redundant),
                Insert::Full => refusal = Some(Refusal::Full),
            }
        }
        events.push(Event::Transfer { tick, from, to, gene: gene_id, via, distance, refusal });
    }


    /// Remove a dead node. If transformation is on it broadcasts its mobile genes to its
    /// peers on the way out: a population keeps what its dead knew for a while.
    fn reap(&mut self, id: NodeId, tick: u32, events: &mut Vec<Event>) {
        let cause = if self.lysed.contains(&id) { Cause::Lysed } else { Cause::Starved };
        self.reap_with(id, tick, cause, events);
    }

    fn reap_with(&mut self, id: NodeId, tick: u32, cause: Cause, events: &mut Vec<Event>) {
        let Some(node) = self.nodes.remove(&id) else { return };
        if self.cfg.mechanisms.transformation {
            let genes: Vec<Gene> =
                node.genome.mobile(self.cfg.offer_unproven).map(|c| c.gene.clone()).collect();
            if !genes.is_empty() {
                let live: Vec<NodeId> = node
                    .peers
                    .iter()
                    .copied()
                    .filter(|p| !self.is_local(*p) || self.nodes.contains_key(p))
                    .collect();
                for peer in live {
                    let msg = Message::Eulogy { strain: node.strain, genes: genes.clone() };
                    self.transport.send(Envelope::new(id, peer, msg));
                }
            }
        }
        self.transport.forget(id);
        events.push(Event::Death { tick, node: id, cause });
        self.stats.deaths += 1;
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

    /// How many live nodes hold a gene that actually answers `kind` — by function, not by
    /// gene id, so a discovered variant counts.
    pub fn solvers(&self, kind: u8) -> usize {
        let budget = self.cfg.vm_budget;
        self.nodes
            .values()
            .filter(|n| n.genome.iter().any(|c| self.env.solves(&c.gene.code, kind, budget)))
            .count()
    }

    /// How many live nodes hold `gene`.
    pub fn carriers(&self, gene: GeneId) -> usize {
        self.nodes.values().filter(|n| n.genome.contains(gene)).count()
    }
}

/// A gene arriving at a node, however it travelled.
struct Arrival {
    from: NodeId,
    to: NodeId,
    donor_strain: u8,
    gene: Gene,
    via: Acquisition,
}

/// A founder's copy of a compiled resistance gene. Mobile, so it can be donated: a gene
/// nobody can pass on is not what this sandbox is about.
fn founder_gene(env: &Environment, kind: u8) -> Carried {
    let gene = Gene::new(env.resistance_gene(kind), 0, 0, true);
    Carried::new(gene, Acquisition::Founder, None, 0, true)
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
