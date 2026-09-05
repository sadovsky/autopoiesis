//! Phase 3-4 acceptance: a gene reaches a node whose ancestors never had it, and only
//! when a transfer mechanism is switched on.
//!
//! These tests read the event stream rather than the world, for the same reason the
//! metrics do: an acquisition is an event with a donor, and "this node has the gene" is
//! not evidence of how it got there.

use hgt::config::{HgtConfig, Mechanisms};
use hgt::event::{Event, Refusal};
use hgt::gene::{Acquisition, GeneId, fnv1a};
use hgt::hazard::Environment;
use hgt::world::World;

fn cfg(mechanisms: Mechanisms) -> HgtConfig {
    HgtConfig {
        nodes: 24,
        max_nodes: 128,
        epoch_ticks: 400,
        hazard_kinds: 2,
        founder_carriers: 1,
        restriction: 0.0,
        // These tests are about transfer, so the stressor is answered exactly or not at
        // all: partial credit softens the crisis and changes how often nodes die, which
        // is what transformation depends on. Discovery has its own tests.
        hazard_gradient: 0.0,
        mechanisms,
        ..HgtConfig::default()
    }
}

/// Run and return (events, final carrier count of `gene`).
fn run(cfg: HgtConfig, seed: u64, ticks: u32, gene: GeneId) -> (Vec<Event>, usize) {
    let mut w = World::new(cfg, seed).expect("valid config");
    let mut events = w.founding_events();
    for _ in 0..ticks {
        events.extend(w.step());
        if w.extinct() {
            break;
        }
    }
    let carriers = w.carriers(gene);
    (events, carriers)
}

fn future_gene(cfg: &HgtConfig, seed: u64) -> GeneId {
    fnv1a(&Environment::new(cfg, seed).resistance_gene(1))
}

#[test]
fn without_a_mechanism_a_gene_only_ever_descends_the_tree() {
    let c = cfg(Mechanisms::none());
    let gene = future_gene(&c, 11);
    let (events, _) = run(c, 11, 300, gene);

    let acquisitions = events.iter().filter(|e| matches!(e, Event::Acquire { .. })).count();
    assert_eq!(acquisitions, 0, "no mechanism is on, so nothing may be acquired laterally");

    // Every copy that exists came down the tree: it was in a genome at birth, and nothing
    // else ever put it in one. (Whether the lineage still has it at the end is a question
    // about drift, not about transfer — a rare gene can be lost.)
    let inherited = events
        .iter()
        .filter(|e| matches!(e, Event::Birth { genes, .. } if genes.contains(&gene)))
        .count();
    assert!(inherited > 0, "the seeded gene was never passed to a child");
}

#[test]
fn conjugation_carries_a_gene_to_nodes_that_never_inherited_it() {
    let seed = 11;
    let bare = cfg(Mechanisms::none());
    let gene = future_gene(&bare, seed);
    let (_, without) = run(bare, seed, 300, gene);

    let mut c = cfg(Mechanisms::none());
    c.mechanisms.conjugation = true;
    let (events, with) = run(c, seed, 300, gene);

    let lateral: Vec<&Event> = events
        .iter()
        .filter(|e| matches!(e, Event::Acquire { via: Acquisition::Conjugation, .. }))
        .collect();
    assert!(!lateral.is_empty(), "conjugation is on and nothing was transferred");
    assert!(
        with > without * 2,
        "the gene reached {with} nodes with conjugation and {without} without it"
    );

    // The handshake is a real exchange: every acquisition has a donor that held the gene.
    for e in lateral {
        if let Event::Acquire { node, from, gene: g, .. } = e {
            assert!(from.is_some(), "a conjugated gene must name its donor");
            assert_ne!(Some(*node), *from, "a node cannot conjugate with itself");
            assert!(*g != 0);
        }
    }
}

