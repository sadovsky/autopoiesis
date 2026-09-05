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
    let (events, carriers) = run(c, 11, 300, gene);
    let acquisitions = events.iter().filter(|e| matches!(e, Event::Acquire { .. })).count();
    assert_eq!(acquisitions, 0, "no mechanism is on, so nothing may be acquired laterally");
    assert!(carriers > 0, "the seeded lineage should still be carrying its gene");
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