#[test]
fn the_restriction_barrier_is_graded_by_strain_distance() {
    let mut c = cfg(Mechanisms::none());
    c.mechanisms.conjugation = true;
    c.restriction = 1.0;
    c.strains = 4; // labels 0..3, so the widest distance is 2 bits
    c.strain_drift = 0.0;
    let gene = future_gene(&c, 7);
    let (events, _) = run(c, 7, 300, gene);

    // Attempts and restriction refusals, by strain distance.
    let mut attempts = [0u32; 3];
    let mut restricted = [0u32; 3];
    let mut accepted = [0u32; 3];
    for e in &events {
        if let Event::Transfer { distance, refusal, .. } = e {
            let d = *distance as usize;
            attempts[d] += 1;
            match refusal {
                None => accepted[d] += 1,
                Some(Refusal::Restricted) => restricted[d] += 1,
                Some(_) => {}
            }
        }
    }
    assert!(attempts.iter().all(|&a| a > 0), "every strain distance should be attempted: {attempts:?}");
    assert_eq!(restricted[0], 0, "a node never restricts its own strain");
    assert!(accepted[0] > 0, "same-strain transfers must get through");
    assert_eq!(accepted[2], 0, "at restriction 1.0 the widest strain distance is impassable");
    assert!(restricted[2] > 0, "and the barrier, not luck, is what stopped them");
    assert!(
        restricted[1] > 0 && restricted[1] < attempts[1],
        "distance 1 should be a rate, not a rule: {} of {}",
        restricted[1], attempts[1]
    );
}

/// Each mechanism, alone, in a world under enough pressure that nodes die often — free
/// DNA comes from the dead, so transformation has nothing to move until they do.
fn pressured(mechanisms: Mechanisms) -> HgtConfig {
    HgtConfig { nodes: 40, max_nodes: 120, epoch_ticks: 150, hazard_kinds: 3, ..cfg(mechanisms) }
}

#[test]
fn every_mechanism_moves_genes_on_its_own_and_only_its_own() {
    for (name, m) in [
        ("conjugation", Mechanisms { conjugation: true, transformation: false, transduction: false }),
        ("transformation", Mechanisms { conjugation: false, transformation: true, transduction: false }),
        ("transduction", Mechanisms { conjugation: false, transformation: false, transduction: true }),
    ] {
        let c = pressured(m);
        let gene = future_gene(&c, 3);
        let (events, _) = run(c, 3, 700, gene);
        let mut by_via = std::collections::BTreeMap::new();
        for e in &events {
            if let Event::Acquire { via, .. } = e {
                *by_via.entry(via.name()).or_insert(0usize) += 1;
            }
        }
        assert_eq!(
            by_via.keys().copied().collect::<Vec<_>>(),
            vec![name],
            "with only {name} on, acquisitions were {by_via:?}"
        );
        assert!(by_via[name] > 0);
    }
}

#[test]
fn a_phage_carries_genes_it_was_never_sent_with() {
    // A phage repackages whatever its new host holds, so the gene ids that arrive by
    // transduction are not only the ones that started the journey.
    let c = pressured(Mechanisms { conjugation: false, transformation: false, transduction: true });
    let gene = future_gene(&c, 5);
    let (events, _) = run(c, 5, 700, gene);
    let carried: std::collections::BTreeSet<GeneId> = events
        .iter()
        .filter_map(|e| match e {
            Event::Acquire { via: Acquisition::Transduction, gene, .. } => Some(*gene),
            _ => None,
        })
        .collect();
    assert!(carried.len() > 1, "phages only ever delivered {} distinct gene(s)", carried.len());
}

#[test]
fn a_selfish_node_never_donates_but_its_death_still_leaks_its_genes() {
    // Policy governs what a node chooses to do. Dying is not a choice: a selfish node
    // still releases its DNA when it starves, and a phage passing through does not ask.
    use hgt::node::Policy;
    let mut by_policy = Vec::new();
    for policy in [Policy::AlwaysAccept, Policy::Selfish, Policy::Thrifty] {
        let c = HgtConfig { policy, ..pressured(Mechanisms::default()) };
        let gene = future_gene(&c, 8);
        let (events, _) = run(c, 8, 700, gene);
        let mut conjugated = 0;
        let mut other = 0;
        for e in &events {
            if let Event::Acquire { via, .. } = e {
                if *via == Acquisition::Conjugation {
                    conjugated += 1;
                } else {
                    other += 1;
                }
            }
        }
        by_policy.push((policy, conjugated, other));
    }
    let get = |p: Policy| *by_policy.iter().find(|(q, _, _)| *q == p).unwrap();
    assert_eq!(get(Policy::Selfish).1, 0, "a selfish node must never conjugate");
    assert!(get(Policy::Selfish).2 > 0, "but its dead still leak genes into the population");
    assert!(get(Policy::Thrifty).1 > 0, "a thrifty node trades when it can afford to");
    assert!(
        get(Policy::AlwaysAccept).1 > 0,
        "an always-accept population must conjugate freely"
    );
}
